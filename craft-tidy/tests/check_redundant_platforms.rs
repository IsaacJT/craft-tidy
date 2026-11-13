// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `style-redundant-platforms` check.

mod common;

use craft_tidy::checks;
use craft_tidy::config::Settings;
use craft_tidy::project::Project;

use common::pyyaml_equal;

const NAME: &str = "style-redundant-platforms";

fn settings() -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    settings
}

fn findings_in(path: &str, source: &str) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new(path, source).unwrap();
    checks::run(&checks::enabled(&registry, &settings), &project, &settings)
        .into_iter()
        .map(|d| {
            let (line, column) = d.location.unwrap();
            (line, column, d.message)
        })
        .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    findings_in("snap/snapcraft.yaml", source)
}

/// The fixed source, or `None` if nothing was fixed. Also verifies that the
/// fix is idempotent and leaves nothing to report.
fn fixed(source: &str) -> Option<String> {
    let registry = checks::registry();
    let settings = settings();
    let enabled = checks::enabled(&registry, &settings);
    let project = Project::new("snapcraft.yaml", source).unwrap();
    let fixed = checks::fix(&enabled, &project, &settings)?;
    let fixed = fixed.source().to_owned();
    let again = Project::new("snapcraft.yaml", fixed.as_str()).unwrap();
    assert!(checks::fix(&enabled, &again, &settings).is_none());
    assert!(checks::run(&enabled, &again, &settings).is_empty());
    Some(fixed)
}

fn message(name: &str) -> String {
    format!(
        "platform '{name}' only builds on and for {name}, which is what '{name}:' alone means; \
         remove its build-on and build-for"
    )
}

// Reporting

#[test]
fn reports_platforms_that_build_on_and_for_their_own_architecture() {
    let source = "name: test\nplatforms:\n  arm64:\n    build-on: [arm64]\n    \
                  build-for: [arm64]\n";
    assert_eq!(findings(source), [(3, 3, message("arm64"))]);
}

#[test]
fn accepts_every_way_of_writing_it() {
    for definition in [
        "{build-on: [amd64], build-for: [amd64]}",
        "{build-on: amd64, build-for: amd64}",
        "{build-on: [amd64], build-for: amd64}",
        "{build-for: [amd64], build-on: [amd64]}",
        // build-for defaults to the platform's name.
        "{build-on: [amd64]}",
        "{build-on: ['amd64'], build-for: [\"amd64\"]}",
    ] {
        let source = format!("platforms:\n  amd64: {definition}\n");
        assert_eq!(findings(&source).len(), 1, "{definition}");
    }
}

#[test]
fn checks_every_architecture() {
    for arch in [
        "amd64", "arm64", "armhf", "i386", "ppc64el", "riscv64", "s390x",
    ] {
        let source = format!("platforms:\n  {arch}:\n    build-on: [{arch}]\n");
        assert_eq!(findings(&source), [(2, 3, message(arch))]);
    }
}

#[test]
fn accepts_platforms_that_need_their_definition() {
    for source in [
        // Already short.
        "platforms:\n  amd64:\n  arm64:\n",
        // Several architectures, or cross-building.
        "platforms:\n  riscv64:\n    build-on: [amd64, riscv64]\n    build-for: [riscv64]\n",
        "platforms:\n  armhf:\n    build-on: [amd64]\n    build-for: [armhf]\n",
        "platforms:\n  amd64:\n    build-on: [amd64]\n    build-for: [arm64]\n",
        "platforms:\n  amd64:\n    build-on: [amd64, amd64]\n",
        // Names that are not architectures: the short form needs one.
        "platforms:\n  my-platform:\n    build-on: [amd64]\n    build-for: [amd64]\n",
        "platforms:\n  all:\n    build-on: [all]\n    build-for: [all]\n",
        "platforms:\n  ubuntu@24.04:amd64:\n    build-on: [ubuntu@24.04:amd64]\n    \
         build-for: [ubuntu@24.04:amd64]\n",
        "platforms:\n  AMD64:\n    build-on: [AMD64]\n",
        // Other keys, or build-for without the required build-on.
        "platforms:\n  amd64:\n    build-on: [amd64]\n    extra: x\n",
        "platforms:\n  amd64:\n    build-for: [amd64]\n",
        "platforms:\n  amd64: {}\n",
        // Anchors, tags, aliases and nested values.
        "platforms:\n  amd64: &p\n    build-on: [amd64]\n",
        "platforms:\n  amd64:\n    build-on: &a [amd64]\n",
        "platforms:\n  amd64:\n    build-on: [[amd64]]\n",
        // Not the top-level platforms.
        "parts:\n  p:\n    platforms:\n      amd64:\n        build-on: [amd64]\n",
        "platforms: [amd64]\n",
        "",
    ] {
        assert!(
            findings(source).is_empty(),
            "{source:?}: {:?}",
            findings(source)
        );
        assert_eq!(fixed(source), None, "{source:?}");
    }
}

#[test]
fn leaves_definitions_with_comments_alone() {
    for source in [
        "platforms:\n  amd64:\n    build-on: [amd64] # native only\n",
        "platforms:\n  amd64:\n    # native only\n    build-on: [amd64]\n",
        "platforms:\n  amd64: # native only\n    build-on: [amd64]\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
        assert_eq!(fixed(source), None, "{source:?}");
    }
    // A comment after the definition is kept.
    let source = "platforms:\n  amd64:\n    build-on: [amd64]\n  # more to come\n";
    assert_eq!(
        fixed(source).as_deref(),
        Some("platforms:\n  amd64:\n  # more to come\n")
    );
}

#[test]
fn applies_to_every_craft_tool() {
    let source = "platforms:\n  amd64:\n    build-on: [amd64]\n";
    for path in [
        "snapcraft.yaml",
        "rockcraft.yaml",
        "charmcraft.yaml",
        "imagecraft.yaml",
        "debcraft.yaml",
        "custom.yaml",
    ] {
        assert_eq!(findings_in(path, source).len(), 1, "{path}");
    }
}

// Fixing

#[test]
fn removes_redundant_definitions() {
    let source = "name: test\nplatforms:\n  amd64:\n    build-on: [amd64]\n    \
                  build-for: [amd64]\n  arm64:\n    build-on: arm64\n  riscv64:\n    \
                  build-on: [amd64, riscv64]\n    build-for: [riscv64]\n  \
                  i386: {build-on: [i386], build-for: i386}\nparts: {}\n";
    assert_eq!(
        fixed(source).as_deref(),
        Some(
            "name: test\nplatforms:\n  amd64:\n  arm64:\n  riscv64:\n    \
             build-on: [amd64, riscv64]\n    build-for: [riscv64]\n  i386:\nparts: {}\n"
        )
    );
}

#[test]
fn keeps_crlf_line_endings() {
    let source = "platforms:\r\n  amd64:\r\n    build-on: [amd64]\r\nname: x\r\n";
    assert_eq!(
        fixed(source).as_deref(),
        Some("platforms:\r\n  amd64:\r\nname: x\r\n")
    );
}

#[test]
fn the_fix_means_the_same_to_pyyaml() {
    // Each tool reads `arm64:` as a null definition, which it expands to
    // building on and for arm64.
    let source = "platforms:\n  arm64:\n    build-on: [arm64]\n";
    let fixed = fixed(source).unwrap();
    if let Some(same) = pyyaml_equal(&fixed, "platforms:\n  arm64: null\n") {
        assert!(same);
    }
}
