use crate::desktop::wayland_clipboard;
use std::path::PathBuf;

const FILE_CLIPBOARD_MIME: &str = "x-special/gnome-copied-files";
const URI_LIST_MIME: &str = "text/uri-list";
const KDE_CUT_SELECTION_MIME: &str = "application/x-kde-cutselection";
const TEXT_PLAIN_MIME: &str = "text/plain";
const TEXT_CLIPBOARD_MIMES: &[&str] = &[
    TEXT_PLAIN_MIME,
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];
const IMAGE_CLIPBOARD_MIMES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/bmp",
    "image/webp",
    "image/tiff",
    "image/x-tiff",
    "image/svg+xml",
    "image/x-icon",
    "image/vnd.microsoft.icon",
    "image/x-bmp",
    "image/x-ms-bmp",
    "image/pjpeg",
    "image/x-png",
    "image/avif",
    "image/heic",
    "image/heif",
    "image/jxl",
];
const VIDEO_CLIPBOARD_MIMES: &[&str] = &[
    "video/mp4",
    "video/webm",
    "video/ogg",
    "video/mpeg",
    "video/quicktime",
    "video/x-msvideo",
    "video/x-matroska",
    "video/x-flv",
    "video/3gpp",
    "video/3gpp2",
    "video/x-ms-wmv",
    "video/avi",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileClipboard {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) cut: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardSnapshot {
    pub(crate) files: Option<FileClipboard>,
    pub(crate) content_kind: Option<ClipboardContentKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClipboardContentKind {
    Image,
    Video,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardContent {
    pub(crate) kind: ClipboardContentKind,
    pub(crate) data: Vec<u8>,
    pub(crate) mime_type: String,
}

impl ClipboardContent {
    pub(crate) fn base_file_name(&self) -> &'static str {
        match self.kind {
            ClipboardContentKind::Image => "Pasted Image",
            ClipboardContentKind::Video => "Pasted Video",
            ClipboardContentKind::Text => "Pasted Text",
        }
    }

    pub(crate) fn extension(&self) -> Option<&'static str> {
        match self.kind {
            ClipboardContentKind::Image => image_extension(&self.mime_type),
            ClipboardContentKind::Video => video_extension(&self.mime_type),
            ClipboardContentKind::Text => Some("txt"),
        }
    }
}

pub(crate) fn copy_text(text: &str) -> Result<(), String> {
    wayland_clipboard::publish_mime_data(text_clipboard_offers(text))
}

pub(crate) fn copy_file_list(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no files to copy".to_string());
    }
    wayland_clipboard::publish_mime_data(file_list_clipboard_offers(paths, cut))
}

pub(crate) fn read_clipboard_snapshot() -> Result<ClipboardSnapshot, String> {
    let mime_types = read_available_mime_types()?;
    match read_file_list_from_mime_types(&mime_types) {
        Ok(files) => Ok(ClipboardSnapshot {
            files: Some(files),
            content_kind: None,
        }),
        Err(file_error) => {
            let content_kind = content_kind_from_mime_types(mime_types.iter());
            if content_kind.is_some() {
                Ok(ClipboardSnapshot {
                    files: None,
                    content_kind,
                })
            } else {
                Err(format!(
                    "{file_error}; clipboard does not contain pasteable image, video, or text data"
                ))
            }
        }
    }
}

fn read_file_list_from_mime_types(mime_types: &[String]) -> Result<FileClipboard, String> {
    let mut errors = Vec::new();

    if mime_types_contain(mime_types, FILE_CLIPBOARD_MIME) {
        match read_mime_text(FILE_CLIPBOARD_MIME).and_then(|payload| {
            parse_file_list_payload(&payload)
                .ok_or_else(|| "clipboard file-list does not contain file paths".to_string())
        }) {
            Ok(clipboard) => return Ok(clipboard),
            Err(err) => errors.push(err),
        }
    }

    if mime_types_contain(mime_types, URI_LIST_MIME) {
        match read_mime_text(URI_LIST_MIME).and_then(|payload| {
            let cut = read_kde_cut_selection(mime_types);
            parse_uri_list_payload(&payload, cut)
                .ok_or_else(|| "clipboard uri-list does not contain file paths".to_string())
        }) {
            Ok(clipboard) => return Ok(clipboard),
            Err(err) => errors.push(err),
        }
    }

    if errors.is_empty() {
        Err(format!(
            "clipboard does not advertise {FILE_CLIPBOARD_MIME} or {URI_LIST_MIME}"
        ))
    } else {
        Err(errors.join("; "))
    }
}

