// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `style-collapsed-sequences` check.

mod common;

use craft_tidy::checks;
use craft_tidy::config::{OptionValue, Settings};
use craft_tidy::project::Project;

use common::pyyaml_equal;

const NAME: &str = "style-collapsed-sequences";

/// Settings running only this check (or `extra` checks too), with the given
/// options.
fn settings_with(extra: &str, options: &[(&str, &str, i64)]) -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}{extra}").parse().unwrap();
    for (check, key, value) in options {
        settings
            .check_options
            .set(*check, *key, OptionValue::Integer(*value));
    }
    settings
}

fn settings(max_items: Option<i64>) -> Settings {
    match max_items {
        Some(max) => settings_with("", &[(NAME, "max-collapsed-items", max)]),
        None => settings_with("", &[]),
    }
}

fn project(source: &str) -> Project {
    Project::new("snapcraft.yaml", source).unwrap()
}

/// `(line, column, message)` of every finding.
fn findings_with(source: &str, settings: &Settings) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    checks::run(
        &checks::enabled(&registry, settings),
        &project(source),
        settings,
    )
    .into_iter()
    .map(|d| {
        assert_eq!(d.unfixable, None);
        let (line, column) = d.location.unwrap();
        (line, column, d.message)
    })
    .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    findings_with(source, &settings(None))
}

/// The fixed source, or `None` if nothing was fixed. Also verifies that the
/// fix is idempotent, leaves nothing to report and, where PyYAML is
/// available, that Snapcraft's YAML parser sees the same data.
fn fixed_with(source: &str, settings: &Settings) -> Option<String> {
    let registry = checks::registry();
    let enabled = checks::enabled(&registry, settings);
    let fixed = checks::fix(&enabled, &project(source), settings)?;
    let fixed = fixed.source().to_owned();
    assert_eq!(
        checks::fix(&enabled, &project(&fixed), settings).map(|p| p.source().to_owned()),
        None,
        "fixing again must not change anything:\n{fixed}"
    );
    assert!(
        checks::run(&enabled, &project(&fixed), settings).is_empty(),
        "nothing should remain after fixing:\n{fixed}"
    );
    if let Some(same) = pyyaml_equal(source, &fixed) {
        assert!(same, "PyYAML sees different data after fixing:\n{fixed}");
    }
    Some(fixed)
}

fn fixed(source: &str) -> Option<String> {
    fixed_with(source, &settings(None))
}

// Reporting

