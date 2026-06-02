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

    "rejected reason=external-dnd-unsupported".to_string()
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
        assert_eq!(
            dnd_drop_validation_summary("file:///tmp/source", "text/uri-list"),
            "rejected reason=external-dnd-unsupported"
        );
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
}
