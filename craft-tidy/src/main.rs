// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::process::ExitCode;

use clap::Parser;

use craft_tidy::app::{self, Status};
use craft_tidy::cli::{Cli, Command};
use craft_tidy::lsp;

/// Exit code for usage, configuration and I/O errors, matching clap's.
const EXIT_FAILURE: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.command == Some(Command::Lsp) {
        return match lsp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("craft-tidy: error: language server: {err}");
                ExitCode::from(EXIT_FAILURE)
            }
        };
    }
    let result = app::run(&cli, &mut io::stdout().lock(), &mut io::stderr().lock());
    match result {
        Ok(Status::Success) => ExitCode::SUCCESS,
        Ok(Status::LintErrors) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("craft-tidy: error: {err}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}