pub(crate) fn read_non_file_content() -> Result<ClipboardContent, String> {
    let mime_types = read_available_mime_types()?;
    let mut errors = Vec::new();
    for (kind, mimes) in [
        (ClipboardContentKind::Image, IMAGE_CLIPBOARD_MIMES),
        (ClipboardContentKind::Video, VIDEO_CLIPBOARD_MIMES),
        (ClipboardContentKind::Text, TEXT_CLIPBOARD_MIMES),
    ] {
        match read_first_content_mime(kind, mimes, &mime_types) {
            Ok(content) => return Ok(content),
            Err(err) => errors.push(err),
        }
    }
    Err(format!(
        "clipboard does not contain pasteable image, video, or text data: {}",
        errors.join("; ")
    ))
}

fn read_available_mime_types() -> Result<Vec<String>, String> {
    wayland_clipboard::list_mime_types()
}

fn content_kind_from_mime_types<'a>(
    mimes: impl IntoIterator<Item = &'a String>,
) -> Option<ClipboardContentKind> {
    let mimes = mimes.into_iter().map(String::as_str).collect::<Vec<_>>();
    if mimes
        .iter()
        .any(|mime| IMAGE_CLIPBOARD_MIMES.contains(mime))
    {
        Some(ClipboardContentKind::Image)
    } else if mimes
        .iter()
        .any(|mime| VIDEO_CLIPBOARD_MIMES.contains(mime))
    {
        Some(ClipboardContentKind::Video)
    } else if mimes.iter().any(|mime| TEXT_CLIPBOARD_MIMES.contains(mime)) {
        Some(ClipboardContentKind::Text)
    } else {
        None
    }
}

fn file_list_payload(paths: &[PathBuf], cut: bool) -> String {
    let action = if cut { "cut" } else { "copy" };
    let mut payload = String::from(action);
    for path in paths {
        payload.push('\n');
        payload.push_str(&path_to_file_uri(path));
    }
    payload.push('\n');
    payload
}

fn uri_list_payload(paths: &[PathBuf]) -> String {
    let mut payload = String::new();
    for path in paths {
        payload.push_str(&path_to_file_uri(path));
        payload.push('\n');
    }
    payload
}

fn file_list_clipboard_offers(
    paths: &[PathBuf],
    cut: bool,
) -> Vec<wayland_clipboard::ClipboardOffer> {
    let kde_cut_payload = if cut {
        b"1\n".to_vec()
    } else {
        b"0\n".to_vec()
    };
    vec![
        wayland_clipboard::ClipboardOffer::new(
            FILE_CLIPBOARD_MIME,
            file_list_payload(paths, cut).into_bytes(),
        ),
        wayland_clipboard::ClipboardOffer::new(URI_LIST_MIME, uri_list_payload(paths).into_bytes()),
        wayland_clipboard::ClipboardOffer::new(KDE_CUT_SELECTION_MIME, kde_cut_payload),
    ]
}

fn text_clipboard_offers(text: &str) -> Vec<wayland_clipboard::ClipboardOffer> {
    TEXT_CLIPBOARD_MIMES
        .iter()
        .map(|mime| wayland_clipboard::ClipboardOffer::new(*mime, text.as_bytes().to_vec()))
        .collect()
}

fn parse_file_list_payload(payload: &str) -> Option<FileClipboard> {
    let mut lines = payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let action = lines.next()?;
    let cut = match action {
        "cut" => true,
        "copy" => false,
        _ => return None,
    };
    let paths = lines.filter_map(file_uri_to_path).collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        Some(FileClipboard { paths, cut })
    }
}

