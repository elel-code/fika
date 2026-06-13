use std::path::{Path, PathBuf};

use fika_core::PaneId;

use crate::ui::shortcuts::PlaceInputAction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDraft {
    pub(crate) pane_id: PaneId,
    pub(crate) editing_path: Option<PathBuf>,
    pub(crate) focus: PlaceDraftField,
    pub(crate) label: String,
    pub(crate) path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceDraftField {
    Label,
    Path,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceDraftInputResult {
    Cancel,
    Commit,
    Edited,
    Ignore,
}

pub(crate) fn apply_place_input_action(
    draft: &mut PlaceDraft,
    action: PlaceInputAction,
) -> PlaceDraftInputResult {
    match action {
        PlaceInputAction::Cancel => PlaceDraftInputResult::Cancel,
        PlaceInputAction::Commit => PlaceDraftInputResult::Commit,
        PlaceInputAction::NextField => {
            draft.focus = draft.focus.next();
            PlaceDraftInputResult::Edited
        }
        PlaceInputAction::Backspace => {
            draft.backspace();
            PlaceDraftInputResult::Edited
        }
        PlaceInputAction::Insert(text) => {
            draft.insert_text(&text);
            PlaceDraftInputResult::Edited
        }
        PlaceInputAction::Ignore => PlaceDraftInputResult::Ignore,
    }
}

pub(crate) fn clear_place_draft_for_pane(draft: &mut Option<PlaceDraft>, pane_id: PaneId) -> bool {
    if !draft.as_ref().is_some_and(|draft| draft.pane_id == pane_id) {
        return false;
    }
    *draft = None;
    true
}

pub(crate) fn set_place_draft_focus(
    draft: &mut Option<PlaceDraft>,
    field: PlaceDraftField,
) -> bool {
    let Some(draft) = draft else {
        return false;
    };
    draft.focus = field;
    true
}

impl PlaceDraft {
    pub(crate) fn for_add(pane_id: PaneId, label: String, path: &Path) -> Self {
        Self {
            pane_id,
            editing_path: None,
            focus: PlaceDraftField::Label,
            label,
            path: path.display().to_string(),
        }
    }

    pub(crate) fn for_edit(pane_id: PaneId, label: String, path: &Path) -> Self {
        Self {
            pane_id,
            editing_path: Some(path.to_path_buf()),
            focus: PlaceDraftField::Label,
            label,
            path: path.display().to_string(),
        }
    }

    fn backspace(&mut self) {
        match self.focus {
            PlaceDraftField::Label => {
                self.label.pop();
            }
            PlaceDraftField::Path => {
                self.path.pop();
            }
        }
    }

    fn insert_text(&mut self, text: &str) {
        match self.focus {
            PlaceDraftField::Label => self.label.push_str(text),
            PlaceDraftField::Path => self.path.push_str(text),
        }
    }
}

impl PlaceDraftField {
    fn next(self) -> Self {
        match self {
            PlaceDraftField::Label => PlaceDraftField::Path,
            PlaceDraftField::Path => PlaceDraftField::Label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_draft_input_action_switches_fields_and_edits_focused_value() {
        let mut draft = draft();

        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Insert("Work".to_string())),
            PlaceDraftInputResult::Edited
        );
        assert_eq!(draft.label, "Work");
        assert_eq!(draft.path, "");

        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::NextField),
            PlaceDraftInputResult::Edited
        );
        assert_eq!(draft.focus, PlaceDraftField::Path);

        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Insert("/tmp".to_string())),
            PlaceDraftInputResult::Edited
        );
        assert_eq!(draft.path, "/tmp");

        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Backspace),
            PlaceDraftInputResult::Edited
        );
        assert_eq!(draft.path, "/tm");
    }

    #[test]
    fn place_draft_input_action_reports_control_results_without_mutating() {
        let mut draft = draft();

        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Cancel),
            PlaceDraftInputResult::Cancel
        );
        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Commit),
            PlaceDraftInputResult::Commit
        );
        assert_eq!(
            apply_place_input_action(&mut draft, PlaceInputAction::Ignore),
            PlaceDraftInputResult::Ignore
        );
        assert_eq!(draft, self::draft());
    }

    #[test]
    fn place_draft_constructors_initialize_add_and_edit_modes() {
        let add_path = PathBuf::from("/home/yk/Work");
        let add = PlaceDraft::for_add(PaneId(7), "Work".to_string(), &add_path);
        assert_eq!(add.pane_id, PaneId(7));
        assert_eq!(add.editing_path, None);
        assert_eq!(add.focus, PlaceDraftField::Label);
        assert_eq!(add.label, "Work");
        assert_eq!(add.path, add_path.display().to_string());

        let edit_path = PathBuf::from("/home/yk/Edited");
        let edit = PlaceDraft::for_edit(PaneId(8), "Edited".to_string(), &edit_path);
        assert_eq!(edit.pane_id, PaneId(8));
        assert_eq!(edit.editing_path, Some(edit_path.clone()));
        assert_eq!(edit.focus, PlaceDraftField::Label);
        assert_eq!(edit.label, "Edited");
        assert_eq!(edit.path, edit_path.display().to_string());
    }

    #[test]
    fn place_draft_option_helpers_are_pane_scoped() {
        let mut draft = Some(PlaceDraft::for_add(
            PaneId(1),
            "Work".to_string(),
            Path::new("/home/yk/Work"),
        ));

        assert!(!set_place_draft_focus(&mut None, PlaceDraftField::Path));
        assert!(set_place_draft_focus(&mut draft, PlaceDraftField::Path));
        assert_eq!(
            draft.as_ref().map(|draft| draft.focus),
            Some(PlaceDraftField::Path)
        );

        assert!(!clear_place_draft_for_pane(&mut draft, PaneId(2)));
        assert!(draft.is_some());
        assert!(clear_place_draft_for_pane(&mut draft, PaneId(1)));
        assert!(draft.is_none());
    }

    fn draft() -> PlaceDraft {
        PlaceDraft {
            pane_id: PaneId(1),
            editing_path: None,
            focus: PlaceDraftField::Label,
            label: String::new(),
            path: String::new(),
        }
    }
}
