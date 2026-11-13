// SPDX-License-Identifier: GPL-3.0-only

//! The lint check framework and registry.

mod collapsed_sequences;
mod components;
mod line_length;
mod project_file_location;
mod redundant_platforms;
mod sorted_lists;

use std::fmt;

use crate::config::{CheckOptions, OptionValue, Settings};
use crate::craft::Craft;
use crate::error::Error;
use crate::fix::{self, Edit};
use crate::nolint::Nolint;
use crate::project::Project;
use crate::yaml::Document;

/// Upper bound on fixing passes, in case fixes of different checks keep
/// undoing each other.
const MAX_FIX_PASSES: usize = 10;

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
    /// Why `--fix` cannot fix the finding, if the check knows; reported
    /// for findings that remain after fixing.
    pub unfixable: Option<String>,
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
            unfixable: None,
        }
    }

    /// Sets the 1-based line and column of the finding.
    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.location = Some((line, column));
        self
    }

    /// Records why the finding cannot be fixed automatically.
    pub fn unfixable(mut self, reason: impl Into<String>) -> Self {
        self.unfixable = Some(reason.into());
        self
    }
}

/// Everything a check can inspect.
pub struct Context<'a> {
    pub project: &'a Project,
    pub options: &'a CheckOptions,
    /// The `NOLINT` comments of the project file.
    pub nolint: &'a Nolint,
}

impl Context<'_> {
    /// Applies `edits` to the project file like [`fix::apply_edits`], except
    /// for edits touching a line where `check` is suppressed by a `NOLINT`
    /// comment. Checks must use this rather than `fix::apply_edits` so that
    /// `--fix` leaves suppressed lines alone.
    pub fn apply_edits<T>(
        &self,
        check: &str,
        edits: Vec<(Edit, T)>,
        accept: impl FnMut(&Document, &Document, &[&T]) -> bool,
    ) -> Option<Document> {
        let document = &self.project.document;
        let edits = edits
            .into_iter()
            .filter(|(edit, _)| {
                let first = document.line_index(edit.range.start);
                let last = document.line_index(edit.range.end);
                !self.nolint.suppresses_any(check, first..=last)
            })
            .collect();
        fix::apply_edits(document, edits, accept)
    }
}

/// The type of a check option's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Bool,
    Integer,
    /// A float; integers are accepted too.
    Float,
    String,
}

impl OptionKind {
    /// Whether `value` has this type.
    pub fn accepts(self, value: &OptionValue) -> bool {
        matches!(
            (self, value),
            (Self::Bool, OptionValue::Bool(_))
                | (Self::Integer, OptionValue::Integer(_))
                | (Self::Float, OptionValue::Float(_) | OptionValue::Integer(_))
                | (Self::String, OptionValue::String(_))
        )
    }
}

impl fmt::Display for OptionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bool => "boolean",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::String => "string",
        })
    }
}

/// Describes an option accepted by a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    pub name: &'static str,
    pub kind: OptionKind,
    /// The default value, as shown by `--explain`.
    pub default: &'static str,
    pub description: &'static str,
}

/// A lint check.
///
/// Check names are kebab-case and prefixed with their category, e.g.
/// `style-line-length`, so that users can select them with globs.
pub trait Check {
    fn name(&self) -> &'static str;
    /// One-line description, shown by `--list-checks`.
    fn summary(&self) -> &'static str;
    /// Detailed description, shown by `--explain`.
    fn explanation(&self) -> &'static str;
    /// Options accepted by the check. Values of other options, or of the
    /// wrong type, are rejected before any check runs.
    fn options(&self) -> &'static [OptionSpec] {
        &[]
    }
    /// Validates an option value whose type is already known to be correct,
    /// returning a description of the problem if it is invalid.
    fn validate_option(&self, _name: &str, _value: &OptionValue) -> Result<(), String> {
        Ok(())
    }
    /// The craft tools whose project files the check applies to, or `None`
    /// (the default) for all project files, including files whose tool is
    /// unknown.
    fn crafts(&self) -> Option<&'static [Craft]> {
        None
    }
    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic>;
    /// Fixes as many of the reported problems as possible, returning the
    /// fixed document, or `None` if nothing could be fixed (the runner also
    /// treats a returned document equal to the input as no change). Implementations
    /// should use [`crate::fix::apply_edits`] so that each edit is verified.
    fn fix(&self, _ctx: &Context<'_>) -> Option<Document> {
        None
    }
}

