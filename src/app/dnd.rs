use std::env;

pub(crate) const SLINT_DROPAREA_BACKEND_SOURCE: &str = "Slint DropArea";
const DEBUG_DND_ENV: &str = "FIKA_DEBUG_DND";

pub(crate) struct PlacesDndTrace<'a> {
    pub(crate) backend: &'a str,
    pub(crate) phase: &'a str,
    pub(crate) mime_type: &'a str,
    pub(crate) payload: &'a str,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) slot: i32,
    pub(crate) target: i32,
    pub(crate) over_gap: bool,
    pub(crate) over_item: bool,
}

pub(crate) struct MainDndTrace<'a> {
    pub(crate) backend: &'a str,
    pub(crate) phase: &'a str,
    pub(crate) mime_type: &'a str,
    pub(crate) payload: &'a str,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) rejected: bool,
    pub(crate) target_path: &'a str,
}

pub(crate) fn dnd_debug_enabled_from_env() -> bool {
    env::var(DEBUG_DND_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

pub(crate) fn dnd_places_event_message(trace: &PlacesDndTrace<'_>) -> String {
    format!(
        "[fika dnd] backend={} role={} area=places phase={} mime={} validation={} x={:.1} y={:.1} slot={} target={} gap={} item={} payload={}",
        trace.backend,
        dnd_backend_role(trace.backend),
        trace.phase,
        trace.mime_type,
        dnd_drop_validation_summary(trace.payload, trace.mime_type),
        trace.x,
        trace.y,
        trace.slot,
        trace.target,
        trace.over_gap,
        trace.over_item,
        dnd_payload_summary(trace.payload)
    )
}

pub(crate) fn dnd_main_event_message(trace: &MainDndTrace<'_>) -> String {
    format!(
        "[fika dnd] backend={} role={} area=main phase={} mime={} validation={} x={:.1} y={:.1} rejected={} target_path={} payload={}",
        trace.backend,
        dnd_backend_role(trace.backend),
        trace.phase,
        trace.mime_type,
        dnd_drop_validation_summary(trace.payload, trace.mime_type),
        trace.x,
        trace.y,
        trace.rejected,
        dnd_payload_summary(trace.target_path),
        dnd_payload_summary(trace.payload)
    )
}

fn dnd_backend_role(backend: &str) -> &'static str {
    match backend {
        SLINT_DROPAREA_BACKEND_SOURCE => "slint-primary",
        _ => "internal",
    }
}

fn dnd_drop_validation_summary(_payload: &str, mime_type: &str) -> String {
    if is_internal_drag_mime(mime_type) {
        return "internal-drag".to_string();
    }

    "rejected reason=unsupported-dnd-payload".to_string()
}

fn is_internal_drag_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "place"
            | "folder"
            | "file"
            | "application/x-fika-folder-path"
            | "application/x-fika-file-path"
            | "application/x-fika-place-path"
    )
}

