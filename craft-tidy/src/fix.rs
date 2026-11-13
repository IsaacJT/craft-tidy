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

/// Applies `edits` to `document` and returns the result, or `None` if no
/// edit was kept. See [`kept_edits`].
pub fn apply_edits<T>(
    document: &Document,
    edits: Vec<(Edit, T)>,
    accept: impl FnMut(&Document, &Document, &[&T]) -> bool,
) -> Option<Document> {
    kept_edits(document, edits, accept).0
}

/// Applies `edits` to `document`, returning the result (`None` if no edit
/// was kept) and the indices in `edits` of the edits that were kept.
///
/// Edit ranges always refer to the original text. Edits overlapping one
/// that is kept are skipped, preferring edits nearer the end of the text;
/// they can be proposed again on the edited document. Edits are kept only
/// if the edited text parses and `accept(before, after, data)` returns
/// true, where `before` is the document with the previously kept edits
/// applied, `after` additionally has a batch of edits applied, and `data`
/// holds the data of the edits in that batch.
///
/// To avoid parsing the document once per edit, all edits that do not
/// overlap are first tried as one batch; only a batch that is rejected is
/// split in halves, recursively, to find the edits at fault. Edits that
/// overlapped a rejected edit are then tried in turn.
pub fn kept_edits<T>(
    document: &Document,
    edits: Vec<(Edit, T)>,
    mut accept: impl FnMut(&Document, &Document, &[&T]) -> bool,
) -> (Option<Document>, Vec<usize>) {
    let mut order: Vec<usize> = (0..edits.len()).collect();
    order.sort_by_key(|&i| Reverse(edits[i].0.range.start));
    let mut state = State {
        document,
        edits: &edits,
        kept: Vec::new(),
        current: None,
    };
    let mut remaining = order;
    loop {
        // The edits nearest the end that overlap neither each other nor a
        // kept edit.
        let mut batch: Vec<usize> = Vec::new();
        let mut later = Vec::new();
        for i in remaining {
            let range = &edits[i].0.range;
            let overlaps = |&j: &usize| overlap(range, &edits[j].0.range);
            if state.kept.iter().any(overlaps) {
                continue;
            }
            if batch.iter().any(overlaps) {
                later.push(i);
            } else {
                batch.push(i);
            }
        }
        if batch.is_empty() {
            break;
        }
        state.verify(&batch, &mut accept);
        remaining = later;
    }
    let mut kept = state.kept;
    kept.sort_unstable();
    (state.current, kept)
}

fn overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

struct State<'a, T> {
    document: &'a Document,
    edits: &'a [(Edit, T)],
    /// Indices of the kept edits.
    kept: Vec<usize>,
    /// The document with the kept edits applied, if any.
    current: Option<Document>,
}

impl<T> State<'_, T> {
    /// Keeps `batch` if it is accepted as a whole, or else the parts of it
    /// that are, found by splitting it in halves.
    fn verify(
        &mut self,
        batch: &[usize],
        accept: &mut impl FnMut(&Document, &Document, &[&T]) -> bool,
    ) {
        if batch.is_empty() {
            return;
        }
        let candidates: Vec<usize> = self.kept.iter().chain(batch).copied().collect();
        if let Ok(after) = Document::parse(self.apply(&candidates)) {
            let before = self.current.as_ref().unwrap_or(self.document);
            let data: Vec<&T> = batch.iter().map(|&i| &self.edits[i].1).collect();
            if accept(before, &after, &data) {
                self.kept.extend_from_slice(batch);
                self.current = Some(after);
                return;
            }
        }
        if batch.len() > 1 {
            let (first, second) = batch.split_at(batch.len() / 2);
            self.verify(first, accept);
            self.verify(second, accept);
        }
    }

    /// The original text with the edits `indices` (which do not overlap)
    /// applied.
    fn apply(&self, indices: &[usize]) -> String {
        let mut indices = indices.to_vec();
        indices.sort_by_key(|&i| self.edits[i].0.range.start);
        let source = self.document.source();
        let mut result = String::with_capacity(source.len());
        let mut position = 0;
        for i in indices {
            let edit = &self.edits[i].0;
            result.push_str(&source[position..edit.range.start]);
            result.push_str(&edit.replacement);
            position = edit.range.end;
        }
        result.push_str(&source[position..]);
        result
    }
}