/// All available checks, sorted by name.
pub fn registry() -> Vec<Box<dyn Check>> {
    let mut checks: Vec<Box<dyn Check>> = vec![
        Box::new(collapsed_sequences::CollapsedSequences),
        Box::new(components::Components),
        Box::new(line_length::LineLength),
        Box::new(project_file_location::ProjectFileLocation),
        Box::new(redundant_platforms::RedundantPlatforms),
        Box::new(sorted_lists::SortedLists),
    ];
    checks.sort_by_key(|check| check.name());
    checks
}

/// Validates `options` against the options declared by the checks in
/// `registry`. Options of unknown checks are not an error, since the check
/// may come from a newer version, but each such check is returned so that
/// it can be reported.
pub fn validate_options<'a>(
    registry: &[Box<dyn Check>],
    options: &'a CheckOptions,
) -> Result<Vec<&'a str>, Error> {
    let mut unknown_checks = Vec::new();
    for (check_name, name, value) in options.iter() {
        let Some(check) = registry.iter().find(|check| check.name() == check_name) else {
            if unknown_checks.last() != Some(&check_name) {
                unknown_checks.push(check_name);
            }
            continue;
        };
        let invalid = |message: String| Error::InvalidOption {
            check: check_name.to_owned(),
            option: name.to_owned(),
            message,
        };
        let Some(spec) = check.options().iter().find(|spec| spec.name == name) else {
            let known: Vec<_> = check.options().iter().map(|spec| spec.name).collect();
            return Err(invalid(if known.is_empty() {
                "the check has no options".to_owned()
            } else {
                format!("unknown option (available: {})", known.join(", "))
            }));
        };
        if !spec.kind.accepts(value) {
            return Err(invalid(format!("expected a value of type {}", spec.kind)));
        }
        check.validate_option(name, value).map_err(invalid)?;
    }
    Ok(unknown_checks)
}

/// Checks from `checks` that are enabled by `settings`.
pub fn enabled<'a>(checks: &'a [Box<dyn Check>], settings: &Settings) -> Vec<&'a dyn Check> {
    checks
        .iter()
        .map(Box::as_ref)
        .filter(|check| settings.checks.matches(check.name()))
        .collect()
}

/// Whether `check` applies to project files of the tool `craft`.
pub fn applies_to(check: &dyn Check, craft: Option<Craft>) -> bool {
    match check.crafts() {
        None => true,
        Some(crafts) => craft.is_some_and(|craft| crafts.contains(&craft)),
    }
}

/// Runs those of `checks` that apply to `project` and applies
/// `warnings-as-errors`.
pub fn run(checks: &[&dyn Check], project: &Project, settings: &Settings) -> Vec<Diagnostic> {
    let nolint = Nolint::parse(&project.document);
    let ctx = Context {
        project,
        options: &settings.check_options,
        nolint: &nolint,
    };
    let mut diagnostics: Vec<Diagnostic> = checks
        .iter()
        .filter(|check| applies_to(**check, project.craft))
        .flat_map(|check| check.run(&ctx))
        .filter(|diagnostic| match diagnostic.location {
            Some((line, _)) => !nolint.suppresses(diagnostic.check, line),
            None => true,
        })
        .collect();
    diagnostics.extend(nolint.problems().iter().cloned());
    for diagnostic in &mut diagnostics {
        if settings.warnings_as_errors.matches(diagnostic.check) {
            diagnostic.severity = Severity::Error;
        }
    }
    diagnostics.sort_by_key(|d| (d.location, d.check));
    diagnostics
}

/// Applies the fixes of `checks` to `project` and returns the fixed project,
/// or `None` if nothing changed.
///
/// Checks fix in turn, each seeing the result of the previous ones. Passes
/// are repeated until none of the checks changes anything, since one fix can
/// enable another (e.g. a list made shorter by one check now fits on a line).
pub fn fix(checks: &[&dyn Check], project: &Project, settings: &Settings) -> Option<Project> {
    let mut current = project.clone();
    for _ in 0..MAX_FIX_PASSES {
        let mut changed = false;
        for check in checks
            .iter()
            .filter(|check| applies_to(**check, project.craft))
        {
            // Edits move lines, so the comments are found again each time.
            let nolint = Nolint::parse(&current.document);
            let ctx = Context {
                project: &current,
                options: &settings.check_options,
                nolint: &nolint,
            };
            if let Some(document) = check.fix(&ctx)
                && document.source() != current.source()
            {
                current.document = document;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (current.source() != project.source()).then_some(current)
}
