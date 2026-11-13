// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `style-line-length` check.

mod common;

use craft_tidy::checks;
use craft_tidy::config::{CheckOptions, OptionValue, Settings};
use craft_tidy::project::Project;

use common::pyyaml_equal;

const NAME: &str = "style-line-length";

fn settings(max_length: Option<i64>) -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    if let Some(max) = max_length {
        settings
            .check_options
            .set(NAME, "max-length", OptionValue::Integer(max));
    }
    settings
}

fn project(source: &str) -> Project {
    Project::new("snapcraft.yaml", source).unwrap()
}

/// `(line, column, message)` of every finding.
fn findings_with(source: &str, max_length: Option<i64>) -> Vec<(usize, usize, String)> {
    let registry = checks::registry();
    let settings = settings(max_length);
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
    findings_with(source, None)
}

/// The fixed source, or `None` if nothing was fixed. Also verifies that the
/// fix is idempotent and, where PyYAML is available, that Snapcraft's YAML
/// parser sees the same data before and after.
fn fixed_with(source: &str, max_length: Option<i64>) -> Option<String> {
    let registry = checks::registry();
    let settings = settings(max_length);
    let enabled = checks::enabled(&registry, &settings);
    let fixed = checks::fix(&enabled, &project(source), &settings)?;
    let fixed = fixed.source().to_owned();
    assert_eq!(
        checks::fix(&enabled, &project(&fixed), &settings).map(|p| p.source().to_owned()),
        None,
        "fixing again must not change anything:\n{fixed}"
    );
    if let Some(same) = pyyaml_equal(source, &fixed) {
        assert!(same, "PyYAML sees different data after fixing:\n{fixed}");
    }
    Some(fixed)
}

fn fixed(source: &str) -> Option<String> {
    fixed_with(source, None)
}

