// SPDX-License-Identifier: GPL-3.0-only

//! `style-line-length`: lines must not exceed a maximum length.

use crate::checks::{Check, Context, Diagnostic, OptionKind, OptionSpec};
use crate::config::OptionValue;
use crate::fix::{Edit, apply_edits};
use crate::yaml::{CollectionStyle, Document, Node, NodeKind, ScalarStyle};

const NAME: &str = "style-line-length";
const DEFAULT_MAX_LENGTH: usize = 80;

/// Wrapping is not attempted when continuation lines would have less room
/// than this, as the result would be harder to read than the long line.
const MIN_WRAP_WIDTH: usize = 20;

const OPTIONS: &[OptionSpec] = &[OptionSpec {
    name: "max-length",
    kind: OptionKind::Integer,
    default: "80",
    description: "Maximum number of characters per line.",
}];

pub struct LineLength;

impl Check for LineLength {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Lines must not be longer than max-length characters"
    }

    fn explanation(&self) -> &'static str {
        "Reports every line of the project file that is longer than \
         max-length characters.\n\
         \n\
         With --fix, long lines are wrapped where this does not change the \
         data: plain and quoted strings continue on further indented lines, \
         which YAML joins with single spaces; lines of folded (>) block \
         scalars are broken at spaces; flow sequences such as [a, b] become \
         block sequences; and prose comments are re-wrapped. Literal (|) \
         block scalars, such as override-build scripts, and single words \
         such as long URLs cannot be wrapped and are only reported."
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn validate_option(&self, _name: &str, value: &OptionValue) -> Result<(), String> {
        match value.as_i64() {
            Some(max) if max >= 1 => Ok(()),
            _ => Err("must be a positive integer".to_owned()),
        }
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let max = max_length(ctx);
        let document = &ctx.project.document;
        (0..document.line_count())
            .filter_map(|index| {
                let length = document.line(index).chars().count();
                (length > max).then(|| {
                    Diagnostic::warning(
                        NAME,
                        format!("line is {length} characters long, exceeding the limit of {max}"),
                    )
                    .at(index + 1, max + 1)
                })
            })
            .collect()
    }

    fn fix(&self, ctx: &Context<'_>) -> Option<Document> {
        let document = &ctx.project.document;
        let wrapper = Wrapper {
            document,
            max: max_length(ctx),
        };
        let mut edits = Vec::new();
        if let Some(root) = document.root() {
            wrapper.visit(root, Parent::Root, &mut edits);
        }
        wrapper.wrap_comments(&mut edits);
        let edits = edits.into_iter().map(|edit| (edit, ())).collect();
        apply_edits(document, edits, |before, after, _| {
            before.value() == after.value()
        })
    }
}

fn max_length(ctx: &Context<'_>) -> usize {
    ctx.options
        .get(NAME, "max-length")
        .and_then(OptionValue::as_i64)
        .and_then(|max| usize::try_from(max).ok())
        .unwrap_or(DEFAULT_MAX_LENGTH)
}

/// Where a node sits, which determines how its continuation lines must be
/// indented.
#[derive(Clone, Copy)]
enum Parent {
    Root,
    /// A mapping key; keys are never wrapped.
    Key,
    /// The value of a block mapping entry whose key starts at this column.
    MappingValue {
        key_column: usize,
    },
    /// An item of a block sequence whose `-` is at this column.
    SequenceItem {
        dash_column: usize,
    },
    /// Anything inside a flow collection, which is never rewritten.
    Flow,
}

struct Wrapper<'a> {
    document: &'a Document,
    max: usize,
}

