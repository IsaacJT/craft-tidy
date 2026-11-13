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
fn invalid_project_yaml_is_an_error() {
    let dir = project(None);
    fs::write(dir.path().join("snap/snapcraft.yaml"), "name: [test\n").unwrap();
    let run = execute(&registry(), &[dir.path().to_str().unwrap()]);
    assert!(matches!(run.result, Err(Error::Syntax { .. })));
    assert_eq!(run.out, "");
}

#[test]
fn warns_about_options_for_unknown_checks() {
    let dir = project(None);
    let path = dir.path().to_str().unwrap();
    let run = execute(
        &registry(),
        &[
            path,
            "-o",
            "nope.x=1",
            "-o",
            "nope.y=2",
            "-o",
            "parts-a.extra=hi",
        ],
    );
    assert!(run.result.is_ok());
    assert_eq!(
        run.err,
        "craft-tidy: warning: options given for unknown check 'nope'\n"
    );
}

// Check option validation

fn option_error(args: &[&str]) -> String {
    let dir = project(None);
    let mut all = vec![dir.path().to_str().unwrap()];
    all.extend_from_slice(args);
    match execute(&registry(), &all).result {
        Err(err @ Error::InvalidOption { .. }) => err.to_string(),
        other => panic!("{args:?} should be an invalid option, got {other:?}"),
    }
}

#[test]
fn accepts_declared_options_of_the_right_type() {
    let dir = project(None);
    let path = dir.path().to_str().unwrap();
    for option in [
        "parts-a.extra=hello",
        "parts-a.extra='42'",
        "parts-a.ratio=0.25",
        "parts-a.ratio=1",
    ] {
        let run = execute(&registry(), &[path, "-o", option]);
        assert!(run.result.is_ok(), "{option}: {:?}", run.result);
        assert_eq!(run.err, "", "{option}");
    }
}

#[test]
fn rejects_unknown_options_of_known_checks() {
    assert_eq!(
        option_error(&["-o", "parts-a.nope=1"]),
        "invalid option 'parts-a.nope': unknown option (available: extra, ratio)"
    );
}

#[test]
fn rejects_options_of_the_wrong_type() {
    assert_eq!(
        option_error(&["-o", "parts-a.extra=42"]),
        "invalid option 'parts-a.extra': expected a value of type string"
    );
    assert_eq!(
        option_error(&["-o", "parts-a.ratio=high"]),
        "invalid option 'parts-a.ratio': expected a value of type float"
    );
    assert_eq!(
        option_error(&["-o", "parts-a.ratio=true"]),
        "invalid option 'parts-a.ratio': expected a value of type float"
    );
}

#[test]
fn rejects_option_values_that_fail_check_validation() {
    assert_eq!(
        option_error(&["-o", "parts-a.ratio=1.5"]),
        "invalid option 'parts-a.ratio': must be between 0 and 1"
    );
}

#[test]
fn options_from_the_config_file_are_validated() {
    let dir = project(Some("check-options:\n  parts-a:\n    extra: 3\n"));
    let run = execute(
        &registry(),
        &[dir.path().to_str().unwrap(), "--dump-config"],
    );
    assert!(matches!(run.result, Err(Error::InvalidOption { .. })));
    assert_eq!(run.out, "");
}

// Fixing

const UNFIXED: &str = "name: old\nsummary: old\n";

fn fixable_registry() -> Vec<Box<dyn Check>> {
    vec![
        FakeCheck::new("parts-a")
            .finding("not fixable", Some((1, 1)))
            .boxed(),
        FakeCheck::new("style-word").replacing("old", "new").boxed(),
    ]
}

/// A project whose snapcraft.yaml contains [`UNFIXED`].
fn fixable_project(config: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = project(config);
    let file = dir.path().join("snap/snapcraft.yaml");
    fs::write(&file, UNFIXED).unwrap();
    (dir, file)
}

#[test]
fn fix_rewrites_the_project_file_and_reports_what_remains() {
    let (dir, file) = fixable_project(None);
    let run = execute(
        &fixable_registry(),
        &[dir.path().to_str().unwrap(), "--fix"],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "name: new\nsummary: new\n"
    );
    assert_eq!(
        run.out,
        format!("{}:1:1: warning: not fixable [parts-a]\n", file.display())
    );
    assert_eq!(
        run.err,
        format!(
            "craft-tidy: applied fixes to {} (3 issue(s) found, 1 remaining)\n",
            file.display()
        )
    );
}

fn unfixable_registry() -> Vec<Box<dyn Check>> {
    vec![
        FakeCheck::new("parts-a")
            .finding("stuck", Some((1, 1)))
            .unfixable("it is stuck")
            .boxed(),
        FakeCheck::new("parts-b")
            .finding("no reason", Some((2, 1)))
            .boxed(),
    ]
}

#[test]
fn with_fix_remaining_findings_say_why_they_cannot_be_fixed() {
    let (dir, file) = fixable_project(None);
    for args in [vec!["--fix"], vec!["--no-fix", "--fix"]] {
        let mut args = args.clone();
        args.insert(0, dir.path().to_str().unwrap());
        let run = execute(&unfixable_registry(), &args);
        assert_eq!(
            run.out,
            format!(
                "{0}:1:1: warning: stuck (cannot be fixed: it is stuck) [parts-a]\n\
                 {0}:2:1: warning: no reason [parts-b]\n",
                file.display()
            )
        );
        // Nothing could be fixed, so the file is untouched.
        assert_eq!(run.err, "");
        assert_eq!(fs::read_to_string(&file).unwrap(), UNFIXED);
    }
}

