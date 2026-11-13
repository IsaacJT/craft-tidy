// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `snapcraft-components` check.

mod common;

use craft_tidy::checks;
use craft_tidy::config::{OptionValue, Settings};
use craft_tidy::project::Project;

const NAME: &str = "snapcraft-components";

fn settings(report_unused: Option<bool>) -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    if let Some(report) = report_unused {
        settings
            .check_options
            .set(NAME, "report-unused", OptionValue::Bool(report));
    }
    settings
}

/// `(line, column, message)` of every finding for `source` as the project
/// file `path`.
fn findings_in(path: &str, source: &str, settings: &Settings) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    let project = Project::new(path, source).unwrap();
    checks::run(&checks::enabled(&registry, settings), &project, settings)
        .into_iter()
        .map(|d| {
            let (line, column) = d.location.unwrap();
            (line, column, d.message)
        })
        .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    findings_in("snap/snapcraft.yaml", source, &settings(None))
}

fn messages(source: &str) -> Vec<String> {
    findings(source).into_iter().map(|f| f.2).collect()
}

/// A project defining the components `translations` and `debug-symbols`,
/// followed by `parts`.
fn project(parts: &str) -> String {
    format!(
        "name: test\n\
         components:\n  \
           translations:\n    type: standard\n  \
           debug-symbols:\n    type: standard\n\
         parts:\n{parts}"
    )
}

/// Parts that fill both components.
const FILLS_BOTH: &str = "  a:\n    plugin: nil\n    organize:\n      \
                          la: (component/translations)\n      \
                          dbg: (component/debug-symbols)/usr/lib/debug\n";

// Projects without problems

#[test]
fn accepts_components_that_are_defined_and_used() {
    assert!(findings(&project(FILLS_BOTH)).is_empty());
    // Every kind of use.
    let parts = "  a:\n    plugin: nil\n    stage:\n      - bin/*\n      \
                 - (component/translations)/la\n    prime:\n      \
                 - -(component/translations)/la/unused\n    override-prime: |\n      \
                 craftctl default\n      \
                 cp -r dbg \"${CRAFT_COMPONENT_DEBUG_SYMBOLS_PRIME}/\"\n      \
                 craftctl set components.translations.version=1.0\n      \
                 echo $CRAFT_COMPONENT_DEBUG_SYMBOLS_STAGE $CRAFT_COMPONENT_TRANSLATIONS_OVERLAY\n";
    assert!(
        findings(&project(parts)).is_empty(),
        "{:?}",
        findings(&project(parts))
    );
}

#[test]
fn accepts_projects_without_components() {
    for source in [
        "",
        "name: test\n",
        "name: test\nparts:\n  a:\n    plugin: nil\n    organize:\n      x: usr/x\n    \
         stage: [usr/*, -usr/doc]\n    override-build: echo $CRAFT_STAGE $CRAFT_PART_INSTALL\n",
        // Paths and variables that only look similar.
        "parts:\n  a:\n    organize:\n      x: component/foo\n      y: (default)/x\n    \
         stage: [(components/x)/y, x/(component/y)]\n    \
         override-build: echo $MY_CRAFT_COMPONENT_X_STAGE $CRAFT_COMPONENT_X\n",
    ] {
        assert!(
            findings(source).is_empty(),
            "{source:?}: {:?}",
            findings(source)
        );
    }
}

#[test]
fn ignores_component_references_outside_parts() {
    let source = "name: test\nsummary: copy to (component/x) with $CRAFT_COMPONENT_X_PRIME\n\
                  apps:\n  a:\n    command: craftctl set components.x.version=1\n";
    assert!(findings(source).is_empty());
}

// Undefined components

#[test]
fn reports_undefined_components_in_paths() {
    let parts = format!(
        "{FILLS_BOTH}    stage:\n      - (component/translatons)/la\n    \
         prime:\n      - -(component/docs)/x\n"
    );
    assert_eq!(
        findings(&project(&parts)),
        [
            (
                14,
                20,
                "component 'translatons' is not defined under components; did you mean \
                 'translations'?"
                    .to_owned()
            ),
            (
                16,
                21,
                "component 'docs' is not defined under components".to_owned()
            ),
        ]
    );
}

#[test]
fn reports_undefined_components_in_organize() {
    let parts = "  a:\n    organize:\n      la: (component/translation)\n      \
                 dbg: (component/debug-symbols)\n";
    // With the typo, translations is not filled by any part.
    assert_eq!(
        messages(&project(parts)),
        [
            "component 'translations' is defined but no part puts files in it, so it would be \
             empty",
            "component 'translation' is not defined under components; did you mean \
             'translations'?"
        ]
    );
    // Organize sources are not partition paths.
    let parts = format!("{FILLS_BOTH}      (component/nope)/x: y\n");
    assert!(findings(&project(&parts)).is_empty());
}