impl Wrapper<'_> {
    fn source(&self) -> &str {
        self.document.source()
    }

    /// 0-based character column of byte `offset`.
    fn column(&self, offset: usize) -> usize {
        self.document.position(offset).1 - 1
    }

    fn line_is_long(&self, line: usize) -> bool {
        self.document.line(line).chars().count() > self.max
    }

    /// The rest of the line after byte `offset` if it is only whitespace,
    /// optionally followed by a comment, and its byte range.
    fn tail(&self, offset: usize) -> Option<(&str, std::ops::Range<usize>)> {
        let end = self
            .document
            .line_range(self.document.line_index(offset))
            .end;
        let tail = self.source().get(offset..end)?;
        let rest = tail.trim_start();
        (rest.is_empty() || rest.starts_with('#')).then_some((tail, offset..end))
    }

    fn newline_and_indent(&self, indent: usize) -> String {
        format!("{}{}", self.document.newline(), " ".repeat(indent))
    }

    fn visit(&self, node: &Node, parent: Parent, edits: &mut Vec<Edit>) {
        match &node.kind {
            NodeKind::Scalar { style, .. } => self.wrap_scalar(node, *style, parent, edits),
            NodeKind::Sequence { style, items } => {
                if *style == CollectionStyle::Flow {
                    self.wrap_flow_sequence(node, items, parent, edits);
                }
                let child = match (style, parent) {
                    (CollectionStyle::Block, Parent::Flow) | (CollectionStyle::Flow, _) => {
                        Parent::Flow
                    }
                    (CollectionStyle::Block, _) => Parent::SequenceItem {
                        dash_column: self.column(node.span.start),
                    },
                };
                for item in items {
                    self.visit(item, child, edits);
                }
            }
            NodeKind::Mapping { style, entries } => {
                for (key, value) in entries {
                    let in_flow = *style == CollectionStyle::Flow || matches!(parent, Parent::Flow);
                    self.visit(key, if in_flow { Parent::Flow } else { Parent::Key }, edits);
                    let child = if in_flow {
                        Parent::Flow
                    } else {
                        Parent::MappingValue {
                            key_column: self.column(key.span.start),
                        }
                    };
                    self.visit(value, child, edits);
                }
            }
            NodeKind::Alias => {}
        }
    }

    fn continuation_indent(parent: Parent) -> Option<usize> {
        match parent {
            Parent::MappingValue { key_column } => Some(key_column + 2),
            Parent::SequenceItem { dash_column } => Some(dash_column + 2),
            Parent::Root | Parent::Key | Parent::Flow => None,
        }
    }

    fn wrap_scalar(&self, node: &Node, style: ScalarStyle, parent: Parent, edits: &mut Vec<Edit>) {
        if node.decorated {
            return;
        }
        if style == ScalarStyle::Folded {
            self.wrap_folded(node, edits);
            return;
        }
        let Some(indent) = Self::continuation_indent(parent) else {
            return;
        };
        let line = self.document.line_index(node.span.start);
        if !matches!(
            style,
            ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted
        ) || line != self.document.line_index(node.span.end)
            || !self.line_is_long(line)
            || self.max < indent + MIN_WRAP_WIDTH
        {
            return;
        }
        // A trailing comment stays at the end of the last line; it takes part
        // in filling lines but is never broken.
        let Some((_, tail)) = self.tail(node.span.end) else {
            return;
        };

        let text = &self.source()[node.span.start..tail.end];
        let breaks = break_points(&self.source()[node.span.clone()], style);
        let first_width = self.max.saturating_sub(self.column(node.span.start));
        if let Some(segments) = fill(text, &breaks, first_width, self.max - indent) {
            edits.push(Edit::new(
                node.span.start..tail.end,
                segments.join(&self.newline_and_indent(indent)),
            ));
        }
    }

    /// Breaks long lines of a `>` block scalar. Lines are only broken at
    /// single spaces, and only lines at the scalar's base indentation, since
    /// YAML does not fold more-indented lines.
    fn wrap_folded(&self, node: &Node, edits: &mut Vec<Edit>) {
        if node.span.is_empty() {
            return;
        }
        let first = self.document.line_index(node.span.start);
        let last = self.document.line_index(node.span.end - 1);
        let indent = (first..=last)
            .map(|line| self.document.line(line))
            .find(|text| !text.trim().is_empty())
            .map(|text| text.len() - text.trim_start_matches(' ').len());
        let Some(indent) = indent else {
            return;
        };
        if self.max < indent + MIN_WRAP_WIDTH {
            return;
        }
        for line in first..=last {
            let text = self.document.line(line);
            let content = &text[indent.min(text.len())..];
            if !self.line_is_long(line) || content.starts_with([' ', '\t']) || content.is_empty() {
                continue;
            }
            let breaks = break_points(content, ScalarStyle::Folded);
            let width = self.max - indent;
            if let Some(segments) = fill(content, &breaks, width, width) {
                let range = self.document.line_range(line);
                edits.push(Edit::new(
                    range.start + indent..range.end,
                    segments.join(&self.newline_and_indent(indent)),
                ));
            }
        }
    }

    /// Turns a long single-line flow sequence of scalars that is the value
    /// of a block mapping entry into a block sequence.
    fn wrap_flow_sequence(
        &self,
        node: &Node,
        items: &[Node],
        parent: Parent,
        edits: &mut Vec<Edit>,
    ) {
        let Parent::MappingValue { key_column } = parent else {
            return;
        };
        let line = self.document.line_index(node.span.start);
        let simple_items = items.iter().all(|item| {
            !item.decorated
                && matches!(
                    &item.kind,
                    NodeKind::Scalar {
                        style: ScalarStyle::Plain
                            | ScalarStyle::SingleQuoted
                            | ScalarStyle::DoubleQuoted,
                        ..
                    }
                )
        });
        if node.decorated
            || items.is_empty()
            || !simple_items
            || line != self.document.line_index(node.span.end)
            || !self.line_is_long(line)
        {
            return;
        }
        let Some((tail, tail_range)) = self.tail(node.span.end) else {
            return;
        };
        // Replace from just after the key's `:` so that the sequence starts on
        // its own line; a trailing comment moves to the key's line.
        let before = &self.source()[..node.span.start];
        let Some(colon) = before.trim_end_matches(' ').strip_suffix(':').map(str::len) else {
            return;
        };
        let comment = tail.trim();
        let mut replacement = if comment.is_empty() {
            String::new()
        } else {
            format!(" {comment}")
        };
        let indent = self.newline_and_indent(key_column + 2);
        for item in items {
            replacement.push_str(&format!("{indent}- {}", &self.source()[item.span.clone()]));
        }
        edits.push(Edit::new(colon + 1..tail_range.end, replacement));
    }

    /// Re-wraps long comments that are alone on their line and look like
    /// prose (`# text`), keeping their indentation.
    fn wrap_comments(&self, edits: &mut Vec<Edit>) {
        for range in self.document.comments() {
            let line = self.document.line_index(range.start);
            let line_start = self.document.line_range(line).start;
            let prefix = &self.source()[line_start..range.start];
            let text = &self.source()[range.clone()];
            let prose = text
                .strip_prefix("# ")
                .is_some_and(|body| !body.starts_with([' ', '\t']));
            if !prose || !prefix.trim().is_empty() || !self.line_is_long(line) {
                continue;
            }
            let indent = prefix.chars().count();
            let width = self.max.saturating_sub(indent + 2);
            if width < MIN_WRAP_WIDTH {
                continue;
            }
            let words: Vec<&str> = text[2..].split_whitespace().collect();
            let lines = fill_words(&words, width);
            if lines.len() > 1 {
                let separator = format!("{}{prefix}# ", self.document.newline());
                edits.push(Edit::new(
                    range.clone(),
                    format!("# {}", lines.join(&separator)),
                ));
            }
        }
    }
}

