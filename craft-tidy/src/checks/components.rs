// SPDX-License-Identifier: GPL-3.0-only

//! `snapcraft-components`: components used by the parts of a Snapcraft
//! project must be defined under the top-level `components` key.

use std::collections::BTreeSet;

use crate::checks::{Check, Context, Diagnostic, OptionKind, OptionSpec};
use crate::config::OptionValue;
use crate::craft::Craft;
use crate::yaml::{Document, Node, NodeKind};

const NAME: &str = "snapcraft-components";

const OPTIONS: &[OptionSpec] = &[OptionSpec {
    name: "report-unused",
    kind: OptionKind::Bool,
    default: "true",
    description: "Also report components that are defined but that no part puts \
                  files in, which would be packed empty.",
}];

/// The partition prefix of a component, as used in paths:
/// `(component/<name>)`.
const PATH_PREFIX: &str = "(component/";
/// Environment variables of component partitions:
/// `CRAFT_COMPONENT_<NAME>_{STAGE,PRIME,OVERLAY}`.
const VARIABLE_PREFIX: &str = "CRAFT_COMPONENT_";
const VARIABLE_SUFFIXES: &[&str] = &["_STAGE", "_PRIME", "_OVERLAY"];

pub struct Components;

impl Check for Components {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Components used by parts must be defined under components"
    }

    fn explanation(&self) -> &'static str {
        "Reports components that parts use but that are not defined under the \
         top-level components key, which Snapcraft rejects or silently \
         ignores. A component is used by:\n\
         \n\
         - a (component/<name>) path in a part's organize, stage or prime \
         entries;\n\
         - a $CRAFT_COMPONENT_<NAME>_STAGE, _PRIME or _OVERLAY variable in a \
         part, where <NAME> is the component name in uppercase with - \
         replaced by _;\n\
         - craftctl set (or get) components.<name>.<key> in a part's \
         scripts.\n\
         \n\
         The closest defined name is suggested for likely typos. With the \
         report-unused option (on by default), components that are defined \
         but used by no part are reported too, as they would be packed \
         empty.\n\
         \n\
         Only applies to Snapcraft. These problems cannot be fixed \
         automatically."
    }

    fn crafts(&self) -> Option<&'static [Craft]> {
        Some(&[Craft::Snapcraft])
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let document = &ctx.project.document;
        let Some(root) = document.root() else {
            return Vec::new();
        };
        let defined = defined_components(root);
        let mut uses = Vec::new();
        if let Some(parts) = get(root, "parts") {
            collect_uses(document, parts, &mut uses);
        }

        let mut diagnostics = Vec::new();
        let mut used = BTreeSet::new();
        for usage in &uses {
            let found = defined
                .iter()
                .find(|(name, _)| usage.matches(name))
                .map(|(name, _)| name.as_str());
            match found {
                Some(name) => {
                    if usage.fills {
                        used.insert(name.to_owned());
                    }
                }
                None => {
                    let span = usage.offset..usage.offset + usage.len();
                    diagnostics.push(
                        Diagnostic::warning(NAME, usage.message(&defined)).spanning(document, span),
                    );
                }
            }
        }

        let report_unused = ctx
            .options
            .get(NAME, "report-unused")
            .and_then(OptionValue::as_bool)
            .unwrap_or(true);
        if report_unused {
            for (name, offset) in &defined {
                if !used.contains(name) {
                    diagnostics.push(
                        Diagnostic::warning(
                            NAME,
                            format!(
                                "component '{name}' is defined but no part puts files in it, \
                                 so it would be empty"
                            ),
                        )
                        .spanning(document, *offset..*offset + name.len()),
                    );
                }
            }
        }
        diagnostics
    }
}

/// The value of `key` in the mapping `node`.
fn get<'a>(node: &'a Node, key: &str) -> Option<&'a Node> {
    let NodeKind::Mapping { entries, .. } = &node.kind else {
        return None;
    };
    entries
        .iter()
        .find(|(k, _)| k.as_scalar() == Some(key))
        .map(|(_, value)| value)
}

/// Names of the components defined under the top-level `components` key,
/// and where each is defined.
fn defined_components(root: &Node) -> Vec<(String, usize)> {
    let Some(Node {
        kind: NodeKind::Mapping { entries, .. },
        ..
    }) = get(root, "components")
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(key, _)| Some((key.as_scalar()?.to_owned(), key.span.start)))
        .collect()
}

/// How a component is referred to.
#[derive(Debug)]
enum Reference {
    /// By name, in a path or a `craftctl` key.
    Name(String),
    /// By an environment variable, with the `<NAME>` part.
    Variable { variable: String, name: String },
}

/// A use of a component, at byte `offset` of the document.
#[derive(Debug)]
struct Use {
    reference: Reference,
    offset: usize,
    /// Whether the use puts files in the component (unlike setting its
    /// metadata with `craftctl`).
    fills: bool,
}

impl Use {
    /// Length in bytes of the reference in the source.
    fn len(&self) -> usize {
        match &self.reference {
            Reference::Name(name) => name.len(),
            Reference::Variable { variable, .. } => variable.len(),
        }
    }

    fn matches(&self, name: &str) -> bool {
        match &self.reference {
            Reference::Name(used) => used == name,
            Reference::Variable { name: used, .. } => *used == variable_name(name),
        }
    }

