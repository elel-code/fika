use std::path::{Path, PathBuf};

use super::network::{network_uri_from_path, normalize_network_uri};

const FIKA_CUT_MARKER: &str = "# fika-cut";
const GNOME_COPY_MARKER: &str = "copy";
const GNOME_CUT_MARKER: &str = "cut";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileClipboardRole {
    #[default]
    Copy,
    Cut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileClipboardPayload {
    pub role: FileClipboardRole,
    pub paths: Vec<PathBuf>,
}

impl FileClipboardPayload {
    pub fn new(role: FileClipboardRole, paths: Vec<PathBuf>) -> Self {
        Self { role, paths }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

pub fn encode_file_clipboard_text(role: FileClipboardRole, paths: &[PathBuf]) -> String {
    let mut lines = Vec::with_capacity(paths.len() + 1);
    if role == FileClipboardRole::Cut {
        lines.push(FIKA_CUT_MARKER.to_string());
    }
    lines.extend(paths.iter().map(|path| path_to_file_uri(path)));
    lines.join("\n")
}

pub fn decode_file_clipboard_text(text: &str) -> Option<FileClipboardPayload> {
    let mut role = FileClipboardRole::Copy;
    let mut paths = Vec::new();
    let mut first_payload_line = true;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if line == FIKA_CUT_MARKER {
                role = FileClipboardRole::Cut;
            }
            continue;
        }
        if first_payload_line {
            match line {
                GNOME_CUT_MARKER => {
                    role = FileClipboardRole::Cut;
                    first_payload_line = false;
                    continue;
                }
                GNOME_COPY_MARKER => {
                    role = FileClipboardRole::Copy;
                    first_payload_line = false;
                    continue;
                }
                _ => {}
            }
        }
        first_payload_line = false;

        if let Some(path) = parse_clipboard_path_line(line) {
            paths.push(path);
        }
    }

    (!paths.is_empty()).then_some(FileClipboardPayload { role, paths })
}

fn parse_clipboard_path_line(line: &str) -> Option<PathBuf> {
    if let Some(rest) = line.strip_prefix("file://") {
        return percent_decode_file_uri_path(rest).map(PathBuf::from);
    }
    if let Ok(uri) = normalize_network_uri(line) {
        return Some(PathBuf::from(uri));
    }
    line.starts_with('/').then(|| PathBuf::from(line))
}

fn path_to_file_uri(path: &Path) -> String {
    if let Some(uri) = network_uri_from_path(path) {
        return uri;
    }
    let raw = path.to_string_lossy();
    let mut uri = String::with_capacity(raw.len() + "file://".len());
    uri.push_str("file://");
    for byte in raw.as_bytes() {
        if file_uri_byte_is_unreserved(*byte) || *byte == b'/' {
            uri.push(*byte as char);
        } else {
            uri.push('%');
            uri.push(hex_digit(byte >> 4));
            uri.push(hex_digit(byte & 0x0f));
        }
    }
    uri
}

fn percent_decode_file_uri_path(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut index = 0;
    let text = text.as_bytes();
    while index < text.len() {
        if text[index] == b'%' {
            let high = hex_value(*text.get(index + 1)?)?;
            let low = hex_value(*text.get(index + 2)?)?;
            bytes.push((high << 4) | low);
            index += 3;
        } else {
            bytes.push(text[index]);
            index += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

fn file_uri_byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => unreachable!("hex digit nibble is always 0..=15"),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_clipboard_text_encodes_file_uri_list() {
        let text = encode_file_clipboard_text(
            FileClipboardRole::Copy,
            &[
                PathBuf::from("/tmp/a file.txt"),
                PathBuf::from("/tmp/unicode-文档.txt"),
            ],
        );

        assert_eq!(
            text,
            "file:///tmp/a%20file.txt\nfile:///tmp/unicode-%E6%96%87%E6%A1%A3.txt"
        );
    }

    #[test]
    fn file_clipboard_text_round_trips_cut_role() {
        let paths = vec![PathBuf::from("/tmp/a file.txt"), PathBuf::from("/tmp/b")];
        let text = encode_file_clipboard_text(FileClipboardRole::Cut, &paths);

        assert_eq!(
            decode_file_clipboard_text(&text),
            Some(FileClipboardPayload::new(FileClipboardRole::Cut, paths))
        );
    }

    #[test]
    fn file_clipboard_text_preserves_network_uris() {
        let paths = vec![PathBuf::from("smb://server/share/report%202026.txt")];
        let text = encode_file_clipboard_text(FileClipboardRole::Copy, &paths);

        assert_eq!(text, "smb://server/share/report%202026.txt");
        assert_eq!(
            decode_file_clipboard_text("copy\nsmb://server/share/report%202026.txt\n"),
            Some(FileClipboardPayload::new(FileClipboardRole::Copy, paths))
        );
    }

    #[test]
    fn file_clipboard_text_decodes_plain_absolute_paths() {
        assert_eq!(
            decode_file_clipboard_text("/tmp/a\n/tmp/b\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Copy,
                vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
            ))
        );
    }

    #[test]
    fn file_clipboard_text_decodes_gnome_copied_files_markers() {
        assert_eq!(
            decode_file_clipboard_text("cut\nfile:///tmp/a%20file.txt\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Cut,
                vec![PathBuf::from("/tmp/a file.txt")]
            ))
        );
        assert_eq!(
            decode_file_clipboard_text("copy\nfile:///tmp/a.txt\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Copy,
                vec![PathBuf::from("/tmp/a.txt")]
            ))
        );
    }

    #[test]
    fn file_clipboard_text_rejects_non_file_text() {
        assert_eq!(decode_file_clipboard_text("hello world"), None);
        assert_eq!(decode_file_clipboard_text("file:///tmp/%ZZ"), None);
    }
}
