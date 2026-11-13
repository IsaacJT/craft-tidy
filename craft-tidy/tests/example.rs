// SPDX-License-Identifier: GPL-3.0-only

//! Tests keeping `examples/.craft-tidy` valid and complete.

mod common;

use std::collections::BTreeSet;
use std::fs;

use craft_tidy::checks;
use craft_tidy::config::{self, CONFIG_FILE_NAME, Settings};

use common::{cli, example_config, project};

/// Top-level keys of a YAML mapping.
fn keys(yaml: &str) -> BTreeSet<String> {
    serde_saphyr::from_str::<std::collections::BTreeMap<String, serde::de::IgnoredAny>>(yaml)
        .unwrap()
        .into_keys()
        .collect()
}

#[test]
fn example_is_a_valid_configuration() {
    let settings = Settings::from_file(&example_config()).unwrap();
    assert_eq!(settings.checks.to_string(), "*,-experimental-*");
    assert_eq!(
        settings.warnings_as_errors.to_string(),
        "metadata-*,parts-*"
    );
    assert_eq!(settings.fix, Some(false));
    assert_eq!(
        settings
            .check_options
            .get("style-line-length", "max-length"),
        Some(&craft_tidy::config::OptionValue::Integer(80))
    );

    assert!(settings.checks.matches("parts-plugin"));
    assert!(!settings.checks.matches("experimental-thing"));
    assert!(settings.warnings_as_errors.matches("metadata-summary"));
    assert!(!settings.warnings_as_errors.matches("other-check"));
}

#[test]
fn example_documents_every_setting() {
    let example = fs::read_to_string(example_config()).unwrap();
    let all_settings = Settings::defaults().to_yaml().unwrap();
    assert_eq!(
        keys(&example),
        keys(&all_settings),
        "examples/.craft-tidy must set every configuration key"
    );
}

#[test]
fn example_only_sets_valid_options_of_existing_checks() {
    let settings = Settings::from_file(&example_config()).unwrap();
    let unknown = checks::validate_options(&checks::registry(), &settings.check_options)
        .unwrap_or_else(|err| panic!("examples/.craft-tidy: {err}"));
    assert!(
        unknown.is_empty(),
        "examples/.craft-tidy sets options for unknown checks: {unknown:?}"
    );
}

#[test]
fn example_is_discovered_as_project_config() {
    let example = fs::read_to_string(example_config()).unwrap();
    let dir = project(Some(&example));
    assert_eq!(
        config::discover(dir.path()),
        Some(dir.path().join(CONFIG_FILE_NAME))
    );

    let mut expected = Settings::defaults();
    expected.merge(Settings::from_file(&example_config()).unwrap());
    let resolved =
        craft_tidy::app::resolve_settings(&cli(&[dir.path().to_str().unwrap()])).unwrap();
    assert_eq!(resolved, expected);
}

#[test]
fn command_line_flags_override_the_example() {
    let example = example_config();
    let resolved = craft_tidy::app::resolve_settings(&cli(&[
        "--config-file",
        example.to_str().unwrap(),
        "--checks=experimental-x",
        "--warnings-as-errors=-parts-*",
    ]))
    .unwrap();
    assert!(resolved.checks.matches("experimental-x"));
    assert!(!resolved.checks.matches("experimental-y"));
    assert!(resolved.warnings_as_errors.matches("metadata-a"));
    assert!(!resolved.warnings_as_errors.matches("parts-a"));
}

#[test]
fn a_broken_copy_of_the_example_is_rejected() {
    let example = fs::read_to_string(example_config()).unwrap();
    let broken = example.replace("warnings-as-errors:", "warnings-as-error:");
    assert_ne!(broken, example);
    let message = Settings::from_yaml(&broken, "broken")
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("unknown field `warnings-as-error`"),
        "{message}"
    );
}
