// SPDX-License-Identifier: GPL-3.0-only

//! Tests for locating and loading project files (`craft_tidy::project`).

use std::fs;
use std::path::Path;

use craft_tidy::error::Error;
use craft_tidy::project::{self, PROJECT_FILE_LOCATIONS, Project};
use craft_tidy::yaml::Value;

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn finds_project_file_in_each_location() {
    for location in PROJECT_FILE_LOCATIONS {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), location, "name: test\n");
        assert_eq!(
            project::find(dir.path()).unwrap(),
            dir.path().join(location)
        );
    }
}

#[test]
fn prefers_locations_in_snapcraft_search_order() {
    let dir = tempfile::tempdir().unwrap();
    for location in PROJECT_FILE_LOCATIONS.iter().rev() {
        write(dir.path(), location, "");
        assert_eq!(
            project::find(dir.path()).unwrap(),
            dir.path().join(location)
        );
    }
}

#[test]
fn accepts_a_project_file_path() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "custom.yaml", "name: test\n");
    let file = dir.path().join("custom.yaml");
    assert_eq!(project::find(&file).unwrap(), file);
}

#[test]
fn load_reads_the_project_file() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "snap/snapcraft.yaml", "name: test\n");
    let project = Project::load(dir.path()).unwrap();
    assert_eq!(project.path, dir.path().join("snap/snapcraft.yaml"));
    assert_eq!(project.source(), "name: test\n");
    let scalar = |s: &str| Value::Scalar {
        value: s.into(),
        plain: true,
    };
    assert_eq!(
        project.document.value(),
        Some(Value::Mapping(vec![(scalar("name"), scalar("test"))]))
    );
}

#[test]
fn new_parses_the_source() {
    let project = Project::new("snapcraft.yaml", "a: [b]\n").unwrap();
    assert!(project.document.root().is_some());
    let empty = Project::new("snapcraft.yaml", "").unwrap();
    assert!(empty.document.root().is_none());
}

#[test]
fn invalid_yaml_is_a_located_syntax_error() {
    let err = Project::new("snap/snapcraft.yaml", "name: test\nparts: [a\n").unwrap_err();
    let Error::Syntax { path, error } = &err else {
        panic!("expected a syntax error, got {err:?}");
    };
    assert_eq!(path, Path::new("snap/snapcraft.yaml"));
    assert_eq!((error.line, error.column), (2, 8));
    assert_eq!(
        err.to_string(),
        "snap/snapcraft.yaml:2:8: invalid YAML: unclosed bracket '['"
    );
}

#[test]
fn load_reports_invalid_yaml() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "snapcraft.yaml", "a: b\n  c: d\n");
    assert!(matches!(
        Project::load(dir.path()),
        Err(Error::Syntax { .. })
    ));
}

#[test]
fn empty_directory_has_no_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(path)) if path == dir.path()
    ));
    assert!(matches!(
        Project::load(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));
}

#[test]
fn missing_path_has_no_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        project::find(&dir.path().join("missing")),
        Err(Error::ProjectNotFound(_))
    ));
}

#[test]
fn ignores_directories_at_project_file_locations() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("snap/snapcraft.yaml")).unwrap();
    fs::create_dir_all(dir.path().join("snapcraft.yaml")).unwrap();
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));

    write(dir.path(), ".snapcraft.yaml", "");
    assert_eq!(
        project::find(dir.path()).unwrap(),
        dir.path().join(".snapcraft.yaml")
    );
}

#[test]
fn ignores_unrelated_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "snapcraft.yml", "");
    write(dir.path(), "snap/snapcraft.yml", "");
    write(dir.path(), "other/snapcraft.yaml", "");
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));
}

#[test]
fn non_utf8_project_file_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("snapcraft.yaml");
    fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
    assert!(matches!(
        Project::load(dir.path()),
        Err(Error::Io { path, .. }) if path == file
    ));
}
