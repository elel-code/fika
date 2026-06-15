use std::path::{Path, PathBuf};

use super::super::PlaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RemoveUserPlaceResult {
    Removed { label: String, path: PathBuf },
    CannotRemove,
}

impl RemoveUserPlaceResult {
    pub(crate) fn removed_path(&self) -> Option<&Path> {
        match self {
            RemoveUserPlaceResult::Removed { path, .. } => Some(path.as_path()),
            RemoveUserPlaceResult::CannotRemove => None,
        }
    }

    pub(crate) fn status_message(&self) -> String {
        match self {
            RemoveUserPlaceResult::Removed { label, .. } => format!("Removed place {label}"),
            RemoveUserPlaceResult::CannotRemove => "Place cannot be removed".to_string(),
        }
    }
}

pub(crate) fn remove_user_place(
    places: &mut Vec<PlaceEntry>,
    path: &Path,
) -> RemoveUserPlaceResult {
    let Some(index) = places
        .iter()
        .position(|place| place.path == path && place.removable)
    else {
        return RemoveUserPlaceResult::CannotRemove;
    };

    let removed = places.remove(index);
    RemoveUserPlaceResult::Removed {
        label: removed.label,
        path: removed.path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_user_place_removes_only_removable_places() {
        let user_path = PathBuf::from("/home/yk/User");
        let mut places = vec![
            place("", "Home", "/home/yk", false),
            place("", "User", "/home/yk/User", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(
            remove_user_place(&mut places, Path::new("/home/yk")),
            RemoveUserPlaceResult::CannotRemove
        );
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "User", "Root"]
        );

        let result = remove_user_place(&mut places, &user_path);
        assert_eq!(
            result,
            RemoveUserPlaceResult::Removed {
                label: "User".to_string(),
                path: user_path.clone()
            }
        );
        assert_eq!(result.removed_path(), Some(user_path.as_path()));
        assert_eq!(result.status_message(), "Removed place User");
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Root"]
        );
    }

    fn place(group: &'static str, label: &str, path: &str, removable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path: PathBuf::from(path),
            device_id: None,
            device_mounted: true,
            editable: removable,
            removable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }
}
