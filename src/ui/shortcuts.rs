use fika_core::{SelectionMove, ZoomChange};
use gpui::ScrollDelta;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneShortcut {
    SelectAll,
    ClearSelection,
    Refresh,
    GoParent,
    GoBack,
    GoForward,
    SplitPane,
    ClosePane,
    MoveSelection {
        direction: SelectionMove,
        extend: bool,
    },
    CreateFolder,
    RenameSelection,
    EditLocation,
    ShowFilter,
    Zoom(ZoomChange),
    CopySelection,
    CutSelection,
    PasteIntoPane,
    TrashSelection,
    Undo,
}

pub(crate) fn pane_shortcut(keystroke: &gpui::Keystroke) -> Option<PaneShortcut> {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => Some(PaneShortcut::ClearSelection),
            "/" => Some(PaneShortcut::ShowFilter),
            "f5" => Some(PaneShortcut::Refresh),
            "f6" => Some(PaneShortcut::EditLocation),
            "f3" => Some(PaneShortcut::SplitPane),
            "f2" => Some(PaneShortcut::RenameSelection),
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: false,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: false,
            }),
            "backspace" => Some(PaneShortcut::GoParent),
            "delete" => Some(PaneShortcut::TrashSelection),
            _ => None,
        };
    }

    if keystroke.modifiers.shift && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: true,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: true,
            }),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary()
        && keystroke.modifiers.shift
        && keystroke.modifiers.number_of_modifiers() == 2
    {
        if let Some(shortcut) = zoom_shortcut(keystroke) {
            return Some(shortcut);
        }
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "n" => Some(PaneShortcut::CreateFolder),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary() && keystroke.modifiers.number_of_modifiers() == 1 {
        if let Some(shortcut) = zoom_shortcut(keystroke) {
            return Some(shortcut);
        }
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "a" => Some(PaneShortcut::SelectAll),
            "c" => Some(PaneShortcut::CopySelection),
            "f" => Some(PaneShortcut::ShowFilter),
            "i" => Some(PaneShortcut::ShowFilter),
            "l" => Some(PaneShortcut::EditLocation),
            "v" => Some(PaneShortcut::PasteIntoPane),
            "w" => Some(PaneShortcut::ClosePane),
            "x" => Some(PaneShortcut::CutSelection),
            "z" => Some(PaneShortcut::Undo),
            _ => None,
        };
    }

    if keystroke.modifiers.alt && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "d" => Some(PaneShortcut::EditLocation),
            "left" => Some(PaneShortcut::GoBack),
            "right" => Some(PaneShortcut::GoForward),
            _ => None,
        };
    }

    None
}

fn zoom_shortcut(keystroke: &gpui::Keystroke) -> Option<PaneShortcut> {
    let key = keystroke.key.to_ascii_lowercase();
    let key_char = keystroke.key_char.as_deref();
    if matches!(key.as_str(), "+" | "=" | "plus") || key_char == Some("+") {
        return Some(PaneShortcut::Zoom(ZoomChange::In));
    }
    if matches!(key.as_str(), "-" | "minus") || key_char == Some("-") {
        return Some(PaneShortcut::Zoom(ZoomChange::Out));
    }
    if key == "0" || key_char == Some("0") {
        return Some(PaneShortcut::Zoom(ZoomChange::Reset));
    }
    None
}

pub(crate) fn zoom_change_for_wheel_delta(delta: ScrollDelta) -> Option<ZoomChange> {
    let delta = match delta {
        ScrollDelta::Pixels(delta) => {
            let x = delta.x.as_f32();
            let y = delta.y.as_f32();
            if y.abs() >= x.abs() { y } else { x }
        }
        ScrollDelta::Lines(delta) => {
            if delta.y.abs() >= delta.x.abs() {
                delta.y
            } else {
                delta.x
            }
        }
    };
    if delta < 0.0 {
        Some(ZoomChange::In)
    } else if delta > 0.0 {
        Some(ZoomChange::Out)
    } else {
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RenameInputAction {
    Cancel,
    Commit,
    CommitAndRenameNext,
    Backspace,
    Insert(String),
    Ignore,
}

pub(crate) fn rename_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => RenameInputAction::Cancel,
            "enter" => RenameInputAction::Commit,
            "tab" => RenameInputAction::CommitAndRenameNext,
            "backspace" => RenameInputAction::Backspace,
            _ => rename_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return rename_text_input_action(keystroke);
    }

    RenameInputAction::Ignore
}

fn rename_text_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| RenameInputAction::Insert(text.clone()))
        .unwrap_or(RenameInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LocationInputAction {
    Cancel,
    Commit,
    Complete,
    MoveStart,
    MoveEnd,
    MoveBackward,
    MoveForward,
    Backspace,
    Delete,
    Insert(String),
    Ignore,
}

pub(crate) fn location_input_action(keystroke: &gpui::Keystroke) -> LocationInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => LocationInputAction::Cancel,
            "enter" => LocationInputAction::Commit,
            "tab" => LocationInputAction::Complete,
            "home" => LocationInputAction::MoveStart,
            "end" => LocationInputAction::MoveEnd,
            "left" => LocationInputAction::MoveBackward,
            "right" => LocationInputAction::MoveForward,
            "backspace" => LocationInputAction::Backspace,
            "delete" => LocationInputAction::Delete,
            _ => location_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return location_text_input_action(keystroke);
    }

    LocationInputAction::Ignore
}

fn location_text_input_action(keystroke: &gpui::Keystroke) -> LocationInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| LocationInputAction::Insert(text.clone()))
        .unwrap_or(LocationInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaceInputAction {
    Cancel,
    Commit,
    NextField,
    Backspace,
    Insert(String),
    Ignore,
}

pub(crate) fn place_input_action(keystroke: &gpui::Keystroke) -> PlaceInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => PlaceInputAction::Cancel,
            "enter" => PlaceInputAction::Commit,
            "tab" => PlaceInputAction::NextField,
            "backspace" => PlaceInputAction::Backspace,
            _ => place_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return place_text_input_action(keystroke);
    }

    PlaceInputAction::Ignore
}

fn place_text_input_action(keystroke: &gpui::Keystroke) -> PlaceInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| PlaceInputAction::Insert(text.clone()))
        .unwrap_or(PlaceInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FilterInputAction {
    Cancel,
    FocusView,
    Backspace,
    Insert(String),
    PassToView,
    Ignore,
}

pub(crate) fn filter_input_action(keystroke: &gpui::Keystroke) -> FilterInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => FilterInputAction::Cancel,
            "enter" => FilterInputAction::FocusView,
            "up" | "down" | "pageup" | "pagedown" => FilterInputAction::PassToView,
            "backspace" => FilterInputAction::Backspace,
            _ => filter_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return filter_text_input_action(keystroke);
    }

    FilterInputAction::Ignore
}

fn filter_text_input_action(keystroke: &gpui::Keystroke) -> FilterInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| FilterInputAction::Insert(text.clone()))
        .unwrap_or(FilterInputAction::Ignore)
}

fn has_no_modifiers(keystroke: &gpui::Keystroke) -> bool {
    !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
}
