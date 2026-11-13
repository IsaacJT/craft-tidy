// SPDX-License-Identifier: GPL-3.0-only

//! `style-sorted-lists`: entries of unordered lists must be sorted.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ops::Range;

use crate::checks::{Check, Context, Diagnostic, OptionKind, OptionSpec};
use crate::config::OptionValue;
use crate::fix::Edit;
use crate::glob::GlobList;
use crate::yaml::{CollectionStyle, Document, Node, NodeKind, Value};

const NAME: &str = "style-sorted-lists";

/// Keys whose lists are unordered sets in the projects of the craft tools
/// (most are craft-parts keys shared by all of them), so that sorting them
/// cannot change the meaning of the project.
const DEFAULT_KEYS: &str = "after,assumes,before,build-attributes,build-for,build-on,\
                            build-packages,build-snaps,plugs,slots,stage-packages,stage-snaps";

const OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        name: "keys",
        kind: OptionKind::String,
        default: "\"after,assumes,before,build-attributes,build-for,build-on,build-packages,\
                  build-snaps,plugs,slots,stage-packages,stage-snaps\"",
        description: "Globs selecting the keys whose lists must be sorted, in the same \
                      syntax as --checks. Do not include keys whose order matters, such as \
                      command-chain or build tool arguments.",
    },
    OptionSpec {
        name: "case-sensitive",
        kind: OptionKind::Bool,
        default: "false",
        description: "Sort uppercase letters before lowercase ones instead of ignoring case.",
    },
];

pub struct SortedLists;

impl Check for SortedLists {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Entries of unordered lists must be sorted alphabetically"
    }

    fn explanation(&self) -> &'static str {
        "Reports lists whose entries are not sorted alphabetically, for the \
         keys selected by the keys option (by default, keys such as \
         build-packages, stage-packages, plugs and after, whose order does \
         not matter to the craft tools).\n\
         \n\
         Advanced grammar entries (on ...:, to ...:, for ...:, try:, else: \
         and else fail) must come after all plain entries, in their original order so \
         that each else stays with the statement it belongs to; the lists \
         inside them must be sorted too. Lists containing other kinds of \
         entries, or an else that does not follow another grammar entry, are \
         not checked.\n\
         \n\
         With --fix, the entries are reordered. Comments directly above an \
         entry move with it, except for comments above the first entry, \
         which are kept as a header for the whole list."
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn validate_option(&self, name: &str, value: &OptionValue) -> Result<(), String> {
        match (name, value.as_str()) {
            ("keys", Some(keys)) => keys
                .parse::<GlobList>()
                .map(drop)
                .map_err(|err| err.to_string()),
            _ => Ok(()),
        }
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let document = &ctx.project.document;
        unsorted_lists(ctx)
            .into_iter()
            .map(|list| {
                let (line, column) = document.position(list.location);
                Diagnostic::warning(NAME, list.message).at(line, column)
            })
            .collect()
    }

    fn fix(&self, ctx: &Context<'_>) -> Option<Document> {
        let edits = unsorted_lists(ctx)
            .into_iter()
            .filter_map(|list| list.edit.map(|edit| (edit, (list.path, list.order))))
            .collect();
        // Edits of one batch never overlap, so they sort different lists,
        // and the order of the sorts does not matter.
        ctx.apply_edits(NAME, edits, |before, after, lists| {
            let Some(mut expected) = before.value() else {
                return false;
            };
            for (path, order) in lists {
                let Some(Value::Sequence(items)) = expected.get_mut(path) else {
                    return false;
                };
                *items = order.iter().map(|&i| items[i].clone()).collect();
            }
            after.value() == Some(expected)
        })
    }
}

/// An unsorted list and how to sort it.
struct UnsortedList {
    /// Byte offset of the first misplaced entry.
    location: usize,
    message: String,
    /// Path of the list in the document's [`Value`].
    path: Vec<usize>,
    /// Indices of the current entries in sorted order.
    order: Vec<usize>,
    /// Edit sorting the list, if it can be done safely.
    edit: Option<Edit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grammar {
    /// `on …`, `to …`, `on … to …` or `for …`.
    Selector,
    Try,
    Else,
    ElseFail,
}

/// The grammar statement that a single-entry mapping with this key starts,
/// following craft-grammar's rules (any whitespace after the keyword).
fn statement(key: &str) -> Option<Grammar> {
    let has_selectors = |keyword: &str| {
        key.strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace) && !rest.trim().is_empty())
    };
    match key {
        "try" => Some(Grammar::Try),
        "else" => Some(Grammar::Else),
        _ if ["on", "to", "for"].into_iter().any(has_selectors) => Some(Grammar::Selector),
        _ => None,
    }
}

/// Whether a list entry is the `else fail` statement.
fn is_else_fail(value: &str) -> bool {
    value
        .strip_prefix("else")
        .and_then(|rest| rest.strip_suffix("fail"))
        .is_some_and(|gap| !gap.is_empty() && gap.chars().all(char::is_whitespace))
}

