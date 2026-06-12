use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

pub const NETWORK_ROOT_URI: &str = "network:///";
pub const DOLPHIN_REMOTE_ROOT_URI: &str = "remote:/";
pub const NETWORK_ROOT_LABEL: &str = "Network";
pub const NETWORK_ROOT_ICON: &str = "folder-remote";

const SUPPORTED_NETWORK_SCHEMES: &[&str] = &[
    "network", "remote", "smb", "sftp", "fish", "ftp", "ftps", "nfs", "dav", "davs", "webdav",
    "webdavs",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkLocation {
    pub uri: String,
    pub display_name: String,
    pub local_path: Option<PathBuf>,
    pub scheme: String,
    pub icon_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkUrlError {
    Empty,
    MissingScheme(String),
    InvalidScheme(String),
    UnsupportedScheme(String),
    RootOnlyScheme(String),
    MissingAuthority(String),
    InvalidControlCharacter,
}

impl fmt::Display for NetworkUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "network URL is empty"),
            Self::MissingScheme(uri) => write!(f, "network URL has no scheme: {uri}"),
            Self::InvalidScheme(scheme) => write!(f, "invalid network URL scheme: {scheme}"),
            Self::UnsupportedScheme(scheme) => {
                write!(f, "unsupported network URL scheme: {scheme}")
            }
            Self::RootOnlyScheme(scheme) => {
                write!(
                    f,
                    "network URL scheme is only supported as a root: {scheme}"
                )
            }
            Self::MissingAuthority(scheme) => {
                write!(
                    f,
                    "network URL scheme requires a host/share authority: {scheme}"
                )
            }
            Self::InvalidControlCharacter => write!(f, "network URL contains a control character"),
        }
    }
}

impl Error for NetworkUrlError {}

#[derive(Clone, Eq, PartialEq)]
pub struct NetworkAuth {
    pub username: Option<String>,
    pub domain: Option<String>,
    pub password: Option<String>,
    pub anonymous: bool,
    pub remember: bool,
}

impl fmt::Debug for NetworkAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkAuth")
            .field("username", &self.username)
            .field("domain", &self.domain)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("anonymous", &self.anonymous)
            .field("remember", &self.remember)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkFilesystemKind {
    Local,
    Remote,
    Gvfs,
}

pub fn supported_network_schemes() -> &'static [&'static str] {
    SUPPORTED_NETWORK_SCHEMES
}

pub fn is_supported_network_scheme(scheme: &str) -> bool {
    let scheme = scheme.to_ascii_lowercase();
    SUPPORTED_NETWORK_SCHEMES.contains(&scheme.as_str())
}

pub fn network_root_location() -> NetworkLocation {
    NetworkLocation {
        uri: NETWORK_ROOT_URI.to_string(),
        display_name: NETWORK_ROOT_LABEL.to_string(),
        local_path: None,
        scheme: "network".to_string(),
        icon_name: NETWORK_ROOT_ICON.to_string(),
    }
}

pub fn network_root_path() -> PathBuf {
    PathBuf::from(NETWORK_ROOT_URI)
}

pub fn is_network_root_uri(uri: &str) -> bool {
    normalize_network_uri(uri).is_ok_and(|normalized| normalized == NETWORK_ROOT_URI)
}

pub fn is_network_root_path(path: &Path) -> bool {
    path.to_str().is_some_and(is_network_root_uri)
}

pub fn parse_network_location(uri: &str) -> Result<NetworkLocation, NetworkUrlError> {
    let normalized = normalize_network_uri(uri)?;
    if normalized == NETWORK_ROOT_URI {
        return Ok(network_root_location());
    }
    let (scheme, rest) = split_scheme(&normalized)?;
    let after_slashes = rest
        .strip_prefix("//")
        .ok_or_else(|| NetworkUrlError::MissingAuthority(scheme.to_string()))?;
    let display_name = network_share_display_name(after_slashes);
    let scheme = scheme.to_string();
    Ok(NetworkLocation {
        uri: normalized,
        display_name,
        local_path: None,
        scheme,
        icon_name: "folder-remote".to_string(),
    })
}

pub fn normalize_network_uri(uri: &str) -> Result<String, NetworkUrlError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(NetworkUrlError::Empty);
    }
    if trimmed.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(NetworkUrlError::InvalidControlCharacter);
    }

    let (raw_scheme, rest) = split_scheme(trimmed)?;
    if !valid_scheme(raw_scheme) {
        return Err(NetworkUrlError::InvalidScheme(raw_scheme.to_string()));
    }
    let scheme = raw_scheme.to_ascii_lowercase();
    if !is_supported_network_scheme(&scheme) {
        return Err(NetworkUrlError::UnsupportedScheme(scheme));
    }

    match scheme.as_str() {
        "network" | "remote" => normalize_network_root(&scheme, rest),
        _ => normalize_network_share(&scheme, rest),
    }
}

pub fn classify_network_filesystem(filesystem_type: &str) -> NetworkFilesystemKind {
    let fs = filesystem_type.to_ascii_lowercase();
    if fs == "fuse.gvfsd-fuse" || fs == "gvfsd-fuse" {
        return NetworkFilesystemKind::Gvfs;
    }
    if filesystem_type_is_remote(&fs) {
        NetworkFilesystemKind::Remote
    } else {
        NetworkFilesystemKind::Local
    }
}

