// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the YAML document tree (`craft_tidy::yaml`).

use craft_tidy::yaml::{CollectionStyle, Document, Node, NodeKind, ScalarStyle, Value};

fn parse(source: &str) -> Document {
    Document::parse(source).unwrap_or_else(|err| panic!("{source:?} should parse: {err}"))
}

fn root(doc: &Document) -> &Node {
    doc.root().expect("document has a root")
}

fn entries(node: &Node) -> &[(Node, Node)] {
    match &node.kind {
        NodeKind::Mapping { entries, .. } => entries,
        other => panic!("expected a mapping, got {other:?}"),
    }
}

fn items(node: &Node) -> &[Node] {
    match &node.kind {
        NodeKind::Sequence { items, .. } => items,
        other => panic!("expected a sequence, got {other:?}"),
    }
}

fn text<'a>(doc: &'a Document, node: &Node) -> &'a str {
    &doc.source()[node.span.clone()]
}

fn plain(s: &str) -> Value {
    Value::Scalar {
        value: s.into(),
        plain: true,
    }
}

fn quoted(s: &str) -> Value {
    Value::Scalar {
        value: s.into(),
        plain: false,
    }
}

// Tree structure

#[test]
fn builds_mappings_sequences_and_scalars() {
    let doc = parse("name: test\nparts:\n  a:\n    after: [b, c]\n    list:\n      - x\n");
    let top = entries(root(&doc));
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].0.as_scalar(), Some("name"));
    assert_eq!(top[0].1.as_scalar(), Some("test"));

    let part = entries(&entries(&top[1].1)[0].1);
    let NodeKind::Sequence { style, items } = &part[0].1.kind else {
        panic!("after should be a sequence");
    };
    assert_eq!(*style, CollectionStyle::Flow);
    assert_eq!(items.len(), 2);
    let NodeKind::Sequence { style, .. } = &part[1].1.kind else {
        panic!("list should be a sequence");
    };
    assert_eq!(*style, CollectionStyle::Block);
}

#[test]
fn records_scalar_styles_and_unquoted_values() {
    let doc = parse("a: x\nb: 'y z'\nc: \"q\\tq\"\nd: |\n  lit\ne: >\n  fold\n");
    let styles: Vec<_> = entries(root(&doc))
        .iter()
        .map(|(_, value)| match &value.kind {
            NodeKind::Scalar { value, style } => (value.as_str(), *style),
            other => panic!("expected a scalar, got {other:?}"),
        })
        .collect();
    assert_eq!(
        styles,
        [
            ("x", ScalarStyle::Plain),
            ("y z", ScalarStyle::SingleQuoted),
            ("q\tq", ScalarStyle::DoubleQuoted),
            ("lit\n", ScalarStyle::Literal),
            ("fold\n", ScalarStyle::Folded),
        ]
    );
}

#[test]
fn spans_locate_nodes_in_the_source() {
    let source = "a: plain\nb: 'quoted'\nc: [x, y]\nd:\n  - one\n  - two\ne: |\n  block\n";
    let doc = parse(source);
    let top = entries(root(&doc));
    assert_eq!(text(&doc, &top[0].0), "a");
    assert_eq!(text(&doc, &top[0].1), "plain");
    assert_eq!(text(&doc, &top[1].1), "'quoted'");
    assert_eq!(text(&doc, &top[2].1), "[x, y]");
    assert_eq!(text(&doc, &items(&top[2].1)[1]), "y");
    assert_eq!(text(&doc, &top[3].1), "- one\n  - two");
    assert_eq!(text(&doc, &top[4].1), "block\n");
    assert_eq!(root(&doc).span.start, 0);
}

#[test]
fn block_scalar_spans_end_at_their_last_line_break() {
    let source = "a:\n  - >-\n    folded\n    text\n  - b\nc: |\n  lit\n\nd: e\n";
    let doc = parse(source);
    let top = entries(root(&doc));
    let list = items(&top[0].1);
    assert_eq!(text(&doc, &list[0]), "folded\n    text\n");
    assert_eq!(text(&doc, &top[0].1), "- >-\n    folded\n    text\n  - b");
    assert_eq!(text(&doc, &top[1].1), "lit\n\n");
}

#[test]
fn anchors_and_tags_mark_nodes_as_decorated() {
    let doc = parse("a: &x 1\nb: *x\nc: !!str 2\nd: 3\ne: &y [1]\n");
    let top = entries(root(&doc));
    assert!(top[0].1.decorated);
    assert_eq!(top[1].1.kind, NodeKind::Alias);
    assert!(top[2].1.decorated);
    assert!(!top[3].1.decorated);
    assert!(top[4].1.decorated);
}

#[test]
fn collects_comments() {
    let source = "# top\na: b # right\n# own line\nc: d\n";
    let doc = parse(source);
    let comments: Vec<_> = doc.comments().iter().map(|r| &source[r.clone()]).collect();
    assert_eq!(comments, ["# top", "# right", "# own line"]);
}

#[test]
fn hash_inside_scalars_is_not_a_comment() {
    let doc = parse("a: 'x # y'\nb: |\n  # not a comment\nc: x#y\n");
    assert!(doc.comments().is_empty());
}

