// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `metadata-license` check.

use craft_tidy::checks;
use craft_tidy::config::Settings;
use craft_tidy::project::Project;

const NAME: &str = "metadata-license";

fn settings() -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    settings
}

/// `(line, column, covered text, message)` of every finding for `source`
/// as the project file `path`.
fn findings_in(path: &str, source: &str) -> Vec<(usize, usize, String, String)> {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new(path, source).unwrap();
    checks::run(&checks::enabled(&registry, &settings), &project, &settings)
        .into_iter()
        .map(|d| {
            assert_eq!(
                d.unfixable.as_deref(),
                Some("craft-tidy cannot know the project's license")
            );
            let (line, column) = d.location.unwrap();
            let covered = source[d.span.unwrap()].to_owned();
            (line, column, covered, d.message)
        })
        .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String, String)> {
    findings_in("snap/snapcraft.yaml", source)
}

fn message(problem: &str) -> String {
    format!(
        "{problem}; add a top-level 'license' with an SPDX expression, such as \
         'license: GPL-3.0-only', or 'license: proprietary'"
    )
}

#[test]
fn accepts_projects_with_a_license() {
    for license in [
        "GPL-3.0-only",
        "Apache-2.0 OR MIT",
        "proprietary",
        "'GPL-2.0-or-later'",
    ] {
        let source = format!("name: test\nlicense: {license}\n");
        assert!(findings(&source).is_empty(), "{license}");
    }
}

#[test]
fn reports_projects_without_a_license_at_their_name() {
    assert_eq!(
        findings("summary: x\nname: test\n"),
        [(
            2,
            1,
            "name".to_owned(),
            message("the project does not declare its license")
        )]
    );
    // Without a name, at the start of the file.
    assert_eq!(
        findings("summary: x\n"),
        [(
            1,
            1,
            String::new(),
            message("the project does not declare its license")
        )]
    );
}

#[test]
fn reports_empty_licenses() {
    for value in ["", " ~", " null", " ''", " \"  \""] {
        let source = format!("name: test\nlicense:{value}\n");
        let found = findings(&source);
        assert_eq!(found.len(), 1, "{value:?}");
        assert_eq!((found[0].0, found[0].1), (2, 1));
        assert!(found[0].2.starts_with("license"));
        assert_eq!(found[0].3, message("the license is empty"));
    }
    // A quoted "null" is a license name, if an odd one.
    assert!(findings("name: test\nlicense: 'null'\n").is_empty());
}

#[test]
fn accepts_snapcraft_projects_adopting_metadata() {
    // Snapcraft can take the license from a part's AppStream metadata.
    let source = "name: test\nadopt-info: app\n";
    assert!(findings(source).is_empty());
    // But an empty license is still reported.
    assert_eq!(findings("name: test\nadopt-info: app\nlicense:\n").len(), 1);
    // Other tools do not adopt the license.
    assert_eq!(findings_in("rockcraft.yaml", source).len(), 1);
}

#[test]
fn applies_to_tools_that_accept_a_license() {
    for path in [
        "snapcraft.yaml",
        "rockcraft.yaml",
        "imagecraft.yaml",
        "debcraft.yaml",
    ] {
        assert_eq!(findings_in(path, "name: test\n").len(), 1, "{path}");
    }
    // Charmcraft rejects a license key, and unknown files are not checked.
    for path in ["charmcraft.yaml", "custom.yaml"] {
        assert!(findings_in(path, "name: test\n").is_empty(), "{path}");
    }
}

#[test]
fn ignores_documents_that_are_not_mappings() {
    for source in ["", "- a\n", "just text\n"] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn can_be_suppressed_at_the_name() {
    assert!(findings("name: test # NOLINT(metadata-license)\n").is_empty());
}

#[test]
fn has_no_fix() {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new("snapcraft.yaml", "name: test\n").unwrap();
    assert!(checks::fix(&checks::enabled(&registry, &settings), &project, &settings).is_none());
}
