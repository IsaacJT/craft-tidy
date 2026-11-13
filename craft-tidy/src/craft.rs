// SPDX-License-Identifier: GPL-3.0-only

//! The craft tools whose project files craft-tidy understands.

use std::fmt;
use std::path::Path;

/// A craft tool, such as Snapcraft or Rockcraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Craft {
    Charmcraft,
    Debcraft,
    Imagecraft,
    Rockcraft,
    Snapcraft,
}

impl Craft {
    /// All supported tools, in alphabetical order.
    pub const ALL: [Craft; 5] = [
        Craft::Charmcraft,
        Craft::Debcraft,
        Craft::Imagecraft,
        Craft::Rockcraft,
        Craft::Snapcraft,
    ];

    /// The name of the tool's command, e.g. `snapcraft`.
    pub fn name(self) -> &'static str {
        match self {
            Craft::Charmcraft => "charmcraft",
            Craft::Debcraft => "debcraft",
            Craft::Imagecraft => "imagecraft",
            Craft::Rockcraft => "rockcraft",
            Craft::Snapcraft => "snapcraft",
        }
    }

    /// Locations of the tool's project file relative to the project
    /// directory, in the order the tool searches them; it uses the first one
    /// found.
    ///
    /// Tools built on craft-application only read `<name>.yaml` in the
    /// project directory; Snapcraft also looks in other places
    /// (`snapcraft/parts/yaml_utils.py`).
    pub fn project_file_locations(self) -> &'static [&'static str] {
        match self {
            Craft::Charmcraft => &["charmcraft.yaml"],
            Craft::Debcraft => &["debcraft.yaml"],
            Craft::Imagecraft => &["imagecraft.yaml"],
            Craft::Rockcraft => &["rockcraft.yaml"],
            Craft::Snapcraft => &[
                "snapcraft.yaml",
                "snap/snapcraft.yaml",
                "build-aux/snap/snapcraft.yaml",
                ".snapcraft.yaml",
            ],
        }
    }

    /// The tool whose project file has the name of `path`, if any.
    pub fn from_path(path: &Path) -> Option<Craft> {
        let name = path.file_name()?.to_str()?;
        Craft::ALL.into_iter().find(|craft| {
            craft
                .project_file_locations()
                .iter()
                .any(|location| location.rsplit('/').next() == Some(name))
        })
    }
}

impl fmt::Display for Craft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
