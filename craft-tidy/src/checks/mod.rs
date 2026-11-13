// SPDX-License-Identifier: GPL-3.0-only

//! The lint check framework and registry.

use std::fmt;

use crate::config::{CheckOptions, Settings};
use crate::project::Project;

/// Severity of a reported finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// A finding reported by a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub check: &'static str,
    pub severity: Severity,
    pub message: String,
    /// 1-based line and column in the project file, if known.
    pub location: Option<(usize, usize)>,
}

impl Diagnostic {
    /// Creates a warning; it is promoted to an error if the check matches
    /// `warnings-as-errors`.
    pub fn warning(check: &'static str, message: impl Into<String>) -> Self {
        Self {
            check,
            severity: Severity::Warning,
            message: message.into(),
            location: None,
        }
    }

    /// Sets the 1-based line and column of the finding.
    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.location = Some((line, column));
        self
    }
}

/// Everything a check can inspect.
pub struct Context<'a> {
    pub project: &'a Project,
    pub options: &'a CheckOptions,
}

/// A lint check.
///
/// Check names are kebab-case and prefixed with their category, e.g.
/// `metadata-summary-length`, so that users can select them with globs.
pub trait Check {
    fn name(&self) -> &'static str;
    /// One-line description, shown by `--list-checks`.
    fn summary(&self) -> &'static str;
    /// Detailed description, shown by `--explain`.
    fn explanation(&self) -> &'static str;
    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic>;
}

/// All available checks, sorted by name.
pub fn registry() -> Vec<Box<dyn Check>> {
    let mut checks: Vec<Box<dyn Check>> = Vec::new();
    checks.sort_by_key(|check| check.name());
    checks
}

/// Checks from `checks` that are enabled by `settings`.
pub fn enabled<'a>(checks: &'a [Box<dyn Check>], settings: &Settings) -> Vec<&'a dyn Check> {
    checks
        .iter()
        .map(Box::as_ref)
        .filter(|check| settings.checks.matches(check.name()))
        .collect()
}

/// Runs `checks` on `project` and applies `warnings-as-errors`.
pub fn run(checks: &[&dyn Check], project: &Project, settings: &Settings) -> Vec<Diagnostic> {
    let ctx = Context {
        project,
        options: &settings.check_options,
    };
    let mut diagnostics: Vec<Diagnostic> =
        checks.iter().flat_map(|check| check.run(&ctx)).collect();
    for diagnostic in &mut diagnostics {
        if settings.warnings_as_errors.matches(diagnostic.check) {
            diagnostic.severity = Severity::Error;
        }
    }
    diagnostics.sort_by_key(|d| (d.location, d.check));
    diagnostics
}
