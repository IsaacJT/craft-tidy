// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the top-level program flow (`craft_tidy::app`).

mod common;

use std::fs;

use craft_tidy::app::{self, Status};
use craft_tidy::checks::Check;
use craft_tidy::config::{OptionValue, Settings};
use craft_tidy::error::Error;

use common::{FakeCheck, cli, project};

/// Output of a run: status (or error), stdout and stderr.
struct Run {
    result: Result<Status, Error>,
    out: String,
    err: String,
}

fn execute(registry: &[Box<dyn Check>], args: &[&str]) -> Run {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = app::run_with_checks(&cli(args), registry, &mut out, &mut err);
    Run {
        result,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

fn registry() -> Vec<Box<dyn Check>> {
    vec![
        FakeCheck::new("metadata-a")
            .finding("bad summary", Some((2, 3)))
            .boxed(),
        FakeCheck::new("parts-a")
            .finding("no location", None)
            .boxed(),
    ]
}

// Settings resolution

#[test]
fn resolve_settings_layers_all_sources() {
    let dir = project(Some(
        "checks: '-*,parts-*'\ncheck-options: {x: {a: 1, b: 1, c: 1}}\n",
    ));
    let path = dir.path().to_str().unwrap();
    let settings = app::resolve_settings(&cli(&[
        path,
        "--config",
        "checks: -parts-y\ncheck-options: {x: {b: 2, c: 2}}",
        "--checks=metadata-*",
        "-o",
        "x.c=3",
    ]))
    .unwrap();
    assert_eq!(
        settings.checks.to_string(),
        "-*,parts-*,-parts-y,metadata-*"
    );
    let get = |key| settings.check_options.get("x", key).cloned();
    assert_eq!(get("a"), Some(OptionValue::Integer(1)));
    assert_eq!(get("b"), Some(OptionValue::Integer(2)));
    assert_eq!(get("c"), Some(OptionValue::Integer(3)));
}

#[test]
fn resolve_settings_without_config_uses_defaults() {
    let dir = project(None);
    let path = dir.path().to_str().unwrap();
    assert_eq!(
        app::resolve_settings(&cli(&[path])).unwrap(),
        Settings::defaults()
    );
}

#[test]
fn resolve_settings_discovers_config_from_project_file_path() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    let file = dir.path().join("snap/snapcraft.yaml");
    let settings = app::resolve_settings(&cli(&[file.to_str().unwrap()])).unwrap();
    assert_eq!(settings.warnings_as_errors.to_string(), "parts-*");
}

#[test]
fn explicit_config_file_replaces_discovery() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    let other = dir.path().join("other.yaml");
    fs::write(&other, "warnings-as-errors: metadata-*\n").unwrap();
    let settings = app::resolve_settings(&cli(&[
        dir.path().to_str().unwrap(),
        "--config-file",
        other.to_str().unwrap(),
    ]))
    .unwrap();
    assert_eq!(settings.warnings_as_errors.to_string(), "metadata-*");
}

#[test]
fn no_config_skips_the_config_file_but_not_inline_config() {
    let dir = project(Some("warnings-as-errors: parts-*\n"));
    let path = dir.path().to_str().unwrap();
    let settings = app::resolve_settings(&cli(&[path, "--no-config"])).unwrap();
    assert!(settings.warnings_as_errors.is_empty());

    let settings = app::resolve_settings(&cli(&[
        path,
        "--no-config",
        "--config",
        "warnings-as-errors: x",
    ]))
    .unwrap();
    assert_eq!(settings.warnings_as_errors.to_string(), "x");
}

#[test]
fn invalid_config_sources_are_errors() {
    let dir = project(Some("bogus: 1\n"));
    let path = dir.path().to_str().unwrap();
    let message = app::resolve_settings(&cli(&[path]))
        .unwrap_err()
        .to_string();
    assert!(message.contains(".craft-tidy"), "{message}");

    let message = app::resolve_settings(&cli(&[path, "--no-config", "--config", "bogus: 1"]))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("invalid configuration in --config"),
        "{message}"
    );

    let missing = dir.path().join("missing.yaml");
    assert!(matches!(
        app::resolve_settings(&cli(&[path, "--config-file", missing.to_str().unwrap()])),
        Err(Error::Io { .. })
    ));
}

// Linting

