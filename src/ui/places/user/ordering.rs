use std::path::PathBuf;

use super::super::PlaceEntry;
use super::entry::user_place_entry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MoveUserPlaceResult {
    Moved { label: String },
    AlreadyThere,
    NotMovable,
}

pub(crate) fn insert_user_place(places: &mut Vec<PlaceEntry>, label: String, path: PathBuf) {
    let insert_at = user_place_insert_index(places, places.len());
    insert_user_place_at(places, label, path, insert_at);
}

pub(super) fn insert_user_place_at(
    places: &mut Vec<PlaceEntry>,
    label: String,
    path: PathBuf,
    index: usize,
) {
    let entry = user_place_entry(label, path);
    let insert_at = user_place_insert_index(places, index);
    places.insert(insert_at, entry);
}

pub(crate) fn move_user_place_to_insert_index(
    places: &mut Vec<PlaceEntry>,
    source_index: usize,
    index: usize,
) -> MoveUserPlaceResult {
    let Some(source) = places.get(source_index) else {
        return MoveUserPlaceResult::NotMovable;
    };
    if !(source.editable && source.removable) {
        return MoveUserPlaceResult::NotMovable;
    }

    let target_index = user_place_insert_index(places, index);
    if target_index == source_index || target_index == source_index + 1 {
        return MoveUserPlaceResult::AlreadyThere;
    }

    let label = source.label.clone();
    let place = places.remove(source_index);
    let insert_at = if source_index < target_index {
        target_index.saturating_sub(1)
    } else {
        target_index
    };
    let insert_at = user_place_insert_index(places, insert_at);
    places.insert(insert_at, place);
    MoveUserPlaceResult::Moved { label }
}

pub(crate) fn user_place_insert_index(places: &[PlaceEntry], index: usize) -> usize {
    let first_grouped = places
        .iter()
        .position(|place| !place.group.is_empty())
        .unwrap_or(places.len());
    index.clamp(0, first_grouped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_place_insert_index_stays_inside_user_bookmark_range() {
        let places = vec![
            place("", "Home", "/home/yk", false),
            place("", "Alpha", "/home/yk/Alpha", true),
            place("", "Beta", "/home/yk/Beta", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(user_place_insert_index(&places, 0), 0);
        assert_eq!(user_place_insert_index(&places, 1), 1);
        assert_eq!(user_place_insert_index(&places, 2), 2);
        assert_eq!(user_place_insert_index(&places, 10), 3);
    }

    #[test]
    fn user_place_insert_index_allows_bookmarks_above_home_and_trash() {
        let places = vec![
            place("", "Home", "/home/yk", false),
            place("", "Trash", "/home/yk/.local/share/Trash/files", false),
            place("", "Alpha", "/home/yk/Alpha", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(user_place_insert_index(&places, 0), 0);
        assert_eq!(user_place_insert_index(&places, 1), 1);
        assert_eq!(user_place_insert_index(&places, 2), 2);
        assert_eq!(user_place_insert_index(&places, 10), 3);
    }

    #[test]
    fn insert_user_place_at_allows_inserting_above_home() {
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("Devices", "Root", "/", false),
        ];

        insert_user_place_at(
            &mut places,
            "Work".to_string(),
            PathBuf::from("/home/yk/Work"),
            0,
        );

        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Work", "Home", "Root"]
        );
        assert!(places[0].editable);
        assert!(places[0].removable);
        assert_eq!(places[0].marker, "B");
    }

    #[test]
    fn move_user_place_reorders_only_movable_bookmarks() {
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("", "Alpha", "/home/yk/Alpha", true),
            place("", "Beta", "/home/yk/Beta", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(
            move_user_place_to_insert_index(&mut places, 2, 1),
            MoveUserPlaceResult::Moved {
                label: "Beta".to_string()
            }
        );
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Beta", "Alpha", "Root"]
        );
        assert_eq!(
            move_user_place_to_insert_index(&mut places, 0, 3),
            MoveUserPlaceResult::NotMovable
        );
        assert_eq!(
            {
                let end_index = places.len();
                move_user_place_to_insert_index(&mut places, 2, end_index)
            },
            MoveUserPlaceResult::AlreadyThere
        );
    }

    #[test]
    fn move_user_place_can_reorder_above_home_and_trash() {
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("", "Trash", "/home/yk/.local/share/Trash/files", false),
            place("", "Alpha", "/home/yk/Alpha", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(
            move_user_place_to_insert_index(&mut places, 2, 0),
            MoveUserPlaceResult::Moved {
                label: "Alpha".to_string()
            }
        );
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Home", "Trash", "Root"]
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
}