    fn message(&self, defined: &[(String, usize)]) -> String {
        let (what, used) = match &self.reference {
            Reference::Name(name) => (format!("component '{name}'"), name.clone()),
            Reference::Variable { variable, name } => {
                let component = name.to_lowercase().replace('_', "-");
                (
                    format!("${variable} refers to component '{component}', which"),
                    component,
                )
            }
        };
        let mut message = if defined.is_empty() {
            format!("{what} is not defined (no components are defined)")
        } else {
            format!("{what} is not defined under components")
        };
        let closest = defined
            .iter()
            .map(|(name, _)| (distance(&used, name), name))
            .filter(|(distance, name)| *distance <= 2.max(name.len() / 4))
            .min();
        if let Some((_, name)) = closest {
            message.push_str(&format!("; did you mean '{name}'?"));
        }
        message
    }
}

/// The `<NAME>` of a component's environment variables.
fn variable_name(component: &str) -> String {
    component.to_uppercase().replace(['-', '/', '+'], "_")
}

/// Collects the uses of components in the parts.
fn collect_uses(document: &Document, parts: &Node, uses: &mut Vec<Use>) {
    let NodeKind::Mapping { entries, .. } = &parts.kind else {
        return;
    };
    for (_, part) in entries {
        if let Some(organize) = get(part, "organize") {
            path_uses(document, organize, true, uses);
        }
        for key in ["stage", "prime"] {
            if let Some(files) = get(part, key) {
                path_uses(document, files, false, uses);
            }
        }
        text_uses(document, part, uses);
    }
}

/// Uses of `(component/<name>)` at the start of paths: the values of an
/// `organize` mapping, or the entries of a `stage` or `prime` list (which
/// may be negated with `-`, and may be inside grammar statements).
fn path_uses(document: &Document, node: &Node, organize: bool, uses: &mut Vec<Use>) {
    match &node.kind {
        NodeKind::Scalar { value, .. } => {
            let path = if organize {
                value.as_str()
            } else {
                value.strip_prefix('-').unwrap_or(value)
            };
            if let Some(rest) = path.strip_prefix(PATH_PREFIX)
                && let Some(end) = rest.find(')')
            {
                uses.push(Use {
                    reference: Reference::Name(rest[..end].to_owned()),
                    fills: true,
                    offset: locate(document, node, PATH_PREFIX)
                        .map_or(node.span.start, |o| o + PATH_PREFIX.len()),
                });
            }
        }
        NodeKind::Mapping { entries, .. } => {
            for (_, value) in entries {
                path_uses(document, value, organize, uses);
            }
        }
        NodeKind::Sequence { items, .. } => {
            for item in items {
                path_uses(document, item, organize, uses);
            }
        }
        NodeKind::Alias => {}
    }
}

/// Byte offset of the first `needle` in the source text of `node`.
fn locate(document: &Document, node: &Node, needle: &str) -> Option<usize> {
    document.source()[node.span.clone()]
        .find(needle)
        .map(|index| node.span.start + index)
}

/// Uses of components in the strings of a part: environment variables and
/// `craftctl` keys.
fn text_uses(document: &Document, node: &Node, uses: &mut Vec<Use>) {
    match &node.kind {
        NodeKind::Scalar { .. } => {
            let source = &document.source()[node.span.clone()];
            variable_uses(source, node.span.start, uses);
            craftctl_uses(source, node.span.start, uses);
        }
        NodeKind::Mapping { entries, .. } => {
            for (_, value) in entries {
                text_uses(document, value, uses);
            }
        }
        NodeKind::Sequence { items, .. } => {
            for item in items {
                text_uses(document, item, uses);
            }
        }
        NodeKind::Alias => {}
    }
}

fn is_identifier(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `CRAFT_COMPONENT_<NAME>_{STAGE,PRIME,OVERLAY}` in `text`, which starts at
/// byte `base` of the document.
fn variable_uses(text: &str, base: usize, uses: &mut Vec<Use>) {
    for (index, _) in text.match_indices(VARIABLE_PREFIX) {
        if text[..index].ends_with(is_identifier) {
            continue;
        }
        let rest = &text[index + VARIABLE_PREFIX.len()..];
        let identifier = &rest[..rest.find(|c| !is_identifier(c)).unwrap_or(rest.len())];
        let name = VARIABLE_SUFFIXES
            .iter()
            .find_map(|suffix| identifier.strip_suffix(suffix));
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            uses.push(Use {
                reference: Reference::Variable {
                    variable: format!("{VARIABLE_PREFIX}{identifier}"),
                    name: name.to_owned(),
                },
                offset: base + index,
                fills: true,
            });
        }
    }
}

/// `components.<name>.` keys of `craftctl` commands in `text`, which starts
/// at byte `base` of the document.
fn craftctl_uses(text: &str, base: usize, uses: &mut Vec<Use>) {
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        if let Some(command) = line.find("craftctl") {
            let after = command + "craftctl".len();
            let words = line[after..].split_whitespace();
            for word in words.skip(1) {
                let Some(rest) = word.strip_prefix("components.") else {
                    continue;
                };
                let Some(name) = rest.split('.').next().filter(|name| !name.is_empty()) else {
                    continue;
                };
                let index = line[after..].find(word).map_or(after, |i| after + i);
                uses.push(Use {
                    reference: Reference::Name(name.to_owned()),
                    offset: base + line_start + index + "components.".len(),
                    fills: false,
                });
            }
        }
        line_start += line.len();
    }
}

/// The Levenshtein distance between `a` and `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}