#[test]
fn reports_undefined_components_in_variables() {
    let parts = format!(
        "{FILLS_BOTH}    override-build: |\n      craftctl default\n      \
         cp x $CRAFT_COMPONENT_DEBUG_SYMBOL_PRIME/\n"
    );
    assert_eq!(
        findings(&project(&parts)),
        [(
            15,
            13,
            "$CRAFT_COMPONENT_DEBUG_SYMBOL_PRIME refers to component 'debug-symbol', which is \
             not defined under components; did you mean 'debug-symbols'?"
                .to_owned()
        )]
    );
}

#[test]
fn reports_undefined_components_in_craftctl_keys() {
    let parts = format!(
        "{FILLS_BOTH}    override-pull: |\n      craftctl set components.docs.version=\"$(git describe)\"\n"
    );
    assert_eq!(
        findings(&project(&parts)),
        [(
            14,
            31,
            "component 'docs' is not defined under components".to_owned()
        )]
    );
}

#[test]
fn reports_uses_when_no_components_are_defined() {
    let source = "name: test\nparts:\n  a:\n    organize:\n      la: (component/translations)\n";
    assert_eq!(
        messages(source),
        ["component 'translations' is not defined (no components are defined)"]
    );
}

#[test]
fn finds_uses_inside_grammar() {
    let parts =
        format!("{FILLS_BOTH}    stage:\n      - on arm64:\n          - (component/arm-only)/x\n");
    assert_eq!(
        messages(&project(&parts)),
        ["component 'arm-only' is not defined under components"]
    );
}

#[test]
fn only_suggests_close_names() {
    let parts = format!("{FILLS_BOTH}    stage: [(component/symbols)/x]\n");
    assert_eq!(
        messages(&project(&parts)),
        ["component 'symbols' is not defined under components"]
    );
}

// Unused components

#[test]
fn reports_components_that_no_part_fills() {
    let parts = "  a:\n    organize:\n      la: (component/translations)\n";
    assert_eq!(
        findings(&project(parts)),
        [(
            5,
            3,
            "component 'debug-symbols' is defined but no part puts files in it, so it would \
             be empty"
                .to_owned()
        )]
    );
    // Setting a component's metadata does not fill it.
    let parts =
        format!("{parts}    override-build: craftctl set components.debug-symbols.version=1\n");
    assert_eq!(findings(&project(&parts)).len(), 1);
    // A variable fills it.
    let parts = format!("{parts}    override-prime: cp x $CRAFT_COMPONENT_DEBUG_SYMBOLS_PRIME\n");
    assert!(findings(&project(&parts)).is_empty());
}

#[test]
fn report_unused_can_be_disabled() {
    let source = project("  a:\n    plugin: nil\n");
    assert_eq!(
        findings_in("snapcraft.yaml", &source, &settings(None)).len(),
        2
    );
    assert_eq!(
        findings_in("snapcraft.yaml", &source, &settings(Some(true))).len(),
        2
    );
    assert!(findings_in("snapcraft.yaml", &source, &settings(Some(false))).is_empty());
    // Undefined components are still reported.
    let parts = "  a:\n    organize:\n      la: (component/nope)\n";
    assert_eq!(
        findings_in("snapcraft.yaml", &project(parts), &settings(Some(false))).len(),
        1
    );
}

#[test]
fn rejects_invalid_report_unused() {
    let registry = checks::registry();
    for (value, valid) in [
        (OptionValue::Bool(false), true),
        (OptionValue::Integer(1), false),
        (OptionValue::String("yes".into()), false),
    ] {
        let mut settings = Settings::defaults();
        settings
            .check_options
            .set(NAME, "report-unused", value.clone());
        assert_eq!(
            checks::validate_options(&registry, &settings.check_options).is_ok(),
            valid,
            "{value:?}"
        );
    }
}

// Scope

#[test]
fn only_applies_to_snapcraft() {
    let source = "name: test\nparts:\n  a:\n    organize:\n      la: (component/x)\n";
    for path in [
        "snapcraft.yaml",
        ".snapcraft.yaml",
        "build-aux/snap/snapcraft.yaml",
    ] {
        assert_eq!(
            findings_in(path, source, &settings(None)).len(),
            1,
            "{path}"
        );
    }
    for path in ["rockcraft.yaml", "charmcraft.yaml", "custom.yaml"] {
        assert!(
            findings_in(path, source, &settings(None)).is_empty(),
            "{path}"
        );
    }
}

#[test]
fn has_no_fix() {
    let settings = settings(None);
    let registry = checks::registry();
    let source = "parts:\n  a:\n    organize:\n      la: (component/x)\n";
    let project = Project::new("snapcraft.yaml", source).unwrap();
    assert!(checks::fix(&checks::enabled(&registry, &settings), &project, &settings).is_none());
}

#[test]
fn findings_can_be_suppressed() {
    let parts = "  a:\n    organize:\n      # NOLINTNEXTLINE(snapcraft-components)\n      \
                 la: (component/translatons)\n      dbg: (component/debug-symbols)\n";
    // translations is unused: the typo is suppressed, but not that.
    assert_eq!(
        messages(&project(parts)),
        ["component 'translations' is defined but no part puts files in it, so it would be empty"]
    );
}
