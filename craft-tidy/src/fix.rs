// SPDX-License-Identifier: GPL-3.0-only

//! Helpers for checks that fix the problems they find.
//!
//! Fixes are expressed as text edits of the project file, so that
//! formatting and comments elsewhere are preserved. Every edit is verified
//! before it is kept: the edited text must still be valid YAML, and the
//! check decides whether the resulting data is what it intended, typically
//! by comparing [`Document::value`]s before and after.

use std::cmp::Reverse;
use std::ops::Range;

use crate::yaml::Document;

/// Replacement of a byte range of the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

impl Edit {
    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

/// Applies `edits` to `document` one at a time and returns the result, or
/// `None` if no edit was kept.
///
/// Edits are applied from the end of the source towards the start, so their
/// ranges always refer to the original text; an edit overlapping one that
/// was already kept is skipped, and can be proposed again on the edited
/// document. Each edit is kept only if the edited text parses and
/// `accept(before, after, data)` returns true, where `before` is the
/// document with the previously kept edits applied.
pub fn apply_edits<T>(
    document: &Document,
    mut edits: Vec<(Edit, T)>,
    mut accept: impl FnMut(&Document, &Document, &T) -> bool,
) -> Option<Document> {
    edits.sort_by_key(|(edit, _)| Reverse(edit.range.start));
    let mut current: Option<Document> = None;
    let mut kept: Vec<Range<usize>> = Vec::new();

    for (edit, data) in edits {
        if kept
            .iter()
            .any(|range| edit.range.start < range.end && range.start < edit.range.end)
        {
            continue;
        }
        let before = current.as_ref().unwrap_or(document);
        let mut source = before.source().to_owned();
        source.replace_range(edit.range.clone(), &edit.replacement);
        let Ok(after) = Document::parse(source) else {
            continue;
        };
        if accept(before, &after, &data) {
            kept.push(edit.range);
            current = Some(after);
        }
    }
    current
}
