use crate::config::paths::expand_user_path;
use std::path::PathBuf;

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
}
