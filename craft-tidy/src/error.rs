// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::yaml::SyntaxError;

#[derive(Debug)]
pub enum Error {
    /// Reading a file failed.
    Io { path: PathBuf, source: io::Error },
    /// Writing program output failed.
    Output(io::Error),
    /// A configuration source is invalid.
    Config { source: String, details: String },
    /// The effective configuration could not be serialized.
    Serialize(String),
    /// No snapcraft.yaml was found for the given path.
    ProjectNotFound(PathBuf),
    /// The project file is not valid YAML.
    Syntax { path: PathBuf, error: SyntaxError },
    /// A check name given by the user does not exist.
    UnknownCheck(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Output(source) => write!(f, "failed to write output: {source}"),
            Self::Config { source, details } => {
                write!(f, "invalid configuration in {source}: {details}")
            }
            Self::Serialize(message) => {
                write!(f, "failed to serialize configuration: {message}")
            }
            Self::ProjectNotFound(path) => {
                write!(f, "no snapcraft.yaml found in {}", path.display())
            }
            Self::Syntax { path, error } => write!(
                f,
                "{}:{}:{}: invalid YAML: {}",
                path.display(),
                error.line,
                error.column,
                error.message
            ),
            Self::UnknownCheck(name) => write!(
                f,
                "unknown check '{name}' (run `craft-tidy --list-checks --checks='*'` to list all checks)"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::Output(source) => Some(source),
            _ => None,
        }
    }
}
