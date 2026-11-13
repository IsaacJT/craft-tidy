// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `style-sorted-lists` check.

mod common;

use craft_tidy::checks;
use craft_tidy::config::{CheckOptions, OptionValue, Settings};
use craft_tidy::project::Project;

use common::pyyaml_equal;

const NAME: &str = "style-sorted-lists";

fn settings(options: &[(&str, OptionValue)]) -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    for (key, value) in options {
        settings.check_options.set(NAME, *key, value.clone());
    }
    settings
}

fn keys(keys: &str) -> Vec<(&'static str, OptionValue)> {
    vec![("keys", OptionValue::String(keys.to_owned()))]
}

fn project(source: &str) -> Project {
    Project::new("snapcraft.yaml", source).unwrap()
}

/// `(line, column, message)` of every finding.
fn findings_with(source: &str, options: &[(&str, OptionValue)]) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    let settings = settings(options);
    checks::run(
        &checks::enabled(&registry, &settings),
        &project(source),
        &settings,
    )
    .into_iter()
    .map(|d| {
        let (line, column) = d.location.unwrap();
        (line, column, d.message)
    })
    .collect()
}

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    findings_with(source, &[])
}

fn messages(source: &str) -> Vec<String> {
    findings(source).into_iter().map(|f| f.2).collect()
}

/// The fixed source, or `None` if nothing was fixed. Also verifies that the
/// fix is idempotent and leaves nothing to report.
fn fixed_with(source: &str, options: &[(&str, OptionValue)]) -> Option<String> {
    let registry = checks::registry();
    let settings = settings(options);
    let enabled = checks::enabled(&registry, &settings);
    let fixed = checks::fix(&enabled, &project(source), &settings)?
        .source()
        .to_owned();
    assert_eq!(
        checks::fix(&enabled, &project(&fixed), &settings).map(|p| p.source().to_owned()),
        None,
        "fixing again must not change anything:\n{fixed}"
    );
    Some(fixed)
}

fn fixed(source: &str) -> Option<String> {
    fixed_with(source, &[])
}

/// Asserts that `source` is fixed to exactly `expected`, and that PyYAML
/// (Snapcraft's parser) agrees that it holds the expected data.
fn assert_fixed(source: &str, expected: &str) {
    let actual = fixed(source).unwrap_or_else(|| panic!("{source:?} should be fixed"));
    assert_eq!(actual, expected);
    assert!(findings(&actual).is_empty(), "{actual}");
    if let Some(same) = pyyaml_equal(&actual, expected) {
        assert!(same);
    }
}

// Registration and options

#[test]
fn is_registered_with_documented_options() {
    let registry = checks::registry();
    let check = registry.iter().find(|c| c.name() == NAME).expect(NAME);
    let options: Vec<_> = check.options().iter().map(|o| o.name).collect();
    assert_eq!(options, ["keys", "case-sensitive"]);
}

#[test]
fn accepts_valid_options() {
    let registry = checks::registry();
    for (key, value) in [
        ("keys", OptionValue::String("*".into())),
        ("keys", OptionValue::String("plugs,-slots,build-*".into())),
        ("keys", OptionValue::String(String::new())),
        ("case-sensitive", OptionValue::Bool(true)),
    ] {
        let mut options = CheckOptions::default();
        options.set(NAME, key, value.clone());
        assert!(
            checks::validate_options(&registry, &options).is_ok(),
            "{key}={value:?}"
        );
    }
}

#[test]
fn rejects_invalid_options() {
    let registry = checks::registry();
    for (key, value) in [
        ("keys", OptionValue::String("Build-Packages".into())),
        ("keys", OptionValue::String("plugs slots".into())),
        ("keys", OptionValue::Integer(1)),
        ("case-sensitive", OptionValue::String("yes".into())),
    ] {
        let mut options = CheckOptions::default();
        options.set(NAME, key, value.clone());
        assert!(
            checks::validate_options(&registry, &options).is_err(),
            "{key}={value:?} should be rejected"
        );
    }
}

