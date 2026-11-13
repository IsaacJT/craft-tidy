// SPDX-License-Identifier: GPL-3.0-only

//! Tests for command line parsing (`craft_tidy::cli`).

mod common;

use std::path::Path;

use clap::CommandFactory;
use clap::error::ErrorKind;
use craft_tidy::cli::Cli;
use craft_tidy::config::OptionValue;

use common::{cli, try_cli};

fn rejected(args: &[&str]) -> ErrorKind {
    match try_cli(args) {
        Ok(parsed) => panic!("{args:?} should be rejected, got {parsed:?}"),
        Err(err) => err.kind(),
    }
}

#[test]
fn command_definition_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn defaults() {
    let parsed = cli(&[]);
    assert_eq!(parsed.path, Path::new("."));
    assert_eq!(parsed.checks, None);
    assert_eq!(parsed.warnings_as_errors, None);
    assert!(parsed.check_options.is_empty());
    assert_eq!(parsed.config, None);
    assert_eq!(parsed.config_file, None);
    assert!(!parsed.no_config);
    assert!(!parsed.dump_config && !parsed.list_checks);
    assert_eq!(parsed.explain, None);
    assert_eq!(parsed.settings(), Default::default());
}

#[test]
fn accepts_a_path() {
    assert_eq!(cli(&["some/project"]).path, Path::new("some/project"));
    assert_eq!(
        cli(&["--dump-config", "snap/snapcraft.yaml"]).path,
        Path::new("snap/snapcraft.yaml")
    );
}

#[test]
fn rejects_more_than_one_path() {
    assert_eq!(rejected(&["a", "b"]), ErrorKind::UnknownArgument);
}

#[test]
fn glob_flags_form_a_settings_layer() {
    let settings = cli(&["--checks=-*,parts-*", "--warnings-as-errors", "parts-*"]).settings();
    assert_eq!(settings.checks.to_string(), "-*,parts-*");
    assert_eq!(settings.warnings_as_errors.to_string(), "parts-*");
}

#[test]
fn rejects_invalid_glob_flags() {
    for args in [
        &["--checks=Bad"][..],
        &["--checks", "a b"],
        &["--warnings-as-errors=-"],
        &["--warnings-as-errors=x.y"],
    ] {
        assert_eq!(rejected(args), ErrorKind::ValueValidation, "{args:?}");
    }
}

#[test]
fn rejects_missing_flag_values() {
    for flag in [
        "--checks",
        "--warnings-as-errors",
        "-o",
        "--config",
        "--config-file",
        "--explain",
    ] {
        assert_eq!(rejected(&[flag]), ErrorKind::InvalidValue, "{flag}");
    }
}

#[test]
fn check_options_are_parsed_as_yaml_scalars() {
    let settings = cli(&[
        "-o",
        "a.int=79",
        "--check-option=a.bool=true",
        "--check-option",
        "a.str='79'",
        "-o",
        "a.eq=x=y",
        "-o",
        "a.empty=",
        "-o",
        "b-2.key-2=1.5",
    ])
    .settings();
    let get = |check, key| settings.check_options.get(check, key).cloned();
    assert_eq!(get("a", "int"), Some(OptionValue::Integer(79)));
    assert_eq!(get("a", "bool"), Some(OptionValue::Bool(true)));
    assert_eq!(get("a", "str"), Some(OptionValue::String("79".into())));
    assert_eq!(get("a", "eq"), Some(OptionValue::String("x=y".into())));
    assert_eq!(get("a", "empty"), Some(OptionValue::String(String::new())));
    assert_eq!(get("b-2", "key-2"), Some(OptionValue::Float(1.5)));
}

#[test]
fn repeated_check_options_use_the_last_value() {
    let settings = cli(&["-o", "a.x=1", "-o", "a.x=2"]).settings();
    assert_eq!(
        settings.check_options.get("a", "x"),
        Some(&OptionValue::Integer(2))
    );
}

#[test]
fn rejects_malformed_check_options() {
    for bad in [
        "a.b", "ab=1", ".b=1", "a.=1", "=1", "A.b=1", "a.B=1", "a.b.c=1", "a b.c=1", "a_b.c=1",
    ] {
        assert_eq!(
            rejected(&["-o", bad]),
            ErrorKind::ValueValidation,
            "{bad:?}"
        );
    }
}

#[test]
fn config_flags() {
    let parsed = cli(&["--config", "checks: x", "--config-file", "cfg.yaml"]);
    assert_eq!(parsed.config.as_deref(), Some("checks: x"));
    assert_eq!(parsed.config_file.as_deref(), Some(Path::new("cfg.yaml")));
    assert!(cli(&["--no-config"]).no_config);
    assert!(cli(&["--no-config", "--config", "checks: x"]).no_config);
}

#[test]
fn config_file_conflicts_with_no_config() {
    assert_eq!(
        rejected(&["--config-file=x", "--no-config"]),
        ErrorKind::ArgumentConflict
    );
}

#[test]
fn each_action_is_accepted_alone() {
    assert!(cli(&["--dump-config"]).dump_config);
    assert!(cli(&["--list-checks"]).list_checks);
    assert_eq!(cli(&["--explain", "x"]).explain.as_deref(), Some("x"));
}

#[test]
fn actions_are_mutually_exclusive() {
    for args in [
        &["--dump-config", "--list-checks"][..],
        &["--dump-config", "--explain", "x"],
        &["--list-checks", "--explain", "x"],
    ] {
        assert_eq!(rejected(args), ErrorKind::ArgumentConflict, "{args:?}");
    }
}

#[test]
fn rejects_unknown_flags() {
    assert_eq!(rejected(&["--chekcs=x"]), ErrorKind::UnknownArgument);
    assert_eq!(rejected(&["-x"]), ErrorKind::UnknownArgument);
}

#[test]
fn help_and_version_are_available() {
    assert_eq!(rejected(&["--help"]), ErrorKind::DisplayHelp);
    assert_eq!(rejected(&["--version"]), ErrorKind::DisplayVersion);
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains(".craft-tidy"), "{help}");
}
