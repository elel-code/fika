use crate::app::events::ExternalFileDrop;
use crate::app::geometry::place_drop_geometry;
use crate::app::state::AppState;
use crate::config::paths::expand_user_path;
use crate::desktop::systemd_launch;
use crate::fs::places::{builtin_places, place_entry, save_places};
use crate::{AppWindow, PlaceEntry};
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
    add_place_at_slot_inner(ui, state, path, slot, None);
}

pub(crate) fn add_place_at_slot_from_external_drop(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
    slot: i32,
    source: &str,
) {
    add_place_at_slot_inner(ui, state, path, slot, Some(source));
}

pub(crate) fn add_place_at_slot_from_external_payload(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    payload: &str,
    slot: i32,
    mime_type: &str,
) {
    match external_path_drop_from_payload(payload, mime_type) {
        Ok(drop) => add_place_at_slot_inner(ui, state, drop.path, slot, Some(drop.source.as_str())),
        Err(rejection) => ui.set_status(rejection.status_message().into()),
    }
}

fn add_place_at_slot_inner(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
    slot: i32,
    source: Option<&str>,
) {
    let path = normalize_dropped_path(path);
    if !path.is_dir() {
        ui.set_status("Only folders can be added to Places".into());
        return;
    }

    let mut state = state.borrow_mut();
    let path_string = path.display().to_string();
    if state
        .places
        .iter()
        .any(|place| place.path.as_str() == path_string)
    {
        ui.set_status("Folder is already in Places".into());
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
        .unwrap_or(state.places.len())
        .min(state.places.len());
    state.places.insert(slot, entry);
    save_places(&state.places);
    sync_places(ui, &state.places);
    ui.set_status(external_drop_status("Folder added to Places", source).into());
}

pub(crate) fn normalize_dropped_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    dropped_path_from_text(text.as_ref()).unwrap_or_else(|| expand_user_path(text.as_ref()))
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ExternalPathDrop {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ExternalPathDropRejection {
    UnsupportedMime(String),
    EmptyPayload,
    NoLocalFilePath,
}

impl ExternalPathDropRejection {
    pub(crate) fn status_message(&self) -> String {
        match self {
            Self::UnsupportedMime(mime_type) => {
                format!("External drop MIME is not supported: {mime_type}")
            }
            Self::EmptyPayload => "External drop payload was empty".to_string(),
            Self::NoLocalFilePath => "External drop did not contain a local file path".to_string(),
        }
    }

    pub(crate) fn debug_reason(&self) -> String {
        match self {
            Self::UnsupportedMime(mime_type) => format!("unsupported-mime:{mime_type}"),
            Self::EmptyPayload => "empty-payload".to_string(),
            Self::NoLocalFilePath => "no-local-file-path".to_string(),
        }
    }
}

pub(crate) fn external_path_drop_from_payload(
    payload: &str,
    mime_type: &str,
) -> Result<ExternalPathDrop, ExternalPathDropRejection> {
    if !is_external_path_drop_mime(mime_type) {
        return Err(ExternalPathDropRejection::UnsupportedMime(
            mime_type.to_string(),
        ));
    }

    dropped_path_from_text_result(payload).map(|path| ExternalPathDrop {
        path,
        source: format!("Slint DropArea {mime_type}"),
    })
}

pub(crate) fn external_path_drop_rejection_reason(
    payload: &str,
    mime_type: &str,
) -> Option<String> {
    external_path_drop_from_payload(payload, mime_type)
        .err()
        .map(|rejection| rejection.debug_reason())
}

pub(crate) fn is_external_path_drop_mime(mime_type: &str) -> bool {
    matches!(mime_type, "text/uri-list" | "text/plain")
}

pub(crate) fn is_supported_places_drop_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "application/x-fika-folder-path"
            | "application/x-fika-file-path"
            | "application/x-fika-place-path"
    ) || is_external_path_drop_mime(mime_type)
}

pub(crate) fn places_drop_force_gap(mime_type: &str) -> bool {
    is_external_path_drop_mime(mime_type)
}

fn dropped_path_from_text(text: &str) -> Option<PathBuf> {
    dropped_path_from_text_result(text).ok()
}

