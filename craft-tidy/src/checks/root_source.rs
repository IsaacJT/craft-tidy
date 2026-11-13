// SPDX-License-Identifier: GPL-3.0-only

//! `project-root-source`: parts should not use the whole project directory
//! as their source.

use crate::checks::{Check, Context, Diagnostic};
use crate::craft::Craft;
use crate::yaml::{Node, NodeKind};

const NAME: &str = "project-root-source";

pub struct RootSource;

impl Check for RootSource {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "Parts should not use the whole project directory (source: .) as their source"
    }

    fn explanation(&self) -> &'static str {
        "Reports parts whose source is the project directory itself, such as \
         source: . or source: ./.\n\
         \n\
         Such a part's source is everything in the project directory, \
         including build artifacts and other generated files (for example \
         target/, build/, node_modules/, .git/ or component files) and the \
         packaging itself. Only the tool's own work directories and \
         packages (such as *.snap) are excluded. When building with \
         --destructive-mode, where the project directory is also the build \
         directory, any of these files changing makes the part look out of \
         date, so it is pulled and built again; and all of them are copied \
         into the part's source directory, which is slow for large \
         projects.\n\
         \n\
         Point source at the directory that contains the code instead, such \
         as source: src, keeping the packaging and build output out of it.\n\
         \n\
         Does not apply to Charmcraft, whose charm plugin builds the charm \
         from the project directory by design. Cannot be fixed \
         automatically."
    }

    fn crafts(&self) -> Option<&'static [Craft]> {
        Some(&[
            Craft::Debcraft,
            Craft::Imagecraft,
            Craft::Rockcraft,
            Craft::Snapcraft,
        ])
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let document = &ctx.project.document;
        let Some(root) = document.root() else {
            return Vec::new();
        };
        let Some(NodeKind::Mapping { entries: parts, .. }) = get(root, "parts").map(|p| &p.kind)
        else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();
        for (name, part) in parts {
            let Some(source) = get(part, "source") else {
                continue;
            };
            let name = name.as_scalar().unwrap_or_default();
            let mut roots = Vec::new();
            root_sources(source, &mut roots);
            for node in roots {
                let (line, column) = document.position(node.span.start);
                let value = node.as_scalar().unwrap_or_default();
                diagnostics.push(
                    Diagnostic::warning(
                        NAME,
                        format!(
                            "part '{name}' uses the whole project directory as its source \
                             ('{value}'), so it picks up build artifacts there, which with \
                             --destructive-mode trigger rebuilds; use the directory with the \
                             code instead, such as 'src'"
                        ),
                    )
                    .at(line, column)
                    .unfixable("craft-tidy cannot know which directory holds the code"),
                );
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

/// The scalars in `source` (a string, or advanced grammar) that name the
/// project directory.
fn root_sources<'a>(source: &'a Node, found: &mut Vec<&'a Node>) {
    match &source.kind {
        NodeKind::Scalar { value, .. } => {
            if is_project_root(value) {
                found.push(source);
            }
        }
        NodeKind::Sequence { items, .. } => {
            for item in items {
                root_sources(item, found);
            }
        }
        NodeKind::Mapping { entries, .. } => {
            for (_, value) in entries {
                root_sources(value, found);
            }
        }
        NodeKind::Alias => {}
    }
}

/// Whether the relative path `path` is the directory it is relative to,
/// such as `.`, `./` or `./.`.
fn is_project_root(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path.split('/').all(|part| part.is_empty() || part == ".")
}
