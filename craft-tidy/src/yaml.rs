// SPDX-License-Identifier: GPL-3.0-only

//! A YAML document tree that keeps source positions.
//!
//! Checks inspect the tree to find problems, and fixes use its byte ranges to
//! edit the source text without disturbing formatting or comments. The
//! [`Value`] model strips all presentation details, so that fixes can verify
//! that an edit changed the data only as intended.

use std::fmt;
use std::ops::Range;

use serde_saphyr::granit_parser::{self as parser, Event, Marker, Parser, StructureStyle};

/// How a scalar is written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    /// A `|` block scalar.
    Literal,
    /// A `>` block scalar.
    Folded,
}

impl From<parser::ScalarStyle> for ScalarStyle {
    fn from(style: parser::ScalarStyle) -> Self {
        match style {
            parser::ScalarStyle::Plain => Self::Plain,
            parser::ScalarStyle::SingleQuoted => Self::SingleQuoted,
            parser::ScalarStyle::DoubleQuoted => Self::DoubleQuoted,
            parser::ScalarStyle::Literal => Self::Literal,
            parser::ScalarStyle::Folded => Self::Folded,
        }
    }
}

/// How a collection is written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionStyle {
    /// Indentation based, e.g. `- item` or `key: value` lines.
    Block,
    /// Bracketed, e.g. `[a, b]` or `{a: b}`.
    Flow,
}

impl From<StructureStyle> for CollectionStyle {
    fn from(style: StructureStyle) -> Self {
        match style {
            StructureStyle::Block => Self::Block,
            StructureStyle::Flow => Self::Flow,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Scalar {
        /// The value after unquoting and escape processing.
        value: String,
        style: ScalarStyle,
    },
    Sequence {
        style: CollectionStyle,
        items: Vec<Node>,
    },
    Mapping {
        style: CollectionStyle,
        entries: Vec<(Node, Node)>,
    },
    Alias,
}

/// A node of the document tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    /// Byte range of the node in the source. Quoted scalars include their
    /// quotes, block scalars cover their content lines (not the `|`/`>`
    /// header), flow collections include their brackets and block
    /// collections run from their first entry to the end of their last one.
    pub span: Range<usize>,
    /// Whether the node has an anchor or a tag. These precede the node and
    /// are not included in `span`.
    pub decorated: bool,
}

impl Node {
    /// The scalar value, if this is a scalar.
    pub fn as_scalar(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Scalar { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The value with all presentation details removed.
    pub fn to_value(&self) -> Value {
        match &self.kind {
            NodeKind::Scalar { value, style } => Value::Scalar {
                value: value.clone(),
                plain: *style == ScalarStyle::Plain,
            },
            NodeKind::Sequence { items, .. } => {
                Value::Sequence(items.iter().map(Node::to_value).collect())
            }
            NodeKind::Mapping { entries, .. } => Value::Mapping(
                entries
                    .iter()
                    .map(|(key, value)| (key.to_value(), value.to_value()))
                    .collect(),
            ),
            NodeKind::Alias => Value::Alias,
        }
    }
}

/// The data of a YAML node, without source positions or formatting.
///
/// Plain scalars are distinguished from quoted and block scalars because
/// their type (string, number, boolean, null) depends on their content.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Scalar {
        value: String,
        plain: bool,
    },
    Sequence(Vec<Value>),
    /// Entries in source order.
    Mapping(Vec<(Value, Value)>),
    Alias,
}

impl Value {
    /// The value at `path`, where each element indexes a sequence item or,
    /// in a mapping, selects the value of the entry at that index.
    pub fn get_mut(&mut self, path: &[usize]) -> Option<&mut Value> {
        let Some((&index, rest)) = path.split_first() else {
            return Some(self);
        };
        match self {
            Value::Sequence(items) => items.get_mut(index)?.get_mut(rest),
            Value::Mapping(entries) => entries.get_mut(index)?.1.get_mut(rest),
            _ => None,
        }
    }
}

/// A YAML syntax error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    /// 1-based line and column.
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// A parsed YAML document together with its source text.
#[derive(Debug, Clone)]
pub struct Document {
    source: String,
    root: Option<Node>,
    comments: Vec<Range<usize>>,
    line_starts: Vec<usize>,
}

impl Document {
    /// Parses a single YAML document. An empty source has no root node.
    pub fn parse(source: impl Into<String>) -> Result<Self, SyntaxError> {
        let source = source.into();
        let (root, comments) = build_tree(&source)?;
        let line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Ok(Self {
            source,
            root,
            comments,
            line_starts,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn root(&self) -> Option<&Node> {
        self.root.as_ref()
    }

    /// The data of the whole document.
    pub fn value(&self) -> Option<Value> {
        self.root.as_ref().map(Node::to_value)
    }

    /// Byte ranges of all comments, each starting at its `#`.
    pub fn comments(&self) -> &[Range<usize>] {
        &self.comments
    }

    /// Line terminator used by the source: `"\r\n"` if its first line ends
    /// that way, `"\n"` otherwise.
    pub fn newline(&self) -> &'static str {
        match self.source.find('\n') {
            Some(i) if self.source[..i].ends_with('\r') => "\r\n",
            _ => "\n",
        }
    }

    /// Number of lines. A trailing line terminator does not start a new line.
    pub fn line_count(&self) -> usize {
        if self.source.ends_with('\n') {
            self.line_starts.len() - 1
        } else {
            self.line_starts.len()
        }
    }

    /// 0-based index of the line containing byte `offset`.
    pub fn line_index(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset) - 1
    }

    /// Byte range of the 0-based line `index`, excluding its terminator.
    pub fn line_range(&self, index: usize) -> Range<usize> {
        let start = self.line_starts[index];
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.source.len(), |&next| next - 1);
        let end = if self.source[start..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        start..end
    }

    /// Byte range of the 0-based line `index`, including its terminator.
    pub fn full_line_range(&self, index: usize) -> Range<usize> {
        let start = self.line_starts[index];
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.source.len());
        start..end
    }