pub(crate) fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn dnd_payload_summary(payload: &str) -> String {
    const MAX_CHARS: usize = 96;
    let mut summary = String::new();
    for ch in payload.chars().take(MAX_CHARS) {
        match ch {
            '\n' => summary.push_str("\\n"),
            '\r' => summary.push_str("\\r"),
            '\t' => summary.push_str("\\t"),
            _ => summary.push(ch),
        }
    }
    if payload.chars().count() > MAX_CHARS {
        summary.push_str("...");
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_flag_truthy_values_enable_debug_flags() {
        for value in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(env_flag_is_truthy(value));
        }
        for value in ["", "0", "false", "no", "off", "anything-else"] {
            assert!(!env_flag_is_truthy(value));
        }
    }

    #[test]
    fn dnd_validation_recognizes_data_transfer_internal_kinds() {
        for mime_type in ["place", "folder", "file"] {
            assert_eq!(
                dnd_drop_validation_summary("/tmp/source", mime_type),
                "internal-drag"
            );
        }
    }

    #[test]
    fn dnd_validation_rejects_non_internal_drop_payloads() {
        for mime_type in [
            "text/uri-list",
            "text/plain",
            "text/plain;charset=utf-8",
            "application/octet-stream",
        ] {
            assert_eq!(
                dnd_drop_validation_summary("file:///tmp/source", mime_type),
                "rejected reason=unsupported-dnd-payload"
            );
        }
    }

    #[test]
    fn dnd_scope_does_not_keep_native_window_fallbacks() {
        let sources = [
            include_str!("../../Cargo.toml"),
            include_str!("../main.rs"),
            include_str!("../../ui/app.slint"),
            include_str!("../../ui/dnd_bridge.slint"),
        ];

        for source in sources {
            for forbidden in ["winit", "x11", "X11", "native-window", "native_window"] {
                assert!(
                    !source.contains(forbidden),
                    "drag and drop should stay on Slint DropArea without {forbidden} fallback"
                );
            }
        }
    }

    #[test]
    fn dnd_payload_summary_escapes_control_chars_and_truncates() {
        assert_eq!(
            dnd_payload_summary("file:///tmp/A\nfile:///tmp/B\tx"),
            "file:///tmp/A\\nfile:///tmp/B\\tx"
        );

        assert_eq!(
            dnd_payload_summary(&"a".repeat(97)),
            format!("{}...", "a".repeat(96))
        );
    }

    #[test]
    fn dnd_places_trace_message_reports_drop_geometry() {
        assert_eq!(
            dnd_places_event_message(&PlacesDndTrace {
                backend: SLINT_DROPAREA_BACKEND_SOURCE,
                phase: "can-drop-accepted",
                mime_type: "folder",
                payload: "/tmp/A",
                x: 12.25,
                y: 99.75,
                slot: 3,
                target: 2,
                over_gap: false,
                over_item: true,
            }),
            "[fika dnd] backend=Slint DropArea role=slint-primary area=places phase=can-drop-accepted mime=folder validation=internal-drag x=12.2 y=99.8 slot=3 target=2 gap=false item=true payload=/tmp/A"
        );
    }

    #[test]
    fn dnd_main_trace_message_reports_target_and_rejection_state() {
        assert_eq!(
            dnd_main_event_message(&MainDndTrace {
                backend: SLINT_DROPAREA_BACKEND_SOURCE,
                phase: "can-drop-rejected",
                mime_type: "file",
                payload: "/tmp/A",
                x: 12.25,
                y: 99.75,
                rejected: true,
                target_path: "/tmp/Target Folder",
            }),
            "[fika dnd] backend=Slint DropArea role=slint-primary area=main phase=can-drop-rejected mime=file validation=internal-drag x=12.2 y=99.8 rejected=true target_path=/tmp/Target Folder payload=/tmp/A"
        );
    }

    #[test]
    fn side_button_navigation_stays_scoped_to_main_view_sources() {
        let app = include_str!("../../ui/app.slint");
        let main_pane_start = app
            .find("main-pane := Rectangle")
            .expect("app.slint should keep an explicit main pane root");
        let before_main_pane = &app[..main_pane_start];
        assert!(
            !before_main_pane.contains("PointerEventButton.back")
                && !before_main_pane.contains("PointerEventButton.forward"),
            "mouse side buttons must not navigate from topbar, sidebar, or splitter sources"
        );

        let file_pane = app
            .split_once("component FilePane inherits Rectangle {")
            .expect("app.slint should define the reusable FilePane component")
            .1
            .split_once("component PaneSlot inherits FilePane")
            .expect("FilePane should be defined before PaneSlot")
            .0;
        assert!(file_pane.contains("navigate_back => { root.go_back(root.pane-side); }"));
        assert!(file_pane.contains("navigate_forward => { root.go_forward(root.pane-side); }"));

        let pane_slot = app
            .split_once("component PaneSlot inherits FilePane {")
            .expect("app.slint should define the routed PaneSlot component")
            .1
            .split_once("export component AppWindow inherits Window")
            .expect("PaneSlot should be defined before AppWindow")
            .0;
        assert!(pane_slot.contains("go_back(side) => { PaneRouting.go-back(side); }"));
        assert!(pane_slot.contains("go_forward(side) => { PaneRouting.go-forward(side); }"));

        let main_pane = &app[main_pane_start..];
        assert_eq!(main_pane.matches("PaneSlot {").count(), 2);
        assert!(app.contains("public function route-pane-go-back(side: int)"));
        assert!(app.contains("root.pane_go_back(side);"));
        assert!(app.contains("public function route-pane-go-forward(side: int)"));
        assert!(app.contains("root.pane_go_forward(side);"));
        assert!(!app.contains("root.inactive_go_back();"));
        assert!(!app.contains("root.left_pane_go_back();"));
        assert!(!app.contains("root.inactive_go_forward();"));
        assert!(!app.contains("root.left_pane_go_forward();"));

        let split_pane = include_str!("../../ui/split_pane.slint");
        assert!(split_pane.contains("PointerEventButton.back"));
        assert!(split_pane.contains("PointerEventButton.forward"));
        assert!(split_pane.contains("root.navigate_back();"));
        assert!(split_pane.contains("root.navigate_forward();"));

        let file_tile = include_str!("../../ui/file_tile.slint");
        assert!(file_tile.contains("PointerEventButton.back"));
        assert!(file_tile.contains("PointerEventButton.forward"));

        for (name, source) in [
            ("top_bar.slint", include_str!("../../ui/top_bar.slint")),
            ("widgets.slint", include_str!("../../ui/widgets.slint")),
            ("menus.slint", include_str!("../../ui/menus.slint")),
            (
                "status_bar.slint",
                include_str!("../../ui/status_bar.slint"),
            ),
        ] {
            assert!(
                !source.contains("PointerEventButton.back")
                    && !source.contains("PointerEventButton.forward"),
                "mouse side buttons must not navigate from {name}"
            );
        }
    }
}