#[test]
fn documented_default_keys_match_the_behaviour() {
    let registry = checks::registry();
    let check = registry.iter().find(|c| c.name() == NAME).unwrap();
    let default = check.options()[0].default.trim_matches('"');
    let source = "\
after: [b, a]\nassumes: [b, a]\nbefore: [b, a]\nbuild-attributes: [b, a]\nbuild-for: [b, a]
build-on: [b, a]\nbuild-packages: [b, a]\nbuild-snaps: [b, a]\nplugs: [b, a]\nslots: [b, a]
stage-packages: [b, a]\nstage-snaps: [b, a]\ncommand-chain: [b, a]\nother: [b, a]\n";
    let default_findings = findings(source);
    assert_eq!(default_findings.len(), 12);
    assert_eq!(findings_with(source, &keys(default)), default_findings);
}

// Detection

#[test]
fn sorted_lists_are_fine() {
    for source in [
        "build-packages: [a, b, c]\n",
        "build-packages:\n  - a\n  - b\n",
        "build-packages: []\n",
        "build-packages: [a, a, b]\n",
        "build-packages: [Bash, curl, zlib]\n",
        "build-packages:\n  - a\n  - on arm64: [b, c]\n  - else: [d]\n",
        "stage-packages:\n  - a\n  - to armhf: [b]\n  - else fail\n",
        "build-packages:\n  - z\n  - try: [b]\n  - else: [a]\n",
    ] {
        assert!(
            findings(source).is_empty(),
            "{source:?}: {:?}",
            findings(source)
        );
    }
}

#[test]
fn reports_unsorted_lists_at_the_first_misplaced_entry() {
    assert_eq!(
        findings("parts:\n  foo:\n    build-packages:\n      - b\n      - c\n      - a\n"),
        [(
            4,
            9,
            "entries of 'build-packages' are not sorted: 'a' should come before 'b'".to_owned()
        )]
    );
    assert_eq!(
        findings("plugs: [home, audio, network]\n"),
        [(
            1,
            9,
            "entries of 'plugs' are not sorted: 'audio' should come before 'home'".to_owned()
        )]
    );
}

#[test]
fn compares_unquoted_values() {
    assert_eq!(
        messages("build-packages: ['b', \"a\"]\n"),
        ["entries of 'build-packages' are not sorted: 'a' should come before 'b'"]
    );
}

#[test]
fn ignores_case_by_default() {
    assert!(findings("plugs: [Bash, curl]\n").is_empty());
    assert_eq!(findings("plugs: [curl, Bash]\n").len(), 1);
}

#[test]
fn case_sensitive_option_sorts_uppercase_first() {
    let options = [("case-sensitive", OptionValue::Bool(true))];
    assert!(findings_with("plugs: [Curl, bash]\n", &options).is_empty());
    assert_eq!(findings_with("plugs: [bash, Curl]\n", &options).len(), 1);
    assert_eq!(findings("plugs: [Curl, bash]\n").len(), 1);
}

#[test]
fn grammar_must_come_after_plain_entries() {
    assert_eq!(
        messages("build-packages:\n  - on arm64: [x]\n  - a\n"),
        [
            "entries of 'build-packages' are not sorted: 'a' should come before advanced \
             grammar entry 'on arm64'"
        ]
    );
    assert_eq!(
        messages("stage-packages:\n  - to armhf: [x]\n  - else fail\n  - a\n").len(),
        1
    );
}

#[test]
fn grammar_statements_keep_their_order() {
    let source = "build-packages:\n  - a\n  - on s390x: [x]\n  - on arm64: [y]\n  - else: [z]\n";
    assert!(findings(source).is_empty());
}

