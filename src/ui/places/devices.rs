use std::collections::BTreeSet;

use fika_core::DeviceInfo;

use super::model::{
    DEVICES_GROUP, PlaceEntry, REMOVABLE_DEVICES_GROUP, removable_device_place_entries,
};

pub(crate) fn replace_removable_device_places(
    places: &mut Vec<PlaceEntry>,
    devices: &[DeviceInfo],
) -> bool {
    let existing_paths = places
        .iter()
        .filter(|place| place.group != REMOVABLE_DEVICES_GROUP)
        .map(|place| place.path.clone())
        .collect::<BTreeSet<_>>();
    let entries = removable_device_place_entries(devices, &existing_paths);
    let old_entries = places
        .iter()
        .filter(|place| place.group == REMOVABLE_DEVICES_GROUP)
        .cloned()
        .collect::<Vec<_>>();
    if old_entries == entries {
        return false;
    }

    places.retain(|place| place.group != REMOVABLE_DEVICES_GROUP);
    let insert_at = places
        .iter()
        .position(|place| place.group == DEVICES_GROUP)
        .unwrap_or(places.len());
    for entry in entries.into_iter().rev() {
        places.insert(insert_at, entry);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn replaces_removable_device_section_before_static_devices_group() {
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("", "User", "/home/yk/Work", true),
            place(DEVICES_GROUP, "Root", "/", false),
        ];

        assert!(replace_removable_device_places(
            &mut places,
            &[
                test_device("/run/media/yk/USB", "USB", true),
                test_device("/run/media/yk/Backup", "Backup", true),
            ],
        ));
        assert_eq!(
            places
                .iter()
                .map(|place| (place.group, place.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("", "Home"),
                ("", "User"),
                (REMOVABLE_DEVICES_GROUP, "Backup"),
                (REMOVABLE_DEVICES_GROUP, "USB"),
                (DEVICES_GROUP, "Root"),
            ]
        );

        assert!(!replace_removable_device_places(
            &mut places,
            &[
                test_device("/run/media/yk/USB", "USB", true),
                test_device("/run/media/yk/Backup", "Backup", true),
            ],
        ));

        assert!(replace_removable_device_places(
            &mut places,
            &[test_device("/run/media/yk/Camera", "Camera", true)],
        ));
        assert_eq!(
            places
                .iter()
                .filter(|place| place.group == REMOVABLE_DEVICES_GROUP)
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Camera"]
        );
        assert_eq!(
            places
                .iter()
                .filter(|place| place.group.is_empty())
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "User"]
        );
    }

    fn place(group: &'static str, label: &str, path: &str, editable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path: PathBuf::from(path),
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }

    fn test_device(path: &str, label: &str, removable: bool) -> DeviceInfo {
        DeviceInfo {
            device_path: PathBuf::from(format!("/dev/{label}")),
            mount_point: Some(PathBuf::from(path)),
            filesystem_type: Some("exfat".to_string()),
            label: Some(label.to_string()),
            capacity_bytes: Some(1024),
            removable,
            ejectable: false,
            can_power_off: false,
        }
    }
}