    /// Text of the 0-based line `index`, excluding its terminator.
    pub fn line(&self, index: usize) -> &str {
        &self.source[self.line_range(index)]
    }

    /// 1-based line and character column of byte `offset`.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let line = self.line_index(offset);
        let start = self.line_starts[line];
        (line + 1, self.source[start..offset].chars().count() + 1)
    }
}

/// A collection being built while its child events are processed.
enum Frame {
    Sequence {
        style: CollectionStyle,
        start: usize,
        decorated: bool,
        items: Vec<Node>,
    },
    Mapping {
        style: CollectionStyle,
        start: usize,
        decorated: bool,
        entries: Vec<(Node, Node)>,
        key: Option<Node>,
    },
}

fn offset(marker: &Marker) -> usize {
    // Always present for string input.
    marker.byte_offset().unwrap_or(marker.index())
}

fn syntax_error(marker: &Marker, message: impl Into<String>) -> SyntaxError {
    SyntaxError {
        message: message.into(),
        line: marker.line(),
        column: marker.col() + 1,
    }
}

type Tree = (Option<Node>, Vec<Range<usize>>);

fn build_tree(source: &str) -> Result<Tree, SyntaxError> {
    let mut root = None;
    let mut comments = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut documents = 0;

    for event in Parser::new_from_str(source) {
        let (event, span) = event.map_err(|err| syntax_error(err.marker(), err.info()))?;
        let (start, end) = (offset(&span.start), offset(&span.end));
        let node = match event {
            Event::DocumentStart(..) => {
                documents += 1;
                if documents > 1 {
                    return Err(syntax_error(
                        &span.start,
                        "multiple YAML documents are not supported",
                    ));
                }
                continue;
            }
            Event::Comment(..) => {
                comments.push(start..end);
                continue;
            }
            Event::Scalar(value, style, anchor, tag) => Node {
                kind: NodeKind::Scalar {
                    value: value.into_owned(),
                    style: style.into(),
                },
                span: start..end,
                decorated: anchor != 0 || tag.is_some(),
            },
            Event::Alias(_) => Node {
                kind: NodeKind::Alias,
                span: start..end,
                decorated: false,
            },
            Event::SequenceStart(style, anchor, tag) => {
                stack.push(Frame::Sequence {
                    style: style.into(),
                    start,
                    decorated: anchor != 0 || tag.is_some(),
                    items: Vec::new(),
                });
                continue;
            }
            Event::MappingStart(style, anchor, tag) => {
                stack.push(Frame::Mapping {
                    style: style.into(),
                    start,
                    decorated: anchor != 0 || tag.is_some(),
                    entries: Vec::new(),
                    key: None,
                });
                continue;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                let Some(frame) = stack.pop() else {
                    return Err(syntax_error(&span.start, "unbalanced collection end"));
                };
                close(frame, end)
            }
            _ => continue,
        };

        match stack.last_mut() {
            None => root = Some(node),
            Some(Frame::Sequence { items, .. }) => items.push(node),
            Some(Frame::Mapping { entries, key, .. }) => match key.take() {
                None => *key = Some(node),
                Some(k) => entries.push((k, node)),
            },
        }
    }

    Ok((root, comments))
}

/// Turns a finished frame into a node. `end` is the end of the closing
/// event, which is only meaningful for flow collections.
fn close(frame: Frame, end: usize) -> Node {
    let (kind, start, style, decorated) = match frame {
        Frame::Sequence {
            style,
            start,
            decorated,
            items,
        } => (NodeKind::Sequence { style, items }, start, style, decorated),
        Frame::Mapping {
            style,
            start,
            decorated,
            entries,
            ..
        } => (
            NodeKind::Mapping { style, entries },
            start,
            style,
            decorated,
        ),
    };
    let end = match style {
        CollectionStyle::Flow => end,
        CollectionStyle::Block => last_child_end(&kind).unwrap_or(start),
    };
    Node {
        kind,
        span: start..end,
        decorated,
    }
}

fn last_child_end(kind: &NodeKind) -> Option<usize> {
    match kind {
        NodeKind::Sequence { items, .. } => items.last().map(|node| node.span.end),
        NodeKind::Mapping { entries, .. } => entries.last().map(|(_, value)| value.span.end),
        _ => None,
    }
}