fn dropped_path_from_text_result(text: &str) -> Result<PathBuf, ExternalPathDropRejection> {
    let mut saw_payload_line = false;
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
    {
        saw_payload_line = true;
        if let Some(path) = dropped_path_from_line(line) {
            return Ok(path);
        }
    }

    if saw_payload_line {
        Err(ExternalPathDropRejection::NoLocalFilePath)
    } else {
        Err(ExternalPathDropRejection::EmptyPayload)
    }
}

fn dropped_path_from_line(line: &str) -> Option<PathBuf> {
    let line = line.trim();
    if line.starts_with("file://") {
        return local_file_uri_to_path(line);
    }
    if line.contains("://") {
        return None;
    }
    Some(expand_user_path(line))
}

fn local_file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri
        .strip_prefix("file://localhost/")
        .or_else(|| uri.strip_prefix("file:///"))
        .map(|path| format!("/{path}"))?;
    Some(PathBuf::from(percent_decode(&path)))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            output.push(hex);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

pub(crate) fn rename_place(ui: &AppWindow, state: &Rc<RefCell<AppState>>, index: i32, label: &str) {
    let label = label.trim();
    if label.is_empty() {
        ui.set_status("Place name cannot be empty".into());
        return;
    }

    let mut state = state.borrow_mut();
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let Some(place) = state.places.get_mut(index) else {
        return;
    };
    if place.is_builtin {
        ui.set_status("Built-in places cannot be renamed".into());
        return;
    }

    place.label = label.into();
    place.marker = place_marker(label).into();
    save_places(&state.places);
    sync_places(ui, &state.places);
    ui.set_status("Place renamed".into());
}

pub(crate) fn remove_place(ui: &AppWindow, state: &Rc<RefCell<AppState>>, index: i32) {
    let mut state = state.borrow_mut();
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if index >= state.places.len() {
        return;
    }
    if state.places[index].is_builtin {
        ui.set_status("Built-in places cannot be removed".into());
        return;
    }

    state.places.remove(index);
    save_places(&state.places);
    sync_places(ui, &state.places);
    ui.set_status("Place removed".into());
}

pub(crate) fn apply_external_file_drop(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    drop: ExternalFileDrop,
) {
    if drop.x > ui.get_sidebar_width_px() {
        ui.set_status("Drop folders on the Places sidebar to add them".into());
        return;
    }

    let slot = place_drop_geometry(
        drop.y,
        state.borrow().places.len(),
        ui.get_places_list_y_px(),
        ui.get_places_row_stride_px(),
        true,
    )
    .slot;
    add_place_at_slot_from_external_drop(ui, state, drop.path, slot, drop.source.as_str());
}

pub(crate) fn restore_default_places(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    state.places = builtin_places();
    save_places(&state.places);
    sync_places(ui, &state.places);
    ui.set_status("Default places restored".into());
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
        ui.set_status("Cannot locate Fika executable".into());
        return;
    };
    let program = exe.to_string_lossy().to_string();
    let args = vec![path];
    match systemd_launch::spawn_in_user_scope(&program, &args, Some("Fika New Window")) {
        Ok(launch) => {
            if let Some(unit) = &launch.unit {
                state.borrow_mut().launched_units.push(unit.clone());
            }
            ui.set_status(format_new_window_status(&label, &launch).into());
        }
        Err(err) => ui.set_status(format!("Cannot open new window: {err}").into()),
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
    if from < 0 || to_slot < 0 {
        return;
    }

    let mut state = state.borrow_mut();
    let from = from as usize;
    let mut to = to_slot as usize;
    if from >= state.places.len() {
        return;
    }

    to = to.min(state.places.len());
    if to > from {
        to -= 1;
    }
    if from == to {
        return;
    }

    let place = state.places.remove(from);
    state.places.insert(to, place);
    save_places(&state.places);
    sync_places(ui, &state.places);
}

pub(crate) fn reorder_place_path(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: &str,
    to_slot: i32,
) {
    let from = {
        let state = state.borrow();
        state
            .places
            .iter()
            .position(|place| place.path.as_str() == path)
    };
    let Some(from) = from else {
        ui.set_status("Place is no longer available".into());
        return;
    };
    reorder_place(ui, state, from as i32, to_slot);
}

fn place_marker(label: &str) -> String {
    label
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "+".to_string())
}

