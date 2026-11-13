// SPDX-License-Identifier: GPL-3.0-only

//! `style-collapsed-sequences`: short sequences are written on one line as
//! flow sequences, longer ones as block sequences.

use std::ops::Range;

use crate::checks::{Check, Context, Diagnostic, OptionKind, OptionSpec};
use crate::config::OptionValue;
use crate::fix::{Edit, apply_edits};
use crate::yaml::{CollectionStyle, Document, Node, NodeKind, ScalarStyle};

const NAME: &str = "style-collapsed-sequences";
const DEFAULT_MAX_ITEMS: usize = 1;

/// `style-line-length`'s option and default, which decide whether a
/// collapsed sequence fits on its line.
const LINE_LENGTH: (&str, &str) = ("style-line-length", "max-length");
const DEFAULT_MAX_LENGTH: usize = 80;

/// Indentation of the `-` of a block sequence relative to its key, if the
/// document has no block sequence to copy it from.
const DEFAULT_INDENT: usize = 2;

const OPTIONS: &[OptionSpec] = &[OptionSpec {
    name: "max-collapsed-items",
    kind: OptionKind::Integer,
    default: "1",
    description: "Sequences with at most this many items are written on one line \
                  ([a, b]); longer ones as block sequences, one item per line.",
}];

pub struct CollapsedSequences;

impl Check for CollapsedSequences {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Short sequences must be on one line, longer ones one item per line"
    }

    fn explanation(&self) -> &'static str {
        "Reports sequences of simple strings with at most max-collapsed-items \
         items that are written as block sequences (one - item per line), \
         and longer ones written as flow sequences ([a, b, c]).\n\
         \n\
         A short sequence is left as a block sequence if its one-line form \
         would be longer than style-line-length's max-length, so that the \
         two checks never disagree.\n\
         \n\
         Only sequences that are the value of a key and contain nothing but \
         single-line strings are checked; sequences with comments, anchors, \
         tags, nested collections (such as advanced grammar) or empty \
         sequences are left alone.\n\
         \n\
         With --fix, sequences are collapsed or expanded. Expanded sequences \
         use the indentation of the file's other block sequences."
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn validate_option(&self, _name: &str, value: &OptionValue) -> Result<(), String> {
        match value.as_i64() {
            Some(max) if max >= 0 => Ok(()),
            _ => Err("must be a non-negative integer".to_owned()),
        }
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let document = &ctx.project.document;
        let max = max_items(ctx);
        changes(ctx)
            .into_iter()
            .map(|change| {
                let (line, column) = document.position(change.sequence_start);
                let key = &change.key;
                let message = match (change.collapse, change.items) {
                    (true, 1) => format!(
                        "'{key}' has a single item, so it should be on one line: \
                         {key}: [{}]",
                        change.joined
                    ),
                    (true, items) => format!(
                        "'{key}' has {items} items (max-collapsed-items is {max}), so it \
                         should be on one line: {key}: [{}]",
                        change.joined
                    ),
                    (false, items) => format!(
                        "'{key}' has {items} {}, more than max-collapsed-items ({max}), so it \
                         should have one item per line",
                        if items == 1 { "item" } else { "items" }
                    ),
                };
                Diagnostic::warning(NAME, message).at(line, column)
            })
            .collect()
    }

    fn fix(&self, ctx: &Context<'_>) -> Option<Document> {
        let edits = changes(ctx)
            .into_iter()
            .map(|change| (change.edit, ()))
            .collect();
        apply_edits(&ctx.project.document, edits, |before, after, _| {
            before.value() == after.value()
        })
    }
}