fn parse_uri_list_payload(payload: &str, cut: bool) -> Option<FileClipboard> {
    let paths = payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(file_uri_to_path)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        Some(FileClipboard { paths, cut })
    }
}

fn read_kde_cut_selection(mime_types: &[String]) -> bool {
    if !mime_types_contain(mime_types, KDE_CUT_SELECTION_MIME) {
        return false;
    }
    read_mime_text(KDE_CUT_SELECTION_MIME)
        .ok()
        .is_some_and(|payload| kde_cut_selection_payload_is_cut(&payload))
}

fn kde_cut_selection_payload_is_cut(payload: &str) -> bool {
    payload.as_bytes().first().is_some_and(|byte| *byte == b'1')
}

fn path_to_file_uri(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(*byte as char);
        } else {
            uri.push('%');
            uri.push(hex(byte >> 4));
            uri.push(hex(byte & 0x0f));
        }
    }
    uri
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
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

fn hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => unreachable!(),
    }
}

fn mime_types_contain(mime_types: &[String], mime: &str) -> bool {
    mime_types.iter().any(|candidate| candidate == mime)
}

fn read_mime_text(mime: &str) -> Result<String, String> {
    String::from_utf8(read_mime_bytes(mime)?)
        .map_err(|err| format!("Wayland clipboard {mime}: {err}"))
}

