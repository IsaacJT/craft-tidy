// SPDX-License-Identifier: GPL-3.0-only

//! clang-tidy style check selection globs.
//!
//! A glob list is a comma-separated sequence of patterns such as
//! `-*,metadata-*,-metadata-icon`. `*` matches any (possibly empty) sequence
//! of characters and a leading `-` negates the pattern. Patterns are evaluated
//! in order and the last one matching a name decides whether it is selected;
//! names matched by no pattern are not selected.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserialize, Deserializer, SeqAccess, Visitor};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Glob {
    pattern: String,
    negated: bool,
}

/// An ordered list of check selection globs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobList {
    globs: Vec<Glob>,
}

/// Error returned when a glob list contains an invalid pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidGlob(String);

impl fmt::Display for InvalidGlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid check glob '{}': globs consist of lowercase letters, digits, '-' and '*', \
             optionally prefixed by a single '-' to disable matching checks",
            self.0
        )
    }
}

impl std::error::Error for InvalidGlob {}

impl GlobList {
    /// A list selecting every name (`*`).
    pub fn all() -> Self {
        Self {
            globs: vec![Glob {
                pattern: "*".to_owned(),
                negated: false,
            }],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.globs.is_empty()
    }

    /// Appends the globs of `other`, giving them precedence over existing ones.
    pub fn extend(&mut self, other: GlobList) {
        for glob in other.globs {
            self.push(glob);
        }
    }

    fn push(&mut self, glob: Glob) {
        // `*` and `-*` decide every name, so earlier globs can never match.
        if glob.pattern == "*" {
            self.globs.clear();
        }
        self.globs.push(glob);
    }

    /// Returns whether `name` is selected by this list.
    pub fn matches(&self, name: &str) -> bool {
        self.globs
            .iter()
            .rev()
            .find(|glob| wildcard_match(glob.pattern.as_bytes(), name.as_bytes()))
            .is_some_and(|glob| !glob.negated)
    }
}

impl FromStr for GlobList {
    type Err = InvalidGlob;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut list = Self::default();
        for entry in s.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (negated, pattern) = match entry.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, entry),
            };
            let valid = !pattern.is_empty()
                && !pattern.starts_with('-')
                && pattern
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-*".contains(&b));
            if !valid {
                return Err(InvalidGlob(entry.to_owned()));
            }
            list.push(Glob {
                pattern: pattern.to_owned(),
                negated,
            });
        }
        Ok(list)
    }
}

impl fmt::Display for GlobList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, glob) in self.globs.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            if glob.negated {
                f.write_str("-")?;
            }
            f.write_str(&glob.pattern)?;
        }
        Ok(())
    }
}

impl Serialize for GlobList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Accepts either a comma-separated string or a YAML list of strings (each of
/// which may itself be comma-separated). A null value is an empty list.
impl<'de> Deserialize<'de> for GlobList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GlobListVisitor;

        impl<'de> Visitor<'de> for GlobListVisitor {
            type Value = GlobList;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a comma-separated string or a list of check globs")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<GlobList, E> {
                v.parse().map_err(E::custom)
            }

            fn visit_unit<E: de::Error>(self) -> Result<GlobList, E> {
                Ok(GlobList::default())
            }

            fn visit_none<E: de::Error>(self) -> Result<GlobList, E> {
                Ok(GlobList::default())
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GlobList, A::Error> {
                let mut list = GlobList::default();
                while let Some(GlobEntry(entry)) = seq.next_element()? {
                    list.extend(entry);
                }
                Ok(list)
            }
        }

        deserializer.deserialize_any(GlobListVisitor)
    }
}

/// A single string entry of a YAML glob list. Unlike `String`, this rejects
/// non-string scalars such as numbers instead of converting them.
struct GlobEntry(GlobList);

impl<'de> Deserialize<'de> for GlobEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GlobEntryVisitor;

        impl Visitor<'_> for GlobEntryVisitor {
            type Value = GlobEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a check glob string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<GlobEntry, E> {
                v.parse().map(GlobEntry).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(GlobEntryVisitor)
    }
}

/// Matches `text` against `pattern`, where `*` matches any byte sequence.
fn wildcard_match(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(b'*') => {
                backtrack = Some((p, t));
                p += 1;
            }
            Some(&c) if c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match backtrack {
                Some((star, matched)) => {
                    p = star + 1;
                    t = matched + 1;
                    backtrack = Some((star, matched + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}