#[test]
fn recognises_every_craft_grammar_statement() {
    for statement in [
        "on arm64: [x]",
        "on amd64,arm64: [x]",
        "to armhf: [x]",
        "on amd64 to arm64: [x]",
        "for ubuntu@24.04:riscv64: [x]",
        "try: [x]",
        "'on  arm64': [x]",
        "\"for\\tamd64\": [x]",
    ] {
        let source = format!("build-packages:\n  - {statement}\n  - a\n");
        assert_eq!(
            findings(&source).len(),
            1,
            "{statement:?} should be recognised as grammar"
        );
        let sorted = format!("build-packages:\n  - a\n  - {statement}\n  - else: [y]\n");
        assert!(findings(&sorted).is_empty(), "{statement:?}");
    }
}

#[test]
fn recognises_else_fail_with_any_whitespace() {
    for else_fail in ["else fail", "'else  fail'", "\"else\\tfail\""] {
        let source = format!("stage-packages:\n  - a\n  - to armhf: [x]\n  - {else_fail}\n");
        assert!(findings(&source).is_empty(), "{else_fail:?}");
    }
}

#[test]
fn allows_else_after_else_fail() {
    // craft-grammar keeps the statement open after `else fail`.
    let source = "build-packages:\n  - a\n  - on arm64: [x]\n  - else fail\n  - else: [y]\n";
    assert!(findings(source).is_empty());
}

#[test]
fn similar_looking_entries_are_not_grammar() {
    // Mappings that are not grammar statements make the list unsortable.
    for entry in [
        "onarm64: [x]",
        "on: [x]",
        "for: [x]",
        "to : [x]",
        "trying: [x]",
        "else now: [x]",
    ] {
        let source = format!("build-packages:\n  - {entry}\n  - b\n  - a\n");
        assert!(
            findings(&source).is_empty(),
            "{entry:?} must not be grammar"
        );
    }
    // Strings that are not `else fail` are plain entries, so they must not
    // follow a grammar statement (a real `else fail` would be fine there).
    for entry in ["else fails", "'else fail now'", "' else fail'", "elsefail"] {
        let source = format!("build-packages:\n  - on arm64: [x]\n  - {entry}\n");
        assert_eq!(
            findings(&source).len(),
            1,
            "{entry:?} should be a plain entry"
        );
    }
    assert!(findings("build-packages:\n  - on arm64: [x]\n  - else fail\n").is_empty());
}

#[test]
fn checks_lists_inside_grammar_statements() {
    assert_eq!(
        findings("build-packages:\n  - on arm64:\n      - b\n      - a\n"),
        [(
            3,
            9,
            "entries of 'build-packages: on arm64' are not sorted: 'a' should come before 'b'"
                .to_owned()
        )]
    );
    assert_eq!(
        messages("build-packages:\n  - on arm64:\n      - on arm64 to armhf: [b, a]\n"),
        [
            "entries of 'build-packages: on arm64: on arm64 to armhf' are not sorted: 'a' \
             should come before 'b'"
        ]
    );
}

