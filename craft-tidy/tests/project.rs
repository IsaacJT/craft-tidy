// SPDX-License-Identifier: GPL-3.0-only

//! Tests for locating and loading project files (`craft_tidy::project`).

use std::fs;
use std::path::{Path, PathBuf};

use craft_tidy::craft::Craft;
use craft_tidy::error::Error;
use craft_tidy::project::{self, Project};
use craft_tidy::yaml::Value;

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn found(dir: &Path) -> Vec<PathBuf> {
    project::find(dir)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(dir).unwrap().to_owned())
        .collect()
}

// Finding project files

#[test]
fn finds_the_project_file_of_each_craft_tool() {
    for craft in Craft::ALL {
        for location in craft.project_file_locations() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), location, "name: test\n");
            assert_eq!(found(dir.path()), [PathBuf::from(location)], "{craft}");
        }
    }
}

#[test]
fn finds_the_project_files_of_all_tools_in_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["snap/snapcraft.yaml", "rockcraft.yaml", "charmcraft.yaml"] {
        write(dir.path(), name, "name: test\n");
    }
    assert_eq!(
        found(dir.path()),
        [
            PathBuf::from("charmcraft.yaml"),
            PathBuf::from("rockcraft.yaml"),
            PathBuf::from("snap/snapcraft.yaml"),
        ]
    );
}

#[test]
fn uses_the_same_project_file_as_snapcraft() {
    // Snapcraft prefers snapcraft.yaml in the root over snap/snapcraft.yaml,
    // and .snapcraft.yaml comes last; only the first one found is used.
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".snapcraft.yaml", "");
    write(dir.path(), "build-aux/snap/snapcraft.yaml", "");
    assert_eq!(
        found(dir.path()),
        [PathBuf::from("build-aux/snap/snapcraft.yaml")]
    );
    write(dir.path(), "snap/snapcraft.yaml", "");
    assert_eq!(found(dir.path()), [PathBuf::from("snap/snapcraft.yaml")]);
    write(dir.path(), "snapcraft.yaml", "");
    assert_eq!(found(dir.path()), [PathBuf::from("snapcraft.yaml")]);
}

#[test]
fn other_tools_only_read_their_project_file_from_the_root() {
    let dir = tempfile::tempdir().unwrap();
    for relative in [
        "rock/rockcraft.yaml",
        ".rockcraft.yaml",
        "charm/charmcraft.yaml",
        "build-aux/rock/rockcraft.yaml",
    ] {
        write(dir.path(), relative, "");
    }
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));
}

#[test]
fn accepts_a_project_file_path_of_any_name() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "custom.yaml", "name: test\n");
    let file = dir.path().join("custom.yaml");
    assert_eq!(project::find(&file).unwrap(), std::slice::from_ref(&file));
}

#[test]
fn empty_or_missing_directories_have_no_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(path)) if path == dir.path()
    ));
    assert!(matches!(
        project::find(&dir.path().join("missing")),
        Err(Error::ProjectNotFound(_))
    ));
}

#[test]
fn not_found_error_names_the_project_files() {
    let err = Error::ProjectNotFound(PathBuf::from("some/dir"));
    assert_eq!(
        err.to_string(),
        "no craft project file found in some/dir (looked for charmcraft.yaml, \
         debcraft.yaml, imagecraft.yaml, rockcraft.yaml, snapcraft.yaml)"
    );
}

#[test]
fn ignores_directories_at_project_file_locations() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("snap/snapcraft.yaml")).unwrap();
    fs::create_dir_all(dir.path().join("snapcraft.yaml")).unwrap();
    fs::create_dir_all(dir.path().join("rockcraft.yaml")).unwrap();
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));

    write(dir.path(), ".snapcraft.yaml", "");
    assert_eq!(found(dir.path()), [PathBuf::from(".snapcraft.yaml")]);
}

#[test]
fn ignores_unrelated_files() {
    let dir = tempfile::tempdir().unwrap();
    for relative in [
        "snapcraft.yml",
        "snap/snapcraft.yml",
        "other/snapcraft.yaml",
        "rockcraft.yml",
        "metadata.yaml",
    ] {
        write(dir.path(), relative, "");
    }
    assert!(matches!(
        project::find(dir.path()),
        Err(Error::ProjectNotFound(_))
    ));
}

// Loading project files

#[test]
fn load_reads_and_parses_the_project_file() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "snap/snapcraft.yaml", "name: test\n");
    let project = Project::load(&dir.path().join("snap/snapcraft.yaml")).unwrap();
    assert_eq!(project.path, dir.path().join("snap/snapcraft.yaml"));
    assert_eq!(project.craft, Some(Craft::Snapcraft));
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
fn load_all_loads_every_project_file_found() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "rockcraft.yaml", "name: rock\n");
    write(dir.path(), "snap/snapcraft.yaml", "name: snap\n");
    let projects = Project::load_all(dir.path()).unwrap();
    let crafts: Vec<_> = projects.iter().map(|p| p.craft).collect();
    assert_eq!(crafts, [Some(Craft::Rockcraft), Some(Craft::Snapcraft)]);
    assert_eq!(projects[0].source(), "name: rock\n");

    let file = dir.path().join("rockcraft.yaml");
    let projects = Project::load_all(&file).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].path, file);
}

#[test]
fn new_identifies_the_craft_tool_from_the_file_name() {
    for (path, craft) in [
        ("snap/snapcraft.yaml", Some(Craft::Snapcraft)),
        (".snapcraft.yaml", Some(Craft::Snapcraft)),
        ("charmcraft.yaml", Some(Craft::Charmcraft)),
        ("x/rockcraft.yaml", Some(Craft::Rockcraft)),
        ("custom.yaml", None),
    ] {
        assert_eq!(Project::new(path, "").unwrap().craft, craft, "{path}");
    }
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
fn load_reports_invalid_yaml_and_unreadable_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "snapcraft.yaml", "a: b\n  c: d\n");
    assert!(matches!(
        Project::load_all(dir.path()),
        Err(Error::Syntax { .. })
    ));

    let file = dir.path().join("rockcraft.yaml");
    fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
    assert!(matches!(
        Project::load(&file),
        Err(Error::Io { path, .. }) if path == file
    ));
    assert!(matches!(
        Project::load(&dir.path().join("missing.yaml")),
        Err(Error::Io { .. })
    ));
}
