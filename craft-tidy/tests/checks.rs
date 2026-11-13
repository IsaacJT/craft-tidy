// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the check framework (`craft_tidy::checks`).

mod common;

use craft_tidy::checks::{self, Check, Diagnostic, OptionKind, Severity};
use craft_tidy::config::{CheckOptions, OptionValue, Settings};
use craft_tidy::craft::Craft;
use craft_tidy::error::Error;
use craft_tidy::project::Project;

use common::FakeCheck;

fn project() -> Project {
    Project::new("snapcraft.yaml", "name: test\n").unwrap()
}

fn settings(checks: &str, warnings_as_errors: &str) -> Settings {
    let mut settings = Settings::defaults();
    settings.checks.extend(checks.parse().unwrap());
    settings.warnings_as_errors = warnings_as_errors.parse().unwrap();
    settings
}

fn names(checks: &[&dyn Check]) -> Vec<&'static str> {
    checks.iter().map(|check| check.name()).collect()
}

fn summary(diagnostics: &[Diagnostic]) -> Vec<(&'static str, Severity, &str)> {
    diagnostics
        .iter()
        .map(|d| (d.check, d.severity, d.message.as_str()))
        .collect()
}

fn registry() -> Vec<Box<dyn Check>> {
    vec![
        FakeCheck::new("metadata-a")
            .finding("meta", Some((1, 1)))
            .boxed(),
        FakeCheck::new("parts-a")
            .finding("part a", Some((3, 5)))
            .boxed(),
        FakeCheck::new("parts-b")
            .finding("part b", Some((2, 1)))
            .boxed(),
    ]
}

// Diagnostics

#[test]
fn diagnostics_start_as_located_or_unlocated_warnings() {
    let diagnostic = Diagnostic::warning("check", "message");
    assert_eq!(diagnostic.check, "check");
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert_eq!(diagnostic.message, "message");
    assert_eq!(diagnostic.location, None);
    assert_eq!(diagnostic.at(4, 2).location, Some((4, 2)));
}

#[test]
fn diagnostics_can_say_why_they_cannot_be_fixed() {
    let diagnostic = Diagnostic::warning("parts-a", "message");
    assert_eq!(diagnostic.unfixable, None);
    let diagnostic = diagnostic.unfixable("a reason").at(1, 2);
    assert_eq!(diagnostic.unfixable.as_deref(), Some("a reason"));
    assert_eq!(diagnostic.location, Some((1, 2)));
}

#[test]
fn severities_display_in_lowercase_and_are_ordered() {
    assert_eq!(Severity::Warning.to_string(), "warning");
    assert_eq!(Severity::Error.to_string(), "error");
    assert!(Severity::Warning < Severity::Error);
}

// Option specifications

#[test]
fn option_kinds_accept_matching_values() {
    use OptionKind::*;
    let cases = [
        (Bool, OptionValue::Bool(true)),
        (Integer, OptionValue::Integer(3)),
        (Float, OptionValue::Float(0.5)),
        (Float, OptionValue::Integer(3)),
        (String, OptionValue::String("s".into())),
    ];
    for (kind, value) in cases {
        assert!(kind.accepts(&value), "{kind} should accept {value:?}");
    }
}

#[test]
fn option_kinds_reject_other_values() {
    use OptionKind::*;
    let cases = [
        (Bool, OptionValue::Integer(1)),
        (Bool, OptionValue::String("true".into())),
        (Integer, OptionValue::Float(1.0)),
        (Integer, OptionValue::String("1".into())),
        (Float, OptionValue::Bool(false)),
        (String, OptionValue::Integer(1)),
        (String, OptionValue::Bool(true)),
    ];
    for (kind, value) in cases {
        assert!(!kind.accepts(&value), "{kind} should reject {value:?}");
    }
}

