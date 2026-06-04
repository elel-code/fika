use crate::app::state::AppState;
use crate::config::paths::expand_user_path;
use crate::desktop::systemd_launch;
use crate::fs::places::{builtin_places, place_entry, save_places};
use crate::{AppWindow, PlaceEntry, set_status};
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) fn sync_places(ui: &AppWindow, places: &[PlaceEntry]) {
    ui.set_places(ModelRc::new(Rc::new(VecModel::from(places.to_vec()))));
}

pub(crate) fn contains_place_path(state: &AppState, path: &str) -> bool {
    state.places.iter().any(|place| place.path.as_str() == path)
}

pub(crate) fn add_place(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    let slot = state.borrow().places.len() as i32;
    add_place_at_slot(ui, state, path, slot);
}

pub(crate) fn add_place_at_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
    slot: i32,
) {
    add_place_at_slot_inner(ui, state, path, slot);
}

fn add_place_at_slot_inner(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
    slot: i32,
) {
    let path = normalize_place_path(path);
    if !path.is_dir() {
        set_status(ui, state, "Only folders can be added to Places");
        return;
    }

    let mut state_ref = state.borrow_mut();
    let path_string = path.display().to_string();
    if state_ref
        .places
        .iter()
        .any(|place| place.path.as_str() == path_string)
    {
        set_status(ui, state, "Folder is already in Places");
        return;
    }

    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path_string.as_str())
        .to_string();
    let marker = place_marker(&label);

    let entry = place_entry(&label, path, marker.as_str());
    let slot = usize::try_from(slot)
        .unwrap_or(state_ref.places.len())
        .min(state_ref.places.len());
    state_ref.places.insert(slot, entry);
    save_places(&state_ref.places);
    sync_places(ui, &state_ref.places);
    set_status(ui, state, "Folder added to Places");
}

pub(crate) fn normalize_place_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    expand_user_path(text.as_ref())
}

pub(crate) fn rename_place(ui: &AppWindow, state: &Rc<RefCell<AppState>>, index: i32, label: &str) {
    let label = label.trim();
    if label.is_empty() {
        set_status(ui, state, "Place name cannot be empty");
        return;
    }

    let mut state_ref = state.borrow_mut();
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let Some(place) = state_ref.places.get_mut(index) else {
        return;
    };
    if place.is_builtin {
        set_status(ui, state, "Built-in places cannot be renamed");
        return;
    }

    place.label = label.into();
    place.marker = place_marker(label).into();
    save_places(&state_ref.places);
    sync_places(ui, &state_ref.places);
    set_status(ui, state, "Place renamed");
}

pub(crate) fn remove_place(ui: &AppWindow, state: &Rc<RefCell<AppState>>, index: i32) {
    let mut state_ref = state.borrow_mut();
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if index >= state_ref.places.len() {
        return;
    }
    if state_ref.places[index].is_builtin {
        set_status(ui, state, "Built-in places cannot be removed");
        return;
    }

    state_ref.places.remove(index);
    save_places(&state_ref.places);
    sync_places(ui, &state_ref.places);
    set_status(ui, state, "Place removed");
}

pub(crate) fn restore_default_places(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let mut state_ref = state.borrow_mut();
    state_ref.places = builtin_places();
    save_places(&state_ref.places);
    sync_places(ui, &state_ref.places);
    set_status(ui, state, "Default places restored");
}

pub(crate) fn open_place_new_window(ui: &AppWindow, state: &Rc<RefCell<AppState>>, index: i32) {
    let (label, path) = {
        let state = state.borrow();
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(place) = state.places.get(index) else {
            return;
        };
        (place.label.to_string(), place.path.to_string())
    };
    let Ok(exe) = std::env::current_exe() else {
        set_status(ui, state, "Cannot locate Fika executable");
        return;
    };
    let program = exe.to_string_lossy().to_string();
    let args = vec![path];
    match systemd_launch::spawn_in_user_scope(&program, &args, Some("Fika New Window")) {
        Ok(launch) => {
            if let Some(unit) = &launch.unit {
                state.borrow_mut().launched_units.push(unit.clone());
            }
            set_status(ui, state, &format_new_window_status(&label, &launch));
        }
        Err(err) => set_status(ui, state, &format!("Cannot open new window: {err}")),
    }
}

