use std::path::{Path, PathBuf};

use fika_core::resolve_location_input;

use super::super::PlaceEntry;
use super::ordering::insert_user_place;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommitUserPlaceDraftResult {
    Added { label: String },
    Updated { label: String },
    EmptyLabel,
    EmptyPath,
    NotFolder { path: PathBuf },
    AlreadyExists,
    CannotEdit,
}

impl CommitUserPlaceDraftResult {
    pub(crate) fn changed(&self) -> bool {
        matches!(
            self,
            CommitUserPlaceDraftResult::Added { .. } | CommitUserPlaceDraftResult::Updated { .. }
        )
    }

    pub(crate) fn status_message(&self) -> String {
        match self {
            CommitUserPlaceDraftResult::Added { label } => format!("Added place {label}"),
            CommitUserPlaceDraftResult::Updated { label } => format!("Updated place {label}"),
            CommitUserPlaceDraftResult::EmptyLabel => "Place label cannot be empty".to_string(),
            CommitUserPlaceDraftResult::EmptyPath => "Place path cannot be empty".to_string(),
            CommitUserPlaceDraftResult::NotFolder { path } => {
                format!("Place path is not a folder: {}", path.display())
            }
            CommitUserPlaceDraftResult::AlreadyExists => "Place already exists".to_string(),
            CommitUserPlaceDraftResult::CannotEdit => "Place cannot be edited".to_string(),
        }
    }
}

pub(crate) fn commit_user_place_draft(
    places: &mut Vec<PlaceEntry>,
    current_dir: &Path,
    label_input: &str,
    path_input: &str,
    editing_path: Option<&Path>,
) -> CommitUserPlaceDraftResult {
    let label = label_input.trim().to_string();
    if label.is_empty() {
        return CommitUserPlaceDraftResult::EmptyLabel;
    }

    let Some(path) = resolve_location_input(current_dir, path_input) else {
        return CommitUserPlaceDraftResult::EmptyPath;
    };
    if !path.is_dir() {
        return CommitUserPlaceDraftResult::NotFolder { path };
    }

    let duplicate = places.iter().position(|place| place.path == path);
    if let Some(editing_path) = editing_path {
        let Some(index) = places
            .iter()
            .position(|place| place.path == editing_path && place.editable)
        else {
            return CommitUserPlaceDraftResult::CannotEdit;
        };
        if duplicate.is_some_and(|duplicate| duplicate != index) {
            return CommitUserPlaceDraftResult::AlreadyExists;
        }

        places[index].label = label.clone();
        places[index].path = path;
        return CommitUserPlaceDraftResult::Updated { label };
    }

    if duplicate.is_some() {
        return CommitUserPlaceDraftResult::AlreadyExists;
    }
    insert_user_place(places, label.clone(), path);
    CommitUserPlaceDraftResult::Added { label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_user_place_draft_adds_bookmark_before_grouped_entries() {
        let root = temp_dir("place-edit-add");
        let current = root.join("current");
        let added = current.join("added");
        std::fs::create_dir_all(&added).unwrap();
        let mut places = vec![
            place("", "Home", &current, false),
            place("Devices", "Root", Path::new("/"), false),
        ];

        let result = commit_user_place_draft(&mut places, &current, "  Added  ", "added", None);

        assert_eq!(
            result,
            CommitUserPlaceDraftResult::Added {
                label: "Added".to_string()
            }
        );
        assert_eq!(result.status_message(), "Added place Added");
        assert_eq!(
            places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Added", "Root"]
        );
        assert_eq!(places[1].path, added);
        assert!(places[1].editable);
        assert!(places[1].removable);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn commit_user_place_draft_updates_editable_bookmark_and_rejects_duplicate() {
        let root = temp_dir("place-edit-update");
        let original = root.join("original");
        let duplicate = root.join("duplicate");
        let edited = root.join("edited");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::create_dir_all(&duplicate).unwrap();
        std::fs::create_dir_all(&edited).unwrap();
        let mut places = vec![
            place("", "Original", &original, true),
            place("", "Duplicate", &duplicate, true),
        ];

        assert_eq!(
            commit_user_place_draft(
                &mut places,
                &root,
                "Rejected",
                "duplicate",
                Some(original.as_path()),
            ),
            CommitUserPlaceDraftResult::AlreadyExists
        );
        assert_eq!(places[0].label, "Original");
        assert_eq!(places[0].path, original);

        let result = commit_user_place_draft(
            &mut places,
            &root,
            "Edited",
            "edited",
            Some(original.as_path()),
        );

        assert_eq!(
            result,
            CommitUserPlaceDraftResult::Updated {
                label: "Edited".to_string()
            }
        );
        assert_eq!(places[0].label, "Edited");
        assert_eq!(places[0].path, edited);
        assert_eq!(result.status_message(), "Updated place Edited");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn commit_user_place_draft_reports_validation_errors_without_mutating() {
        let root = temp_dir("place-edit-validation");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("note.txt");
        std::fs::write(&file, "note").unwrap();
        let missing = root.join("missing");
        let original_places = vec![place("", "Home", &root, false)];
        let mut places = original_places.clone();

        assert_eq!(
            commit_user_place_draft(&mut places, &root, " ", ".", None),
            CommitUserPlaceDraftResult::EmptyLabel
        );
        assert_eq!(
            commit_user_place_draft(&mut places, &root, "File", "", None),
            CommitUserPlaceDraftResult::EmptyPath
        );
        assert_eq!(
            commit_user_place_draft(&mut places, &root, "File", "note.txt", None),
            CommitUserPlaceDraftResult::NotFolder { path: file }
        );
        assert_eq!(
            commit_user_place_draft(&mut places, &root, "Missing", "missing", None),
            CommitUserPlaceDraftResult::NotFolder { path: missing }
        );
        assert_eq!(
            commit_user_place_draft(&mut places, &root, "Duplicate", ".", None),
            CommitUserPlaceDraftResult::AlreadyExists
        );
        assert_eq!(places, original_places);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn commit_user_place_draft_requires_editable_target_when_updating() {
        let root = temp_dir("place-edit-cannot-edit");
        std::fs::create_dir_all(&root).unwrap();
        let mut places = vec![place("", "Home", &root, false)];

        assert_eq!(
            commit_user_place_draft(&mut places, &root, "Home", ".", Some(root.as_path())),
            CommitUserPlaceDraftResult::CannotEdit
        );
        assert_eq!(places[0].label, "Home");

        let _ = std::fs::remove_dir_all(root);
    }

    fn place(group: &'static str, label: &str, path: &Path, editable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path: path.to_path_buf(),
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
    }
}