#[test]
fn option_kinds_display_their_names() {
    let names: Vec<_> = [
        OptionKind::Bool,
        OptionKind::Integer,
        OptionKind::Float,
        OptionKind::String,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(names, ["boolean", "integer", "float", "string"]);
}

#[test]
fn validate_options_accepts_valid_options_and_lists_unknown_checks() {
    let registry = registry();
    let mut options = CheckOptions::default();
    assert_eq!(
        checks::validate_options(&registry, &options).unwrap(),
        Vec::<&str>::new()
    );

    options.set("parts-a", "extra", OptionValue::String("x".into()));
    options.set("parts-b", "ratio", OptionValue::Integer(1));
    options.set("zzz-unknown", "a", OptionValue::Bool(true));
    options.set("zzz-unknown", "b", OptionValue::Bool(true));
    options.set("aaa-unknown", "a", OptionValue::Bool(true));
    assert_eq!(
        checks::validate_options(&registry, &options).unwrap(),
        ["aaa-unknown", "zzz-unknown"]
    );
}

#[test]
fn validate_options_rejects_invalid_options() {
    let registry = registry();
    for (name, value, message) in [
        (
            "nope",
            OptionValue::Integer(1),
            "unknown option (available: extra, ratio)",
        ),
        (
            "extra",
            OptionValue::Integer(1),
            "expected a value of type string",
        ),
        ("ratio", OptionValue::Float(2.0), "must be between 0 and 1"),
    ] {
        let mut options = CheckOptions::default();
        options.set("parts-a", name, value);
        match checks::validate_options(&registry, &options) {
            Err(Error::InvalidOption {
                check,
                option,
                message: actual,
            }) => {
                assert_eq!((check.as_str(), option.as_str()), ("parts-a", name));
                assert_eq!(actual, message);
            }
            other => panic!("{name} should be rejected, got {other:?}"),
        }
    }
}

#[test]
fn validate_options_rejects_options_for_checks_without_options() {
    struct Bare;
    impl Check for Bare {
        fn name(&self) -> &'static str {
            "bare-check"
        }
        fn summary(&self) -> &'static str {
            ""
        }
        fn explanation(&self) -> &'static str {
            ""
        }
        fn run(&self, _: &craft_tidy::checks::Context<'_>) -> Vec<Diagnostic> {
            Vec::new()
        }
    }
    let registry: Vec<Box<dyn Check>> = vec![Box::new(Bare)];
    let mut options = CheckOptions::default();
    options.set("bare-check", "x", OptionValue::Bool(true));
    let err = checks::validate_options(&registry, &options).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid option 'bare-check.x': the check has no options"
    );
}

// Registry

#[test]
fn registry_is_sorted_with_unique_valid_names() {
    let registry = checks::registry();
    let names: Vec<_> = registry.iter().map(|c| c.name()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names, sorted, "registry must be sorted and unique");
    for check in &registry {
        let name = check.name();
        assert!(
            name.contains('-')
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{name} must be kebab-case with a category prefix"
        );
        assert!(!check.summary().is_empty(), "{name} needs a summary");
        assert!(
            !check.explanation().is_empty(),
            "{name} needs an explanation"
        );
        for spec in check.options() {
            let option = format!("{name}.{}", spec.name);
            assert!(
                !spec.name.is_empty()
                    && spec
                        .name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{option} must be kebab-case"
            );
            assert!(!spec.description.is_empty(), "{option} needs a description");
            let default = OptionValue::parse(spec.default);
            assert!(
                spec.kind.accepts(&default),
                "{option}: default {:?} is not a {}",
                spec.default,
                spec.kind
            );
            assert_eq!(
                check.validate_option(spec.name, &default),
                Ok(()),
                "{option}: default must be valid"
            );
        }
    }
}

// Selecting checks

#[test]
fn all_checks_are_enabled_by_default() {
    let registry = registry();
    assert_eq!(
        names(&checks::enabled(&registry, &Settings::defaults())),
        ["metadata-a", "parts-a", "parts-b"]
    );
}

#[test]
fn checks_globs_select_checks() {
    let registry = registry();
    let enabled = |globs| names(&checks::enabled(&registry, &settings(globs, "")));
    assert_eq!(enabled("-*,parts-*"), ["parts-a", "parts-b"]);
    assert_eq!(enabled("-parts-*"), ["metadata-a"]);
    assert_eq!(enabled("-*,parts-*,-parts-b"), ["parts-a"]);
}

#[test]
fn no_checks_are_enabled_when_all_are_disabled() {
    let registry = registry();
    assert!(checks::enabled(&registry, &settings("-*", "")).is_empty());
    assert!(checks::enabled(&registry, &settings("-*,unknown-*", "")).is_empty());
    assert!(checks::enabled(&[], &Settings::defaults()).is_empty());
}

