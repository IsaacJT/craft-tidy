// SPDX-License-Identifier: GPL-3.0-only

//! Helpers shared by the test crates in this directory.

// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::cell::Cell;
use std::fs;
use std::rc::Rc;

use clap::Parser;
use craft_tidy::checks::{Check, Context, Diagnostic};
use craft_tidy::cli::Cli;
use tempfile::TempDir;

/// Creates a temporary project containing `snap/snapcraft.yaml` and, if
/// given, a `.craft-tidy` file.
pub fn project(config: Option<&str>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("snap")).unwrap();
    fs::write(dir.path().join("snap/snapcraft.yaml"), "name: test\n").unwrap();
    if let Some(config) = config {
        fs::write(dir.path().join(".craft-tidy"), config).unwrap();
    }
    dir
}

/// Parses command line arguments (without the program name).
pub fn try_cli(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("craft-tidy").chain(args.iter().copied()))
}

/// Parses command line arguments that are expected to be valid.
pub fn cli(args: &[&str]) -> Cli {
    try_cli(args).unwrap_or_else(|err| panic!("{args:?} should be valid: {err}"))
}

/// A check with predefined findings, for exercising the check framework.
///
/// If the option `<name>.extra` is set to a string, an additional finding
/// with that message is reported, which lets tests observe check options.
pub struct FakeCheck {
    pub name: &'static str,
    pub summary: &'static str,
    pub findings: Vec<(&'static str, Option<(usize, usize)>)>,
    pub runs: Rc<Cell<usize>>,
}

impl FakeCheck {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            summary: "A fake check",
            findings: Vec::new(),
            runs: Rc::default(),
        }
    }

    pub fn finding(mut self, message: &'static str, location: Option<(usize, usize)>) -> Self {
        self.findings.push((message, location));
        self
    }

    pub fn boxed(self) -> Box<dyn Check> {
        Box::new(self)
    }
}

impl Check for FakeCheck {
    fn name(&self) -> &'static str {
        self.name
    }

    fn summary(&self) -> &'static str {
        self.summary
    }

    fn explanation(&self) -> &'static str {
        "Reports predefined findings."
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        self.runs.set(self.runs.get() + 1);
        let mut diagnostics: Vec<Diagnostic> = self
            .findings
            .iter()
            .map(|&(message, location)| {
                let diagnostic = Diagnostic::warning(self.name, message);
                match location {
                    Some((line, column)) => diagnostic.at(line, column),
                    None => diagnostic,
                }
            })
            .collect();
        if let Some(extra) = ctx.options.get(self.name, "extra").and_then(|v| v.as_str()) {
            diagnostics.push(Diagnostic::warning(self.name, extra));
        }
        diagnostics
    }
}
