// SPDX-License-Identifier: GPL-3.0-only

//! `project-file-location`: Snapcraft's project file belongs in the `snap/`
//! directory rather than the project root.

use std::path::Path;

use crate::checks::{Check, Context, Diagnostic};
use crate::craft::Craft;

const NAME: &str = "project-file-location";

pub struct ProjectFileLocation;

impl Check for ProjectFileLocation {
    fn name(&self) -> &'static str {
        NAME
    }

    fn summary(&self) -> &'static str {
        "The Snapcraft project file must be in the snap/ directory, not the project root"
    }

    fn explanation(&self) -> &'static str {
        "Reports a Snapcraft project file (snapcraft.yaml or .snapcraft.yaml) \
         that is in the root of the project. Keep it at snap/snapcraft.yaml \
         instead, next to the other snap-specific files such as snap/hooks, \
         snap/gui and snap/local, so that the packaging is kept apart from \
         the application's own files. build-aux/snap/snapcraft.yaml, which \
         Snapcraft supports for projects that use snap/ for something else, \
         is accepted as well.\n\
         \n\
         Moving the file does not change how the snap is built: Snapcraft \
         resolves paths such as source: . relative to the project root, \
         wherever the project file is.\n\
         \n\
         Only applies to Snapcraft: the other craft tools only read their \
         project file from the project root."
    }

    fn crafts(&self) -> Option<&'static [Craft]> {
        Some(&[Craft::Snapcraft])
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        let path = &ctx.project.path;
        // Make the path absolute so that e.g. `snapcraft.yaml` given from
        // within snap/ is recognised as being in snap/.
        let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        let in_snap_directory = path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|directory| directory == "snap");
        if in_snap_directory {
            return Vec::new();
        }
        let name = path
            .file_name()
            .map_or_else(Default::default, |name| name.to_string_lossy());
        vec![Diagnostic::warning(
            NAME,
            format!("{name} is in the project root; move it to snap/snapcraft.yaml"),
        )]
    }
}