fn format_new_window_status(label: &str, launch: &systemd_launch::LaunchResult) -> String {
    let suffix = match (&launch.unit, &launch.diagnostic) {
        (Some(unit), _) => format!(" ({unit})"),
        (None, Some(diagnostic)) => format!("; {diagnostic}"),
        (None, None) => String::new(),
    };
    format!("Opened '{label}' in a new window{suffix}")
}

pub(crate) fn reorder_place(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    from: i32,
    to_slot: i32,
) {
    let mut state_ref = state.borrow_mut();
    let Some((from, to)) = place_reorder_indices(state_ref.places.len(), from, to_slot) else {
        return;
    };

    let place = state_ref.places.remove(from);
    state_ref.places.insert(to, place);
    save_places(&state_ref.places);
    sync_places(ui, &state_ref.places);
}

pub(crate) fn reorder_place_path(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: &str,
    to_slot: i32,
) {
    let from = {
        let state_ref = state.borrow();
        state_ref
            .places
            .iter()
            .position(|place| place.path.as_str() == path)
    };
    let Some(from) = from else {
        set_status(ui, state, "Place is no longer available");
        return;
    };
    reorder_place(ui, state, from as i32, to_slot);
}

fn place_reorder_indices(len: usize, from: i32, to_slot: i32) -> Option<(usize, usize)> {
    let from = usize::try_from(from).ok()?;
    let mut to = usize::try_from(to_slot).ok()?;
    if from >= len {
        return None;
    }

    to = to.min(len);
    if to > from {
        to -= 1;
    }
    if from == to {
        return None;
    }

    Some((from, to))
}

fn place_marker(label: &str) -> String {
    label
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "+".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::systemd_launch::LaunchResult;

    #[test]
    fn new_window_status_includes_systemd_unit_when_available() {
        assert_eq!(
            format_new_window_status(
                "Projects",
                &LaunchResult {
                    unit: Some("fika-open-42-1.scope".to_string()),
                    diagnostic: None,
                },
            ),
            "Opened 'Projects' in a new window (fika-open-42-1.scope)"
        );
    }

    #[test]
    fn new_window_status_keeps_non_fatal_systemd_diagnostic() {
        assert_eq!(
            format_new_window_status(
                "Projects",
                &LaunchResult {
                    unit: None,
                    diagnostic: Some("systemd user scope unavailable: no bus".to_string()),
                },
            ),
            "Opened 'Projects' in a new window; systemd user scope unavailable: no bus"
        );
    }

    #[test]
    fn new_window_status_without_scope_is_still_successful() {
        assert_eq!(
            format_new_window_status(
                "Projects",
                &LaunchResult {
                    unit: None,
                    diagnostic: None,
                },
            ),
            "Opened 'Projects' in a new window"
        );
    }

    #[test]
    fn contains_place_path_matches_exact_place_path() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.places = vec![place_entry("Projects", PathBuf::from("/tmp/projects"), "P")];

        assert!(contains_place_path(&state, "/tmp/projects"));
        assert!(!contains_place_path(&state, "/tmp/projects-old"));
    }

    #[test]
    fn place_reorder_indices_follow_insertion_slot_semantics() {
        assert_eq!(place_reorder_indices(5, 1, 4), Some((1, 3)));
        assert_eq!(place_reorder_indices(5, 1, 5), Some((1, 4)));
        assert_eq!(place_reorder_indices(5, 3, 1), Some((3, 1)));
    }

    #[test]
    fn place_reorder_indices_ignore_invalid_or_same_position_drops() {
        assert_eq!(place_reorder_indices(5, -1, 2), None);
        assert_eq!(place_reorder_indices(5, 5, 2), None);
        assert_eq!(place_reorder_indices(5, 1, -1), None);
        assert_eq!(place_reorder_indices(5, 1, 1), None);
        assert_eq!(place_reorder_indices(5, 1, 2), None);
    }
}