fn read_first_content_mime(
    kind: ClipboardContentKind,
    mimes: &[&str],
    available_mimes: &[String],
) -> Result<ClipboardContent, String> {
    let mut errors = Vec::new();
    for mime in mimes {
        if !mime_types_contain(available_mimes, mime) {
            continue;
        }
        match read_mime_bytes(mime) {
            Ok(data) if !data.is_empty() => {
                return Ok(ClipboardContent {
                    kind,
                    data,
                    mime_type: (*mime).to_string(),
                });
            }
            Ok(_) => errors.push(format!("{mime}: empty clipboard data")),
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Err(format!("clipboard does not advertise {}", mimes.join(", ")))
    } else {
        Err(errors.join("; "))
    }
}

fn read_mime_bytes(mime: &str) -> Result<Vec<u8>, String> {
    wayland_clipboard::read_mime(mime).map(|data| data.data)
}

fn image_extension(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" | "image/x-png" => Some("png"),
        "image/jpeg" | "image/pjpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/bmp" | "image/x-bmp" | "image/x-ms-bmp" => Some("bmp"),
        "image/webp" => Some("webp"),
        "image/tiff" | "image/x-tiff" => Some("tiff"),
        "image/svg+xml" => Some("svg"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        "image/avif" => Some("avif"),
        "image/heic" => Some("heic"),
        "image/heif" => Some("heif"),
        "image/jxl" => Some("jxl"),
        _ => None,
    }
}

fn video_extension(mime: &str) -> Option<&'static str> {
    match mime {
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/ogg" => Some("ogv"),
        "video/mpeg" => Some("mpeg"),
        "video/quicktime" => Some("mov"),
        "video/x-msvideo" | "video/avi" => Some("avi"),
        "video/x-matroska" => Some("mkv"),
        "video/x-flv" => Some("flv"),
        "video/3gpp" => Some("3gp"),
        "video/3gpp2" => Some("3g2"),
        "video/x-ms-wmv" => Some("wmv"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_list_payload_uses_desktop_file_clipboard_format() {
        assert_eq!(
            file_list_payload(
                &[
                    PathBuf::from("/tmp/Fika Test/a.txt"),
                    PathBuf::from("/tmp/数值.txt"),
                ],
                true,
            ),
            "cut\nfile:///tmp/Fika%20Test/a.txt\nfile:///tmp/%E6%95%B0%E5%80%BC.txt\n"
        );
    }

    #[test]
    fn parses_desktop_file_clipboard_payload() {
        let payload = "cut\n# comment\nfile:///tmp/Fika%20Test/a.txt\nfile://localhost/tmp/%E6%95%B0%E5%80%BC.txt\n";
        let clipboard = parse_file_list_payload(payload).unwrap();
        assert!(clipboard.cut);
        assert_eq!(
            clipboard.paths,
            vec![
                PathBuf::from("/tmp/Fika Test/a.txt"),
                PathBuf::from("/tmp/数值.txt"),
            ]
        );
    }

    #[test]
    fn parses_text_uri_list_as_copy() {
        let payload = "# comment\nfile:///tmp/one.txt\nfile:///tmp/two.txt\n";
        let clipboard = parse_uri_list_payload(payload, false).unwrap();
        assert!(!clipboard.cut);
        assert_eq!(
            clipboard.paths,
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );
    }

    #[test]
    fn rejects_non_local_file_uris() {
        assert_eq!(file_uri_to_path("file://remote-host/tmp/file.txt"), None);
        assert_eq!(file_uri_to_path("sftp://host/tmp/file.txt"), None);
        assert_eq!(
            file_uri_to_path("file:///tmp/local.txt"),
            Some(PathBuf::from("/tmp/local.txt"))
        );
        assert_eq!(
            file_uri_to_path("file://localhost/tmp/local.txt"),
            Some(PathBuf::from("/tmp/local.txt"))
        );
    }

    #[test]
    fn parses_kde_cutselection_marker_for_uri_list() {
        let payload = "file:///tmp/cut-me.txt\n";
        let clipboard = parse_uri_list_payload(payload, true).unwrap();
        assert!(clipboard.cut);
        assert_eq!(clipboard.paths, vec![PathBuf::from("/tmp/cut-me.txt")]);
    }

    #[test]
    fn kde_cutselection_uses_first_byte_like_dolphin() {
        assert!(kde_cut_selection_payload_is_cut("1"));
        assert!(kde_cut_selection_payload_is_cut("1\n"));
        assert!(!kde_cut_selection_payload_is_cut(""));
        assert!(!kde_cut_selection_payload_is_cut("0"));
        assert!(!kde_cut_selection_payload_is_cut(" true"));
    }

    #[test]
    fn uri_list_payload_uses_plain_file_uri_lines_for_copy_fallback() {
        assert_eq!(
            uri_list_payload(&[
                PathBuf::from("/tmp/Fika Test/a.txt"),
                PathBuf::from("/tmp/数值.txt"),
            ]),
            "file:///tmp/Fika%20Test/a.txt\nfile:///tmp/%E6%95%B0%E5%80%BC.txt\n"
        );
    }

    #[test]
    fn non_file_clipboard_kind_prefers_image_then_video_then_text() {
        let image_mimes = ["text/plain".to_string(), "image/png".to_string()];
        assert_eq!(
            content_kind_from_mime_types(image_mimes.iter()),
            Some(ClipboardContentKind::Image)
        );

        let video_mimes = ["text/plain".to_string(), "video/webm".to_string()];
        assert_eq!(
            content_kind_from_mime_types(video_mimes.iter()),
            Some(ClipboardContentKind::Video)
        );

        let text_mimes = ["UTF8_STRING".to_string()];
        assert_eq!(
            content_kind_from_mime_types(text_mimes.iter()),
            Some(ClipboardContentKind::Text)
        );
    }

    #[test]
    fn non_file_clipboard_content_maps_to_default_file_names() {
        let image = ClipboardContent {
            kind: ClipboardContentKind::Image,
            data: vec![1, 2, 3],
            mime_type: "image/jpeg".to_string(),
        };
        assert_eq!(image.base_file_name(), "Pasted Image");
        assert_eq!(image.extension(), Some("jpg"));

        let video = ClipboardContent {
            kind: ClipboardContentKind::Video,
            data: vec![1, 2, 3],
            mime_type: "video/x-matroska".to_string(),
        };
        assert_eq!(video.base_file_name(), "Pasted Video");
        assert_eq!(video.extension(), Some("mkv"));

        let text = ClipboardContent {
            kind: ClipboardContentKind::Text,
            data: b"hello".to_vec(),
            mime_type: TEXT_PLAIN_MIME.to_string(),
        };
        assert_eq!(text.base_file_name(), "Pasted Text");
        assert_eq!(text.extension(), Some("txt"));
    }
}