fn external_drop_status(message: &str, source: Option<&str>) -> String {
    source.filter(|source| !source.is_empty()).map_or_else(
        || message.to_string(),
        |source| format!("{message} via {source}"),
    )
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
    fn external_drop_status_identifies_drop_backend_when_known() {
        assert_eq!(
            external_drop_status(
                "Folder added to Places",
                Some("Slint DropArea text/uri-list")
            ),
            "Folder added to Places via Slint DropArea text/uri-list"
        );
        assert_eq!(
            external_drop_status("Folder added to Places", Some("")),
            "Folder added to Places"
        );
        assert_eq!(
            external_drop_status("Folder added to Places", None),
            "Folder added to Places"
        );
    }

    #[test]
    fn external_path_drop_payload_decodes_supported_slint_mime_types() {
        assert!(is_supported_places_drop_mime("text/uri-list"));
        assert!(is_supported_places_drop_mime("text/plain"));
        assert!(places_drop_force_gap("text/uri-list"));
        assert!(places_drop_force_gap("text/plain"));
        assert!(is_supported_places_drop_mime(
            "application/x-fika-folder-path"
        ));
        assert!(is_supported_places_drop_mime(
            "application/x-fika-file-path"
        ));
        assert!(is_supported_places_drop_mime(
            "application/x-fika-place-path"
        ));
        assert!(!places_drop_force_gap("application/x-fika-folder-path"));
        assert!(!places_drop_force_gap("application/x-fika-file-path"));
        assert!(!places_drop_force_gap("application/x-fika-place-path"));
        assert!(!is_supported_places_drop_mime("application/octet-stream"));

        assert_eq!(
            external_path_drop_from_payload(
                "# comment\nfile://localhost/tmp/Hello%20World\nfile:///tmp/Second\n",
                "text/uri-list"
            ),
            Ok(ExternalPathDrop {
                path: PathBuf::from("/tmp/Hello World"),
                source: "Slint DropArea text/uri-list".to_string(),
            })
        );
        assert_eq!(
            external_path_drop_from_payload("~/Projects", "text/plain"),
            Ok(ExternalPathDrop {
                path: expand_user_path("~/Projects"),
                source: "Slint DropArea text/plain".to_string(),
            })
        );
        assert_eq!(
            external_path_drop_from_payload("file:///tmp/ignored", "application/octet-stream"),
            Err(ExternalPathDropRejection::UnsupportedMime(
                "application/octet-stream".to_string()
            ))
        );
    }

    #[test]
    fn external_path_drop_rejects_non_local_uri_payloads() {
        assert_eq!(
            external_path_drop_from_payload("file://remote-host/tmp/Project", "text/uri-list"),
            Err(ExternalPathDropRejection::NoLocalFilePath)
        );
        assert_eq!(
            external_path_drop_from_payload("sftp://host/tmp/Project", "text/uri-list"),
            Err(ExternalPathDropRejection::NoLocalFilePath)
        );
        assert_eq!(
            external_path_drop_from_payload(
                "# comment\nfile://remote-host/tmp/Project\nfile:///tmp/Local",
                "text/uri-list"
            ),
            Ok(ExternalPathDrop {
                path: PathBuf::from("/tmp/Local"),
                source: "Slint DropArea text/uri-list".to_string(),
            })
        );
    }

    #[test]
    fn external_path_drop_rejection_reason_distinguishes_failures() {
        assert_eq!(
            external_path_drop_rejection_reason("file:///tmp/Project", "application/octet-stream"),
            Some("unsupported-mime:application/octet-stream".to_string())
        );
        assert_eq!(
            external_path_drop_rejection_reason("# comment\n\n", "text/uri-list"),
            Some("empty-payload".to_string())
        );
        assert_eq!(
            external_path_drop_rejection_reason("file://remote-host/tmp/Project", "text/uri-list"),
            Some("no-local-file-path".to_string())
        );
        assert_eq!(
            external_path_drop_rejection_reason("file:///tmp/Project", "text/uri-list"),
            None
        );
    }

    #[test]
    fn contains_place_path_matches_exact_place_path() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.places = vec![place_entry("Projects", PathBuf::from("/tmp/projects"), "P")];

        assert!(contains_place_path(&state, "/tmp/projects"));
        assert!(!contains_place_path(&state, "/tmp/projects-old"));
    }
}