fn long_words(count: usize) -> String {
    (0..count)
        .map(|i| format!("word{i:02}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// Registration and options

#[test]
fn is_registered_with_documented_options() {
    let registry = checks::registry();
    let check = registry.iter().find(|c| c.name() == NAME).expect(NAME);
    let options: Vec<_> = check
        .options()
        .iter()
        .map(|o| (o.name, o.default))
        .collect();
    assert_eq!(options, [("max-length", "80")]);
}

#[test]
fn accepts_positive_max_lengths() {
    let registry = checks::registry();
    for max in [1, 80, 120] {
        let mut options = CheckOptions::default();
        options.set(NAME, "max-length", OptionValue::Integer(max));
        assert!(
            checks::validate_options(&registry, &options).is_ok(),
            "{max}"
        );
    }
}

#[test]
fn rejects_invalid_max_lengths() {
    let registry = checks::registry();
    for value in [
        OptionValue::Integer(0),
        OptionValue::Integer(-5),
        OptionValue::String("80".into()),
        OptionValue::Float(80.5),
    ] {
        let mut options = CheckOptions::default();
        options.set(NAME, "max-length", value.clone());
        assert!(
            checks::validate_options(&registry, &options).is_err(),
            "{value:?} should be rejected"
        );
    }
}

// Detection

#[test]
fn short_lines_are_fine() {
    assert!(findings("name: test\nsummary: short\n").is_empty());
    assert!(findings("").is_empty());
}

#[test]
fn reports_lines_longer_than_the_default_limit() {
    let at_limit = format!("a: {}\n", "x".repeat(77));
    let over_limit = format!("b: {}\n", "x".repeat(78));
    assert_eq!(at_limit.trim_end().len(), 80);
    assert!(findings(&at_limit).is_empty());
    assert_eq!(
        findings(&format!("{at_limit}{over_limit}")),
        [(
            2,
            81,
            "line is 81 characters long, exceeding the limit of 80".to_owned()
        )]
    );
}

#[test]
fn honours_max_length_option() {
    let source = "summary: twenty-five chars\n";
    assert!(findings_with(source, Some(26)).is_empty());
    assert_eq!(
        findings_with(source, Some(20)),
        [(
            1,
            21,
            "line is 26 characters long, exceeding the limit of 20".to_owned()
        )]
    );
}

#[test]
fn counts_characters_not_bytes() {
    let source = format!("a: {}\n", "é".repeat(77));
    assert!(source.len() > 80);
    assert!(findings(&source).is_empty());
}

#[test]
fn ignores_line_terminators() {
    let line = format!("a: {}", "x".repeat(77));
    assert!(findings(&format!("{line}\r\n{line}\r\n")).is_empty());
}

#[test]
fn reports_every_long_line_including_comments_and_block_scalars() {
    let long = "x".repeat(90);
    let source = format!("# {long}\na: |\n  {long}\nb: {long}\n");
    let lines: Vec<_> = findings(&source).iter().map(|f| f.0).collect();
    assert_eq!(lines, [1, 3, 4]);
}

// Fixing plain and quoted scalars

#[test]
fn wraps_plain_mapping_values() {
    let source = "summary: A very long summary line that goes on and on and certainly exceeds the line limit\n";
    assert_eq!(
        fixed(source).unwrap(),
        "summary: A very long summary line that goes on and on and certainly exceeds the\n  \
         line limit\n"
    );
}

#[test]
fn wraps_quoted_scalars() {
    let single = format!("source: '{}'\n", long_words(12));
    assert_eq!(
        fixed(&single).unwrap(),
        "source: 'word00 word01 word02 word03 word04 word05 word06 word07 word08 word09\n  \
         word10 word11'\n"
    );
    let double = format!("source: \"{}\"\n", long_words(12));
    assert_eq!(
        fixed(&double).unwrap(),
        "source: \"word00 word01 word02 word03 word04 word05 word06 word07 word08 word09\n  \
         word10 word11\"\n"
    );
}

#[test]
fn wraps_long_values_over_several_lines() {
    let source = format!("description: {}\n", long_words(30));
    let fixed = fixed(&source).unwrap();
    assert_eq!(fixed.lines().count(), 3);
    assert!(
        fixed.lines().all(|line| line.chars().count() <= 80),
        "{fixed}"
    );
    assert!(findings(&fixed).is_empty());
}

#[test]
fn indents_continuations_relative_to_the_parent() {
    let words = long_words(12);
    let source = format!(
        "parts:\n  foo:\n    source: {words}\n    list:\n      - {words}\n      - key: {words}\n"
    );
    let fixed = fixed(&source).unwrap();
    assert_eq!(
        fixed,
        "parts:\n  foo:\n    \
         source: word00 word01 word02 word03 word04 word05 word06 word07 word08\n      \
         word09 word10 word11\n    \
         list:\n      \
         - word00 word01 word02 word03 word04 word05 word06 word07 word08 word09\n        \
         word10 word11\n      \
         - key: word00 word01 word02 word03 word04 word05 word06 word07 word08\n          \
         word09 word10 word11\n"
    );
}

#[test]
fn breaks_before_command_line_options() {
    let source = "    command: bin/demo --with-a-very-long-option=value --and-another-option=another-value\n";
    let source = format!("apps:\n  demo:\n{}", &source[0..]);
    let fixed = fixed(&source).unwrap();
    assert_eq!(
        fixed,
        "apps:\n  demo:\n    command: bin/demo --with-a-very-long-option=value\n      \
         --and-another-option=another-value\n"
    );
}

#[test]
fn keeps_trailing_comments_at_the_end() {
    let source = format!("key: {} # note\n", long_words(11));
    let fixed = fixed(&source).unwrap();
    assert!(fixed.ends_with("word10 # note\n"), "{fixed}");
    assert_eq!(fixed.lines().count(), 2);
}

#[test]
fn preserves_crlf_line_terminators() {
    let source = format!("summary: {}\r\nname: x\r\n", long_words(12));
    let fixed = fixed(&source).unwrap();
    assert_eq!(fixed.matches("\r\n").count(), 3, "{fixed:?}");
    assert_eq!(fixed.matches('\n').count(), 3, "{fixed:?}");
}

#[test]
fn honours_max_length_when_fixing() {
    let source = format!("summary: {}\n", long_words(8));
    let fixed = fixed_with(&source, Some(40)).unwrap();
    assert!(fixed.lines().all(|l| l.chars().count() <= 40), "{fixed}");
    assert!(findings_with(&fixed, Some(40)).is_empty());
}

// Fixing other constructs

#[test]
fn breaks_lines_of_folded_block_scalars() {
    let source = format!("description: >-\n  {}\n  Short line.\n", long_words(12));
    assert_eq!(
        fixed(&source).unwrap(),
        "description: >-\n  \
         word00 word01 word02 word03 word04 word05 word06 word07 word08 word09 word10\n  \
         word11\n  \
         Short line.\n"
    );
}

#[test]
fn converts_long_flow_sequences_to_block_sequences() {
    let source = "apps:\n  demo:\n    plugs: [home, network, network-bind, removable-media, audio-playback, desktop]\n";
    assert_eq!(
        fixed(source).unwrap(),
        "apps:\n  demo:\n    plugs:\n      - home\n      - network\n      - network-bind\n      \
         - removable-media\n      - audio-playback\n      - desktop\n"
    );
}

#[test]
fn moves_flow_sequence_comments_to_the_key() {
    let source =
        "plugs: [home, network, network-bind, removable-media, audio-playback] # needed by x\n";
    assert!(source.trim_end().len() > 80);
    assert_eq!(
        fixed(source).unwrap(),
        "plugs: # needed by x\n  - home\n  - network\n  - network-bind\n  - removable-media\n  \
         - audio-playback\n"
    );
}

#[test]
fn keeps_quotes_of_flow_sequence_items() {
    let source =
        "build-packages: ['libfoo-dev', \"libbar-dev\", libbaz-dev, libqux-dev, libquux-dev]\n";
    assert!(source.trim_end().len() > 80);
    assert_eq!(
        fixed(source).unwrap(),
        "build-packages:\n  - 'libfoo-dev'\n  - \"libbar-dev\"\n  - libbaz-dev\n  - libqux-dev\n  \
         - libquux-dev\n"
    );
}

#[test]
fn rewraps_prose_comments() {
    let source = format!("parts:\n  # {}\n  foo: {{}}\n", long_words(12));
    assert_eq!(
        fixed(&source).unwrap(),
        "parts:\n  \
         # word00 word01 word02 word03 word04 word05 word06 word07 word08 word09 word10\n  \
         # word11\n  \
         foo: {}\n"
    );
}

#[test]
fn fixes_a_realistic_project_while_preserving_its_data() {
    let source = "\
name: demo
# This is a rather long comment that explains something about the project file in detail.
summary: A very long summary line that goes on and on and certainly exceeds the line limit
description: >-
  This folded description has a single line that is definitely far longer than eighty characters wide.
  Short line.
apps:
  demo:
    command: bin/demo --with-a-very-long-option=value --and-another-option=another-value
    plugs: [home, network, network-bind, removable-media, audio-playback, desktop, wayland]
parts:
  demo:
    plugin: nil
    build-packages:
      - \"a quoted list item that is long enough to need wrapping when it is checked here\"
    source: 'single quoted string value that is long enough to need wrapping at some point'
    key-with-comment: some value that is long and also has a trailing comment # comment here
";
    let fixed = fixed(source).unwrap();
    assert!(findings(&fixed).is_empty(), "{fixed}");
    assert_eq!(
        project(source).document.value(),
        project(&fixed).document.value()
    );
}

// Problems that must not be fixed

#[test]
fn leaves_unfixable_lines_alone() {
    let long = long_words(12);
    let url = format!("https://example.com/{}", "a/".repeat(40));
    let cases = [
        // Literal block scalars: newlines are content.
        format!("script: |\n  {long}\n"),
        // More-indented lines of folded scalars are not folded by YAML.
        format!("text: >\n  short\n    {long}\n"),
        // No space to break at.
        format!("source: {url}\n"),
        // Keys are never wrapped.
        format!("{}: value\n", "k".repeat(85)),
        // Anchored and tagged scalars.
        format!("a: &anchor {long}\n"),
        format!("a: !!str {long}\n"),
        // Inside flow mappings and nested flow collections.
        format!("a: {{key: {long}}}\n"),
        format!("a: [[{}], b]\n", long.replace(' ', ", ")),
        // Flow sequences that are sequence items.
        format!("a:\n  - [{}]\n", long.replace(' ', ", ")),
        // Already multi-line scalars.
        format!("a: short\n  {long}\n"),
        // Comments after content, and preformatted comments.
        format!("a: b # {long}\n"),
        format!("#   {long}\n"),
        format!("#{long}\n"),
    ];
    for source in cases {
        assert!(
            !findings(&source).is_empty(),
            "{source:?} should be reported"
        );
        assert_eq!(fixed(&source), None, "{source:?} must not be changed");
    }
}

#[test]
fn never_breaks_where_a_plain_continuation_could_look_like_syntax() {
    let filler = "x".repeat(60);
    for word in [
        "- item",
        "&anchor",
        "*alias",
        "!tag",
        "'quoted'",
        "\"quoted\"",
        "[a]",
        "{a}",
        "|",
        ">",
        "%x",
        "@x",
        "`x`",
        "?x",
        ":x",
    ] {
        let source = format!("a: {filler} {word}{}\n", "y".repeat(20));
        if let Some(fixed) = fixed(&source) {
            assert!(
                fixed
                    .lines()
                    .skip(1)
                    .all(|line| !line.trim_start().starts_with(word)),
                "must not start a continuation line with {word:?}:\n{fixed}"
            );
        }
    }
}

#[test]
fn never_breaks_escaped_spaces_in_double_quotes() {
    let source = format!("a: \"{}\\ {}\"\n", "x".repeat(60), "y".repeat(30));
    assert_eq!(fixed(&source), None);
}

#[test]
fn does_not_wrap_when_continuations_would_be_too_narrow() {
    let source = format!("a: {}\n", long_words(6));
    assert_eq!(fixed_with(&source, Some(15)), None);
}

#[test]
fn leaves_files_without_long_lines_alone() {
    assert_eq!(fixed("name: test\nsummary: short\n"), None);
}

// Reasons for lines that cannot be fixed

/// `(line, reason)` of every finding; the reason is `None` for lines that
/// the fix wraps.
fn reasons_with(source: &str, max_length: Option<i64>) -> Vec<(usize, Option<String>)> {
    let registry = checks::registry();
    let settings = settings(max_length);
    checks::run(
        &checks::enabled(&registry, &settings),
        &project(source),
        &settings,
    )
    .into_iter()
    .map(|d| (d.location.unwrap().0, d.unfixable))
    .collect()
}

/// The reason given for the only long line of `source`.
fn reason(source: &str) -> String {
    let reasons = reasons_with(source, Some(40));
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    reasons[0]
        .1
        .clone()
        .expect("the line should not be fixable")
}

const NO_BREAK: &str =
    "it has no space at which it can be broken safely, such as in a long URL or word";

#[test]
fn lines_that_can_be_wrapped_have_no_reason() {
    for source in [
        "summary: a summary with plenty of spaces to wrap it at\n",
        "summary: 'a quoted summary with plenty of spaces to wrap'\n",
        "a:\n  - an item with plenty of spaces to wrap it somewhere\n",
        "description: >\n  folded text with plenty of spaces to wrap it at\n",
        "plugs: [network, home, audio-playback, camera, opengl]\n",
        "# a prose comment with plenty of spaces to wrap it at\na: b\n",
    ] {
        let reasons: Vec<_> = reasons_with(source, Some(40))
            .into_iter()
            .map(|(_, reason)| reason)
            .collect();
        assert_eq!(reasons, [None], "{source:?}");
        if source.starts_with("description") {
            continue;
        }
        assert!(fixed_with(source, Some(40)).is_some(), "{source:?}");
    }
}

#[test]
fn long_words_cannot_be_broken() {
    assert_eq!(
        reason("source: https://example.com/a/very/long/url.tar.gz\n"),
        NO_BREAK
    );
    assert_eq!(
        reason("a:\n  - https://example.com/a/very/long/url.tar.gz\n"),
        NO_BREAK
    );
    assert_eq!(
        reason("description: >\n  https://example.com/a/very/long/url.tar.gz\n"),
        NO_BREAK
    );
    assert_eq!(
        reason("# https://example.com/a/very/long/url.tar.gz\na: b\n"),
        NO_BREAK
    );
    // A space followed by something that looks like YAML syntax is no
    // safe place to break a plain string either.
    assert_eq!(
        reason("summary: aaaaaaaaaaaaaaaaaaaaaaaaaaa :bbbbb\n"),
        NO_BREAK
    );
}

#[test]
fn a_long_word_left_by_the_fix_is_reported() {
    let source = "summary: some words then https://example.com/a/very/long/url/x.tar.gz\n";
    let fixed = fixed_with(source, Some(40)).unwrap();
    assert_eq!(
        fixed,
        "summary: some words then\n  https://example.com/a/very/long/url/x.tar.gz\n"
    );
    assert_eq!(
        reasons_with(&fixed, Some(40)),
        [(2, Some(NO_BREAK.to_owned()))]
    );
}

#[test]
fn reports_why_other_lines_cannot_be_wrapped() {
    for (source, expected) in [
        (
            "override-build: |\n  echo a literal block line that is too long\n",
            "lines of literal (|) block scalars cannot be wrapped without changing them",
        ),
        (
            "description: >\n  folded\n    a more-indented folded line that is too long\n",
            "YAML does not fold more-indented lines of folded (>) block scalars, so they \
             cannot be wrapped",
        ),
        (
            "a-very-long-mapping-key-that-is-much-too-long: 1\n",
            "mapping keys cannot be wrapped",
        ),
        (
            "a: {key: a flow mapping value that is too long}\n",
            "values inside flow mappings ({...}) cannot be wrapped",
        ),
        (
            "a: &anchor an anchored value that is much too long\n",
            "values with an anchor or tag are not wrapped",
        ),
        (
            "a: !tag a tagged value that is much too long here\n",
            "values with an anchor or tag are not wrapped",
        ),
        (
            "a:\n  - [network, home, audio-playback, camera, x11]\n",
            "flow sequences are only rewritten as the value of a mapping entry",
        ),
        (
            "a: [network, home, {audio: playback}, camera, x11]\n",
            "flow sequences are only rewritten if all their items are strings",
        ),
        (
            "a: b # a trailing comment that is much too long here\n",
            "comments after other content are not wrapped",
        ),
        (
            "#a comment without a space that is much too long\nb: c\n",
            "only comments that look like prose (# followed by text) are wrapped",
        ),
        (
            "a story that is told as a whole document, too long\n",
            "a document that is a single string cannot be wrapped",
        ),
        (
            "a:\n  - &x long value\n  - *x                                        \n",
            "aliases cannot be wrapped",
        ),
    ] {
        let reasons = reasons_with(source, Some(40));
        let long: Vec<_> = reasons.iter().filter_map(|(_, r)| r.as_deref()).collect();
        assert_eq!(long, [expected], "{source:?}: {reasons:?}");
    }
}

#[test]
fn deeply_indented_lines_are_not_wrapped() {
    // Continuation lines would have less than 20 characters of room.
    let mut source = String::new();
    for depth in 0..11 {
        source.push_str(&format!("{}k{depth}:\n", "  ".repeat(depth)));
    }
    source.push_str(&format!("{}value: some words to wrap\n", "  ".repeat(11)));
    assert_eq!(
        reasons_with(&source, Some(40)),
        [(
            12,
            Some("it is indented too deeply to wrap within 40 characters".to_owned())
        )]
    );
    assert_eq!(fixed_with(&source, Some(40)), None);
}

#[test]
fn strings_spanning_several_lines_are_not_rewrapped() {
    let source = "summary: a plain string\n  that continues with a line that is too long\n";
    assert_eq!(
        reasons_with(source, Some(40)),
        [(
            2,
            Some("strings that span several lines are not re-wrapped".to_owned())
        )]
    );
    assert_eq!(fixed_with(source, Some(40)), None);
}

#[test]
fn other_long_lines_get_a_generic_reason() {
    // Trailing whitespace after a key whose value starts on the next line.
    let source = format!("a:{}\n  b: c\n", " ".repeat(50));
    assert_eq!(
        reasons_with(&source, Some(40)),
        [(1, Some("it cannot be wrapped automatically".to_owned()))]
    );
}

// NOLINT comments

#[test]
fn nolint_comments_are_never_wrapped_or_moved() {
    // A long NOLINT comment on its own line stays as it is.
    let comment = format!("# NOLINTNEXTLINE(style-sorted-lists) {}", long_words(8));
    let source = format!("{comment}\na: [b, a]\n");
    assert_eq!(fixed(&source), None);
    assert_eq!(
        reasons_with(&source, None),
        [(
            1,
            Some(
                "NOLINT comments must stay on their line, so they are not wrapped or moved"
                    .to_owned()
            )
        )]
    );
    // Wrapping a value would move its trailing NOLINT comment to another
    // line, where it would suppress the wrong findings.
    let source = format!("summary: {} # NOLINT(style-sorted-lists)\n", long_words(8));
    assert_eq!(fixed(&source), None);
}

#[test]
fn suppressed_lines_are_neither_reported_nor_wrapped() {
    let source = format!("summary: {} # NOLINT(style-line-length)\n", long_words(12));
    assert!(findings(&source).is_empty());
    assert_eq!(fixed(&source), None);
    let source = format!(
        "# NOLINTNEXTLINE(style-*)\nsummary: {0}\ndescription: {0}\n",
        long_words(12)
    );
    let lines: Vec<_> = findings(&source).into_iter().map(|f| f.0).collect();
    assert_eq!(lines, [3]);
    let fixed = fixed(&source).unwrap();
    assert!(fixed.starts_with(&format!(
        "# NOLINTNEXTLINE(style-*)\nsummary: {}\ndescription:",
        long_words(12)
    )));
}
