use std::path::PathBuf;

use super::super::PlaceEntry;
use super::super::model::default_place_label;
use super::ordering::insert_user_place_at;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AddDroppedUserPlaceResult {
    Added { label: String },
    DropOneFolder,
    NotFolder,
    AlreadyExists,
}

impl AddDroppedUserPlaceResult {
    pub(crate) fn added(&self) -> bool {
        matches!(self, AddDroppedUserPlaceResult::Added { .. })
    }

    pub(crate) fn status_message(&self) -> String {
        match self {
            AddDroppedUserPlaceResult::Added { label } => format!("Added place {label}"),
            AddDroppedUserPlaceResult::DropOneFolder => {
                "Drop one folder to add a place".to_string()
            }
            AddDroppedUserPlaceResult::NotFolder => {
                "Only folders can be added to Places".to_string()
            }
            AddDroppedUserPlaceResult::AlreadyExists => "Place already exists".to_string(),
        }
    }
}

pub(crate) fn add_user_place_from_dropped_paths(
    places: &mut Vec<PlaceEntry>,
    paths: &[PathBuf],
    index: usize,
) -> AddDroppedUserPlaceResult {
    let [path] = paths else {
        return AddDroppedUserPlaceResult::DropOneFolder;
    };
    if !path.is_dir() {
        return AddDroppedUserPlaceResult::NotFolder;
    }
    if places.iter().any(|place| place.path == *path) {
        return AddDroppedUserPlaceResult::AlreadyExists;
    }
    let label = default_place_label(path);
    insert_user_place_at(places, label.clone(), path.clone(), index);
    AddDroppedUserPlaceResult::Added { label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_user_place_from_dropped_paths_validates_and_inserts_folder() {
        let root =
            std::env::temp_dir().join(format!("fika-place-drop-user-{}", std::process::id()));
        let dropped = root.join("Dropped");
        let file = root.join("note.txt");
        std::fs::create_dir_all(&dropped).unwrap();
        std::fs::write(&file, "note").unwrap();
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(
            add_user_place_from_dropped_paths(&mut places, &[file], 0).status_message(),
            "Only folders can be added to Places"
        );
        assert_eq!(
            add_user_place_from_dropped_paths(&mut places, &[dropped.clone(), root.clone()], 0)
                .status_message(),
            "Drop one folder to add a place"
        );

        let result =
            add_user_place_from_dropped_paths(&mut places, std::slice::from_ref(&dropped), 0);
        assert!(result.added());
        assert_eq!(result.status_message(), "Added place Dropped");
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Dropped", "Home", "Root"]
        );
        assert_eq!(
            add_user_place_from_dropped_paths(&mut places, std::slice::from_ref(&dropped), 0)
                .status_message(),
            "Place already exists"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn place(group: &'static str, label: &str, path: &str, editable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path: PathBuf::from(path),
            device_id: None,
            device_mounted: true,
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }
}