#[test]
fn reports_findings_in_compiler_style() {
    let dir = project(None);
    let run = execute(&registry(), &[dir.path().to_str().unwrap()]);
    let file = dir.path().join("snap/snapcraft.yaml");
    let file = file.display();
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        run.out,
        format!(
            "{file}: warning: no location [parts-a]\n\
             {file}:2:3: warning: bad summary [metadata-a]\n"
        )
    );
    assert_eq!(run.err, "");
}

#[test]
fn errors_set_the_lint_errors_status() {
    let dir = project(Some("warnings-as-errors: metadata-*\n"));
    let run = execute(&registry(), &[dir.path().to_str().unwrap()]);
    assert_eq!(run.result.unwrap(), Status::LintErrors);
    assert!(run.out.contains(":2:3: error: bad summary [metadata-a]"));
    assert!(run.out.contains(": warning: no location [parts-a]"));
}

#[test]
fn clean_project_succeeds_silently() {
    let dir = project(None);
    let registry = vec![FakeCheck::new("parts-a").boxed()];
    let run = execute(&registry, &[dir.path().to_str().unwrap()]);
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(run.out, "");
    assert_eq!(run.err, "");
}

#[test]
fn warns_when_no_checks_are_enabled() {
    let dir = project(None);
    let run = execute(&registry(), &[dir.path().to_str().unwrap(), "--checks=-*"]);
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(run.out, "");
    assert_eq!(run.err, "craft-tidy: warning: no checks enabled\n");
}

#[test]
fn missing_project_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let run = execute(&registry(), &[dir.path().to_str().unwrap(), "--no-config"]);
    assert!(matches!(run.result, Err(Error::ProjectNotFound(_))));
    assert_eq!(run.out, "");
}

#[test]
fn warns_about_options_for_unknown_checks() {
    let dir = project(None);
    let path = dir.path().to_str().unwrap();
    let run = execute(&registry(), &[path, "-o", "nope.x=1", "-o", "parts-a.x=1"]);
    assert!(run.result.is_ok());
    assert_eq!(
        run.err,
        "craft-tidy: warning: options given for unknown check 'nope'\n"
    );
}

// Actions

#[test]
fn dump_config_prints_effective_settings() {
    let dir = project(Some("checks: -parts-*\n"));
    let run = execute(
        &registry(),
        &[
            dir.path().to_str().unwrap(),
            "--dump-config",
            "-o",
            "parts-a.x=1",
        ],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        run.out,
        "checks: \"*,-parts-*\"\nwarnings-as-errors: \"\"\ncheck-options:\n  parts-a:\n    x: 1\n"
    );
    assert_eq!(run.err, "");
}

#[test]
fn dump_config_does_not_require_a_project_file() {
    let dir = tempfile::tempdir().unwrap();
    let run = execute(
        &registry(),
        &[dir.path().to_str().unwrap(), "--dump-config"],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
}

#[test]
fn list_checks_shows_enabled_checks_with_summaries() {
    let dir = project(None);
    let path = dir.path().to_str().unwrap();
    let run = execute(&registry(), &[path, "--list-checks"]);
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        run.out,
        "Enabled checks:\n    metadata-a  A fake check\n    parts-a     A fake check\n"
    );

    let run = execute(
        &registry(),
        &[path, "--list-checks", "--checks=-metadata-*"],
    );
    assert_eq!(run.out, "Enabled checks:\n    parts-a  A fake check\n");
}

#[test]
fn list_checks_reports_when_none_are_enabled() {
    let dir = project(None);
    let run = execute(
        &registry(),
        &[dir.path().to_str().unwrap(), "--list-checks", "--checks=-*"],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(run.out, "No checks enabled.\n");
}

#[test]
fn explain_describes_a_check() {
    let dir = project(None);
    let run = execute(
        &registry(),
        &[dir.path().to_str().unwrap(), "--explain", "parts-a"],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        run.out,
        "parts-a: A fake check\n\nReports predefined findings.\n"
    );
}

#[test]
fn explain_works_for_disabled_checks() {
    let dir = project(None);
    let run = execute(
        &registry(),
        &[
            dir.path().to_str().unwrap(),
            "--checks=-*",
            "--explain",
            "parts-a",
        ],
    );
    assert!(run.out.starts_with("parts-a: "));
}

#[test]
fn explain_rejects_unknown_checks() {
    let dir = project(None);
    let run = execute(
        &registry(),
        &[dir.path().to_str().unwrap(), "--explain", "parts-*"],
    );
    assert!(matches!(run.result, Err(Error::UnknownCheck(name)) if name == "parts-*"));
    assert_eq!(run.out, "");
}
