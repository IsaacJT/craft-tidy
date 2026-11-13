// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `project-root-source` check.

use craft_tidy::checks;
use craft_tidy::config::Settings;
use craft_tidy::project::Project;

const NAME: &str = "project-root-source";

fn settings() -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    settings
}

/// `(line, column, message)` of every finding for `source` as `path`.
fn findings_in(path: &str, source: &str) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new(path, source).unwrap();
    checks::run(&checks::enabled(&registry, &settings), &project, &settings)
        .into_iter()
        .map(|d| {
            assert_eq!(
                d.unfixable.as_deref(),
                Some("craft-tidy cannot know which directory holds the code")
            );
            let (line, column) = d.location.unwrap();
            (line, column, d.message)
        })
        .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    findings_in("snap/snapcraft.yaml", source)
}

/// A project with a single part whose source is `value`.
fn part_with_source(value: &str) -> String {
    format!("name: test\nparts:\n  app:\n    plugin: dump\n    source: {value}\n")
}

fn message(part: &str, value: &str) -> String {
    format!(
        "part '{part}' uses the whole project directory as its source ('{value}'), so it picks \
         up build artifacts there, which with --destructive-mode trigger rebuilds; use the \
         directory with the code instead, such as 'src'"
    )
}

#[test]
fn reports_parts_using_the_project_directory() {
    assert_eq!(
        findings(&part_with_source(".")),
        [(5, 13, message("app", "."))]
    );
    for (value, path) in [
        ("./", "./"),
        ("./.", "./."),
        (".//", ".//"),
        ("'.'", "."),
        ("\".\"", "."),
    ] {
        assert_eq!(
            findings(&part_with_source(value)),
            [(5, 13, message("app", path))],
            "{value}"
        );
    }
}

#[test]
fn accepts_other_sources() {
    for value in [
        "src",
        "./src",
        "src/.",
        ".config",
        "..",
        "../app",
        "/",
        "/srv/app",
        "''",
        "https://example.com/app.tar.gz",
        "git@github.com:example/app.git",
    ] {
        assert!(findings(&part_with_source(value)).is_empty(), "{value}");
    }
    // Parts without a source, and keys other than source.
    let source = "parts:\n  a:\n    plugin: nil\n  b:\n    plugin: dump\n    \
                  source-subdir: .\n    override-build: cp -r . $CRAFT_PART_INSTALL\n";
    assert!(findings(source).is_empty());
    // Not a part.
    assert!(findings("source: .\napps:\n  a:\n    source: .\n").is_empty());
}

#[test]
fn checks_every_part_and_grammar() {
    let source = "parts:\n  a:\n    source: .\n  b:\n    source: src\n  c:\n    source:\n      \
                  - on amd64: .\n      - else: src\n";
    let found: Vec<_> = findings(source)
        .into_iter()
        .map(|(line, column, _)| (line, column))
        .collect();
    assert_eq!(found, [(3, 13), (8, 19)]);
}

#[test]
fn applies_to_every_tool_but_charmcraft() {
    let source = part_with_source(".");
    for path in [
        "snapcraft.yaml",
        "rockcraft.yaml",
        "imagecraft.yaml",
        "debcraft.yaml",
    ] {
        assert_eq!(findings_in(path, &source).len(), 1, "{path}");
    }
    // Charmcraft's charm plugin builds from the project directory.
    for path in ["charmcraft.yaml", "custom.yaml"] {
        assert!(findings_in(path, &source).is_empty(), "{path}");
    }
}

#[test]
fn has_no_fix() {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new("snapcraft.yaml", part_with_source(".")).unwrap();
    assert!(checks::fix(&checks::enabled(&registry, &settings), &project, &settings).is_none());
}

#[test]
fn can_be_suppressed() {
    let source = "parts:\n  a:\n    source: . # NOLINT(project-root-source): upstream layout\n";
    assert!(findings(source).is_empty());
}
