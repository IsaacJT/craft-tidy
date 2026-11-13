// SPDX-License-Identifier: GPL-3.0-only

//! `metadata-license`: projects should declare their license.

use crate::checks::{Check, Context, Diagnostic};
use crate::craft::Craft;
use crate::yaml::{Node, NodeKind};

const NAME: &str = "metadata-license";

pub struct License;

impl Check for License {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "The project must declare its license"
    }

    fn explanation(&self) -> &'static str {
        "Reports projects without a top-level license key, or with an empty \
         one. Set it to the project's license as an SPDX expression, such as \
         license: GPL-3.0-only or license: Apache-2.0 OR MIT, or to \
         proprietary.\n\
         \n\
         Snapcraft puts the license in the snap's metadata, shown in the Snap \
         Store (and its own metadata linter warns about it after building); \
         Rockcraft records it in the OCI image annotation \
         org.opencontainers.image.licenses; Imagecraft and Debcraft record \
         it in the build manifest. A Snapcraft project with adopt-info is not \
         reported, as Snapcraft can take the license from the AppStream \
         metadata of that part.\n\
         \n\
         Does not apply to Charmcraft, which does not accept a license key \
         (a charm's license is in its LICENSE file). Cannot be fixed \
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
        let Some(
            root @ Node {
                kind: NodeKind::Mapping { entries, .. },
                ..
            },
        ) = document.root()
        else {
            return Vec::new();
        };
        let entry = |name: &str| {
            entries
                .iter()
                .find(|(key, _)| key.as_scalar() == Some(name))
        };

        let (span, message) = match entry("license") {
            Some((_, value)) if !is_empty(value) => return Vec::new(),
            Some((key, value)) => (
                key.span.start..value.span.end.max(key.span.end),
                "the license is empty",
            ),
            None => {
                if ctx.project.craft == Some(Craft::Snapcraft) && entry("adopt-info").is_some() {
                    return Vec::new();
                }
                // Report at the project's name, so that the finding can be
                // suppressed with a NOLINT comment there.
                let span = entry("name").map_or(root.span.start..root.span.start, |(key, _)| {
                    key.span.clone()
                });
                (span, "the project does not declare its license")
            }
        };
        vec![
            Diagnostic::warning(
                NAME,
                format!(
                    "{message}; add a top-level 'license' with an SPDX expression, such as \
                     'license: GPL-3.0-only', or 'license: proprietary'"
                ),
            )
            .spanning(document, span)
            .unfixable("craft-tidy cannot know the project's license"),
        ]
    }
}

/// Whether `value` is empty: null, or an empty string.
fn is_empty(value: &Node) -> bool {
    match &value.kind {
        NodeKind::Scalar { value: text, style } => {
            let plain = *style == crate::yaml::ScalarStyle::Plain;
            text.trim().is_empty()
                || (plain && matches!(text.as_str(), "~" | "null" | "Null" | "NULL"))
        }
        _ => false,
    }
}