// Running checks

#[test]
fn runs_enabled_checks_sorted_by_location() {
    let registry = registry();
    let settings = Settings::defaults();
    let diagnostics = checks::run(
        &checks::enabled(&registry, &settings),
        &project(),
        &settings,
    );
    assert_eq!(
        summary(&diagnostics),
        [
            ("metadata-a", Severity::Warning, "meta"),
            ("parts-b", Severity::Warning, "part b"),
            ("parts-a", Severity::Warning, "part a"),
        ]
    );
}

#[test]
fn checks_only_run_and_fix_for_the_crafts_they_apply_to() {
    let snap_only = FakeCheck::new("parts-a")
        .finding("x", None)
        .replacing("foo", "bar")
        .only_for(&[Craft::Snapcraft]);
    let (runs, fixes) = (snap_only.runs.clone(), snap_only.fixes.clone());
    let settings = Settings::defaults();
    let checks: [&dyn Check; 1] = [&snap_only];

    let rock = Project::new("rockcraft.yaml", "a: foo\n").unwrap();
    let other = Project::new("custom.yaml", "a: foo\n").unwrap();
    for project in [&rock, &other] {
        assert!(checks::run(&checks, project, &settings).is_empty());
        assert!(checks::fix(&checks, project, &settings).is_none());
    }
    assert_eq!((runs.get(), fixes.get()), (0, 0));

    let snap = Project::new("snap/snapcraft.yaml", "a: foo\n").unwrap();
    assert_eq!(checks::run(&checks, &snap, &settings).len(), 2);
    let fixed = checks::fix(&checks, &snap, &settings).unwrap();
    assert_eq!(fixed.source(), "a: bar\n");
}

#[test]
fn applies_to_matches_crafts() {
    let all = FakeCheck::new("parts-a");
    assert!(checks::applies_to(&all, None));
    assert!(checks::applies_to(&all, Some(Craft::Rockcraft)));
    let some = FakeCheck::new("parts-a").only_for(&[Craft::Charmcraft, Craft::Rockcraft]);
    assert!(checks::applies_to(&some, Some(Craft::Rockcraft)));
    assert!(!checks::applies_to(&some, Some(Craft::Snapcraft)));
    assert!(!checks::applies_to(&some, None));
}

#[test]
fn disabled_checks_are_not_run() {
    let enabled = FakeCheck::new("parts-a").finding("x", None);
    let disabled = FakeCheck::new("parts-b").finding("y", None);
    let (enabled_runs, disabled_runs) = (enabled.runs.clone(), disabled.runs.clone());
    let registry = vec![enabled.boxed(), disabled.boxed()];
    let settings = settings("-parts-b", "");

    let diagnostics = checks::run(
        &checks::enabled(&registry, &settings),
        &project(),
        &settings,
    );
    assert_eq!(summary(&diagnostics), [("parts-a", Severity::Warning, "x")]);
    assert_eq!(enabled_runs.get(), 1);
    assert_eq!(disabled_runs.get(), 0);
}

#[test]
fn checks_without_findings_report_nothing() {
    let registry = vec![FakeCheck::new("parts-a").boxed()];
    let settings = Settings::defaults();
    let enabled = checks::enabled(&registry, &settings);
    assert!(checks::run(&enabled, &project(), &settings).is_empty());
    assert!(checks::run(&[], &project(), &settings).is_empty());
}

#[test]
fn warnings_as_errors_promotes_matching_checks_only() {
    let registry = registry();
    let settings = settings("*", "parts-*,-parts-b");
    let diagnostics = checks::run(
        &checks::enabled(&registry, &settings),
        &project(),
        &settings,
    );
    assert_eq!(
        summary(&diagnostics),
        [
            ("metadata-a", Severity::Warning, "meta"),
            ("parts-b", Severity::Warning, "part b"),
            ("parts-a", Severity::Error, "part a"),
        ]
    );
}

