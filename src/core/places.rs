use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::network::{network_uri_from_path, normalize_network_uri};

const FIKA_DATA_DIR_NAME: &str = "fika";
const USER_PLACES_FILE_NAME: &str = "places.xbel";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPlace {
    pub label: String,
    pub path: PathBuf,
}

impl UserPlace {
    pub fn new(label: String, path: PathBuf) -> Self {
        Self { label, path }
    }
}

pub fn default_user_places_path() -> PathBuf {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    user_places_path_for_data_home(data_home)
}

fn user_places_path_for_data_home(data_home: PathBuf) -> PathBuf {
    data_home
        .join(FIKA_DATA_DIR_NAME)
        .join(USER_PLACES_FILE_NAME)
}

pub fn load_user_places(path: &Path) -> Result<Vec<UserPlace>, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to read user places {}: {error}",
                path.display()
            ));
        }
    };
    parse_user_places_xbel(&contents)
}

pub fn save_user_places(path: &Path, places: &[UserPlace]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create user places directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(path, user_places_xbel(places))
        .map_err(|error| format!("failed to write user places {}: {error}", path.display()))
}

pub fn parse_user_places_xbel(contents: &str) -> Result<Vec<UserPlace>, String> {
    let mut places = Vec::new();
    let mut rest = contents;

    while let Some(bookmark_start) = rest.find("<bookmark") {
        rest = &rest[bookmark_start..];
        let Some(tag_end) = rest.find('>') else {
            return Err("user places bookmark tag is not closed".to_string());
        };
        let tag = &rest[..=tag_end];
        let body_and_tail = &rest[tag_end + 1..];
        let Some(bookmark_end) = body_and_tail.find("</bookmark>") else {
            return Err("user places bookmark is not closed".to_string());
        };
        let body = &body_and_tail[..bookmark_end];
        rest = &body_and_tail[bookmark_end + "</bookmark>".len()..];

        let Some(href) = xml_attribute(tag, "href") else {
            continue;
        };
        let Some(path) = place_href_to_path(&href) else {
            continue;
        };
        let Some(title) = xml_element_text(body, "title") else {
            continue;
        };
        let label = decode_xml_text(&title)?;
        if label.trim().is_empty() {
            continue;
        }
        places.push(UserPlace::new(label, path));
    }

    Ok(places)
}

pub fn user_places_xbel(places: &[UserPlace]) -> String {
    let mut output = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE xbel>\n\
         <xbel version=\"1.0\">\n",
    );
    for place in places {
        output.push_str("  <bookmark href=\"");
        output.push_str(&escape_xml_attr(&path_to_place_href(&place.path)));
        output.push_str("\">\n");
        output.push_str("    <title>");
        output.push_str(&escape_xml_text(&place.label));
        output.push_str("</title>\n");
        output.push_str("  </bookmark>\n");
    }
    output.push_str("</xbel>\n");
    output
}

fn xml_attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let index = rest.find(name)?;
        let after_name = &rest[index + name.len()..];
        let after_equals = after_name.trim_start();
        if !after_equals.starts_with('=') {
            rest = &after_name[after_name.len().min(1)..];
            continue;
        }
        let value = after_equals[1..].trim_start();
        let mut chars = value.chars();
        let quote = chars.next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value = &value[quote.len_utf8()..];
        let end = value.find(quote)?;
        return decode_xml_text(&value[..end]).ok();
    }
}

fn xml_element_text(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    Some(body[start..end].to_string())
}

fn path_to_file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut uri = String::with_capacity(raw.len() + "file://".len());
    uri.push_str("file://");
    for byte in raw.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(*byte as char);
        } else {
            uri.push('%');
            uri.push(hex_digit(byte >> 4));
            uri.push(hex_digit(byte & 0x0f));
        }
    }
    uri
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    percent_decode(path).map(PathBuf::from)
}

fn place_href_to_path(uri: &str) -> Option<PathBuf> {
    file_uri_to_path(uri).or_else(|| normalize_network_uri(uri).ok().map(PathBuf::from))
}

fn path_to_place_href(path: &Path) -> String {
    network_uri_from_path(path).unwrap_or_else(|| path_to_file_uri(path))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn escape_xml_attr(text: &str) -> String {
    escape_xml_text(text).replace('"', "&quot;")
}

fn escape_xml_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn decode_xml_text(text: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        decoded.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        let Some(end) = rest.find(';') else {
            return Err("user places XML entity is not closed".to_string());
        };
        let entity = &rest[..end];
        match entity {
            "amp" => decoded.push('&'),
            "lt" => decoded.push('<'),
            "gt" => decoded.push('>'),
            "quot" => decoded.push('"'),
            "apos" => decoded.push('\''),
            _ => return Err(format!("unsupported user places XML entity: &{entity};")),
        }
        rest = &rest[end + 1..];
    }
    decoded.push_str(rest);
    Ok(decoded)
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
    fn user_places_xbel_round_trips_file_bookmarks() {
        let places = vec![
            UserPlace::new("Projects & Work".to_string(), PathBuf::from("/tmp/a b")),
            UserPlace::new("文档".to_string(), PathBuf::from("/tmp/unicode-文档")),
        ];

        let xbel = user_places_xbel(&places);

        assert!(xbel.contains("file:///tmp/a%20b"));
        assert!(xbel.contains("Projects &amp; Work"));
        assert_eq!(parse_user_places_xbel(&xbel), Ok(places));
    }

    #[test]
    fn user_places_xbel_round_trips_network_bookmarks() {
        let places = vec![UserPlace::new(
            "Team Share".to_string(),
            PathBuf::from("smb://server/Share%20Name/"),
        )];

        let xbel = user_places_xbel(&places);

        assert!(xbel.contains("href=\"smb://server/Share%20Name/\""));
        assert!(!xbel.contains("file://smb"));
        assert_eq!(parse_user_places_xbel(&xbel), Ok(places));
    }

    #[test]
    fn parse_user_places_xbel_ignores_non_file_and_missing_titles() {
        let xbel = r#"
            <xbel version="1.0">
              <bookmark href="trash:/"><title>Trash</title></bookmark>
              <bookmark href="file:///tmp/ok"><title>OK</title></bookmark>
              <bookmark href="file:///tmp/missing-title"></bookmark>
            </xbel>
        "#;

        assert_eq!(
            parse_user_places_xbel(xbel),
            Ok(vec![UserPlace::new(
                "OK".to_string(),
                PathBuf::from("/tmp/ok")
            )])
        );
    }

    #[test]
    fn save_user_places_creates_parent_and_loads_again() {
        let root = test_dir("places-xbel");
        let path = root.join("nested/places.xbel");
        let places = vec![UserPlace::new(
            "Bookmark".to_string(),
            PathBuf::from("/tmp/bookmark"),
        )];

        save_user_places(&path, &places).unwrap();

        assert_eq!(load_user_places(&path), Ok(places));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn default_user_places_path_is_fika_scoped() {
        assert_eq!(
            user_places_path_for_data_home(PathBuf::from("/xdg/data")),
            PathBuf::from("/xdg/data/fika/places.xbel")
        );
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-core-{name}-{}-{nanos}", std::process::id()))
    }
}
