// SPDX-License-Identifier: GPL-3.0-only

//! Configuration settings and the `.craft-tidy` file.
//!
//! A [`Settings`] value is a single configuration layer. The effective
//! configuration is built by merging layers in order of increasing precedence:
//! built-in defaults, the `.craft-tidy` file, inline `--config` YAML and
//! finally individual command line flags. When merging, glob lists are
//! appended (so later globs win, as in clang-tidy), scalar settings set in a
//! later layer replace earlier ones, and check options are overridden key by
//! key.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserializer, Unexpected, Visitor};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::glob::GlobList;

/// Name of the per-project configuration file.
pub const CONFIG_FILE_NAME: &str = ".craft-tidy";

/// A scalar value for a check option.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum OptionValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
}

impl<'de> Deserialize<'de> for OptionValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OptionValueVisitor;

        impl Visitor<'_> for OptionValueVisitor {
            type Value = OptionValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a boolean, integer, float or string")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<OptionValue, E> {
                Ok(OptionValue::Bool(v))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<OptionValue, E> {
                Ok(OptionValue::Integer(v))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<OptionValue, E> {
                i64::try_from(v)
                    .map(OptionValue::Integer)
                    .map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<OptionValue, E> {
                Ok(OptionValue::Float(v))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<OptionValue, E> {
                Ok(OptionValue::String(v.to_owned()))
            }
        }

        deserializer.deserialize_any(OptionValueVisitor)
    }
}

impl OptionValue {
    /// Interprets `s` as a YAML scalar, falling back to a plain string, so
    /// that `79` is an integer, `true` a boolean and `'79'` a string.
    pub fn parse(s: &str) -> Self {
        if s.trim().is_empty() {
            return Self::String(s.to_owned());
        }
        serde_saphyr::from_str(s).unwrap_or_else(|_| Self::String(s.to_owned()))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Per-check options, keyed by check name and then option name.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct CheckOptions(BTreeMap<String, BTreeMap<String, OptionValue>>);

impl CheckOptions {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, check: &str, key: &str) -> Option<&OptionValue> {
        self.0.get(check)?.get(key)
    }

    pub fn set(&mut self, check: impl Into<String>, key: impl Into<String>, value: OptionValue) {
        self.0
            .entry(check.into())
            .or_default()
            .insert(key.into(), value);
    }

    /// Names of all checks that have options set.
    pub fn checks(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// All options as `(check, option, value)`, sorted by check and option.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, &OptionValue)> {
        self.0.iter().flat_map(|(check, options)| {
            options
                .iter()
                .map(move |(name, value)| (check.as_str(), name.as_str(), value))
        })
    }

    /// Merges `other` into `self`; options in `other` take precedence.
    pub fn merge(&mut self, other: CheckOptions) {
        for (check, options) in other.0 {
            self.0.entry(check).or_default().extend(options);
        }
    }
}

/// One layer of configuration, as read from `.craft-tidy`, `--config` or the
/// command line flags.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    /// Globs selecting which checks run.
    pub checks: GlobList,
    /// Globs selecting which checks report errors instead of warnings.
    pub warnings_as_errors: GlobList,
    /// Whether to fix problems in the project file where possible.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_bool"
    )]
    pub fix: Option<bool>,
    /// Options for individual checks.
    pub check_options: CheckOptions,
}

impl Settings {
    /// The built-in defaults: every check enabled, nothing treated as an
    /// error, nothing fixed.
    pub fn defaults() -> Self {
        Self {
            checks: GlobList::all(),
            fix: Some(false),
            ..Self::default()
        }
    }

    /// Parses a YAML configuration layer. `source_name` is used in error
    /// messages. An empty document yields an empty layer.
    pub fn from_yaml(yaml: &str, source_name: &str) -> Result<Self, Error> {
        serde_saphyr::from_str::<Option<Self>>(yaml)
            .map(Option::unwrap_or_default)
            .map_err(|err| {
                let rendered = err.render_with_options(serde_saphyr::render_options! {
                    source_name: Some(source_name),
                });
                Error::Config {
                    source: source_name.to_owned(),
                    details: rendered
                        .strip_prefix("error: ")
                        .unwrap_or(&rendered)
                        .to_owned(),
                }
            })
    }

    /// Reads a YAML configuration layer from `path`.
    pub fn from_file(path: &Path) -> Result<Self, Error> {
        let yaml = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::from_yaml(&yaml, &path.display().to_string())
    }

    /// Serializes these settings in `.craft-tidy` format.
    pub fn to_yaml(&self) -> Result<String, Error> {
        serde_saphyr::to_string(self).map_err(|err| Error::Serialize(err.to_string()))
    }

    /// Merges a higher-precedence layer into `self`.
    pub fn merge(&mut self, other: Settings) {
        self.checks.extend(other.checks);
        self.warnings_as_errors.extend(other.warnings_as_errors);
        if other.fix.is_some() {
            self.fix = other.fix;
        }
        self.check_options.merge(other.check_options);
    }

    /// Whether fixes should be applied.
    pub fn fix_enabled(&self) -> bool {
        self.fix == Some(true)
    }
}

/// Deserializes a boolean or null. Unlike `Option<bool>`, this rejects
/// strings such as `'true'` instead of converting them.
fn deserialize_optional_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    struct OptionalBoolVisitor;

    impl Visitor<'_> for OptionalBoolVisitor {
        type Value = Option<bool>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("true or false")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Option<bool>, E> {
            Ok(Some(v))
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<bool>, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<bool>, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(OptionalBoolVisitor)
}

/// Searches `start` and its ancestors for a `.craft-tidy` file.
pub fn discover(start: &Path) -> Option<PathBuf> {
    let start = std::path::absolute(start).ok()?;
    start
        .ancestors()
        .map(|dir| dir.join(CONFIG_FILE_NAME))
        .find(|candidate| candidate.is_file())
}
