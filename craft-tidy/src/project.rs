// SPDX-License-Identifier: GPL-3.0-only

//! Locating and loading the Snapcraft project file.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Locations of the project file relative to the project directory, in the
/// order Snapcraft searches them.
pub const PROJECT_FILE_LOCATIONS: &[&str] = &[
    "snap/snapcraft.yaml",
    "snapcraft.yaml",
    ".snapcraft.yaml",
    "build-aux/snap/snapcraft.yaml",
];

/// A Snapcraft project file to be linted.
#[derive(Debug, Clone)]
pub struct Project {
    /// Path to the project file.
    pub path: PathBuf,
    /// Contents of the project file.
    pub source: String,
}

impl Project {
    /// Loads the project file at `path`, or found in the directory `path`.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let path = find(path)?;
        let source = fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(Self { path, source })
    }
}

/// Resolves `path` to a project file: either `path` itself if it is a file,
/// or the first project file found in the directory `path`.
pub fn find(path: &Path) -> Result<PathBuf, Error> {
    if path.is_file() {
        return Ok(path.to_owned());
    }
    PROJECT_FILE_LOCATIONS
        .iter()
        .map(|location| path.join(location))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| Error::ProjectNotFound(path.to_owned()))
}
