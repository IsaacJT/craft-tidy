// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the `project-file-location` check.

use std::path::{Path, PathBuf};

use craft_tidy::checks::{self, Diagnostic};
use craft_tidy::config::Settings;
use craft_tidy::project::Project;

const NAME: &str = "project-file-location";

fn settings() -> Settings {
    let mut settings = Settings::defaults();
    settings.checks = format!("-*,{NAME}").parse().unwrap();
    settings
}

fn diagnostics(path: &Path) -> Vec<Diagnostic> {
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new(path, "name: test\n").unwrap();
    checks::run(&checks::enabled(&registry, &settings), &project, &settings)
}

fn messages(path: &Path) -> Vec<String> {
    diagnostics(path).into_iter().map(|d| d.message).collect()
}

fn in_tempdir(relative: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(relative);
    (dir, path)
}

#[test]
fn is_registered_without_options() {
    let registry = checks::registry();
    let check = registry.iter().find(|c| c.name() == NAME).expect(NAME);
    assert!(check.options().is_empty());
}

// Accepted locations

#[test]
fn accepts_project_files_in_snap_directories() {
    for relative in [
        "snap/snapcraft.yaml",
        "build-aux/snap/snapcraft.yaml",
        "snap/.snapcraft.yaml",
    ] {
        let (_dir, path) = in_tempdir(relative);
        assert!(messages(&path).is_empty(), "{relative}");
    }
}

#[test]
fn accepts_relative_paths_inside_snap_directories() {
    assert!(messages(Path::new("snap/snapcraft.yaml")).is_empty());
    assert!(messages(Path::new("./build-aux/snap/snapcraft.yaml")).is_empty());
}

#[test]
fn ignores_files_snapcraft_would_not_find_in_the_root() {
    for relative in [
        "custom.yaml",
        "snapcraft.yml",
        "my-snapcraft.yaml",
        "snap.yaml",
    ] {
        let (_dir, path) = in_tempdir(relative);
        assert!(messages(&path).is_empty(), "{relative}");
    }
}

// Reported locations

#[test]
fn reports_snapcraft_yaml_in_the_project_root() {
    let (_dir, path) = in_tempdir("snapcraft.yaml");
    let diagnostics = diagnostics(&path);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].check, NAME);
    assert_eq!(
        diagnostics[0].message,
        "snapcraft.yaml is in the project root; move it to snap/snapcraft.yaml"
    );
    assert_eq!(diagnostics[0].location, None);
}

#[test]
fn reports_hidden_snapcraft_yaml_in_the_project_root() {
    let (_dir, path) = in_tempdir(".snapcraft.yaml");
    assert_eq!(
        messages(&path),
        [".snapcraft.yaml is in the project root; move it to snap/snapcraft.yaml"]
    );
}

#[test]
fn only_a_directory_named_exactly_snap_counts() {
    for relative in [
        "snaps/snapcraft.yaml",
        "Snap/snapcraft.yaml",
        "snap-files/snapcraft.yaml",
        "build-aux/snapcraft.yaml",
    ] {
        let (_dir, path) = in_tempdir(relative);
        assert_eq!(messages(&path).len(), 1, "{relative}");
    }
}

#[test]
fn reports_relative_paths_in_the_root() {
    assert_eq!(messages(Path::new("snapcraft.yaml")).len(), 1);
}

#[test]
fn does_not_depend_on_the_file_contents() {
    let (_dir, path) = in_tempdir("snapcraft.yaml");
    let registry = checks::registry();
    let settings = settings();
    for source in ["", "name: test\n", "# just a comment\n"] {
        let project = Project::new(&path, source).unwrap();
        let found = checks::run(&checks::enabled(&registry, &settings), &project, &settings);
        assert_eq!(found.len(), 1, "{source:?}");
    }
}

#[test]
fn is_not_fixed_automatically() {
    let (_dir, path) = in_tempdir("snapcraft.yaml");
    let registry = checks::registry();
    let settings = settings();
    let project = Project::new(&path, "name: test\n").unwrap();
    assert!(checks::fix(&checks::enabled(&registry, &settings), &project, &settings).is_none());
}
