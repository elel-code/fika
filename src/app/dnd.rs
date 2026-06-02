use crate::config::paths::expand_user_path;
use std::env;
use std::path::PathBuf;

pub(crate) const WINIT_DROPPED_FILE_FALLBACK_SOURCE: &str = "winit DroppedFile fallback";
pub(crate) const WINIT_DROPPED_FILE_MIME: &str = "winit/dropped-file";
pub(crate) const SLINT_DROPAREA_BACKEND_SOURCE: &str = "Slint DropArea";
const DISABLE_WINIT_DROP_FALLBACK_ENV: &str = "FIKA_DISABLE_WINIT_DROP_FALLBACK";
const DEBUG_DND_ENV: &str = "FIKA_DEBUG_DND";
const SLINT_DROPAREA_PAYLOAD_SUMMARY: &str = "internal-data-transfer,text/uri-list,text/plain";

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

    path_from_external_text_result(payload).map(|path| ExternalPathDrop {
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

pub(crate) fn winit_file_drop_fallback_enabled_from_env() -> bool {
    winit_file_drop_fallback_enabled(env::var(DISABLE_WINIT_DROP_FALLBACK_ENV).ok().as_deref())
}

fn winit_file_drop_fallback_enabled(disable_env_value: Option<&str>) -> bool {
    disable_env_value
        .map(|value| !env_flag_is_truthy(value))
        .unwrap_or(true)
}

pub(crate) fn dnd_debug_enabled_from_env() -> bool {
    env::var(DEBUG_DND_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

pub(crate) fn dnd_startup_summary(winit_fallback_enabled: bool) -> String {
    format!(
        "slint_droparea=primary slint_droparea_payloads={} winit_fallback={} winit_fallback_role=compat disable_winit_env={}",
        SLINT_DROPAREA_PAYLOAD_SUMMARY,
        if winit_fallback_enabled {
            "enabled"
        } else {
            "disabled"
        },
        DISABLE_WINIT_DROP_FALLBACK_ENV
    )
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

#[cfg(test)]
fn drop_target_rejection_debug_reason(reason: &str) -> &'static str {
    match reason {
        "Cannot drop an item onto itself" => "self-target",
        "Cannot drop a folder into itself" => "descendant-target",
        _ => "target-rejected",
    }
}

fn dnd_backend_role(backend: &str) -> &'static str {
    match backend {
        SLINT_DROPAREA_BACKEND_SOURCE => "slint-primary",
        WINIT_DROPPED_FILE_FALLBACK_SOURCE => "winit-fallback",
        _ => "internal",
    }
}

fn dnd_drop_validation_summary(payload: &str, mime_type: &str) -> String {
    if is_internal_drag_mime(mime_type) {
        return "internal-drag".to_string();
    }

    if mime_type == WINIT_DROPPED_FILE_MIME {
        return path_from_external_text_result(payload)
            .map(|path| {
                format!(
                    "external-local-path path={}",
                    dnd_payload_summary(&path.display().to_string())
                )
            })
            .unwrap_or_else(|rejection| format!("rejected reason={}", rejection.debug_reason()));
    }

    if is_external_path_drop_mime(mime_type) {
        return external_path_drop_from_payload(payload, mime_type)
            .map(|drop| {
                format!(
                    "external-local-path path={}",
                    dnd_payload_summary(&drop.path.display().to_string())
                )
            })
            .unwrap_or_else(|rejection| format!("rejected reason={}", rejection.debug_reason()));
    }

    format!("rejected reason=unsupported-mime:{mime_type}")
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

pub(crate) fn path_from_external_text(text: &str) -> Option<PathBuf> {
    path_from_external_text_result(text).ok()
}

fn path_from_external_text_result(text: &str) -> Result<PathBuf, ExternalPathDropRejection> {
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
    fn external_path_drop_payload_decodes_supported_slint_mime_types() {
        assert!(is_external_path_drop_mime("text/uri-list"));
        assert!(is_external_path_drop_mime("text/plain"));
        assert!(!is_external_path_drop_mime("application/octet-stream"));

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
    fn env_flag_truthy_values_disable_winit_drop_fallback() {
        for value in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(env_flag_is_truthy(value));
            assert!(!winit_file_drop_fallback_enabled(Some(value)));
        }
        for value in ["", "0", "false", "no", "off", "anything-else"] {
            assert!(!env_flag_is_truthy(value));
            assert!(winit_file_drop_fallback_enabled(Some(value)));
        }
        assert!(winit_file_drop_fallback_enabled(None));
    }

    #[test]
    fn dnd_startup_summary_reports_drop_backends() {
        assert_eq!(
            dnd_startup_summary(true),
            "slint_droparea=primary slint_droparea_payloads=internal-data-transfer,text/uri-list,text/plain winit_fallback=enabled winit_fallback_role=compat disable_winit_env=FIKA_DISABLE_WINIT_DROP_FALLBACK"
        );
        assert!(
            dnd_startup_summary(false).contains("winit_fallback=disabled"),
            "disabled fallback state should be visible in startup diagnostics"
        );
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
                mime_type: "text/uri-list",
                payload: "file:///tmp/A\nfile:///tmp/B",
                x: 12.25,
                y: 99.75,
                slot: 3,
                target: 2,
                over_gap: false,
                over_item: true,
            }),
            "[fika dnd] backend=Slint DropArea role=slint-primary area=places phase=can-drop-accepted mime=text/uri-list validation=external-local-path path=/tmp/A x=12.2 y=99.8 slot=3 target=2 gap=false item=true payload=file:///tmp/A\\nfile:///tmp/B"
        );
    }

    #[test]
    fn dnd_main_trace_message_reports_target_and_rejection_state() {
        assert_eq!(
            dnd_main_event_message(&MainDndTrace {
                backend: SLINT_DROPAREA_BACKEND_SOURCE,
                phase: "can-drop-rejected",
                mime_type: "text/uri-list",
                payload: "file:///tmp/A\nfile:///tmp/B",
                x: 12.25,
                y: 99.75,
                rejected: true,
                target_path: "/tmp/Target Folder",
            }),
            "[fika dnd] backend=Slint DropArea role=slint-primary area=main phase=can-drop-rejected mime=text/uri-list validation=external-local-path path=/tmp/A x=12.2 y=99.8 rejected=true target_path=/tmp/Target Folder payload=file:///tmp/A\\nfile:///tmp/B"
        );
    }

    #[test]
    fn dnd_winit_fallback_trace_reports_compat_role() {
        assert_eq!(
            dnd_places_event_message(&PlacesDndTrace {
                backend: WINIT_DROPPED_FILE_FALLBACK_SOURCE,
                phase: "dropped",
                mime_type: WINIT_DROPPED_FILE_MIME,
                payload: "/tmp/External Folder",
                x: 22.0,
                y: 40.0,
                slot: 1,
                target: -1,
                over_gap: true,
                over_item: false,
            }),
            "[fika dnd] backend=winit DroppedFile fallback role=winit-fallback area=places phase=dropped mime=winit/dropped-file validation=external-local-path path=/tmp/External Folder x=22.0 y=40.0 slot=1 target=-1 gap=true item=false payload=/tmp/External Folder"
        );
    }

    #[test]
    fn drop_target_rejection_debug_reason_is_stable() {
        assert_eq!(
            drop_target_rejection_debug_reason("Cannot drop an item onto itself"),
            "self-target"
        );
        assert_eq!(
            drop_target_rejection_debug_reason("Cannot drop a folder into itself"),
            "descendant-target"
        );
        assert_eq!(
            drop_target_rejection_debug_reason("Some future transfer rejection"),
            "target-rejected"
        );
    }
}