/// How an entry takes part in sorting.
enum Entry<'a> {
    Plain(&'a str),
    Grammar {
        kind: Grammar,
        label: &'a str,
        /// The list inside the statement, and its path below the entry.
        nested: Option<&'a Node>,
    },
}

impl Entry<'_> {
    fn label(&self) -> String {
        match self {
            Entry::Plain(value) => format!("'{value}'"),
            Entry::Grammar { label, .. } => format!("advanced grammar entry '{label}'"),
        }
    }
}

fn classify(node: &Node) -> Option<Entry<'_>> {
    if node.decorated {
        return None;
    }
    match &node.kind {
        NodeKind::Scalar { value, .. } if is_else_fail(value) => Some(Entry::Grammar {
            kind: Grammar::ElseFail,
            label: value,
            nested: None,
        }),
        NodeKind::Scalar { value, .. } => Some(Entry::Plain(value)),
        NodeKind::Mapping { entries, .. } if entries.len() == 1 => {
            let (key, value) = &entries[0];
            let label = key.as_scalar().filter(|_| !key.decorated)?;
            let kind = statement(label)?;
            let nested = matches!(value.kind, NodeKind::Sequence { .. }).then_some(value);
            Some(Entry::Grammar {
                kind,
                label,
                nested,
            })
        }
        _ => None,
    }
}

struct Analysis<'a> {
    document: &'a Document,
    keys: GlobList,
    case_sensitive: bool,
    lists: Vec<UnsortedList>,
}

fn unsorted_lists(ctx: &Context<'_>) -> Vec<UnsortedList> {
    let keys = ctx
        .options
        .get(NAME, "keys")
        .and_then(OptionValue::as_str)
        .unwrap_or(DEFAULT_KEYS)
        .parse()
        .unwrap_or_default();
    let case_sensitive = ctx
        .options
        .get(NAME, "case-sensitive")
        .and_then(OptionValue::as_bool)
        .unwrap_or(false);
    let mut analysis = Analysis {
        document: &ctx.project.document,
        keys,
        case_sensitive,
        lists: Vec::new(),
    };
    if let Some(root) = ctx.project.document.root() {
        analysis.visit(root, &mut Vec::new());
    }
    analysis.lists
}

impl Analysis<'_> {
    fn visit(&mut self, node: &Node, path: &mut Vec<usize>) {
        match &node.kind {
            NodeKind::Mapping { entries, .. } => {
                for (index, (key, value)) in entries.iter().enumerate() {
                    path.push(index);
                    let selected = key.as_scalar().is_some_and(|key| self.keys.matches(key));
                    match (&value.kind, key.as_scalar()) {
                        (NodeKind::Sequence { .. }, Some(name)) if selected => {
                            self.check_list(value, path, name.to_owned());
                        }
                        _ => self.visit(value, path),
                    }
                    path.pop();
                }
            }
            NodeKind::Sequence { items, .. } => {
                for (index, item) in items.iter().enumerate() {
                    path.push(index);
                    self.visit(item, path);
                    path.pop();
                }
            }
            NodeKind::Scalar { .. } | NodeKind::Alias => {}
        }
    }

    fn compare(&self, a: &str, b: &str) -> Ordering {
        if self.case_sensitive {
            a.cmp(b)
        } else {
            a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
        }
    }

    fn check_list(&mut self, list: &Node, path: &mut Vec<usize>, name: String) {
        let NodeKind::Sequence { style, items } = &list.kind else {
            return;
        };
        let Some(entries) = items.iter().map(classify).collect::<Option<Vec<_>>>() else {
            // Not a list of plain values and grammar; look inside it instead.
            for (index, item) in items.iter().enumerate() {
                path.push(index);
                self.visit(item, path);
                path.pop();
            }
            return;
        };

        // An else belongs to the statement before it, so it must directly
        // follow a grammar entry (craft-grammar ends a statement at the next
        // plain entry).
        let valid_grammar = entries.iter().enumerate().all(|(i, entry)| match entry {
            Entry::Grammar {
                kind: Grammar::Else | Grammar::ElseFail,
                ..
            } => i > 0 && matches!(entries[i - 1], Entry::Grammar { .. }),
            _ => true,
        });
        if !valid_grammar {
            return;
        }

        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by(|&a, &b| match (&entries[a], &entries[b]) {
            (Entry::Plain(x), Entry::Plain(y)) => self.compare(x, y),
            (Entry::Plain(_), Entry::Grammar { .. }) => Ordering::Less,
            (Entry::Grammar { .. }, Entry::Plain(_)) => Ordering::Greater,
            (Entry::Grammar { .. }, Entry::Grammar { .. }) => a.cmp(&b),
        });

        if let Some(first) = (0..order.len()).find(|&i| order[i] != i) {
            let message = format!(
                "entries of '{name}' are not sorted: {} should come before {}",
                entries[order[first]].label(),
                entries[first].label()
            );
            let edit = match style {
                CollectionStyle::Block => block_edit(self.document, list, items, &order),
                CollectionStyle::Flow => flow_edit(self.document, items, &order),
            };
            self.lists.push(UnsortedList {
                location: items[first].span.start,
                message,
                path: path.clone(),
                order,
                edit,
            });
        }

        for (index, entry) in entries.iter().enumerate() {
            if let Entry::Grammar {
                label,
                nested: Some(nested),
                ..
            } = entry
            {
                // The statement is a single-entry mapping; its list is the
                // value of entry 0.
                path.extend([index, 0]);
                self.check_list(nested, path, format!("{name}: {label}"));
                path.truncate(path.len() - 2);
            }
        }
    }
}

