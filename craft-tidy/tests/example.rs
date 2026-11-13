// SPDX-License-Identifier: GPL-3.0-only

//! Tests keeping `examples/.craft-tidy` valid and complete.

mod common;

use std::collections::BTreeSet;
use std::fs;

use craft_tidy::checks;
use craft_tidy::config::{self, CONFIG_FILE_NAME, OptionValue, Settings};

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
    assert_eq!(settings.checks.to_string(), "*");
    assert!(settings.warnings_as_errors.is_empty());
    assert_eq!(settings.fix, Some(false));
    assert_eq!(
        settings
            .check_options
            .get("style-line-length", "max-length"),
        Some(&OptionValue::Integer(80))
    );
    assert_eq!(
        settings
            .check_options
            .get("style-sorted-lists", "case-sensitive"),
        Some(&OptionValue::Bool(false))
    );
}

#[test]
fn example_is_the_default_configuration() {
    let mut from_example = Settings::defaults();
    from_example.merge(Settings::from_file(&example_config()).unwrap());
    let mut expected = Settings::defaults();
    for check in checks::registry() {
        for spec in check.options() {
            expected
                .check_options
                .set(check.name(), spec.name, OptionValue::parse(spec.default));
        }
    }
    let registry = checks::registry();
    let enabled = |settings: &Settings| -> Vec<_> {
        checks::enabled(&registry, settings)
            .iter()
            .map(|check| check.name())
            .collect()
    };
    assert_eq!(enabled(&from_example), enabled(&expected));
    for check in &registry {
        assert_eq!(
            from_example.warnings_as_errors.matches(check.name()),
            expected.warnings_as_errors.matches(check.name()),
            "{}",
            check.name()
        );
    }
    assert_eq!(from_example.fix, expected.fix);
}

#[test]
fn example_shows_the_default_option_values() {
    let settings = Settings::from_file(&example_config()).unwrap();
    let registry = checks::registry();
    // Glob lists may continue on the next line, which YAML folds to ", ".
    let normalize = |value: &OptionValue| match value.as_str() {
        Some(s) => OptionValue::String(s.replace(", ", ",")),
        None => value.clone(),
    };
    for (check_name, option, value) in settings.check_options.iter() {
        let check = registry.iter().find(|c| c.name() == check_name).unwrap();
        let spec = check.options().iter().find(|s| s.name == option).unwrap();
        assert_eq!(
            normalize(value),
            normalize(&OptionValue::parse(spec.default)),
            "examples/.craft-tidy should show the default of {check_name}.{option}"
        );
    }
}

#[test]
fn example_shows_every_option_of_every_check() {
    let settings = Settings::from_file(&example_config()).unwrap();
    for check in checks::registry() {
        for spec in check.options() {
            assert!(
                settings
                    .check_options
                    .get(check.name(), spec.name)
                    .is_some(),
                "examples/.craft-tidy should document {}.{}",
                check.name(),
                spec.name
            );
        }
    }
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
        "--checks=-style-sorted-lists",
        "--warnings-as-errors=project-*",
        "--fix",
        "-o",
        "style-line-length.max-length=100",
    ]))
    .unwrap();
    assert!(resolved.checks.matches("style-line-length"));
    assert!(!resolved.checks.matches("style-sorted-lists"));
    assert!(resolved.warnings_as_errors.matches("project-file-location"));
    assert!(!resolved.warnings_as_errors.matches("style-line-length"));
    assert_eq!(resolved.fix, Some(true));
    assert_eq!(
        resolved
            .check_options
            .get("style-line-length", "max-length"),
        Some(&OptionValue::Integer(100))
    );
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