#[test]
fn checks_receive_their_own_options_only() {
    let registry = vec![
        FakeCheck::new("parts-a").boxed(),
        FakeCheck::new("parts-b").boxed(),
    ];
    let mut settings = Settings::defaults();
    settings.check_options.set(
        "parts-a",
        "extra",
        OptionValue::String("from option".into()),
    );
    settings
        .check_options
        .set("parts-b", "other", OptionValue::String("ignored".into()));

    let diagnostics = checks::run(
        &checks::enabled(&registry, &settings),
        &project(),
        &settings,
    );
    assert_eq!(
        summary(&diagnostics),
        [("parts-a", Severity::Warning, "from option")]
    );
}

// Fixing

fn project_with(source: &str) -> Project {
    Project::new("snap/snapcraft.yaml", source).unwrap()
}

fn fix_with(checks: Vec<Box<dyn Check>>, source: &str) -> Option<String> {
    let settings = Settings::defaults();
    let enabled = checks::enabled(&checks, &settings);
    checks::fix(&enabled, &project_with(source), &settings).map(|p| p.source().to_owned())
}

#[test]
fn fix_applies_the_fixes_of_enabled_checks() {
    let fixed = checks::fix(
        &checks::enabled(
            &[FakeCheck::new("parts-a").replacing("old", "new").boxed()],
            &Settings::defaults(),
        ),
        &project_with("a: old\nb: [old]\n"),
        &Settings::defaults(),
    )
    .unwrap();
    assert_eq!(fixed.path, std::path::Path::new("snap/snapcraft.yaml"));
    assert_eq!(fixed.source(), "a: new\nb: [new]\n");
    assert!(fixed.document.root().is_some());
}

#[test]
fn fix_returns_none_when_nothing_changes() {
    assert_eq!(
        fix_with(vec![FakeCheck::new("parts-a").boxed()], "a: x\n"),
        None
    );
    assert_eq!(
        fix_with(
            vec![FakeCheck::new("parts-a").replacing("old", "new").boxed()],
            "a: x\n"
        ),
        None
    );
    assert_eq!(fix_with(vec![], "a: old\n"), None);
}

#[test]
fn fix_skips_disabled_checks() {
    let check = FakeCheck::new("parts-a").replacing("old", "new");
    let fixes = check.fixes.clone();
    let registry = vec![check.boxed()];
    let settings = settings("-parts-a", "");
    let fixed = checks::fix(
        &checks::enabled(&registry, &settings),
        &project_with("a: old\n"),
        &settings,
    );
    assert!(fixed.is_none());
    assert_eq!(fixes.get(), 0);
}

#[test]
fn later_checks_see_earlier_fixes() {
    let fixed = fix_with(
        vec![
            FakeCheck::new("a-first").replacing("one", "two").boxed(),
            FakeCheck::new("b-second").replacing("two", "three").boxed(),
        ],
        "a: one\n",
    );
    assert_eq!(fixed.as_deref(), Some("a: three\n"));
}

#[test]
fn fixing_repeats_until_nothing_changes() {
    // b-second runs first but can only act once a-first has fixed.
    let fixed = fix_with(
        vec![
            FakeCheck::new("b-second").replacing("two", "three").boxed(),
            FakeCheck::new("a-first").replacing("one", "two").boxed(),
        ],
        "a: one\n",
    );
    assert_eq!(fixed.as_deref(), Some("a: three\n"));
}

#[test]
fn conflicting_fixes_terminate() {
    let check = FakeCheck::new("a-ping").replacing("ping", "pong");
    let fixes = check.fixes.clone();
    let fixed = fix_with(
        vec![
            check.boxed(),
            FakeCheck::new("b-pong").replacing("pong", "ping").boxed(),
        ],
        "a: ping\n",
    );
    // Each pass turns ping into pong and back, so nothing changes overall.
    assert_eq!(fixed, None);
    assert!((2..=10).contains(&fixes.get()), "{} passes", fixes.get());
}

#[test]
fn wrongly_typed_options_are_ignored_by_typed_accessors() {
    let registry = vec![FakeCheck::new("parts-a").boxed()];
    let mut settings = Settings::defaults();
    settings
        .check_options
        .set("parts-a", "extra", OptionValue::Integer(1));
    let diagnostics = checks::run(
        &checks::enabled(&registry, &settings),
        &project(),
        &settings,
    );
    assert!(diagnostics.is_empty());
}
