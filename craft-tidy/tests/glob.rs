// SPDX-License-Identifier: GPL-3.0-only

//! Tests for check selection globs (`craft_tidy::glob`).

use craft_tidy::glob::GlobList;

fn globs(s: &str) -> GlobList {
    s.parse()
        .unwrap_or_else(|err| panic!("{s:?} should parse: {err}"))
}

// Parsing and formatting

#[test]
fn parses_and_formats_glob_lists() {
    for s in [
        "parts-*",
        "-*,parts-*",
        "*,-metadata-*,-parts-x",
        "a1-b2,-c3*",
    ] {
        assert_eq!(globs(s).to_string(), s);
    }
}

#[test]
fn parsing_trims_whitespace_and_skips_empty_entries() {
    assert_eq!(globs(" -* , parts-* ,,").to_string(), "-*,parts-*");
    assert!(globs("").is_empty());
    assert!(globs(" , ,").is_empty());
}

#[test]
fn match_all_glob_supersedes_earlier_globs() {
    assert_eq!(globs("parts-*,-*,metadata-*").to_string(), "-*,metadata-*");
    assert_eq!(globs("-parts-*,*").to_string(), "*");
}

#[test]
fn rejects_invalid_globs() {
    for bad in [
        "-",
        "--x",
        "Parts-*",
        "parts plugin",
        "parts.plugin",
        "parts_plugin",
        "parts-?",
        "ok,Bad",
    ] {
        let err = bad.parse::<GlobList>().expect_err(bad);
        assert!(err.to_string().contains("invalid check glob"), "{err}");
    }
}

// Matching

#[test]
fn star_matches_any_sequence() {
    assert!(globs("*").matches("parts-plugin"));
    assert!(globs("parts-*").matches("parts-plugin"));
    assert!(globs("parts-*").matches("parts-"));
    assert!(globs("*-plugin").matches("parts-plugin"));
    assert!(globs("p*s-*n").matches("parts-plugin"));
    assert!(globs("a*a*a").matches("aaaaa"));
    assert!(globs("parts-plugin").matches("parts-plugin"));
}

#[test]
fn globs_must_match_the_whole_name() {
    assert!(!globs("parts").matches("parts-plugin"));
    assert!(!globs("plugin").matches("parts-plugin"));
    assert!(!globs("parts-*").matches("metadata-summary"));
    assert!(!globs("a*b").matches("aXbY"));
    assert!(!globs("a*a*a").matches("aa"));
}

#[test]
fn last_matching_glob_wins() {
    let list = globs("-*,parts-*,-parts-deprecated-plugin");
    assert!(list.matches("parts-build-packages"));
    assert!(!list.matches("parts-deprecated-plugin"));
    assert!(!list.matches("metadata-summary"));

    let list = globs("parts-x,-parts-x");
    assert!(!list.matches("parts-x"));
    let list = globs("-parts-x,parts-x");
    assert!(list.matches("parts-x"));
}

#[test]
fn unmatched_names_are_not_selected() {
    assert!(!GlobList::default().matches("anything"));
    assert!(!globs("parts-*").matches("metadata-summary"));
}

#[test]
fn all_matches_every_name() {
    let all = GlobList::all();
    assert_eq!(all.to_string(), "*");
    assert!(all.matches("parts-plugin"));
    assert!(all.matches("x"));
}

// Combining lists

#[test]
fn extend_gives_appended_globs_precedence() {
    let mut list = globs("*");
    list.extend(globs("-metadata-*"));
    assert!(list.matches("parts-plugin"));
    assert!(!list.matches("metadata-summary"));
    assert_eq!(list.to_string(), "*,-metadata-*");

    let mut list = globs("-*,parts-*");
    list.extend(globs("*,-metadata-*"));
    assert_eq!(list.to_string(), "*,-metadata-*");
}

#[test]
fn extending_with_an_empty_list_changes_nothing() {
    let mut list = globs("-*,parts-*");
    list.extend(GlobList::default());
    assert_eq!(list, globs("-*,parts-*"));
}

// YAML (de)serialization

#[test]
fn deserializes_from_string_list_or_null() {
    let expected = globs("-*,parts-*,metadata-*");
    for yaml in [
        "'-*,parts-*,metadata-*'",
        "['-*', parts-*, metadata-*]",
        "['-*,parts-*', metadata-*]",
        "['-*', 'parts-*', \"metadata-*\"]",
        "- '-*'\n- parts-*\n- metadata-*\n",
    ] {
        let parsed: GlobList = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(parsed, expected, "{yaml}");
    }
    let null: GlobList = serde_saphyr::from_str("~").unwrap();
    assert!(null.is_empty());
}

#[test]
fn rejects_non_glob_yaml_values() {
    for yaml in [
        "42",
        "true",
        "{a: b}",
        "[1, 2]",
        "[ok, true]",
        "[ok, ~]",
        "[[a]]",
        "[{a: b}]",
        "'Bad'",
        "[ok, Bad]",
    ] {
        assert!(
            serde_saphyr::from_str::<GlobList>(yaml).is_err(),
            "{yaml} should be rejected"
        );
    }
}

#[test]
fn serializes_as_a_string() {
    let yaml = serde_saphyr::to_string(&globs("*,-parts-*")).unwrap();
    assert_eq!(yaml.trim(), "\"*,-parts-*\"");
    let parsed: GlobList = serde_saphyr::from_str(&yaml).unwrap();
    assert_eq!(parsed, globs("*,-parts-*"));
}
