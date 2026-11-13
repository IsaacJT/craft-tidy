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
        "checks: \"*\"\nwarnings-as-errors: \"\"\nfix: false\ncheck-options: {}\n"
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
         fix: false\n\
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
fn fix_without_fixable_problems_leaves_the_project_file_alone() {
    let dir = project(Some("fix: true\n"));
    for args in [&["--fix"][..], &[]] {
        let output = craft_tidy(dir.path(), args);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(!stderr(&output).contains("applied fixes"));
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("snap/snapcraft.yaml")).unwrap(),
        "name: test\n"
    );
}

#[test]
fn reports_and_fixes_long_lines() {
    let dir = project(None);
    let file = dir.path().join("snap/snapcraft.yaml");
    let long =
        "summary: a summary that is far too long to fit on a single line of eighty characters\n";
    fs::write(&file, format!("name: test\n{long}")).unwrap();

    let output = craft_tidy(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        "./snap/snapcraft.yaml:2:81: warning: line is 84 characters long, exceeding the \
         limit of 80 [style-line-length]\n"
    );

    let output = craft_tidy(dir.path(), &["--warnings-as-errors=style-*"]);
    assert_eq!(output.status.code(), Some(1));

    let output = craft_tidy(dir.path(), &["--fix", "--warnings-as-errors=style-*"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "name: test\nsummary: a summary that is far too long to fit on a single line of eighty\n  \
         characters\n"
    );
}

#[test]
fn line_length_limit_is_configurable() {
    let dir = project(Some(
        "check-options:\n  style-line-length:\n    max-length: 8\n",
    ));
    let output = craft_tidy(dir.path(), &[]);
    assert!(stdout(&output).contains("limit of 8 [style-line-length]"));

    let output = craft_tidy(dir.path(), &["-o", "style-line-length.max-length=0"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("must be a positive integer"));
}

#[test]
fn explains_style_line_length() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--explain", "style-line-length"]);
    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    assert!(out.starts_with("style-line-length: "), "{out}");
    assert!(out.contains("max-length (integer, default: 80)"), "{out}");
}

#[test]
fn reports_and_fixes_unsorted_lists() {
    let dir = project(None);
    let file = dir.path().join("snap/snapcraft.yaml");
    let source = "\
name: test
parts:
  test:
    plugin: nil
    build-packages:
      - zlib1g-dev
      - on arm64: [libb, liba]
      - curl
    override-build: |
      craftctl default
";
    fs::write(&file, source).unwrap();

    let output = craft_tidy(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        "./snap/snapcraft.yaml:6:9: warning: entries of 'build-packages' are not sorted: \
         'curl' should come before 'zlib1g-dev' [style-sorted-lists]\n\
         ./snap/snapcraft.yaml:7:20: warning: entries of 'build-packages: on arm64' are not \
         sorted: 'liba' should come before 'libb' [style-sorted-lists]\n"
    );

    let output = craft_tidy(dir.path(), &["--fix"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "\
name: test
parts:
  test:
    plugin: nil
    build-packages:
      - curl
      - zlib1g-dev
      - on arm64: [liba, libb]
    override-build: |
      craftctl default
"
    );
}

#[test]
fn sorted_list_keys_are_configurable() {
    let dir = project(Some(
        "check-options:\n  style-sorted-lists:\n    keys: command-chain\n",
    ));
    fs::write(
        dir.path().join("snap/snapcraft.yaml"),
        "plugs: [b, a]\ncommand-chain: [b, a]\n",
    )
    .unwrap();
    let output = craft_tidy(dir.path(), &[]);
    assert!(stdout(&output).contains(":2:"), "{}", stdout(&output));
    assert!(!stdout(&output).contains(":1:"), "{}", stdout(&output));

    let output = craft_tidy(dir.path(), &["-o", "style-sorted-lists.keys=Not Valid"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("invalid option 'style-sorted-lists.keys'"));
}

#[test]
fn explains_style_sorted_lists() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--explain", "style-sorted-lists"]);
    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    assert!(out.starts_with("style-sorted-lists: "), "{out}");
    assert!(out.contains("keys (string, default: "), "{out}");
    assert!(
        out.contains("case-sensitive (boolean, default: false)"),
        "{out}"
    );
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
fn invalid_project_yaml_is_an_error() {
    let dir = project(None);
    fs::write(dir.path().join("snap/snapcraft.yaml"), "name: [test\n").unwrap();
    let output = craft_tidy(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        "craft-tidy: error: ./snap/snapcraft.yaml:1:7: invalid YAML: unclosed bracket '['\n"
    );
}

#[test]
fn explain_unknown_check_is_an_error() {
    let dir = project(None);
    let output = craft_tidy(dir.path(), &["--explain", "no-such-check"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown check 'no-such-check'"));
}
