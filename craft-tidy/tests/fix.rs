// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the fix helpers (`craft_tidy::fix`).

use std::cell::RefCell;

use craft_tidy::fix::{Edit, apply_edits};
use craft_tidy::yaml::Document;

fn doc(source: &str) -> Document {
    Document::parse(source).unwrap()
}

fn edit(range: std::ops::Range<usize>, replacement: &str) -> (Edit, ()) {
    (Edit::new(range, replacement), ())
}

fn accept_all(_: &Document, _: &Document, _: &()) -> bool {
    true
}

#[test]
fn applies_a_single_edit() {
    let source = "a: one\n";
    let fixed = apply_edits(&doc(source), vec![edit(3..6, "two")], accept_all).unwrap();
    assert_eq!(fixed.source(), "a: two\n");
}

#[test]
fn applies_edits_in_any_order_against_original_offsets() {
    let source = "a: 1\nb: 2\nc: 3\n";
    let edits = vec![edit(3..4, "10"), edit(13..14, "30"), edit(8..9, "20")];
    let fixed = apply_edits(&doc(source), edits, accept_all).unwrap();
    assert_eq!(fixed.source(), "a: 10\nb: 20\nc: 30\n");
}

#[test]
fn supports_insertions_and_deletions() {
    let source = "a: x\nb: y\n";
    let edits = vec![edit(4..4, " # note"), edit(5..10, "")];
    let fixed = apply_edits(&doc(source), edits, accept_all).unwrap();
    assert_eq!(fixed.source(), "a: x # note\n");
}

#[test]
fn rejected_edits_are_skipped_but_others_kept() {
    let source = "a: keep\nb: drop\nc: keep\n";
    let edits = vec![
        (Edit::new(3..7, "kept"), true),
        (Edit::new(11..15, "gone"), false),
        (Edit::new(19..23, "kept"), true),
    ];
    let fixed = apply_edits(&doc(source), edits, |_, _, ok| *ok).unwrap();
    assert_eq!(fixed.source(), "a: kept\nb: drop\nc: kept\n");
}

#[test]
fn accept_sees_previously_kept_edits() {
    let source = "a: 1\nb: 2\n";
    let seen = RefCell::new(Vec::new());
    let edits = vec![edit(3..4, "x"), edit(8..9, "y")];
    apply_edits(&doc(source), edits, |before, after, _| {
        seen.borrow_mut()
            .push((before.source().to_owned(), after.source().to_owned()));
        true
    });
    assert_eq!(
        seen.into_inner(),
        [
            ("a: 1\nb: 2\n".to_owned(), "a: 1\nb: y\n".to_owned()),
            ("a: 1\nb: y\n".to_owned(), "a: x\nb: y\n".to_owned()),
        ]
    );
}

#[test]
fn edits_producing_invalid_yaml_are_skipped() {
    let source = "a: 1\nb: 2\n";
    let edits = vec![edit(3..4, "[unclosed"), edit(8..9, "3")];
    let fixed = apply_edits(&doc(source), edits, accept_all).unwrap();
    assert_eq!(fixed.source(), "a: 1\nb: 3\n");
}

#[test]
fn overlapping_edits_keep_the_later_one() {
    let source = "a: abcdef\n";
    let edits = vec![edit(3..7, "XXXX"), edit(5..9, "YYYY")];
    let fixed = apply_edits(&doc(source), edits, accept_all).unwrap();
    assert_eq!(fixed.source(), "a: abYYYY\n");
}

#[test]
fn adjacent_edits_do_not_overlap() {
    let source = "a: abcd\n";
    let edits = vec![edit(3..5, "X"), edit(5..7, "Y")];
    let fixed = apply_edits(&doc(source), edits, accept_all).unwrap();
    assert_eq!(fixed.source(), "a: XY\n");
}

#[test]
fn returns_none_when_nothing_is_kept() {
    let source = "a: 1\n";
    assert!(apply_edits(&doc(source), Vec::<(Edit, ())>::new(), accept_all).is_none());
    assert!(apply_edits(&doc(source), vec![edit(3..4, "2")], |_, _, _| false).is_none());
    assert!(apply_edits(&doc(source), vec![edit(0..1, "[")], accept_all).is_none());
}
