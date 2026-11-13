// SPDX-License-Identifier: GPL-3.0-only

//! Command line interface.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

use crate::config::{OptionValue, Settings};
use crate::glob::GlobList;

const AFTER_HELP: &str = "\
Configuration is read from a YAML file named .craft-tidy, found by searching the
project directory and its parents. Settings are applied in this order, with
later sources taking precedence:

  1. built-in defaults (all checks enabled)
  2. the .craft-tidy file (or --config-file)
  3. inline YAML passed with --config
  4. command line flags

Check globs from each source are appended to the previous ones (a '*' or '-*'
glob replaces all globs before it), and the last glob matching a check
decides whether it is enabled. Use --dump-config to see
the effective configuration.";

/// Commands other than linting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Run a language server (LSP) on standard input and output, for
    /// editors: open project files are checked as they are edited, with
    /// the configuration found for them. To lint a directory named lsp,
    /// pass it as ./lsp.
    Lsp,
}

#[derive(Debug, Parser)]
#[command(
    name = "craft-tidy",
    version,
    about = "A linter for the project files of Canonical's craft tools \
             (Snapcraft, Charmcraft, Rockcraft, Imagecraft and Debcraft)",
    after_long_help = AFTER_HELP,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Project directory, or project file such as snapcraft.yaml, to lint.
    /// In a directory, the project file of every craft tool found is linted.
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: PathBuf,

    /// Comma-separated globs selecting checks to run. Prefix a glob with '-'
    /// to disable matching checks, e.g. '-*,style-*'. Appended to the globs
    /// from the configuration file.
    #[arg(long, value_name = "GLOBS", value_parser = parse_globs)]
    pub checks: Option<GlobList>,

    /// Comma-separated globs selecting checks whose findings are reported as
    /// errors. Appended to the globs from the configuration file.
    #[arg(long, value_name = "GLOBS", value_parser = parse_globs)]
    pub warnings_as_errors: Option<GlobList>,

    /// Fix problems in the project file where possible, rewriting it in
    /// place, then report the remaining problems.
    #[arg(long, overrides_with_all = ["fix", "no_fix"])]
    pub fix: bool,

    /// Do not fix problems, even if enabled in the configuration file.
    #[arg(long, overrides_with_all = ["fix", "no_fix"])]
    pub no_fix: bool,

    /// Set an option for a check, e.g. 'style-line-length.max-length=100'.
    /// The value is interpreted as a YAML scalar. May be repeated.
    #[arg(
        short = 'o',
        long = "check-option",
        value_name = "CHECK.KEY=VALUE",
        value_parser = parse_check_option,
        action = ArgAction::Append
    )]
    pub check_options: Vec<CheckOptionArg>,

    /// Inline YAML configuration, in the same format as .craft-tidy, applied
    /// on top of the configuration file.
    #[arg(long, value_name = "YAML")]
    pub config: Option<String>,

    /// Read the configuration file from PATH instead of searching for
    /// .craft-tidy.
    #[arg(long, value_name = "PATH", conflicts_with = "no_config")]
    pub config_file: Option<PathBuf>,

    /// Do not read any configuration file.
    #[arg(long)]
    pub no_config: bool,

    /// Print the effective configuration as YAML and exit.
    #[arg(long, group = "action")]
    pub dump_config: bool,

    /// List the enabled checks and exit. Combine with --checks='*' to list
    /// all checks.
    #[arg(long, group = "action")]
    pub list_checks: bool,

    /// Describe CHECK and exit.
    #[arg(long, value_name = "CHECK", group = "action")]
    pub explain: Option<String>,
}

/// A `--check-option` argument.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckOptionArg {
    pub check: String,
    pub key: String,
    pub value: OptionValue,
}

impl Cli {
    /// The configuration layer formed by the individual command line flags.
    pub fn settings(&self) -> Settings {
        let mut settings = Settings {
            checks: self.checks.clone().unwrap_or_default(),
            warnings_as_errors: self.warnings_as_errors.clone().unwrap_or_default(),
            fix: match (self.fix, self.no_fix) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
            ..Settings::default()
        };
        for option in &self.check_options {
            settings
                .check_options
                .set(&option.check, &option.key, option.value.clone());
        }
        settings
    }
}

fn parse_globs(s: &str) -> Result<GlobList, String> {
    s.parse().map_err(|err| format!("{err}"))
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn parse_check_option(s: &str) -> Result<CheckOptionArg, String> {
    let (name, value) = s
        .split_once('=')
        .ok_or_else(|| "expected CHECK.KEY=VALUE".to_owned())?;
    let (check, key) = name
        .split_once('.')
        .ok_or_else(|| "expected CHECK.KEY=VALUE".to_owned())?;
    if !is_identifier(check) || !is_identifier(key) {
        return Err(format!(
            "invalid option name '{name}': check and key names consist of lowercase letters, \
             digits and '-'"
        ));
    }
    Ok(CheckOptionArg {
        check: check.to_owned(),
        key: key.to_owned(),
        value: OptionValue::parse(value),
    })
}
