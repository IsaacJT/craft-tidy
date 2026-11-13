// SPDX-License-Identifier: GPL-3.0-only

//! Tests for `NOLINT` comments (`craft_tidy::nolint`).

use craft_tidy::nolint::{self, Nolint};
use craft_tidy::yaml::Document;

fn parse(source: &str) -> Nolint {
    Nolint::parse(&Document::parse(source).unwrap())
}

/// The 1-based lines of `source` on which `check` is suppressed.
fn suppressed(source: &str, check: &str) -> Vec<usize> {
    let nolint = parse(source);
    let lines = source.lines().count() + 1;
    (1..=lines)
        .filter(|&line| nolint.suppresses(check, line))
        .collect()
}

/// `(line, column, message)` of the mistakes found in NOLINT comments.
fn problems(source: &str) -> Vec<(usize, usize, String)> {
    parse(source)
        .problems()
        .iter()
        .map(|d| {
            assert_eq!(d.check, nolint::NAME);
            let (line, column) = d.location.unwrap();
            (line, column, d.message.clone())
        })
        .collect()
}

// Directives

#[test]
fn nolint_suppresses_its_own_line() {
    let source = "a: 1 # NOLINT\nb: 2\n# NOLINT\nc: 3\n";
    assert_eq!(suppressed(source, "style-line-length"), [1, 3]);
    assert!(problems(source).is_empty());
}

#[test]
fn nolintnextline_suppresses_the_next_line() {
    let source = "# NOLINTNEXTLINE\na: 1\nb: 2 # NOLINTNEXTLINE\nc: 3\nd: 4\n";
    assert_eq!(suppressed(source, "x"), [2, 4]);
}

#[test]
fn nolintbegin_and_end_suppress_the_lines_between() {
    let source = "a: 1\n# NOLINTBEGIN\nb: 2\nc: 3\n# NOLINTEND\nd: 4\n";
    assert_eq!(suppressed(source, "x"), [2, 3, 4, 5]);
    // Ranges can nest and overlap.
    let source = "# NOLINTBEGIN(a)\nx: 1\n# NOLINTBEGIN(b)\ny: 2\n# NOLINTEND(a)\nz: 3\n\
                  # NOLINTEND(b)\n";
    assert_eq!(suppressed(source, "a"), [1, 2, 3, 4, 5]);
    assert_eq!(suppressed(source, "b"), [3, 4, 5, 6, 7]);
    assert!(problems(source).is_empty());
}

#[test]
fn check_lists_limit_what_is_suppressed() {
    let source = "a: 1 # NOLINT(style-line-length, snapcraft-*)\n";
    assert_eq!(suppressed(source, "style-line-length"), [1]);
    assert_eq!(suppressed(source, "snapcraft-components"), [1]);
    assert!(suppressed(source, "style-sorted-lists").is_empty());
    // Matching BEGIN and END lists may be written in any order.
    let source = "# NOLINTBEGIN(b,a)\nx: 1\n# NOLINTEND( a , b )\n";
    assert_eq!(suppressed(source, "a"), [1, 2, 3]);
    assert!(problems(source).is_empty());
}

#[test]
fn text_may_follow_a_directive() {
    let source = "a: 1 # NOLINT(style-line-length): the URL cannot be shortened\n\
                  # NOLINTNEXTLINE because of upstream\nb: 2\n";
    assert_eq!(suppressed(source, "style-line-length"), [1, 3]);
    assert!(problems(source).is_empty());
}

#[test]
fn other_comments_are_not_directives() {
    for source in [
        "a: 1 # nolint\n",
        "a: 1 # NOLINTED\n",
        "a: 1 # NOLINT_X\n",
        "a: 1 # NOLINTNEXTLINES\n",
        "a: 1 # see NOLINT\n",
        "a: '# NOLINT'\n",
        "a: |\n  # NOLINT\n",
    ] {
        assert!(suppressed(source, "x").is_empty(), "{source:?}");
        assert!(problems(source).is_empty(), "{source:?}");
    }
}

#[test]
fn only_findings_on_a_line_are_suppressed() {
    let nolint = parse("a: 1 # NOLINT\n");
    assert!(!nolint.suppresses("x", 0));
    assert!(!nolint.suppresses("x", 2));
    assert!(nolint.suppresses_any("x", 0..=3));
    assert!(!nolint.suppresses_any("x", 1..=3));
}

#[test]
fn recognises_directives() {
    let document = Document::parse("a: 1 # NOLINT(x)\n# plain\n# NOLINT(\n").unwrap();
    let starts: Vec<_> = document.comments().iter().map(|c| c.start).collect();
    assert!(nolint::is_directive(&document, starts[0]));
    assert!(!nolint::is_directive(&document, starts[1]));
    // Malformed directives are directives too, so that they are not wrapped.
    assert!(nolint::is_directive(&document, starts[2]));
}

// Mistakes

#[test]
fn reports_malformed_check_lists() {
    assert_eq!(
        problems("a: 1 # NOLINT(style-line-length\n"),
        [(1, 8, "NOLINT has an unclosed '('".to_owned())]
    );
    assert_eq!(
        problems("# NOLINTNEXTLINE()\na: 1\n"),
        [(
            1,
            3,
            "NOLINTNEXTLINE() lists no checks; omit the parentheses to suppress all".to_owned()
        )]
    );
    assert_eq!(
        problems("a: 1 # NOLINT(Style_Line)\n"),
        [(
            1,
            8,
            "NOLINT has an invalid check list 'Style_Line': list check names or globs \
             (lowercase letters, digits, '-' and '*'), separated by commas"
                .to_owned()
        )]
    );
    assert_eq!(
        problems("a: 1 # NOLINT(*,-style-line-length)\n"),
        [(
            1,
            8,
            "NOLINT cannot disable suppression with '-style-line-length'".to_owned()
        )]
    );
    // Malformed directives suppress nothing.
    assert!(suppressed("a: 1 # NOLINT(x\n", "x").is_empty());
}

#[test]
fn reports_unmatched_begin_and_end() {
    assert_eq!(
        problems("# NOLINTEND\na: 1\n"),
        [(
            1,
            3,
            "NOLINTEND has no matching NOLINTBEGIN before it".to_owned()
        )]
    );
    let source = "# NOLINTBEGIN(a)\nx: 1\n# NOLINTEND(b)\n";
    assert_eq!(
        problems(source),
        [
            (
                3,
                3,
                "NOLINTEND(b) has no matching NOLINTBEGIN(b) before it".to_owned()
            ),
            (
                1,
                3,
                "NOLINTBEGIN(a) has no matching NOLINTEND(a) after it, so it suppresses nothing"
                    .to_owned()
            ),
        ]
    );
    // An unclosed NOLINTBEGIN suppresses nothing.
    assert!(suppressed(source, "a").is_empty());
    // An END before its BEGIN does not match it.
    assert_eq!(problems("# NOLINTEND\n# NOLINTBEGIN\n").len(), 2);
}
