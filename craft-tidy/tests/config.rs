// SPDX-License-Identifier: GPL-3.0-only

//! Tests for configuration settings (`craft_tidy::config`).

use std::fs;

use craft_tidy::config::{self, CONFIG_FILE_NAME, CheckOptions, OptionValue, Settings};
use craft_tidy::error::Error;

fn parse(yaml: &str) -> Settings {
    Settings::from_yaml(yaml, "test").unwrap_or_else(|err| panic!("{yaml:?} should parse: {err}"))
}

fn parse_err(yaml: &str) -> String {
    match Settings::from_yaml(yaml, "test") {
        Ok(settings) => panic!("{yaml:?} should be rejected, got {settings:?}"),
        Err(err @ Error::Config { .. }) => err.to_string(),
        Err(err) => panic!("unexpected error kind: {err:?}"),
    }
}

fn int(i: i64) -> OptionValue {
    OptionValue::Integer(i)
}

fn string(s: &str) -> OptionValue {
    OptionValue::String(s.to_owned())
}

// Parsing a configuration layer

#[test]
fn parses_all_settings() {
    let settings = parse(
        "checks: '-*,parts-*'\n\
         warnings-as-errors: [parts-*]\n\
         check-options:\n  \
           metadata-summary-length:\n    \
             max-length: 79\n    \
             strict: true\n    \
             ratio: 0.5\n    \
             label: '79'\n    \
             name: hello\n",
    );
    assert_eq!(settings.checks.to_string(), "-*,parts-*");
    assert_eq!(settings.warnings_as_errors.to_string(), "parts-*");
    let get = |key| settings.check_options.get("metadata-summary-length", key);
    assert_eq!(get("max-length"), Some(&int(79)));
    assert_eq!(get("strict"), Some(&OptionValue::Bool(true)));
    assert_eq!(get("ratio"), Some(&OptionValue::Float(0.5)));
    assert_eq!(get("label"), Some(&string("79")));
    assert_eq!(get("name"), Some(&string("hello")));
}

#[test]
fn every_key_is_optional() {
    assert_eq!(parse(""), Settings::default());
    assert_eq!(parse("# only a comment\n"), Settings::default());
    assert_eq!(parse("{}"), Settings::default());
    assert_eq!(parse("checks: x\n").warnings_as_errors, Default::default());
    assert!(parse("checks: x\n").check_options.is_empty());
}

#[test]
fn empty_glob_values_are_empty_lists() {
    let settings = parse("checks:\nwarnings-as-errors: ''\n");
    assert!(settings.checks.is_empty());
    assert!(settings.warnings_as_errors.is_empty());
}

#[test]
fn rejects_unknown_keys_with_location() {
    let message = Settings::from_yaml("checks: '*'\nchekcs: x\n", "my-config")
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("invalid configuration in my-config"),
        "{message}"
    );
    assert!(message.contains("unknown field `chekcs`"), "{message}");
    assert!(message.contains("my-config:2:1"), "{message}");
}

#[test]
fn rejects_duplicate_keys() {
    assert!(parse_err("checks: a\nchecks: b\n").contains("duplicate"));
    assert!(parse_err("check-options:\n  x:\n    k: 1\n    k: 2\n").contains("duplicate"));
}

#[test]
fn rejects_invalid_structure() {
    for yaml in [
        "just a string\n",
        "- a\n- b\n",
        "checks: {a: b}\n",
        "checks: 42\n",
        "checks: Bad-Glob\n",
        "check-options: 3\n",
        "check-options: [a]\n",
        "check-options:\n  x: 3\n",
        "checks: a\n---\nchecks: b\n",
        "checks: [a\n",
    ] {
        parse_err(yaml);
    }
}

#[test]
fn rejects_non_scalar_option_values() {
    for value in ["~", "[1, 2]", "{a: b}", "18446744073709551615"] {
        let message = parse_err(&format!("check-options:\n  x:\n    k: {value}\n"));
        assert!(message.contains("test:3:"), "{message}");
    }
    let message = parse_err("check-options:\n  x:\n    k: [1]\n");
    assert!(
        message.contains("a boolean, integer, float or string"),
        "{message}"
    );
}

// Reading files

#[test]
fn reads_settings_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CONFIG_FILE_NAME);
    fs::write(&path, "checks: parts-*\n").unwrap();
    assert_eq!(
        Settings::from_file(&path).unwrap().checks.to_string(),
        "parts-*"
    );
}

#[test]
fn file_errors_name_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CONFIG_FILE_NAME);
    fs::write(&path, "bogus: 1\n").unwrap();
    let message = Settings::from_file(&path).unwrap_err().to_string();
    assert!(message.contains(&path.display().to_string()), "{message}");
}

#[test]
fn missing_or_unreadable_files_are_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    assert!(matches!(
        Settings::from_file(&missing),
        Err(Error::Io { path, .. }) if path == missing
    ));
    assert!(matches!(
        Settings::from_file(dir.path()),
        Err(Error::Io { .. })
    ));
    let binary = dir.path().join("binary");
    fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
    assert!(matches!(
        Settings::from_file(&binary),
        Err(Error::Io { .. })
    ));
}

// Defaults and merging

#[test]
fn defaults_enable_all_checks_as_warnings() {
    let defaults = Settings::defaults();
    assert!(defaults.checks.matches("any-check"));
    assert!(defaults.warnings_as_errors.is_empty());
    assert!(defaults.check_options.is_empty());
}