#[test]
fn empty_documents_have_no_root() {
    for source in ["", "# just a comment\n", "\n\n"] {
        let doc = parse(source);
        assert!(doc.root().is_none(), "{source:?}");
        assert_eq!(doc.value(), None);
    }
}

// Value model

#[test]
fn value_ignores_presentation() {
    let block = parse("a:\n  - x\n  - 'y'\nb: >-\n  folded\n  text\n");
    let flow = parse("{a: [x, \"y\"], b: folded text}");
    let Value::Mapping(block_entries) = block.value().unwrap() else {
        panic!("expected a mapping");
    };
    let Value::Mapping(flow_entries) = flow.value().unwrap() else {
        panic!("expected a mapping");
    };
    assert_eq!(block_entries[0], flow_entries[0]);
    assert_eq!(
        block_entries[0].1,
        Value::Sequence(vec![plain("x"), quoted("y")])
    );
    assert_eq!(block_entries[1].1, quoted("folded text"));
    assert_eq!(flow_entries[1].1, plain("folded text"));
}

#[test]
fn value_distinguishes_plain_from_quoted_scalars() {
    assert_ne!(parse("a: 79").value(), parse("a: '79'").value());
    assert_eq!(parse("a: '79'").value(), parse("a: \"79\"").value());
}

#[test]
fn multi_line_scalars_fold_to_the_same_value() {
    let one_line = parse("a: one two three\n").value();
    assert_eq!(parse("a: one two\n  three\n").value(), one_line);
    assert_eq!(
        parse("a: 'one two\n  three'\n").value(),
        parse("a: 'one two three'\n").value()
    );
    assert_eq!(
        parse("a: >-\n  one two\n  three\n").value(),
        parse("a: 'one two three'\n").value()
    );
}

#[test]
fn value_get_mut_follows_paths() {
    let mut value = parse("a: [x, y]\nb: {c: [z]}\n").value().unwrap();
    assert_eq!(value.get_mut(&[]).cloned(), Some(value.clone()));
    assert_eq!(value.get_mut(&[0, 1]).cloned(), Some(plain("y")));
    assert_eq!(value.get_mut(&[1, 0, 0]).cloned(), Some(plain("z")));

    *value.get_mut(&[0, 0]).unwrap() = plain("changed");
    assert_eq!(value.get_mut(&[0, 0]).cloned(), Some(plain("changed")));
}

#[test]
fn value_get_mut_rejects_invalid_paths() {
    let mut value = parse("a: [x]\n").value().unwrap();
    assert!(value.get_mut(&[1]).is_none());
    assert!(value.get_mut(&[0, 5]).is_none());
    assert!(value.get_mut(&[0, 0, 0]).is_none());
}

// Lines and positions

#[test]
fn line_helpers() {
    let doc = parse("a: 1\nbb: 2\r\nccc: 3");
    assert_eq!(doc.line_count(), 3);
    assert_eq!(doc.line(0), "a: 1");
    assert_eq!(doc.line(1), "bb: 2");
    assert_eq!(doc.line(2), "ccc: 3");
    assert_eq!(doc.line_range(1), 5..10);
    assert_eq!(doc.full_line_range(1), 5..12);
    assert_eq!(doc.full_line_range(2), 12..18);
    assert_eq!(doc.line_index(0), 0);
    assert_eq!(doc.line_index(4), 0);
    assert_eq!(doc.line_index(5), 1);
    assert_eq!(doc.line_index(17), 2);
}

#[test]
fn trailing_newline_does_not_start_a_line() {
    assert_eq!(parse("a: 1\n").line_count(), 1);
    assert_eq!(parse("a: 1\n\n").line_count(), 2);
    assert_eq!(parse("a: 1").line_count(), 1);
}

#[test]
fn positions_are_one_based_character_columns() {
    let source = "a: 1\nnäme: wert\n";
    let doc = parse(source);
    assert_eq!(doc.position(0), (1, 1));
    assert_eq!(doc.position(source.find("wert").unwrap()), (2, 7));
}

#[test]
fn detects_line_terminators() {
    assert_eq!(parse("a: 1\nb: 2\n").newline(), "\n");
    assert_eq!(parse("a: 1\r\nb: 2\r\n").newline(), "\r\n");
    assert_eq!(parse("a: 1").newline(), "\n");
}

// Errors

#[test]
fn syntax_errors_are_located() {
    for (source, line, column) in [
        ("a: [b\n", 1, 4),
        ("a: b\n  c: d\n", 2, 4),
        ("a: 'unterminated\n", 2, 1),
        ("a: b\nb: c\nb\n", 3, 1),
    ] {
        let err = Document::parse(source).expect_err(source);
        assert_eq!((err.line, err.column), (line, column), "{source:?}: {err}");
        assert!(!err.message.is_empty());
    }
}

#[test]
fn multiple_documents_are_rejected() {
    let err = Document::parse("a: 1\n---\nb: 2\n").unwrap_err();
    assert!(err.message.contains("multiple YAML documents"), "{err}");
    assert_eq!(err.line, 2);
}

#[test]
fn syntax_error_display_includes_position() {
    let err = Document::parse("a: [b\n").unwrap_err();
    assert_eq!(err.to_string(), format!("1:4: {}", err.message));
}