pub fn filesystem_type_is_remote(filesystem_type: &str) -> bool {
    let fs = filesystem_type.to_ascii_lowercase();
    matches!(
        fs.as_str(),
        "cifs"
            | "smb3"
            | "smbfs"
            | "nfs"
            | "nfs4"
            | "sshfs"
            | "davfs"
            | "davfs2"
            | "ceph"
            | "glusterfs"
            | "lustre"
            | "rclone"
            | "s3fs"
            | "goofys"
            | "gcsfuse"
            | "fuse.gvfsd-fuse"
            | "gvfsd-fuse"
    ) || fs.starts_with("fuse.sshfs")
        || fs.starts_with("fuse.rclone")
        || fs.starts_with("fuse.s3fs")
        || fs.starts_with("fuse.gcsfuse")
        || fs.starts_with("fuse.davfs")
}

fn split_scheme(uri: &str) -> Result<(&str, &str), NetworkUrlError> {
    uri.split_once(':')
        .ok_or_else(|| NetworkUrlError::MissingScheme(uri.to_string()))
}

fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn normalize_network_root(scheme: &str, rest: &str) -> Result<String, NetworkUrlError> {
    if rest.is_empty() || rest.chars().all(|ch| ch == '/') {
        Ok(NETWORK_ROOT_URI.to_string())
    } else {
        Err(NetworkUrlError::RootOnlyScheme(scheme.to_string()))
    }
}

fn normalize_network_share(scheme: &str, rest: &str) -> Result<String, NetworkUrlError> {
    let Some(after_slashes) = rest.strip_prefix("//") else {
        return Err(NetworkUrlError::MissingAuthority(scheme.to_string()));
    };
    let authority = after_slashes
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return Err(NetworkUrlError::MissingAuthority(scheme.to_string()));
    }
    if after_slashes.contains('/') {
        Ok(format!("{scheme}://{after_slashes}"))
    } else {
        Ok(format!("{scheme}://{after_slashes}/"))
    }
}

fn network_share_display_name(after_slashes: &str) -> String {
    let without_query = after_slashes
        .split(['?', '#'])
        .next()
        .unwrap_or(after_slashes);
    let (authority, path) = without_query
        .split_once('/')
        .map_or((without_query, ""), |(authority, path)| (authority, path));
    let host = display_host(authority);
    let last_segment = path.rsplit('/').find(|segment| !segment.is_empty());
    match last_segment {
        Some(segment) => format!("{} on {host}", percent_decode_lossy(segment)),
        None => host.to_string(),
    }
}

fn display_host(authority: &str) -> &str {
    let host = authority.rsplit('@').next().unwrap_or(authority);
    host.strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(ipv6, _)| ipv6))
        .unwrap_or_else(|| host.split(':').next().unwrap_or(host))
}

fn percent_decode_lossy(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
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
    fn network_root_normalizes_dolphin_and_cosmic_roots() {
        assert_eq!(
            normalize_network_uri("remote:/"),
            Ok(NETWORK_ROOT_URI.to_string())
        );
        assert_eq!(
            normalize_network_uri("network:///"),
            Ok(NETWORK_ROOT_URI.to_string())
        );
        assert_eq!(
            normalize_network_uri("NETWORK:/"),
            Ok(NETWORK_ROOT_URI.to_string())
        );
        assert!(is_network_root_path(&PathBuf::from(
            DOLPHIN_REMOTE_ROOT_URI
        )));
        assert_eq!(network_root_path(), PathBuf::from(NETWORK_ROOT_URI));
    }

    #[test]
    fn network_share_uri_parses_supported_schemes() {
        assert_eq!(
            normalize_network_uri("SMB://server/Share%20Name"),
            Ok("smb://server/Share%20Name".to_string())
        );
        assert_eq!(
            parse_network_location("smb://server/Share%20Name").unwrap(),
            NetworkLocation {
                uri: "smb://server/Share%20Name".to_string(),
                display_name: "Share Name on server".to_string(),
                local_path: None,
                scheme: "smb".to_string(),
                icon_name: "folder-remote".to_string(),
            }
        );
        assert_eq!(
            parse_network_location("sftp://user@example.test/home/yk")
                .unwrap()
                .display_name,
            "yk on example.test"
        );
    }

    #[test]
    fn network_uri_rejects_unsupported_or_incomplete_values() {
        assert_eq!(normalize_network_uri(""), Err(NetworkUrlError::Empty));
        assert_eq!(
            normalize_network_uri("/tmp/share"),
            Err(NetworkUrlError::MissingScheme("/tmp/share".to_string()))
        );
        assert_eq!(
            normalize_network_uri("http://example.test/"),
            Err(NetworkUrlError::UnsupportedScheme("http".to_string()))
        );
        assert_eq!(
            normalize_network_uri("smb:/server/share"),
            Err(NetworkUrlError::MissingAuthority("smb".to_string()))
        );
        assert_eq!(
            normalize_network_uri("network:///server"),
            Err(NetworkUrlError::RootOnlyScheme("network".to_string()))
        );
    }

    #[test]
    fn network_auth_debug_redacts_password() {
        let auth = NetworkAuth {
            username: Some("yk".to_string()),
            domain: Some("WORKGROUP".to_string()),
            password: Some("secret".to_string()),
            anonymous: false,
            remember: true,
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("yk"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn filesystem_type_classifies_remote_and_gvfs_mounts() {
        assert_eq!(
            classify_network_filesystem("ext4"),
            NetworkFilesystemKind::Local
        );
        assert_eq!(
            classify_network_filesystem("cifs"),
            NetworkFilesystemKind::Remote
        );
        assert_eq!(
            classify_network_filesystem("fuse.sshfs"),
            NetworkFilesystemKind::Remote
        );
        assert_eq!(
            classify_network_filesystem("fuse.gvfsd-fuse"),
            NetworkFilesystemKind::Gvfs
        );
    }
}
