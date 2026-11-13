// SPDX-License-Identifier: GPL-3.0-only

//! Locating and loading the Snapcraft project file.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::yaml::Document;

/// Locations of the project file relative to the project directory, in the
/// order Snapcraft searches them; the first one found is used.
pub const PROJECT_FILE_LOCATIONS: &[&str] = &[
    "snapcraft.yaml",
    "snap/snapcraft.yaml",
    "build-aux/snap/snapcraft.yaml",
    ".snapcraft.yaml",
];

/// A Snapcraft project file to be linted.
#[derive(Debug, Clone)]
pub struct Project {
    /// Path to the project file.
    pub path: PathBuf,
    /// The parsed project file.
    pub document: Document,
}

impl Project {
    /// Parses `source` as the contents of the project file at `path`.
    pub fn new(path: impl Into<PathBuf>, source: impl Into<String>) -> Result<Self, Error> {
        let path = path.into();
        match Document::parse(source) {
            Ok(document) => Ok(Self { path, document }),
            Err(error) => Err(Error::Syntax { path, error }),
        }
    }

    /// Loads the project file at `path`, or found in the directory `path`.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let path = find(path)?;
        let source = fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Self::new(path, source)
    }

    /// Contents of the project file.
    pub fn source(&self) -> &str {
        self.document.source()
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
