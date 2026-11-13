// SPDX-License-Identifier: GPL-3.0-only

//! Suppressing findings with `NOLINT` comments, as in clang-tidy:
//!
//! - `# NOLINT` suppresses findings on its own line,
//! - `# NOLINTNEXTLINE` on the next line,
//! - `# NOLINTBEGIN` and `# NOLINTEND` on the lines from one to the other.
//!
//! Each may be followed directly by a parenthesised, comma-separated list
//! of check globs, e.g. `# NOLINT(style-line-length,snapcraft-*)`, to only
//! suppress those checks, and then by any text, such as a reason.

use std::ops::RangeInclusive;

use crate::checks::Diagnostic;
use crate::glob::GlobList;
use crate::yaml::Document;

/// The name under which mistakes in `NOLINT` comments are reported.
pub const NAME: &str = "craft-tidy-nolint";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    NextLine,
    Begin,
    End,
}

/// The directives, longest first so that `NOLINT` does not match the
/// others.
const KEYWORDS: &[(&str, Kind)] = &[
    ("NOLINTNEXTLINE", Kind::NextLine),
    ("NOLINTBEGIN", Kind::Begin),
    ("NOLINTEND", Kind::End),
    ("NOLINT", Kind::Line),
];

/// A parsed `NOLINT` comment.
struct Directive {
    kind: Kind,
    /// 0-based line of the comment.
    line: usize,
    /// 1-based column of the keyword.
    column: usize,
    /// The checks it applies to; `None` for all.
    checks: Option<GlobList>,
    /// The check list as written, to match `NOLINTBEGIN` with `NOLINTEND`.
    written: String,
}

/// The suppressions of a document, and the mistakes in its `NOLINT`
/// comments.
#[derive(Debug, Default)]
pub struct Nolint {
    /// 0-based lines, and the checks suppressed on them (`None` for all).
    ranges: Vec<(RangeInclusive<usize>, Option<GlobList>)>,
    problems: Vec<Diagnostic>,
}

impl Nolint {
    /// Finds the `NOLINT` comments of `document`.
    pub fn parse(document: &Document) -> Self {
        let mut nolint = Nolint::default();
        let mut open: Vec<Directive> = Vec::new();
        for comment in document.comments() {
            let directive = match parse_comment(document, comment.start) {
                Ok(Some(directive)) => directive,
                Ok(None) => continue,
                Err((column, message)) => {
                    let line = document.line_index(comment.start) + 1;
                    nolint
                        .problems
                        .push(Diagnostic::warning(NAME, message).at(line, column));
                    continue;
                }
            };
            match directive.kind {
                Kind::Line => nolint
                    .ranges
                    .push((directive.line..=directive.line, directive.checks)),
                Kind::NextLine => nolint
                    .ranges
                    .push((directive.line + 1..=directive.line + 1, directive.checks)),
                Kind::Begin => open.push(directive),
                Kind::End => {
                    let begin = open
                        .iter()
                        .rposition(|begin| same_checks(&begin.written, &directive.written));
                    match begin {
                        Some(index) => {
                            let begin = open.remove(index);
                            nolint
                                .ranges
                                .push((begin.line..=directive.line, begin.checks));
                        }
                        None => nolint.problems.push(
                            Diagnostic::warning(
                                NAME,
                                format!(
                                    "NOLINTEND{} has no matching NOLINTBEGIN{0} before it",
                                    list_suffix(&directive.written)
                                ),
                            )
                            .at(directive.line + 1, directive.column),
                        ),
                    }
                }
            }
        }
        for begin in open {
            nolint.problems.push(
                Diagnostic::warning(
                    NAME,
                    format!(
                        "NOLINTBEGIN{} has no matching NOLINTEND{0} after it, so it \
                         suppresses nothing",
                        list_suffix(&begin.written)
                    ),
                )
                .at(begin.line + 1, begin.column),
            );
        }
        nolint
    }

    /// Whether findings of `check` on the 1-based `line` are suppressed.
    pub fn suppresses(&self, check: &str, line: usize) -> bool {
        let Some(index) = line.checked_sub(1) else {
            return false;
        };
        self.ranges.iter().any(|(lines, checks)| {
            lines.contains(&index) && checks.as_ref().is_none_or(|checks| checks.matches(check))
        })
    }

    /// Whether findings of `check` are suppressed on any of the 0-based
    /// lines `lines`.
    pub fn suppresses_any(&self, check: &str, lines: RangeInclusive<usize>) -> bool {
        lines
            .into_iter()
            .any(|line| self.suppresses(check, line + 1))
    }

    /// The mistakes found in `NOLINT` comments, as findings.
    pub fn problems(&self) -> &[Diagnostic] {
        &self.problems
    }
}

/// Whether the comment starting at byte `start` is a `NOLINT` comment.
pub fn is_directive(document: &Document, start: usize) -> bool {
    !matches!(parse_comment(document, start), Ok(None))
}

fn list_suffix(written: &str) -> String {
    if written.is_empty() {
        String::new()
    } else {
        format!("({written})")
    }
}

fn same_checks(a: &str, b: &str) -> bool {
    fn normalize(s: &str) -> Vec<&str> {
        let mut globs: Vec<&str> = s.split(',').map(str::trim).collect();
        globs.sort_unstable();
        globs
    }
    normalize(a) == normalize(b)
}

/// Parses the comment starting at byte `start` (its `#`): `Ok(None)` if it
/// is not a `NOLINT` comment, or the 1-based column and description of a
/// mistake in it.
fn parse_comment(document: &Document, start: usize) -> Result<Option<Directive>, (usize, String)> {
    let line = document.line_index(start);
    let end = document.line_range(line).end;
    let text = &document.source()[start..end];
    let body = text[1..].trim_start();
    let offset = start + (text.len() - body.len());
    let column = document.position(offset).1;

    let Some((keyword, kind, rest)) = KEYWORDS
        .iter()
        .find_map(|(keyword, kind)| Some((*keyword, *kind, body.strip_prefix(keyword)?)))
    else {
        return Ok(None);
    };
    // `NOLINTED`, `NOLINT_X` and the like are not directives.
    if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Ok(None);
    }
    let (checks, written) = match rest.strip_prefix('(') {
        None => (None, String::new()),
        Some(list) => {
            let Some(close) = list.find(')') else {
                return Err((column, format!("{keyword} has an unclosed '('")));
            };
            let written = list[..close].trim();
            if written.is_empty() {
                return Err((
                    column,
                    format!("{keyword}() lists no checks; omit the parentheses to suppress all"),
                ));
            }
            if let Some(negated) = written
                .split(',')
                .map(str::trim)
                .find(|g| g.starts_with('-'))
            {
                return Err((
                    column,
                    format!("{keyword} cannot disable suppression with '{negated}'"),
                ));
            }
            let globs: GlobList = written.parse().map_err(|_| {
                (
                    column,
                    format!(
                        "{keyword} has an invalid check list '{written}': list check names \
                         or globs (lowercase letters, digits, '-' and '*'), separated by commas"
                    ),
                )
            })?;
            (Some(globs), written.to_owned())
        }
    };
    Ok(Some(Directive {
        kind,
        line,
        column,
        checks,
        written,
    }))
}
