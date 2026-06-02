use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const FILE_CLIPBOARD_MIME: &str = "x-special/gnome-copied-files";
const URI_LIST_MIME: &str = "text/uri-list";
const KDE_CUT_SELECTION_MIME: &str = "application/x-kde-cutselection";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileClipboard {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) cut: bool,
    pub(crate) helper: String,
}

pub(crate) fn copy_text(text: &str) -> Result<String, String> {
    match copy_with("wl-copy", &[], text) {
        Ok(()) => Ok("wl-copy".to_string()),
        Err(CopyError::Missing) => {
            Err("no Wayland clipboard helper found; install wl-copy".to_string())
        }
        Err(CopyError::Failed(err)) => Err(format!("wl-copy: {err}")),
    }
}

pub(crate) fn copy_file_list(paths: &[PathBuf], cut: bool) -> Result<String, String> {
    if paths.is_empty() {
        return Err("no files to copy".to_string());
    }
    match copy_mime(FILE_CLIPBOARD_MIME, &file_list_payload(paths, cut)) {
        Ok(helper) => Ok(helper),
        Err(special_error) if !cut => copy_mime(URI_LIST_MIME, &uri_list_payload(paths))
            .map(|helper| format!("{helper} ({URI_LIST_MIME} fallback)"))
            .map_err(|uri_error| format!("{special_error}; {uri_error}")),
        Err(err) => Err(err),
    }
}

pub(crate) fn read_file_list() -> Result<FileClipboard, String> {
    match read_mime(FILE_CLIPBOARD_MIME).and_then(|(helper, payload)| {
        parse_file_list_payload(&payload, Some(helper))
            .ok_or_else(|| "clipboard does not contain file paths".to_string())
    }) {
        Ok(clipboard) => Ok(clipboard),
        Err(special_error) => read_mime(URI_LIST_MIME)
            .and_then(|(helper, payload)| {
                let cut = read_kde_cut_selection();
                parse_uri_list_payload(&payload, helper, cut)
                    .ok_or_else(|| "clipboard uri-list does not contain file paths".to_string())
            })
            .map_err(|uri_error| format!("{special_error}; {uri_error}")),
    }
}

fn copy_mime(mime: &str, text: &str) -> Result<String, String> {
    match copy_with("wl-copy", &["--type", mime], text) {
        Ok(()) => Ok("wl-copy".to_string()),
        Err(CopyError::Missing) => Err(format!(
            "no Wayland clipboard helper found for {mime}; install wl-copy"
        )),
        Err(CopyError::Failed(err)) => Err(format!("wl-copy: {err}")),
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

fn parse_file_list_payload(payload: &str, helper: Option<String>) -> Option<FileClipboard> {
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
        Some(FileClipboard {
            paths,
            cut,
            helper: helper.unwrap_or_else(|| "test".to_string()),
        })
    }
}

fn parse_uri_list_payload(payload: &str, helper: String, cut: bool) -> Option<FileClipboard> {
    let paths = payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(file_uri_to_path)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        Some(FileClipboard { paths, cut, helper })
    }
}

fn read_kde_cut_selection() -> bool {
    read_mime(KDE_CUT_SELECTION_MIME)
        .ok()
        .is_some_and(|(_, payload)| kde_cut_selection_payload_is_cut(&payload))
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

fn read_mime(mime: &str) -> Result<(String, String), String> {
    match read_with("wl-paste", &["--no-newline", "--type", mime]) {
        Ok(payload) => Ok(("wl-paste".to_string(), payload)),
        Err(CopyError::Missing) => Err(format!(
            "no Wayland clipboard helper found for {mime}; install wl-paste"
        )),
        Err(CopyError::Failed(err)) => Err(format!("wl-paste: {err}")),
    }
}

fn copy_with(program: &str, args: &[&str], text: &str) -> Result<(), CopyError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                CopyError::Missing
            } else {
                CopyError::Failed(err.to_string())
            }
        })?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(CopyError::Failed(
            "clipboard helper has no stdin".to_string(),
        ));
    };
    stdin
        .write_all(text.as_bytes())
        .map_err(|err| CopyError::Failed(err.to_string()))?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|err| CopyError::Failed(err.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CopyError::Failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn read_with(program: &str, args: &[&str]) -> Result<String, CopyError> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                CopyError::Missing
            } else {
                CopyError::Failed(err.to_string())
            }
        })?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|err| CopyError::Failed(err.to_string()))
    } else {
        Err(CopyError::Failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

enum CopyError {
    Missing,
    Failed(String),
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
        let clipboard = parse_file_list_payload(payload, Some("test-helper".to_string())).unwrap();
        assert!(clipboard.cut);
        assert_eq!(clipboard.helper, "test-helper");
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
        let clipboard = parse_uri_list_payload(payload, "test-helper".to_string(), false).unwrap();
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
        let clipboard = parse_uri_list_payload(payload, "test-helper".to_string(), true).unwrap();
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
}