#[test]
fn without_fix_the_reason_is_not_shown() {
    let (dir, file) = fixable_project(Some("fix: true\n"));
    for args in [vec![], vec!["--fix", "--no-fix"]] {
        let mut args = args.clone();
        args.insert(0, dir.path().to_str().unwrap());
        if args.len() == 1 {
            args.push("--no-config");
        }
        let run = execute(&unfixable_registry(), &args);
        assert_eq!(
            run.out,
            format!(
                "{0}:1:1: warning: stuck [parts-a]\n{0}:2:1: warning: no reason [parts-b]\n",
                file.display()
            )
        );
    }
}

#[test]
fn without_fix_the_project_file_is_untouched() {
    let (dir, file) = fixable_project(None);
    let run = execute(&fixable_registry(), &[dir.path().to_str().unwrap()]);
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(fs::read_to_string(&file).unwrap(), UNFIXED);
    assert_eq!(run.out.lines().count(), 3);
    assert_eq!(run.err, "");
}

#[test]
fn fix_can_be_enabled_in_the_config_file() {
    let (dir, file) = fixable_project(Some("fix: true\n"));
    let run = execute(&fixable_registry(), &[dir.path().to_str().unwrap()]);
    assert!(run.result.is_ok());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "name: new\nsummary: new\n"
    );
}

#[test]
fn no_fix_overrides_the_config_file() {
    let (dir, file) = fixable_project(Some("fix: true\n"));
    let run = execute(
        &fixable_registry(),
        &[dir.path().to_str().unwrap(), "--no-fix"],
    );
    assert!(run.result.is_ok());
    assert_eq!(fs::read_to_string(&file).unwrap(), UNFIXED);
    assert_eq!(run.err, "");
}

#[test]
fn exit_status_reflects_the_problems_left_after_fixing() {
    let (dir, _) = fixable_project(Some("warnings-as-errors: style-*\n"));
    let path = dir.path().to_str().unwrap();
    assert_eq!(
        execute(&fixable_registry(), &[path]).result.unwrap(),
        Status::LintErrors
    );
    assert_eq!(
        execute(&fixable_registry(), &[path, "--fix"])
            .result
            .unwrap(),
        Status::Success
    );
}

#[test]
fn fix_leaves_clean_projects_alone() {
    let dir = project(None);
    let file = dir.path().join("snap/snapcraft.yaml");
    let registry = vec![FakeCheck::new("style-word").replacing("old", "new").boxed()];
    let run = execute(&registry, &[dir.path().to_str().unwrap(), "--fix"]);
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(fs::read_to_string(&file).unwrap(), "name: test\n");
    assert_eq!((run.out.as_str(), run.err.as_str()), ("", ""));
}

#[test]
fn fix_without_fixable_problems_changes_nothing() {
    let (dir, file) = fixable_project(None);
    let registry = vec![
        FakeCheck::new("parts-a")
            .finding("not fixable", Some((1, 1)))
            .boxed(),
    ];
    let run = execute(&registry, &[dir.path().to_str().unwrap(), "--fix"]);
    assert!(run.result.is_ok());
    assert_eq!(fs::read_to_string(&file).unwrap(), UNFIXED);
    assert_eq!(run.out.lines().count(), 1);
    assert_eq!(run.err, "");
}

#[test]
fn fix_only_uses_enabled_checks() {
    let (dir, file) = fixable_project(None);
    let run = execute(
        &fixable_registry(),
        &[dir.path().to_str().unwrap(), "--fix", "--checks=-style-*"],
    );
    assert!(run.result.is_ok());
    assert_eq!(fs::read_to_string(&file).unwrap(), UNFIXED);
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
            "parts-a.extra=hi",
        ],
    );
    assert_eq!(run.result.unwrap(), Status::Success);
    assert_eq!(
        run.out,
        "checks: \"*,-parts-*\"\nwarnings-as-errors: \"\"\nfix: false\ncheck-options:\n  parts-a:\n    extra: hi\n"
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
        "parts-a: A fake check\n\
         \n\
         Reports predefined findings.\n\
         \n\
         Options:\n  \
           extra (string, default: \"\")\n      \
             Report an additional finding with this message.\n  \
           ratio (float, default: 0.5)\n      \
             Unused; must be between 0 and 1.\n"
    );
}

#[test]
fn explain_omits_options_section_for_checks_without_options() {
    struct Bare;
    impl Check for Bare {
        fn name(&self) -> &'static str {
            "bare-check"
        }
        fn summary(&self) -> &'static str {
            "No options"
        }
        fn explanation(&self) -> &'static str {
            "Has no options."
        }
        fn run(&self, _: &craft_tidy::checks::Context<'_>) -> Vec<craft_tidy::checks::Diagnostic> {
            Vec::new()
        }
    }
    let dir = project(None);
    let registry: Vec<Box<dyn Check>> = vec![Box::new(Bare)];
    let run = execute(
        &registry,
        &[dir.path().to_str().unwrap(), "--explain", "bare-check"],
    );
    assert_eq!(run.out, "bare-check: No options\n\nHas no options.\n");
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

#[test]
fn settings_for_finds_the_configuration_of_a_file() {
    let dir = project(Some("checks: -style-*\n"));
    let file = dir.path().join("snap/snapcraft.yaml");
    let settings = app::settings_for(&file).unwrap();
    assert!(!settings.checks.matches("style-line-length"));
    assert!(settings.checks.matches("project-file-location"));
    // Without a configuration file, the defaults.
    let dir = project(None);
    let settings = app::settings_for(&dir.path().join("snap/snapcraft.yaml")).unwrap();
    assert_eq!(settings.checks, Settings::defaults().checks);
    // An invalid configuration file is an error.
    let dir = project(Some("checks: [1]\n"));
    assert!(matches!(
        app::settings_for(&dir.path().join("snap/snapcraft.yaml")),
        Err(Error::Config { .. })
    ));
}