#[test]
fn merge_appends_globs() {
    let mut settings = Settings::defaults();
    settings.merge(parse("checks: -parts-*\nwarnings-as-errors: metadata-*\n"));
    settings.merge(parse("checks: parts-x\n"));
    assert_eq!(settings.checks.to_string(), "*,-parts-*,parts-x");
    assert!(settings.checks.matches("parts-x"));
    assert!(!settings.checks.matches("parts-y"));
    assert_eq!(settings.warnings_as_errors.to_string(), "metadata-*");
}

#[test]
fn merge_overrides_check_options_per_key() {
    let mut settings = parse("check-options: {a: {x: 1, y: 2}, b: {z: 3}}");
    settings.merge(parse("check-options: {a: {y: 20, w: 40}}"));
    let opts = &settings.check_options;
    assert_eq!(opts.get("a", "x"), Some(&int(1)));
    assert_eq!(opts.get("a", "y"), Some(&int(20)));
    assert_eq!(opts.get("a", "w"), Some(&int(40)));
    assert_eq!(opts.get("b", "z"), Some(&int(3)));
}

#[test]
fn merging_an_empty_layer_changes_nothing() {
    let original = parse("checks: -*,parts-*\ncheck-options: {a: {x: 1}}\n");
    let mut settings = original.clone();
    settings.merge(Settings::default());
    assert_eq!(settings, original);
}

// Serialization

#[test]
fn serialized_settings_parse_back_identically() {
    let mut settings = Settings::defaults();
    settings.merge(parse(
        "checks: -metadata-*\n\
         warnings-as-errors: parts-*\n\
         check-options: {c: {s: 'true', n: '42', f: 1.5, b: false, i: -3, e: ''}}\n",
    ));
    let yaml = settings.to_yaml().unwrap();
    assert_eq!(parse(&yaml), settings, "{yaml}");
}

#[test]
fn serialized_defaults_list_every_key() {
    let yaml = Settings::defaults().to_yaml().unwrap();
    for key in ["checks:", "warnings-as-errors:", "check-options:"] {
        assert!(yaml.contains(key), "{yaml}");
    }
    assert_eq!(parse(&yaml), Settings::defaults());
}

// Check options

#[test]
fn check_options_get_and_set() {
    let mut opts = CheckOptions::default();
    assert!(opts.is_empty());
    assert_eq!(opts.get("a", "x"), None);

    opts.set("a", "x", int(1));
    opts.set("a", "x", int(2));
    opts.set("b", "y", OptionValue::Bool(true));
    assert!(!opts.is_empty());
    assert_eq!(opts.get("a", "x"), Some(&int(2)));
    assert_eq!(opts.get("a", "y"), None);
    assert_eq!(opts.get("c", "x"), None);
    assert_eq!(opts.checks().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn option_value_parse_uses_yaml_scalar_types() {
    assert_eq!(OptionValue::parse("79"), int(79));
    assert_eq!(OptionValue::parse("-3"), int(-3));
    assert_eq!(OptionValue::parse("true"), OptionValue::Bool(true));
    assert_eq!(OptionValue::parse("false"), OptionValue::Bool(false));
    assert_eq!(OptionValue::parse("1.5"), OptionValue::Float(1.5));
    assert_eq!(OptionValue::parse("'79'"), string("79"));
    assert_eq!(OptionValue::parse("\"true\""), string("true"));
    assert_eq!(OptionValue::parse("hello"), string("hello"));
}

#[test]
fn option_value_parse_falls_back_to_plain_strings() {
    for raw in ["", "  ", "a: b", "[1, 2", "~", "x=y"] {
        assert_eq!(OptionValue::parse(raw), string(raw), "{raw:?}");
    }
}

#[test]
fn option_value_accessors() {
    assert_eq!(OptionValue::Bool(true).as_bool(), Some(true));
    assert_eq!(int(7).as_i64(), Some(7));
    assert_eq!(int(7).as_f64(), Some(7.0));
    assert_eq!(OptionValue::Float(0.5).as_f64(), Some(0.5));
    assert_eq!(string("s").as_str(), Some("s"));

    assert_eq!(int(1).as_bool(), None);
    assert_eq!(string("true").as_bool(), None);
    assert_eq!(OptionValue::Float(1.0).as_i64(), None);
    assert_eq!(string("7").as_i64(), None);
    assert_eq!(OptionValue::Bool(true).as_f64(), None);
    assert_eq!(int(1).as_str(), None);
}

// Discovery

#[test]
fn discovers_config_in_start_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(CONFIG_FILE_NAME), "").unwrap();
    assert_eq!(
        config::discover(dir.path()),
        Some(dir.path().join(CONFIG_FILE_NAME))
    );
}

#[test]
fn discovers_nearest_config_in_ancestors() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("a/b");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(config::discover(&nested), None);

    fs::write(root.path().join(CONFIG_FILE_NAME), "").unwrap();
    assert_eq!(
        config::discover(&nested),
        Some(root.path().join(CONFIG_FILE_NAME))
    );

    fs::write(root.path().join("a").join(CONFIG_FILE_NAME), "").unwrap();
    assert_eq!(
        config::discover(&nested),
        Some(root.path().join("a").join(CONFIG_FILE_NAME))
    );
}

#[test]
fn discovery_ignores_directories_named_like_the_config() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(CONFIG_FILE_NAME)).unwrap();
    assert_eq!(config::discover(dir.path()), None);
}

#[test]
fn discovery_from_missing_directory_still_searches_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(CONFIG_FILE_NAME), "").unwrap();
    assert_eq!(
        config::discover(&dir.path().join("does/not/exist")),
        Some(dir.path().join(CONFIG_FILE_NAME))
    );
}