fn option(ctx: &Context<'_>, check: &str, key: &str, default: usize) -> usize {
    ctx.options
        .get(check, key)
        .and_then(OptionValue::as_i64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

fn max_items(ctx: &Context<'_>) -> usize {
    option(ctx, NAME, "max-collapsed-items", DEFAULT_MAX_ITEMS)
}

/// A sequence to collapse or expand.
struct Change {
    /// The key whose value the sequence is, as written.
    key: String,
    sequence_start: usize,
    items: usize,
    collapse: bool,
    /// The items joined as in a flow sequence, for the message.
    joined: String,
    edit: Edit,
}

/// The changes to make, each verified to leave the data unchanged so that
/// only what `--fix` can fix is reported.
fn changes(ctx: &Context<'_>) -> Vec<Change> {
    let document = &ctx.project.document;
    let Some(root) = document.root() else {
        return Vec::new();
    };
    let planner = Planner {
        document,
        max_items: max_items(ctx),
        max_length: option(ctx, LINE_LENGTH.0, LINE_LENGTH.1, DEFAULT_MAX_LENGTH),
        indent: block_indent(document, root).unwrap_or(DEFAULT_INDENT),
    };
    let mut changes = Vec::new();
    planner.visit(root, &mut changes);
    changes.retain(|change| {
        apply_edits(
            document,
            vec![(change.edit.clone(), ())],
            |before, after, _| before.value() == after.value(),
        )
        .is_some()
    });
    changes
}

/// Where a block sequence starts: the `-` before its first item. (The
/// parser's span of a sequence whose `-` is at the column of its key starts
/// at the first item instead.)
fn block_start(document: &Document, sequence: &Node) -> usize {
    let NodeKind::Sequence { items, .. } = &sequence.kind else {
        return sequence.span.start;
    };
    let Some(first) = items.first() else {
        return sequence.span.start;
    };
    let before = document.source()[..first.span.start].trim_end_matches(' ');
    match before.strip_suffix('-') {
        Some(rest) => rest.len().min(sequence.span.start),
        None => sequence.span.start,
    }
}

/// How far the `-` of the first block sequence that is the value of a key
/// is indented relative to the key.
fn block_indent(document: &Document, node: &Node) -> Option<usize> {
    match &node.kind {
        NodeKind::Mapping { entries, .. } => entries.iter().find_map(|(key, value)| {
            let is_block_sequence = matches!(
                value.kind,
                NodeKind::Sequence {
                    style: CollectionStyle::Block,
                    ref items
                } if !items.is_empty()
            );
            let column = |offset| document.position(offset).1;
            let start = block_start(document, value);
            if is_block_sequence
                && document.line_index(key.span.start) != document.line_index(start)
            {
                column(start).checked_sub(column(key.span.start))
            } else {
                block_indent(document, value)
            }
        }),
        NodeKind::Sequence { items, .. } => {
            items.iter().find_map(|item| block_indent(document, item))
        }
        _ => None,
    }
}

struct Planner<'a> {
    document: &'a Document,
    max_items: usize,
    max_length: usize,
    indent: usize,
}

impl Planner<'_> {
    fn source(&self) -> &str {
        self.document.source()
    }

    fn visit(&self, node: &Node, changes: &mut Vec<Change>) {
        match &node.kind {
            NodeKind::Mapping { style, entries } => {
                for (key, value) in entries {
                    if *style == CollectionStyle::Block
                        && let Some(change) = self.plan(key, value)
                    {
                        changes.push(change);
                    }
                    self.visit(value, changes);
                }
            }
            NodeKind::Sequence { items, .. } => {
                for item in items {
                    self.visit(item, changes);
                }
            }
            NodeKind::Scalar { .. } | NodeKind::Alias => {}
        }
    }

    /// The change needed for the sequence `value` of the entry `key`, if it
    /// is a simple sequence in the wrong style.
    fn plan(&self, key: &Node, value: &Node) -> Option<Change> {
        let NodeKind::Sequence { style, items } = &value.kind else {
            return None;
        };
        if value.decorated || items.is_empty() || !items.iter().all(|item| self.simple(item)) {
            return None;
        }
        // Replace from just after the key's `:` to the end of the sequence.
        let start = match style {
            CollectionStyle::Flow => value.span.start,
            CollectionStyle::Block => block_start(self.document, value),
        };
        let before = &self.source()[..start];
        let colon = before
            .trim_end_matches([' ', '\n', '\r'])
            .strip_suffix(':')?
            .len();
        let end = match style {
            CollectionStyle::Flow => value.span.end,
            CollectionStyle::Block => items.last()?.span.end,
        };
        if self.has_comments(key.span.start, end) {
            return None;
        }
        let texts: Vec<&str> = items
            .iter()
            .map(|item| &self.source()[item.span.clone()])
            .collect();
        let joined = texts.join(", ");
        let collapse = items.len() <= self.max_items;
        let replacement = match (style, collapse) {
            (CollectionStyle::Block, true) => {
                let line_start = self
                    .document
                    .line_range(self.document.line_index(key.span.start))
                    .start;
                let collapsed = format!(" [{joined}]");
                let length =
                    self.source()[line_start..=colon].chars().count() + collapsed.chars().count();
                if length > self.max_length {
                    return None;
                }
                collapsed
            }
            (CollectionStyle::Flow, false) => {
                let key_column = self.document.position(key.span.start).1 - 1;
                let indent = format!(
                    "{}{}",
                    self.document.newline(),
                    " ".repeat(key_column + self.indent)
                );
                texts
                    .iter()
                    .map(|text| format!("{indent}- {text}"))
                    .collect()
            }
            _ => return None,
        };
        Some(Change {
            key: self.source()[key.span.clone()].to_owned(),
            sequence_start: start,
            items: items.len(),
            collapse,
            joined,
            edit: Edit::new(colon + 1..end, replacement),
        })
    }

    /// Whether `item` is a single-line plain or quoted string.
    fn simple(&self, item: &Node) -> bool {
        !item.decorated
            && matches!(
                item.kind,
                NodeKind::Scalar {
                    style: ScalarStyle::Plain
                        | ScalarStyle::SingleQuoted
                        | ScalarStyle::DoubleQuoted,
                    ..
                }
            )
            && self.document.line_index(item.span.start) == self.document.line_index(item.span.end)
    }

    /// Whether there is a comment on any line from the one containing
    /// `start` to the one containing `end`.
    fn has_comments(&self, start: usize, end: usize) -> bool {
        let lines: Range<usize> =
            self.document.line_index(start)..self.document.line_index(end) + 1;
        self.document
            .comments()
            .iter()
            .any(|comment| lines.contains(&self.document.line_index(comment.start)))
    }
}
