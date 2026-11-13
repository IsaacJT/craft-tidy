// SPDX-License-Identifier: GPL-3.0-only

//! Locating and loading craft project files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::craft::Craft;
use crate::error::Error;
use crate::yaml::Document;

/// A craft project file to be linted, such as `snapcraft.yaml`.
#[derive(Debug, Clone)]
pub struct Project {
    /// Path to the project file.
    pub path: PathBuf,
    /// The craft tool the file is for, identified by its name; `None` for a
    /// file with another name given explicitly.
    pub craft: Option<Craft>,
    /// The parsed project file.
    pub document: Document,
}

impl Project {
    /// Parses `source` as the contents of the project file at `path`.
    pub fn new(path: impl Into<PathBuf>, source: impl Into<String>) -> Result<Self, Error> {
        let path = path.into();
        match Document::parse(source) {
            Ok(document) => Ok(Self {
                craft: Craft::from_path(&path),
                path,
                document,
            }),
            Err(error) => Err(Error::Syntax { path, error }),
        }
    }

    /// Loads the project file at `path`.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let source = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::new(path, source)
    }

    /// Loads the project file at `path`, or all project files found in the
    /// directory `path` (see [`find`]).
    pub fn load_all(path: &Path) -> Result<Vec<Self>, Error> {
        find(path)?.iter().map(|path| Self::load(path)).collect()
    }

    /// Contents of the project file.
    pub fn source(&self) -> &str {
        self.document.source()
    }
}

/// Resolves `path` to project files: `path` itself if it is a file, or else
/// the project file of every craft tool that has one in the directory
/// `path`, in alphabetical order of the tools. For each tool, the file it
/// would use itself is chosen, following its search order.
pub fn find(path: &Path) -> Result<Vec<PathBuf>, Error> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    let found: Vec<PathBuf> = Craft::ALL
        .iter()
        .filter_map(|craft| {
            craft
                .project_file_locations()
                .iter()
                .map(|location| path.join(location))
                .find(|candidate| candidate.is_file())
        })
        .collect();
    if found.is_empty() {
        return Err(Error::ProjectNotFound(path.to_owned()));
    }
    Ok(found)
}