#[test]
fn accepts_sequences_in_the_right_style() {
    for source in [
        "plugs: [home]\n",
        "plugs: [home, network]\n",
        "plugs:\n  - home\n  - network\n  - x11\n",
        "a:\n  b:\n    - 1\n    - 2\n    - 3\n  c: ['x', \"y\"]\n",
        "",
        "name: test\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn reports_short_block_sequences() {
    assert_eq!(
        findings("name: test\nplugs:\n  - home\n  - network\n"),
        [(
            3,
            3,
            "sequence has 2 item(s), at most 2; write it on one line as [home, network]".to_owned()
        )]
    );
}

#[test]
fn reports_long_flow_sequences() {
    assert_eq!(
        findings("plugs: [home, network, x11]\n"),
        [(
            1,
            8,
            "sequence has 3 items, more than 2; write one item per line".to_owned()
        )]
    );
}

#[test]
fn checks_nested_sequences() {
    let source = "apps:\n  app:\n    plugs: [a, b, c]\nparts:\n  p:\n    after:\n      - x\n";
    let lines: Vec<_> = findings(source).into_iter().map(|f| f.0).collect();
    assert_eq!(lines, [3, 7]);
    // Including mappings inside sequences.
    let source = "items:\n  - name: x\n    list: [a, b, c]\n";
    assert_eq!(findings(source).len(), 1);
}

#[test]
fn max_collapsed_items_sets_the_limit() {
    let source = "a: [1, 2, 3]\nb:\n  - 1\n  - 2\n  - 3\n  - 4\n";
    let lines = |max| -> Vec<usize> {
        findings_with(source, &settings(Some(max)))
            .into_iter()
            .map(|f| f.0)
            .collect()
    };
    assert_eq!(lines(2), [1]);
    assert_eq!(lines(3), Vec::<usize>::new());
    assert_eq!(lines(4), [3]);
    // 0: every sequence is a block sequence.
    assert_eq!(findings_with("a: [1]\n", &settings(Some(0))).len(), 1);
}

#[test]
fn leaves_other_sequences_alone() {
    for source in [
        // Comments would be lost or misplaced.
        "a:\n  - x # why\n",
        "a: # why\n  - x\n",
        "a:\n  # why\n  - x\n",
        "a: [x, y, z] # why\n",
        // Grammar, mappings and nested sequences.
        "a:\n  - x\n  - on arm64: [y]\n",
        "a:\n  - [x]\n",
        "a: [x, [y], z]\n",
        // Anchors, tags and aliases.
        "a: &list\n  - x\n",
        "a:\n  - &x x\n",
        "a: !!seq\n  - x\n",
        "a:\n  - &x x\nb:\n  - *x\n",
        // Multi-line and block scalars.
        "a:\n  - x\n    y\n",
        "a:\n  - |\n    x\n",
        // Empty sequences, and sequences that are not the value of a key.
        "a: []\n",
        "- x\n- y\n",
        "a:\n  - - x\n",
        "a: {b: [x, y, z]}\n",
    ] {
        assert!(
            findings(source).is_empty(),
            "{source:?}: {:?}",
            findings(source)
        );
    }
}

#[test]
fn does_not_collapse_what_would_change_meaning() {
    // Plain strings containing flow indicators cannot go into [...].
    for source in [
        "a:\n  - x, y\n",
        "a:\n  - '[x]'\n  - b]\n",
        "a:\n  - k: v\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
        assert_eq!(fixed(source), None);
    }
}

#[test]
fn short_sequences_too_long_for_one_line_stay_expanded() {
    let item = "x".repeat(40);
    let source = format!("stage-packages:\n  - {item}\n  - {item}\n");
    assert!(findings(&source).is_empty());
    assert_eq!(fixed(&source), None);
    // The limit is style-line-length's max-length.
    let wide = settings_with("", &[("style-line-length", "max-length", 120)]);
    assert_eq!(findings_with(&source, &wide).len(), 1);
    // A collapsed sequence of exactly max-length characters fits.
    let source = format!("a:\n  - {}\n", "x".repeat(80 - "a: []".len()));
    assert_eq!(findings(&source).len(), 1);
    let source = format!("a:\n  - {}\n", "x".repeat(81 - "a: []".len()));
    assert!(findings(&source).is_empty());
}

// Fixing

#[test]
fn collapses_short_block_sequences() {
    assert_eq!(
        fixed("name: test\nplugs:\n  - home\n  - 'network'\nslots: [x]\n").as_deref(),
        Some("name: test\nplugs: [home, 'network']\nslots: [x]\n")
    );
    assert_eq!(
        fixed("parts:\n  p:\n    after:\n    - a\n    plugin: nil\n").as_deref(),
        Some("parts:\n  p:\n    after: [a]\n    plugin: nil\n")
    );
}

#[test]
fn expands_long_flow_sequences() {
    assert_eq!(
        fixed("apps:\n  app:\n    plugs: [home, \"net\", x11]\n    command: x\n").as_deref(),
        Some(
            "apps:\n  app:\n    plugs:\n      - home\n      - \"net\"\n      - x11\n    command: x\n"
        )
    );
    // Flow sequences spanning several lines too.
    assert_eq!(
        fixed("a: [x,\n  y, z]\n").as_deref(),
        Some("a:\n  - x\n  - y\n  - z\n")
    );
}

#[test]
fn expanded_sequences_use_the_files_indentation() {
    // Block sequences in the file are not indented relative to their key.
    let source = "b:\n- 1\n- 2\n- 3\na: [x, y, z]\n";
    assert_eq!(
        fixed(source).as_deref(),
        Some("b:\n- 1\n- 2\n- 3\na:\n- x\n- y\n- z\n")
    );
    let source = "b:\n    - 1\n    - 2\n    - 3\na: [x, y, z]\n";
    assert_eq!(
        fixed(source).as_deref(),
        Some("b:\n    - 1\n    - 2\n    - 3\na:\n    - x\n    - y\n    - z\n")
    );
}

#[test]
fn keeps_crlf_line_endings() {
    assert_eq!(
        fixed("a: [x, y, z]\r\nb:\r\n  - x\r\n").as_deref(),
        Some("a:\r\n  - x\r\n  - y\r\n  - z\r\nb: [x]\r\n")
    );
}

#[test]
fn fixes_together_with_the_other_checks() {
    // The fix keeps lists sorted, and line length agrees.
    let all = settings_with(",style-sorted-lists,style-line-length", &[]);
    let source = "plugs:\n  - home\n  - network\nbuild-packages: [cmake, curl, zlib1g-dev]\n";
    assert_eq!(
        fixed_with(source, &all).as_deref(),
        Some("plugs: [home, network]\nbuild-packages:\n  - cmake\n  - curl\n  - zlib1g-dev\n")
    );
    // A short flow sequence that is too long is expanded by
    // style-line-length and then left alone.
    let item = "y".repeat(40);
    let source = format!("stage-packages: [{item}, {item}]\n");
    let fixed = fixed_with(&source, &all).unwrap();
    assert_eq!(fixed, format!("stage-packages:\n  - {item}\n  - {item}\n"));
}

// Options

#[test]
fn rejects_invalid_max_collapsed_items() {
    let registry = checks::registry();
    for (value, valid) in [
        (OptionValue::Integer(0), true),
        (OptionValue::Integer(5), true),
        (OptionValue::Integer(-1), false),
        (OptionValue::String("2".into()), false),
        (OptionValue::Bool(true), false),
    ] {
        let mut settings = Settings::defaults();
        settings
            .check_options
            .set(NAME, "max-collapsed-items", value.clone());
        assert_eq!(
            checks::validate_options(&registry, &settings.check_options).is_ok(),
            valid,
            "{value:?}"
        );
    }
}

#[test]
fn sorting_and_collapsing_the_same_list_agree() {
    let all = settings_with(",style-sorted-lists", &[]);
    let registry = checks::registry();
    let enabled = checks::enabled(&registry, &all);
    for source in [
        "plugs:\n  - network\n  - home\n",
        "plugs: [x11, network, home]\n",
    ] {
        let fixed = checks::fix(&enabled, &project(source), &all).unwrap();
        assert!(
            checks::run(&enabled, &fixed, &all).is_empty(),
            "{}",
            fixed.source()
        );
        assert!(checks::fix(&enabled, &fixed, &all).is_none());
    }
    let fixed = checks::fix(&enabled, &project("plugs:\n  - network\n  - home\n"), &all);
    assert_eq!(fixed.unwrap().source(), "plugs: [home, network]\n");
}