/// Reorders the entries of a block sequence by moving whole lines, so that
/// nested content and comments move with their entry.
fn block_edit(document: &Document, list: &Node, items: &[Node], order: &[usize]) -> Option<Edit> {
    let source = document.source();
    // A block sequence starts at its first `-`, and all its `-` share a column.
    let first_line = document.line_index(list.span.start);
    let dash_column = list.span.start - document.line_range(first_line).start;
    let is_dash_line = |line: usize| {
        let text = document.line(line);
        text.len() > dash_column
            && text.as_bytes()[..dash_column].iter().all(|&b| b == b' ')
            && matches!(&text.as_bytes()[dash_column..], [b'-'] | [b'-', b' ', ..])
    };
    let comment_lines: HashSet<usize> = document
        .comments()
        .iter()
        .filter(|range| {
            let line = document.line_index(range.start);
            source[document.line_range(line).start..range.start]
                .trim()
                .is_empty()
        })
        .map(|range| document.line_index(range.start))
        .collect();

    // Each entry occupies whole lines: from its `-` (plus the comment lines
    // directly above, except for the first entry) to its last line.
    let mut chunks: Vec<Range<usize>> = Vec::new();
    let mut previous_end_line = None;
    for (index, item) in items.iter().enumerate() {
        // The entry's `-` is on its first line or, e.g. for block scalars
        // whose content starts on the next line, on an earlier one.
        let lowest = previous_end_line.map_or(first_line, |end| end + 1);
        let mut dash_line = document.line_index(item.span.start);
        while !is_dash_line(dash_line) {
            if dash_line <= lowest {
                return None;
            }
            dash_line -= 1;
        }
        let mut start_line = dash_line;
        if index > 0 {
            while start_line > 0
                && comment_lines.contains(&(start_line - 1))
                && previous_end_line.is_some_and(|end| end < start_line - 1)
            {
                start_line -= 1;
            }
        }
        let end_line = document.line_index(item.span.end.saturating_sub(1).max(item.span.start));
        if previous_end_line.is_some_and(|end| end >= start_line) || end_line < dash_line {
            return None;
        }
        chunks.push(
            document.full_line_range(start_line).start..document.full_line_range(end_line).end,
        );
        previous_end_line = Some(end_line);
    }

    let newline = document.newline();
    let mut texts: Vec<String> = chunks
        .iter()
        .map(|r| source[r.clone()].to_owned())
        .collect();
    let missing_newline = !texts.last()?.ends_with('\n');
    if missing_newline {
        texts.last_mut()?.push_str(newline);
    }
    let mut replacement = String::new();
    for (position, &index) in order.iter().enumerate() {
        replacement.push_str(&texts[index]);
        if let Some(next) = chunks.get(position + 1) {
            replacement.push_str(&source[chunks[position].end..next.start]);
        }
    }
    if missing_newline {
        replacement.truncate(replacement.len() - newline.len());
    }
    Some(Edit::new(
        chunks.first()?.start..chunks.last()?.end,
        replacement,
    ))
}

/// Reorders the entries of a flow sequence of scalars, keeping the original
/// separators.
fn flow_edit(document: &Document, items: &[Node], order: &[usize]) -> Option<Edit> {
    let source = document.source();
    if !items
        .iter()
        .all(|item| matches!(item.kind, NodeKind::Scalar { .. }))
    {
        return None;
    }
    let separators: Vec<&str> = items
        .windows(2)
        .map(|pair| &source[pair[0].span.end..pair[1].span.start])
        .collect();
    if separators.iter().any(|separator| separator.contains('#')) {
        return None;
    }
    let mut replacement = String::new();
    for (position, &index) in order.iter().enumerate() {
        replacement.push_str(&source[items[index].span.clone()]);
        if let Some(separator) = separators.get(position) {
            replacement.push_str(separator);
        }
    }
    Some(Edit::new(
        items.first()?.span.start..items.last()?.span.end,
        replacement,
    ))
}
