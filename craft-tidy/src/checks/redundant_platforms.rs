// SPDX-License-Identifier: GPL-3.0-only

//! `style-redundant-platforms`: platforms that build on and for the
//! architecture they are named after need no `build-on` or `build-for`.

use crate::checks::{Check, Context, Diagnostic};
use crate::fix::{Edit, kept_edits};
use crate::yaml::{Document, Node, NodeKind, Value};

const NAME: &str = "style-redundant-platforms";

/// The architectures that a platform without a definition may be named
/// after (craft-platforms' `DebianArchitecture`).
const ARCHITECTURES: &[&str] = &[
    "amd64", "arm64", "armhf", "i386", "ppc64el", "riscv64", "s390x",
];

pub struct RedundantPlatforms;

impl Check for RedundantPlatforms {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Platforms that build on and for their own architecture need no definition"
    }

    fn explanation(&self) -> &'static str {
        "Reports platforms named after an architecture whose only keys are \
         build-on and build-for, both that same architecture (build-for may \
         also be left out, as it defaults to the platform's name):\n\
         \n\
         platforms:\n  \
           arm64:\n    \
             build-on: [arm64]\n    \
             build-for: [arm64]\n\
         \n\
         A platform named after an architecture with no definition at all \
         (arm64:) means exactly that, for every craft tool, so the \
         definition is redundant.\n\
         \n\
         The architectures are amd64, arm64, armhf, i386, ppc64el, riscv64 \
         and s390x. Platforms with other names (such as Charmcraft's \
         ubuntu@24.04:amd64), other keys, several architectures or \
         comments are left alone.\n\
         \n\
         With --fix, the definition is removed, leaving arm64:."
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let document = &ctx.project.document;
        redundant(document)
            .into_iter()
            .map(|platform| {
                let (line, column) = document.position(platform.key_start);
                Diagnostic::warning(
                    NAME,
                    format!(
                        "platform '{0}' only builds on and for {0}, which is what \
                         '{0}:' alone means; remove its build-on and build-for",
                        platform.name
                    ),
                )
                .at(line, column)
            })
            .collect()
    }

    fn fix(&self, ctx: &Context<'_>) -> Option<Document> {
        let edits = redundant(&ctx.project.document)
            .into_iter()
            .map(|platform| (platform.edit, platform.path))
            .collect();
        ctx.apply_edits(NAME, edits, |before, after, paths| {
            after.value()
                == before
                    .value()
                    .and_then(|value| without_definitions(value, paths))
        })
    }
}

/// `value` with the platforms at `paths` left undefined (an empty value,
/// which the parser reports as the null `~`).
fn without_definitions(mut value: Value, paths: &[&Vec<usize>]) -> Option<Value> {
    for path in paths {
        *value.get_mut(path)? = Value::Scalar {
            value: "~".to_owned(),
            plain: true,
        };
    }
    Some(value)
}

/// A platform with a redundant definition.
struct Platform {
    name: String,
    key_start: usize,
    /// Path of the definition in the document's [`Value`].
    path: Vec<usize>,
    /// Edit removing the definition.
    edit: Edit,
}

/// The platforms with redundant definitions whose removal verifies.
fn redundant(document: &Document) -> Vec<Platform> {
    let Some(Node {
        kind: NodeKind::Mapping { entries, .. },
        ..
    }) = document.root()
    else {
        return Vec::new();
    };
    let Some((index, platforms)) = entries
        .iter()
        .enumerate()
        .find(|(_, (key, _))| key.as_scalar() == Some("platforms"))
        .map(|(index, (_, value))| (index, value))
    else {
        return Vec::new();
    };
    let NodeKind::Mapping { entries, .. } = &platforms.kind else {
        return Vec::new();
    };
    let mut found: Vec<Platform> = entries
        .iter()
        .enumerate()
        .filter_map(|(position, (key, definition))| {
            let name = key.as_scalar().filter(|_| !key.decorated)?;
            if !ARCHITECTURES.contains(&name) || !is_redundant(name, definition) {
                return None;
            }
            // Remove everything from just after the key's `:` to the end of
            // the definition, unless that would lose comments.
            let source = document.source();
            let colon = source[..definition.span.start]
                .trim_end_matches([' ', '\t', '\n', '\r'])
                .strip_suffix(':')?
                .len();
            let last_line = document.line_index(definition.span.end.max(1) - 1);
            let end = document.line_range(last_line).end;
            let has_comment = document
                .comments()
                .iter()
                .any(|comment| (colon..end).contains(&comment.start));
            if has_comment {
                return None;
            }
            Some(Platform {
                name: name.to_owned(),
                key_start: key.span.start,
                path: vec![index, position],
                edit: Edit::new(colon + 1..definition.span.end, ""),
            })
        })
        .collect();

    // Only report what the fix can do.
    let edits = found
        .iter()
        .map(|p| (p.edit.clone(), p.path.clone()))
        .collect();
    let (_, kept) = kept_edits(document, edits, |before, after, paths| {
        after.value()
            == before
                .value()
                .and_then(|value| without_definitions(value, paths))
    });
    let mut index = 0;
    found.retain(|_| {
        let keep = kept.binary_search(&index).is_ok();
        index += 1;
        keep
    });
    found
}

/// Whether `definition` only builds on and for `name`.
fn is_redundant(name: &str, definition: &Node) -> bool {
    let NodeKind::Mapping { entries, .. } = &definition.kind else {
        return false;
    };
    if definition.decorated {
        return false;
    }
    let mut build_on = None;
    let mut build_for = None;
    for (key, value) in entries {
        match key.as_scalar() {
            Some("build-on") if build_on.is_none() => build_on = Some(value),
            Some("build-for") if build_for.is_none() => build_for = Some(value),
            _ => return false,
        }
    }
    // build-on is required; build-for defaults to the platform's name.
    build_on.is_some_and(|value| only(name, value))
        && build_for.is_none_or(|value| only(name, value))
}

/// Whether `node` is `name`, or a list of just `name`.
fn only(name: &str, node: &Node) -> bool {
    if node.decorated {
        return false;
    }
    match &node.kind {
        NodeKind::Scalar { value, .. } => value == name,
        NodeKind::Sequence { items, .. } => {
            matches!(items.as_slice(), [item] if !item.decorated && item.as_scalar() == Some(name))
        }
        _ => false,
    }
}
