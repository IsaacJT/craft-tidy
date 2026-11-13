// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the supported craft tools (`craft_tidy::craft`).

use std::path::Path;

use craft_tidy::craft::Craft;

#[test]
fn lists_every_tool_in_alphabetical_order() {
    let names: Vec<_> = Craft::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        [
            "charmcraft",
            "debcraft",
            "imagecraft",
            "rockcraft",
            "snapcraft"
        ]
    );
    let mut sorted = Craft::ALL;
    sorted.sort();
    assert_eq!(sorted, Craft::ALL);
}

#[test]
fn displays_the_tool_name() {
    assert_eq!(Craft::Rockcraft.to_string(), "rockcraft");
}

#[test]
fn craft_application_tools_read_name_yaml_from_the_root() {
    for craft in Craft::ALL {
        if craft != Craft::Snapcraft {
            assert_eq!(
                craft.project_file_locations(),
                [format!("{}.yaml", craft.name())]
            );
        }
    }
}

#[test]
fn snapcraft_locations_follow_snapcraft_search_order() {
    assert_eq!(
        Craft::Snapcraft.project_file_locations(),
        [
            "snapcraft.yaml",
            "snap/snapcraft.yaml",
            "build-aux/snap/snapcraft.yaml",
            ".snapcraft.yaml"
        ]
    );
}

#[test]
fn identifies_tools_from_project_file_names() {
    for (path, craft) in [
        ("snapcraft.yaml", Craft::Snapcraft),
        ("snap/snapcraft.yaml", Craft::Snapcraft),
        ("/abs/build-aux/snap/snapcraft.yaml", Craft::Snapcraft),
        (".snapcraft.yaml", Craft::Snapcraft),
        ("charmcraft.yaml", Craft::Charmcraft),
        ("debcraft.yaml", Craft::Debcraft),
        ("imagecraft.yaml", Craft::Imagecraft),
        ("some/dir/rockcraft.yaml", Craft::Rockcraft),
    ] {
        assert_eq!(Craft::from_path(Path::new(path)), Some(craft), "{path}");
    }
}

#[test]
fn does_not_identify_other_file_names() {
    for path in [
        "custom.yaml",
        "snapcraft.yml",
        "Snapcraft.yaml",
        ".rockcraft.yaml",
        "rockcraft.yaml.bak",
        "metadata.yaml",
        "snap",
        "",
    ] {
        assert_eq!(Craft::from_path(Path::new(path)), None, "{path}");
    }
}