/// Byte offsets of spaces in `text` at which a line may be broken without
/// changing the value of a scalar of the given style.
fn break_points(text: &str, style: ScalarStyle) -> Vec<usize> {
    let bytes = text.as_bytes();
    let quoted = matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted);
    // Never break directly inside the quotes.
    let (first, last) = if quoted {
        (2, bytes.len().saturating_sub(2))
    } else {
        (1, bytes.len().saturating_sub(1))
    };
    (first..last)
        .filter(|&i| {
            let (prev, next) = (bytes[i - 1], bytes[i + 1]);
            bytes[i] == b' '
                && !prev.is_ascii_whitespace()
                && !next.is_ascii_whitespace()
                && match style {
                    // A continuation line of a plain scalar must not look like
                    // YAML syntax (e.g. `- `, `#`, `&`, quotes) to any parser.
                    // A `-` is fine unless followed by a space, so command line
                    // options such as `--verbose` can start a line.
                    ScalarStyle::Plain => {
                        next.is_ascii_alphanumeric()
                            || b"(/.$_+=~".contains(&next)
                            || (next == b'-'
                                && bytes.get(i + 2).is_some_and(|c| !c.is_ascii_whitespace()))
                    }
                    ScalarStyle::DoubleQuoted => prev != b'\\',
                    _ => true,
                }
        })
        .collect()
}

/// Splits `text` at some of the `breaks` (byte offsets of single spaces,
/// which are dropped) so that the first segment fits in `first_width`
/// characters and the others in `width`, where possible. Returns `None` if
/// no break helps.
fn fill<'a>(
    text: &'a str,
    breaks: &[usize],
    first_width: usize,
    width: usize,
) -> Option<Vec<&'a str>> {
    let chars = |s: &str| s.chars().count();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut limit = first_width;
    let mut candidates = breaks.iter().copied().peekable();

    while chars(&text[start..]) > limit {
        let mut chosen = None;
        while let Some(&b) = candidates.peek() {
            if chosen.is_some() && chars(&text[start..b]) > limit {
                break;
            }
            chosen = Some(b);
            candidates.next();
            if chars(&text[start..b]) > limit {
                // Nothing fits; break as early as possible instead.
                break;
            }
        }
        let Some(b) = chosen else {
            break;
        };
        segments.push(&text[start..b]);
        start = b + 1;
        limit = width;
    }
    if segments.is_empty() {
        return None;
    }
    segments.push(&text[start..]);
    Some(segments)
}

/// Greedily fills lines of at most `width` characters with `words`; words
/// longer than `width` get a line of their own.
fn fill_words(words: &[&str], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in words {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push((*word).to_owned()),
        }
    }
    lines
}
