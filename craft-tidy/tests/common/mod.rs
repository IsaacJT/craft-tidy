// SPDX-License-Identifier: GPL-3.0-only

//! Helpers shared by the test crates in this directory.

// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::Parser;
use craft_tidy::checks::{Check, Context, Diagnostic, OptionKind, OptionSpec};
use craft_tidy::cli::Cli;
use craft_tidy::config::OptionValue;
use craft_tidy::craft::Craft;
use craft_tidy::fix::Edit;
use craft_tidy::yaml::Document;
use tempfile::TempDir;

/// Options declared by [`FakeCheck`].
pub const FAKE_OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        name: "extra",
        kind: OptionKind::String,
        default: "\"\"",
        description: "Report an additional finding with this message.",
    },
    OptionSpec {
        name: "ratio",
        kind: OptionKind::Float,
        default: "0.5",
        description: "Unused; must be between 0 and 1.",
    },
];

/// Path of the example configuration shipped in the repository.
pub fn example_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/.craft-tidy")
}

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

/// Whether PyYAML, the YAML parser used by Snapcraft, loads `a` and `b` as
/// the same data. Returns `None` if Python 3 with PyYAML is not available,
/// so tests can use it as an additional check where possible.
pub fn pyyaml_equal(a: &str, b: &str) -> Option<bool> {
    let dir = tempfile::tempdir().ok()?;
    let (path_a, path_b) = (dir.path().join("a.yaml"), dir.path().join("b.yaml"));
    fs::write(&path_a, a).ok()?;
    fs::write(&path_b, b).ok()?;
    let script = "import sys, yaml\n\
                  load = lambda p: yaml.safe_load(open(p, encoding='utf-8'))\n\
                  sys.exit(0 if load(sys.argv[1]) == load(sys.argv[2]) else 1)\n";
    let status = std::process::Command::new("python3")
        .args(["-c", "import yaml"])
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(&path_a)
        .arg(&path_b)
        .output()
        .ok()?;
    match output.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => panic!(
            "PyYAML failed to load the documents:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ),
    }
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
/// It declares the options in [`FAKE_OPTIONS`]. If `extra` is set, an
/// additional finding with that message is reported, which lets tests
/// observe check options; `ratio` is validated to lie between 0 and 1.
///
/// With [`FakeCheck::replacing`], it also reports every occurrence of a
/// word in the project file and fixes it by replacing the word.
pub struct FakeCheck {
    pub name: &'static str,
    pub summary: &'static str,
    pub findings: Vec<(&'static str, Option<(usize, usize)>)>,
    pub replace: Option<(&'static str, &'static str)>,
    pub runs: Rc<Cell<usize>>,
    pub fixes: Rc<Cell<usize>>,
    pub crafts: Option<&'static [Craft]>,
    pub unfixable: Option<&'static str>,
}

impl FakeCheck {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            summary: "A fake check",
            findings: Vec::new(),
            replace: None,
            runs: Rc::default(),
            fixes: Rc::default(),
            crafts: None,
            unfixable: None,
        }
    }

    pub fn finding(mut self, message: &'static str, location: Option<(usize, usize)>) -> Self {
        self.findings.push((message, location));
        self
    }

    pub fn replacing(mut self, word: &'static str, replacement: &'static str) -> Self {
        self.replace = Some((word, replacement));
        self
    }

    pub fn only_for(mut self, crafts: &'static [Craft]) -> Self {
        self.crafts = Some(crafts);
        self
    }

    /// Marks the predefined findings as not fixable, for this reason.
    pub fn unfixable(mut self, reason: &'static str) -> Self {
        self.unfixable = Some(reason);
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

    fn options(&self) -> &'static [OptionSpec] {
        FAKE_OPTIONS
    }

    fn validate_option(&self, name: &str, value: &OptionValue) -> Result<(), String> {
        match (name, value.as_f64()) {
            ("ratio", Some(ratio)) if !(0.0..=1.0).contains(&ratio) => {
                Err("must be between 0 and 1".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn crafts(&self) -> Option<&'static [Craft]> {
        self.crafts
    }

    fn run(&self, ctx: &Context<'_>) -> Vec<Diagnostic> {
        self.runs.set(self.runs.get() + 1);
        let mut diagnostics: Vec<Diagnostic> = self
            .findings
            .iter()
            .map(|&(message, location)| {
                let mut diagnostic = Diagnostic::warning(self.name, message);
                if let Some(reason) = self.unfixable {
                    diagnostic = diagnostic.unfixable(reason);
                }
                match location {
                    Some((line, column)) => diagnostic.at(line, column),
                    None => diagnostic,
                }
            })
            .collect();
        if let Some(extra) = ctx.options.get(self.name, "extra").and_then(|v| v.as_str()) {
            diagnostics.push(Diagnostic::warning(self.name, extra));
        }
        if let Some((word, _)) = self.replace {
            let document = &ctx.project.document;
            for (offset, _) in document.source().match_indices(word) {
                let (line, column) = document.position(offset);
                diagnostics
                    .push(Diagnostic::warning(self.name, format!("found {word}")).at(line, column));
            }
        }
        diagnostics
    }

    fn fix(&self, ctx: &Context<'_>) -> Option<Document> {
        self.fixes.set(self.fixes.get() + 1);
        let (word, replacement) = self.replace?;
        let document = &ctx.project.document;
        let edits = document
            .source()
            .match_indices(word)
            .map(|(offset, _)| (Edit::new(offset..offset + word.len(), replacement), ()))
            .collect();
        ctx.apply_edits(self.name, edits, |_, _, _| true)
    }
}
