// SPDX-License-Identifier: GPL-3.0-only

//! End-to-end tests running the compiled craft-tidy binary.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{example_config, project};

fn craft_tidy(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_craft-tidy"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run craft-tidy")
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

// Successful runs

#[test]
fn lints_the_current_directory_by_default() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn lints_a_relative_project_file_path() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    fs::write(dir.path().join("snapcraft.yaml"), "name: test\n").unwrap();
    let output = craft_tidy(dir.path(), &["snapcraft.yaml"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let output = craft_tidy(dir.path(), &["snapcraft.yaml", "--dump-config"]);
    assert!(stdout(&output).contains("warnings-as-errors: parts-*"));
}

#[test]
fn dump_config_shows_defaults() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--dump-config"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        "checks: \"*\"\nwarnings-as-errors: \"\"\ncheck-options: {}\n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn configuration_sources_are_layered() {
    let dir = project(Some(
        "checks: '-*,parts-*'\n\
         check-options:\n  parts-x:\n    a: 1\n    b: 2\n",
    ));
    let output = craft_tidy(
        dir.path(),
        &[
            "--config",
            "checks: '-parts-y'\ncheck-options: {parts-x: {b: 3, c: 4}}",
            "--checks=metadata-*",
            "-o",
            "parts-x.c=5",
            "--dump-config",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "checks: -*,parts-*,-parts-y,metadata-*\n\
         warnings-as-errors: \"\"\n\
         check-options:\n  parts-x:\n    a: 1\n    b: 3\n    c: 5\n"
    );
}

#[test]
fn config_is_discovered_from_subdirectories() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    let output = craft_tidy(&dir.path().join("snap"), &["--dump-config"]);
    assert!(stdout(&output).contains("warnings-as-errors: parts-*"));

    let output = craft_tidy(dir.path(), &["snap/snapcraft.yaml", "--dump-config"]);
    assert!(stdout(&output).contains("warnings-as-errors: parts-*"));
}

#[test]
fn explicit_config_file_and_no_config() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    fs::write(
        dir.path().join("other.yaml"),
        "warnings-as-errors: metadata-*\n",
    )
    .unwrap();

    let output = craft_tidy(dir.path(), &["--config-file=other.yaml", "--dump-config"]);
    assert!(stdout(&output).contains("warnings-as-errors: metadata-*"));

    let output = craft_tidy(dir.path(), &["--no-config", "--dump-config"]);
    assert!(stdout(&output).contains("warnings-as-errors: \"\""));
}

#[test]
fn dumped_config_is_a_valid_config_file() {
    let dir = project(None);
    let output = craft_tidy(
        dir.path(),
        &[
            "--checks=-*,parts-*",
            "-o",
            "parts-x.n='7'",
            "--dump-config",
        ],
    );
    fs::write(dir.path().join(".craft-tidy"), &output.stdout).unwrap();
    let again = craft_tidy(dir.path(), &["--dump-config"]);
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    assert_eq!(stdout(&again), stdout(&output));
}

#[test]
fn example_config_is_accepted_without_warnings() {
    let dir = project(None);
    let example = example_config();
    let example = example.to_str().unwrap();

    let output = craft_tidy(dir.path(), &["--config-file", example, "--dump-config"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    assert!(stdout(&output).contains("checks: \"*,-experimental-*\""));

    let output = craft_tidy(dir.path(), &["--config-file", example]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn help_describes_configuration() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains(".craft-tidy"));

    let output = craft_tidy(dir.path(), &["--version"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).starts_with("craft-tidy "));
}

// Failing runs

#[test]
fn invalid_config_file_is_reported_with_location() {
    let dir = project(Some("checks: '*'\nchekcs: x\n"));
    let output = craft_tidy(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    let err = stderr(&output);
    assert!(
        err.starts_with("craft-tidy: error: invalid configuration in "),
        "{err}"
    );
    assert!(err.contains("unknown field `chekcs`"), "{err}");
    assert!(err.contains(".craft-tidy:2:1"), "{err}");
    assert_eq!(stdout(&output), "");
}

#[test]
fn invalid_arguments_are_usage_errors() {
    let dir = project(None);
    for args in [
        &["--checks=Bad"][..],
        &["-o", "missing-value"],
        &["--dump-config", "--list-checks"],
        &["--config-file=x", "--no-config"],
        &["--no-such-flag"],
    ] {
        let output = craft_tidy(dir.path(), args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
    }
}

#[test]
fn missing_config_file_is_an_error() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--config-file=missing.yaml"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("missing.yaml"));
}

#[test]
fn options_for_unknown_checks_are_warned_about() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["-o", "no-such-check.x=1", "--dump-config"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).contains("unknown check 'no-such-check'"));
}

#[test]
fn missing_project_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = craft_tidy(dir.path(), &["--no-config"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("no snapcraft.yaml found"));
}

#[test]
fn explain_unknown_check_is_an_error() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--explain", "no-such-check"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown check 'no-such-check'"));
}
