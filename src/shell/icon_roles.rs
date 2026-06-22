use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FileIconKind {
    Directory,
    Mime {
        mime: Arc<str>,
    },
    PreliminaryFile {
        extension: Option<String>,
    },
    File {
        extension: Option<String>,
    },
    Named {
        icon_name: String,
        fallback: NamedIconFallback,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum NamedIconFallback {
    Service,
    Application,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FileIconRoleCacheKey {
    pub(crate) kind: FileIconKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FileIconPathCacheKey {
    pub(crate) role: FileIconRoleCacheKey,
    pub(crate) size_px: u16,
}

pub(crate) struct FileIconProfile {
    pub(crate) icon_candidates: Vec<String>,
    pub(crate) generic_candidates: Vec<String>,
}

pub(crate) fn file_icon_path_cache_key(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size: f32,
) -> FileIconPathCacheKey {
    FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: file_icon_kind(path, is_dir, mime_type, mime_magic_checked),
        },
        size_px: icon_cache_size(icon_size),
    }
}

pub(crate) fn file_icon_kind(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
) -> FileIconKind {
    if is_dir {
        return FileIconKind::Directory;
    }
    let extension = file_extension(path);
    if !mime_magic_checked && mime_type.as_deref() == Some(fika_core::GENERIC_BINARY_MIME) {
        return FileIconKind::PreliminaryFile { extension };
    }
    match mime_type {
        Some(mime) if mime.as_ref() == fika_core::GENERIC_BINARY_MIME => {
            FileIconKind::File { extension }
        }
        Some(mime) => FileIconKind::Mime { mime },
        None => FileIconKind::File { extension },
    }
}

pub(crate) fn icon_cache_size(icon_size: f32) -> u16 {
    icon_size.round().clamp(16.0, 256.0) as u16
}

pub(crate) fn file_icon_profile(
    kind: &FileIconKind,
    mime: &fika_core::MimeDatabase,
) -> FileIconProfile {
    let (icon_candidates, generic_candidates) = match kind {
        FileIconKind::Directory => (
            vec!["folder".to_string(), "inode-directory".to_string()],
            Vec::new(),
        ),
        FileIconKind::Mime { mime: mime_name } => (
            mime_icon_candidates(mime_name, mime),
            mime_generic_icon_candidates(mime_name, mime),
        ),
        FileIconKind::PreliminaryFile { extension } => (
            preliminary_file_icon_candidates(extension.as_deref(), mime),
            Vec::new(),
        ),
        FileIconKind::File { extension } => (
            fallback_file_icon_candidates(extension.as_deref()),
            mime_generic_icon_candidates(fika_core::GENERIC_BINARY_MIME, mime),
        ),
        FileIconKind::Named {
            icon_name,
            fallback,
        } => named_icon_candidates(icon_name, *fallback),
    };

    FileIconProfile {
        icon_candidates,
        generic_candidates,
    }
}

fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn mime_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();

    if mime_name == fika_core::GENERIC_BINARY_MIME {
        for icon_name in fallback_file_icon_candidates(None) {
            push_icon_candidate(&mut candidates, icon_name);
        }
        return candidates;
    }

    for icon_name in mime_theme_icon_candidates(mime_name, None) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some(icon_name) = mime.icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_generic_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = mime.generic_icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_theme_icon_candidates(mime_name: &str, extension: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = fika_core::mime_icon_name(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some((family, subtype)) = mime_name.split_once('/')
        && family == "text"
    {
        let subtype = subtype.strip_prefix("x-").unwrap_or(subtype);
        if !subtype.is_empty() {
            push_icon_candidate(&mut candidates, format!("text-x-{subtype}"));
        }
        if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
            push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        }
    }
    candidates
}

fn fallback_file_icon_candidates(extension: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
        push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        push_icon_candidate(&mut candidates, format!("application-x-{extension}"));
    }
    push_icon_candidate(&mut candidates, "application-octet-stream");
    candidates
}

fn preliminary_file_icon_candidates(
    extension: Option<&str>,
    mime: &fika_core::MimeDatabase,
) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
        if let Some(mime_name) = mime.mime_for_extension(extension) {
            for icon_name in mime_theme_icon_candidates(mime_name, Some(extension)) {
                push_icon_candidate(&mut candidates, icon_name);
            }
        }
        push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        push_icon_candidate(&mut candidates, format!("application-x-{extension}"));
    }
    push_icon_candidate(&mut candidates, "text-x-generic");
    push_icon_candidate(&mut candidates, "unknown");
    candidates
}

fn push_icon_candidate(candidates: &mut Vec<String>, icon_name: impl Into<String>) {
    let icon_name = icon_name.into();
    if !candidates.iter().any(|existing| existing == &icon_name) {
        candidates.push(icon_name);
    }
}

fn named_icon_candidates(
    icon_name: &str,
    fallback: NamedIconFallback,
) -> (Vec<String>, Vec<String>) {
    let mut candidates = Vec::new();
    push_icon_candidate(&mut candidates, icon_name.trim());
    let generic = match fallback {
        NamedIconFallback::Service => ["configure", "preferences-system", "system-run"].as_slice(),
        NamedIconFallback::Application => [
            "application-x-executable",
            "system-run",
            "application-default-icon",
        ]
        .as_slice(),
    }
    .iter()
    .map(|candidate| (*candidate).to_string())
    .collect();
    (candidates, generic)
}