#[test]
fn only_checks_selected_keys() {
    for source in [
        "command-chain: [b, a]\n",
        "cmake-parameters: [-DB=1, -DA=1]\n",
        "unknown-key: [b, a]\n",
        "parts:\n  foo:\n    override-build: b a\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn checks_selected_keys_at_any_depth() {
    let source = "\
plugs: [b, a]
apps:
  x:
    plugs: [b, a]
hooks:
  install:
    plugs: [b, a]
parts:
  p:
    after: [b, a]
";
    let lines: Vec<_> = findings(source).iter().map(|f| f.0).collect();
    assert_eq!(lines, [1, 4, 7, 10]);
}

#[test]
fn keys_option_selects_lists() {
    let source = "command-chain: [b, a]\nplugs: [b, a]\n";
    let lines = |k: &str| -> Vec<usize> {
        findings_with(source, &keys(k))
            .iter()
            .map(|f| f.0)
            .collect()
    };
    assert_eq!(lines("command-chain"), [1]);
    assert_eq!(lines("*"), [1, 2]);
    assert_eq!(lines("*,-command-chain"), [2]);
    assert_eq!(lines(""), Vec::<usize>::new());
}

#[test]
fn skips_lists_that_cannot_be_sorted_safely() {
    for source in [
        // Entries that are neither plain values nor grammar.
        "plugs:\n  - b\n  - {name: a}\n",
        "plugs:\n  - b\n  - [a]\n",
        "plugs:\n  - &x b\n  - a\n",
        "x: &x a\nplugs:\n  - b\n  - *x\n",
        "plugs:\n  - b\n  - !!str a\n",
        // An else that does not follow a grammar statement.
        "build-packages:\n  - b\n  - else: [x]\n  - a\n",
        "build-packages:\n  - b\n  - else fail\n  - a\n",
        "build-packages:\n  - on arm64: [x]\n  - b\n  - else: [y]\n  - a\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn looks_inside_lists_of_mappings() {
    let source = "lint:\n  ignore:\n    - library:\n        - b\n        - a\n";
    assert!(findings(source).is_empty());
    assert_eq!(
        messages_with(source, "*"),
        ["entries of 'library' are not sorted: 'a' should come before 'b'"]
    );
}

fn messages_with(source: &str, k: &str) -> Vec<String> {
    findings_with(source, &keys(k))
        .into_iter()
        .map(|f| f.2)
        .collect()
}

// Fixing

#[test]
fn sorts_block_lists() {
    assert_fixed(
        "build-packages:\n  - zlib\n  - curl\n  - bash\n",
        "build-packages:\n  - bash\n  - curl\n  - zlib\n",
    );
}

#[test]
fn sorts_flow_lists_keeping_separators() {
    assert_fixed("plugs: [network, home]\n", "plugs: [home, network]\n");
    assert_fixed("plugs: [ c,b , a ]\n", "plugs: [ a,b , c ]\n");
    assert_fixed(
        "plugs: [\n  network,\n  home,\n]\n",
        "plugs: [\n  home,\n  network,\n]\n",
    );
}

#[test]
fn keeps_quoting_of_entries() {
    assert_fixed(
        "build-packages:\n  - 'zlib'\n  - \"curl\"\n  - bash\n",
        "build-packages:\n  - bash\n  - \"curl\"\n  - 'zlib'\n",
    );
}

#[test]
fn moves_comments_with_their_entries() {
    assert_fixed(
        "\
build-packages:
  # Header for the whole list
  - zlib # compression
  # needed for downloads
  - curl
  - bash
",
        "\
build-packages:
  # Header for the whole list
  - bash
  # needed for downloads
  - curl
  - zlib # compression
",
    );
}

#[test]
fn keeps_blank_lines_in_place() {
    assert_fixed(
        "plugs:\n  - c\n\n  - b\n  - a\nother: x\n",
        "plugs:\n  - a\n\n  - b\n  - c\nother: x\n",
    );
}

#[test]
fn moves_grammar_to_the_end_and_sorts_nested_lists() {
    assert_fixed(
        "\
build-packages:
  - zlib
  - on arm64:
      - libz-arm
      - libb-arm
  - else:
      - libx
  - curl
",
        "\
build-packages:
  - curl
  - zlib
  - on arm64:
      - libb-arm
      - libz-arm
  - else:
      - libx
",
    );
}

#[test]
fn keeps_else_fail_with_its_statement() {
    assert_fixed(
        "stage-packages:\n  - to armhf: [c, a]\n  - else fail\n  - python3\n  - libc6\n",
        "stage-packages:\n  - libc6\n  - python3\n  - to armhf: [a, c]\n  - else fail\n",
    );
}

#[test]
fn moves_multi_line_entries_whole() {
    assert_fixed(
        "plugs:\n  - >-\n    zz folded\n    entry\n  - a\n",
        "plugs:\n  - a\n  - >-\n    zz folded\n    entry\n",
    );
}

#[test]
fn handles_lists_at_the_end_of_a_file_without_newline() {
    assert_fixed("plugs:\n  - b\n  - a", "plugs:\n  - a\n  - b");
}

#[test]
fn preserves_crlf_line_terminators() {
    assert_fixed(
        "plugs:\r\n  - b\r\n  - a\r\nname: x\r\n",
        "plugs:\r\n  - a\r\n  - b\r\nname: x\r\n",
    );
}

#[test]
fn fixes_only_selected_keys() {
    let source = "command-chain: [b, a]\nplugs: [b, a]\n";
    assert_eq!(
        fixed(source).unwrap(),
        "command-chain: [b, a]\nplugs: [a, b]\n"
    );
    assert_eq!(
        fixed_with(source, &keys("*")).unwrap(),
        "command-chain: [a, b]\nplugs: [a, b]\n"
    );
}

#[test]
fn fixes_a_realistic_project() {
    let source = "\
name: demo
apps:
  demo:
    command: bin/demo
    command-chain: [bin/zzz-first, bin/aaa-second]
    plugs: [network, home, audio-playback]
parts:
  demo:
    plugin: nil
    after: [zeta, alpha]
    build-packages:
      # Header comment for the whole list
      - zlib1g-dev
      - on arm64:
          - libz-arm
          - libb-arm
      - else:
          - libx
      # curl is needed for downloads
      - curl
      - Bash
      - libssl-dev # TLS
";
    assert_fixed(
        source,
        "\
name: demo
apps:
  demo:
    command: bin/demo
    command-chain: [bin/zzz-first, bin/aaa-second]
    plugs: [audio-playback, home, network]
parts:
  demo:
    plugin: nil
    after: [alpha, zeta]
    build-packages:
      # Header comment for the whole list
      - Bash
      # curl is needed for downloads
      - curl
      - libssl-dev # TLS
      - zlib1g-dev
      - on arm64:
          - libb-arm
          - libz-arm
      - else:
          - libx
",
    );
}

// Problems that must not be fixed

#[test]
fn does_not_fix_flow_lists_with_comments_between_entries() {
    let source = "plugs: [\n  network, # needed\n  home,\n]\n";
    assert_eq!(findings(source).len(), 1);
    assert_eq!(fixed(source), None);
}

#[test]
fn leaves_sorted_and_unselected_lists_alone() {
    for source in [
        "plugs: [a, b]\n",
        "command-chain: [b, a]\n",
        "plugs:\n  - b\n  - {name: a}\n",
        "",
    ] {
        assert_eq!(fixed(source), None, "{source:?}");
    }
}

#[test]
fn works_together_with_line_length_fixes() {
    let registry = checks::registry();
    let mut settings = Settings::defaults();
    settings.checks = "-*,style-*".parse().unwrap();
    let source =
        "plugs: [wayland, removable-media, network-bind, network, home, desktop, audio-playback]\n";
    assert!(source.trim_end().len() > 80);
    let fixed = checks::fix(
        &checks::enabled(&registry, &settings),
        &project(source),
        &settings,
    )
    .unwrap();
    assert_eq!(
        fixed.source(),
        "plugs:\n  - audio-playback\n  - desktop\n  - home\n  - network\n  - network-bind\n  \
         - removable-media\n  - wayland\n"
    );
}

#[test]
fn suppressed_lists_are_neither_reported_nor_sorted() {
    let source = "plugs: [b, a] # NOLINT(style-sorted-lists)\nslots: [d, c]\n";
    assert_eq!(findings(source).len(), 1);
    assert_eq!(
        fixed(source).as_deref(),
        Some("plugs: [b, a] # NOLINT(style-sorted-lists)\nslots: [c, d]\n")
    );
    // A block list is left alone if any of its lines is suppressed.
    let source = "plugs:\n  - b\n  # NOLINTNEXTLINE\n  - a\n";
    assert_eq!(fixed(source), None);
}
