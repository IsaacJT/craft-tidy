// SPDX-License-Identifier: GPL-3.0-only

//! Top-level program logic.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::checks::{self, Check, Severity};
use crate::cli::Cli;
use crate::config::{self, Settings};
use crate::craft::Craft;
use crate::error::Error;
use crate::project::Project;

/// Outcome of a successful run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Nothing was reported at error severity.
    Success,
    /// At least one finding was reported as an error.
    LintErrors,
}

/// Runs craft-tidy with the given arguments, writing results to `out` and
/// warnings about the invocation itself to `err`.
pub fn run(cli: &Cli, out: &mut dyn Write, err: &mut dyn Write) -> Result<Status, Error> {
    run_with_checks(cli, &checks::registry(), out, err)
}

/// Like [`run`], but with an explicit set of available checks.
pub fn run_with_checks(
    cli: &Cli,
    registry: &[Box<dyn Check>],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Status, Error> {
    let settings = resolve_settings(cli)?;

    for name in checks::validate_options(registry, &settings.check_options)? {
        writeln!(
            err,
            "craft-tidy: warning: options given for unknown check '{name}'"
        )
        .map_err(Error::Output)?;
    }

    if cli.dump_config {
        out.write_all(settings.to_yaml()?.as_bytes())
            .map_err(Error::Output)?;
        return Ok(Status::Success);
    }

    let enabled = checks::enabled(registry, &settings);

    if cli.list_checks {
        list_checks(&enabled, out).map_err(Error::Output)?;
        return Ok(Status::Success);
    }

    if let Some(name) = &cli.explain {
        let check = registry
            .iter()
            .find(|check| check.name() == name)
            .ok_or_else(|| Error::UnknownCheck(name.clone()))?;
        explain(check.as_ref(), out).map_err(Error::Output)?;
        return Ok(Status::Success);
    }

    let projects = Project::load_all(&cli.path)?;
    if enabled.is_empty() {
        writeln!(err, "craft-tidy: warning: no checks enabled").map_err(Error::Output)?;
    }

    let mut status = Status::Success;
    for project in projects {
        if lint(project, &enabled, &settings, out, err)? == Status::LintErrors {
            status = Status::LintErrors;
        }
    }
    Ok(status)
}

/// Lints (and with `fix` enabled, fixes) a single project file.
fn lint(
    mut project: Project,
    enabled: &[&dyn Check],
    settings: &Settings,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Status, Error> {
    let mut diagnostics = checks::run(enabled, &project, settings);
    if settings.fix_enabled()
        && !diagnostics.is_empty()
        && let Some(fixed) = checks::fix(enabled, &project, settings)
    {
        fs::write(&fixed.path, fixed.source()).map_err(|source| Error::Io {
            path: fixed.path.clone(),
            source,
        })?;
        let remaining = checks::run(enabled, &fixed, settings);
        writeln!(
            err,
            "craft-tidy: applied fixes to {} ({} issue(s) found, {} remaining)",
            fixed.path.display(),
            diagnostics.len(),
            remaining.len()
        )
        .map_err(Error::Output)?;
        project = fixed;
        diagnostics = remaining;
    }

    for diagnostic in &diagnostics {
        let path = project.path.display();
        let location = match diagnostic.location {
            Some((line, column)) => format!("{path}:{line}:{column}"),
            None => path.to_string(),
        };
        let note = match &diagnostic.unfixable {
            Some(reason) if settings.fix_enabled() => format!(" (cannot be fixed: {reason})"),
            _ => String::new(),
        };
        writeln!(
            out,
            "{location}: {}: {}{note} [{}]",
            diagnostic.severity, diagnostic.message, diagnostic.check
        )
        .map_err(Error::Output)?;
    }

    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        Ok(Status::LintErrors)
    } else {
        Ok(Status::Success)
    }
}

/// Builds the effective configuration from all sources, in order of
/// increasing precedence.
pub fn resolve_settings(cli: &Cli) -> Result<Settings, Error> {
    let mut settings = Settings::defaults();

    let config_file = if cli.no_config {
        None
    } else if let Some(path) = &cli.config_file {
        Some(path.clone())
    } else {
        config::discover(&search_start(&cli.path))
    };
    if let Some(path) = config_file {
        settings.merge(Settings::from_file(&path)?);
    }

    if let Some(yaml) = &cli.config {
        settings.merge(Settings::from_yaml(yaml, "--config")?);
    }

    settings.merge(cli.settings());
    Ok(settings)
}

/// The directory where the search for `.craft-tidy` starts.
fn search_start(path: &Path) -> PathBuf {
    if path.is_file() {
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_owned(),
            _ => PathBuf::from("."),
        }
    } else {
        path.to_owned()
    }
}

fn explain(check: &dyn Check, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        out,
        "{}: {}\n\n{}",
        check.name(),
        check.summary(),
        check.explanation()
    )?;
    if let Some(crafts) = check.crafts() {
        writeln!(out, "\nApplies to: {}", craft_names(crafts))?;
    }
    if !check.options().is_empty() {
        writeln!(out, "\nOptions:")?;
        for spec in check.options() {
            writeln!(
                out,
                "  {} ({}, default: {})\n      {}",
                spec.name, spec.kind, spec.default, spec.description
            )?;
        }
    }
    Ok(())
}

fn list_checks(enabled: &[&dyn Check], out: &mut dyn Write) -> std::io::Result<()> {
    if enabled.is_empty() {
        return writeln!(out, "No checks enabled.");
    }
    let width = enabled.iter().map(|c| c.name().len()).max().unwrap_or(0);
    writeln!(out, "Enabled checks:")?;
    for check in enabled {
        let only = check
            .crafts()
            .map(|crafts| format!(" ({} only)", craft_names(crafts)))
            .unwrap_or_default();
        writeln!(
            out,
            "    {:width$}  {}{only}",
            check.name(),
            check.summary()
        )?;
    }
    Ok(())
}

fn craft_names(crafts: &[Craft]) -> String {
    crafts
        .iter()
        .map(|craft| craft.name())
        .collect::<Vec<_>>()
        .join(", ")
}
