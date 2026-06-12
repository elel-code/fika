use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileIconSnapshot {
    pub(crate) icon_name: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) fallback_marker: String,
    pub(crate) fallback_fg: u32,
    pub(crate) fallback_bg: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FileIconKind {
    Directory,
    Mime {
        mime: Arc<str>,
        extension: Option<String>,
    },
    File {
        extension: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FileIconCacheKey {
    kind: FileIconKind,
    size_px: u16,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NamedIconCacheKey {
    name: String,
    size_px: u16,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FileIconCache {
    cached: HashMap<FileIconCacheKey, FileIconSnapshot>,
    named_cached: HashMap<NamedIconCacheKey, FileIconSnapshot>,
    theme: IconThemeResolver,
    mime: fika_core::MimeDatabase,
}

impl FileIconCache {
    pub(crate) fn icon_for(
        &mut self,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
        icon_size: f32,
    ) -> FileIconSnapshot {
        let kind = file_icon_kind(path, is_dir, mime_type);
        let key = FileIconCacheKey {
            kind,
            size_px: icon_cache_size(icon_size),
        };
        if let Some(icon) = self.cached.get(&key) {
            return icon.clone();
        }

        let icon = file_icon_snapshot(&key.kind, key.size_px, &mut self.theme, &self.mime);
        self.cached.insert(key, icon.clone());
        icon
    }

    pub(crate) fn named_icon(
        &mut self,
        name: &str,
        candidates: &[&str],
        fallback_marker: &str,
        fallback_fg: u32,
        fallback_bg: u32,
        icon_size: f32,
    ) -> FileIconSnapshot {
        let key = NamedIconCacheKey {
            name: name.to_string(),
            size_px: icon_cache_size(icon_size),
        };
        if let Some(icon) = self.named_cached.get(&key) {
            return icon.clone();
        }

        let candidates = candidates
            .iter()
            .map(|candidate| (*candidate).to_string())
            .collect::<Vec<_>>();
        let (icon_name, path) = self
            .theme
            .first_existing(&candidates, key.size_px)
            .map_or_else(
                || {
                    (
                        candidates
                            .first()
                            .cloned()
                            .unwrap_or_else(|| name.to_string()),
                        None,
                    )
                },
                |(name, path)| (name, Some(path)),
            );
        let icon = FileIconSnapshot {
            icon_name,
            path,
            fallback_marker: fallback_marker.to_string(),
            fallback_fg,
            fallback_bg,
        };
        self.named_cached.insert(key, icon.clone());
        icon
    }
}

#[derive(Clone, Debug)]
struct IconThemeResolver {
    roots: Vec<PathBuf>,
    themes: Vec<String>,
    path_cache: HashMap<(String, u16), Option<PathBuf>>,
}

impl Default for IconThemeResolver {
    fn default() -> Self {
        Self {
            roots: icon_theme_roots(),
            themes: icon_theme_names(),
            path_cache: HashMap::new(),
        }
    }
}

impl IconThemeResolver {
    fn find(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        let key = (icon_name.to_string(), desired_size);
        if let Some(path) = self.path_cache.get(&key) {
            return path.clone();
        }

        let path = self.find_uncached(icon_name, desired_size);
        self.path_cache.insert(key, path.clone());
        path
    }

    fn first_existing(
        &mut self,
        icon_names: &[String],
        desired_size: u16,
    ) -> Option<(String, PathBuf)> {
        icon_names.iter().find_map(|name| {
            self.find(name, desired_size)
                .map(|path| (name.clone(), path))
        })
    }

    fn find_uncached(&self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        for theme in self.theme_search_order() {
            for root in &self.roots {
                let theme_root = root.join(&theme);
                if let Some(path) = find_icon_in_theme(&theme_root, icon_name, desired_size) {
                    return Some(path);
                }
            }
        }

        [
            Path::new("/usr/share/pixmaps"),
            Path::new("/usr/local/share/pixmaps"),
        ]
        .into_iter()
        .find_map(|root| find_icon_direct(root, icon_name))
    }

    fn theme_search_order(&self) -> Vec<String> {
        let mut themes = Vec::new();
        for theme in &self.themes {
            self.push_theme_and_inherits(theme, &mut themes, 0);
        }
        themes
    }

    fn push_theme_and_inherits(&self, theme: &str, themes: &mut Vec<String>, depth: usize) {
        if depth > 8 || theme.is_empty() {
            return;
        }
        let already_seen = themes.iter().any(|existing| existing == theme);
        push_unique_icon_theme(themes, theme);
        if already_seen {
            return;
        }
        for inherited in self.inherited_themes(theme) {
            self.push_theme_and_inherits(&inherited, themes, depth + 1);
        }
    }

    fn inherited_themes(&self, theme: &str) -> Vec<String> {
        let mut inherited = Vec::new();
        for root in &self.roots {
            let Ok(contents) = fs::read_to_string(root.join(theme).join("index.theme")) else {
                continue;
            };
            for theme in parse_icon_theme_inherits(&contents) {
                push_unique_icon_theme(&mut inherited, &theme);
            }
        }
        inherited
    }
}

fn file_icon_kind(path: &Path, is_dir: bool, mime_type: Option<Arc<str>>) -> FileIconKind {
    if is_dir {
        return FileIconKind::Directory;
    }
    let extension = file_extension(path);
    match mime_type {
        Some(mime) => FileIconKind::Mime { mime, extension },
        None => FileIconKind::File { extension },
    }
}

fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn icon_cache_size(icon_size: f32) -> u16 {
    icon_size.round().clamp(16.0, 256.0) as u16
}

fn file_icon_snapshot(
    kind: &FileIconKind,
    desired_size: u16,
    theme: &mut IconThemeResolver,
    mime: &fika_core::MimeDatabase,
) -> FileIconSnapshot {
    let profile = file_icon_profile(kind, mime);
    let (icon_name, path) = theme
        .first_existing(&profile.icon_candidates, desired_size)
        .or_else(|| theme.first_existing(&profile.generic_candidates, desired_size))
        .or_else(|| {
            theme.first_existing(
                &[
                    "unknown".to_string(),
                    "application-octet-stream".to_string(),
                ],
                desired_size,
            )
        })
        .map_or_else(
            || {
                (
                    profile
                        .icon_candidates
                        .first()
                        .or_else(|| profile.generic_candidates.first())
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                    None,
                )
            },
            |(name, path)| (name, Some(path)),
        );

    FileIconSnapshot {
        icon_name,
        path,
        fallback_marker: profile.marker,
        fallback_fg: profile.fg,
        fallback_bg: profile.bg,
    }
}

struct FileIconProfile {
    icon_candidates: Vec<String>,
    generic_candidates: Vec<String>,
    marker: String,
    fg: u32,
    bg: u32,
}

fn file_icon_profile(kind: &FileIconKind, mime: &fika_core::MimeDatabase) -> FileIconProfile {
    let (icon_candidates, generic_candidates, marker, fg, bg) = match kind {
        FileIconKind::Directory => (
            vec!["folder".to_string(), "inode-directory".to_string()],
            Vec::new(),
            "DIR".to_string(),
            0x0f4c81,
            0xe7f1fb,
        ),
        FileIconKind::Mime {
            mime: mime_name,
            extension,
        } => {
            let marker = file_marker(mime_name, extension.as_deref());
            let (fg, bg) = file_fallback_colors(mime_name, extension.as_deref());
            (
                mime_icon_candidates(mime_name, extension.as_deref(), mime),
                mime_generic_icon_candidates(mime_name, mime),
                marker,
                fg,
                bg,
            )
        }
        FileIconKind::File { extension } => {
            let marker = file_marker("application/octet-stream", extension.as_deref());
            let (fg, bg) = file_fallback_colors("application/octet-stream", extension.as_deref());
            (
                fallback_file_icon_candidates(extension.as_deref()),
                mime_generic_icon_candidates("application/octet-stream", mime),
                marker,
                fg,
                bg,
            )
        }
    };

    FileIconProfile {
        icon_candidates,
        generic_candidates,
        marker,
        fg,
        bg,
    }
}

fn mime_icon_candidates(
    mime_name: &str,
    extension: Option<&str>,
    mime: &fika_core::MimeDatabase,
) -> Vec<String> {
    let mut candidates = Vec::new();

    for icon_name in mime_theme_icon_candidates(mime_name, extension) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some(icon_name) = mime.icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
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

fn push_icon_candidate(candidates: &mut Vec<String>, icon_name: impl Into<String>) {
    let icon_name = icon_name.into();
    if !candidates.iter().any(|existing| existing == &icon_name) {
        candidates.push(icon_name);
    }
}

fn file_marker(mime: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) if extension.len() <= 4 => extension.to_ascii_uppercase(),
        _ if mime.starts_with("image/") => "IMG".to_string(),
        _ if mime.starts_with("audio/") => "AUD".to_string(),
        _ if mime.starts_with("video/") => "VID".to_string(),
        _ if mime.starts_with("text/") => "TXT".to_string(),
        _ => "FILE".to_string(),
    }
}

fn file_fallback_colors(mime: &str, extension: Option<&str>) -> (u32, u32) {
    if mime.starts_with("image/") || extension.is_some_and(is_image_extension) {
        (0x7c2d12, 0xffedd5)
    } else if mime.starts_with("audio/") || extension.is_some_and(is_audio_extension) {
        (0x6d28d9, 0xf3e8ff)
    } else if mime.starts_with("video/") || extension.is_some_and(is_video_extension) {
        (0x9f1239, 0xffe4e6)
    } else if extension.is_some_and(is_archive_extension) {
        (0x713f12, 0xfef3c7)
    } else if mime == "application/pdf" || extension == Some("pdf") {
        (0x991b1b, 0xfee2e2)
    } else {
        (0x374151, 0xf3f4f6)
    }
}

fn icon_theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(home).join(".local/share/icons"));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(data_home).join("icons"));
    }

    let data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        push_unique_icon_path(&mut roots, Path::new(dir).join("icons"));
    }
    push_unique_icon_path(&mut roots, PathBuf::from("/usr/share/icons"));
    roots
}

fn icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for theme in configured_icon_theme_names() {
        push_unique_icon_theme(&mut themes, &theme);
    }
    if env::var_os("KDE_FULL_SESSION").is_some()
        || env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| desktop.to_ascii_lowercase().contains("kde"))
            .unwrap_or(false)
    {
        push_unique_icon_theme(&mut themes, "breeze");
        push_unique_icon_theme(&mut themes, "breeze-dark");
    }
    for key in [
        "GTK_THEME",
        "ICON_THEME",
        "DESKTOP_SESSION",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(value) = env::var(key) {
            for part in value.split([':', ';']) {
                let theme = part.trim();
                if !theme.is_empty() {
                    push_unique_icon_theme(&mut themes, theme);
                }
            }
        }
    }
    for fallback in [
        "breeze",
        "breeze-dark",
        "Papirus",
        "Papirus-Dark",
        "Papirus-Light",
        "Adwaita",
        "hicolor",
    ] {
        push_unique_icon_theme(&mut themes, fallback);
    }
    themes
}

fn configured_icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for path in icon_theme_config_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for theme in parse_configured_icon_theme_names(&contents) {
            push_unique_icon_theme(&mut themes, &theme);
        }
    }
    themes
}

fn icon_theme_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        let config_home = PathBuf::from(config_home);
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
    }
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        let home = PathBuf::from(home);
        let config_home = home.join(".config");
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
        push_unique_icon_path(&mut paths, home.join(".gtkrc-2.0"));
    }
    paths
}

fn parse_configured_icon_theme_names(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    let mut in_icons_section = false;
    let mut in_icon_theme_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            in_icons_section = section.eq_ignore_ascii_case("Icons");
            in_icon_theme_section = section.eq_ignore_ascii_case("Icon Theme");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("gtk-icon-theme-name")
            || (in_icons_section && key.eq_ignore_ascii_case("Theme"))
            || (in_icon_theme_section && key.eq_ignore_ascii_case("Name"))
        {
            let theme = value.trim().trim_matches('"');
            if !theme.is_empty() {
                push_unique_icon_theme(&mut themes, theme);
            }
        }
    }
    themes
}

fn parse_icon_theme_inherits(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Inherits" {
            continue;
        }
        for theme in value
            .split(',')
            .map(str::trim)
            .filter(|theme| !theme.is_empty())
        {
            push_unique_icon_theme(&mut themes, theme);
        }
    }
    themes
}

fn find_icon_in_theme(theme_root: &Path, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
    const CATEGORIES: &[&str] = &[
        "places",
        "mimetypes",
        "apps",
        "actions",
        "devices",
        "status",
    ];
    if !theme_root.is_dir() {
        return None;
    }
    if let Some(path) = find_icon_direct(theme_root, icon_name) {
        return Some(path);
    }
    for size in preferred_icon_size_dirs(desired_size) {
        for category in CATEGORIES {
            for base in [
                theme_root.join(&size).join(category),
                theme_root.join(category).join(&size),
            ] {
                if let Some(path) = find_icon_direct(&base, icon_name) {
                    return Some(path);
                }
            }
        }
    }
    for category in CATEGORIES {
        if let Some(path) = find_icon_direct(&theme_root.join(category), icon_name) {
            return Some(path);
        }
    }
    None
}

fn preferred_icon_size_dirs(desired_size: u16) -> Vec<String> {
    let mut dirs = Vec::new();
    let fixed_sizes = [256u16, 128, 96, 64, 48, 32, 24, 22, 16];
    let desired = desired_size.max(16);
    let mut ordered = fixed_sizes.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|size| size.abs_diff(desired));
    for size in ordered {
        push_icon_size_dir(&mut dirs, format!("{size}x{size}"));
        push_icon_size_dir(&mut dirs, size.to_string());
    }
    push_icon_size_dir(&mut dirs, "scalable".to_string());
    push_icon_size_dir(&mut dirs, "symbolic".to_string());
    dirs
}

fn push_icon_size_dir(dirs: &mut Vec<String>, value: String) {
    if !dirs.iter().any(|existing| existing == &value) {
        dirs.push(value);
    }
}

fn find_icon_direct(root: &Path, icon_name: &str) -> Option<PathBuf> {
    ["png", "svg", "webp", "jpg", "jpeg", "bmp", "gif", "ico"]
        .into_iter()
        .map(|extension| root.join(format!("{icon_name}.{extension}")))
        .find(|path| is_renderable_icon_file(path))
}

fn is_renderable_icon_file(path: &Path) -> bool {
    if !path.is_file()
        || fs::metadata(path)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true)
    {
        return false;
    }

    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "webp" | "jpg" | "jpeg" | "bmp" | "gif" | "ico")
    )
}

fn push_unique_icon_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn push_unique_icon_theme(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avif" | "bmp" | "gif" | "heic" | "jpeg" | "jpg" | "png" | "svg" | "tif" | "tiff" | "webp"
    )
}

fn is_archive_extension(extension: &str) -> bool {
    matches!(
        extension,
        "7z" | "bz2" | "gz" | "rar" | "tar" | "xz" | "zip" | "zst"
    )
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav"
    )
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_icon_candidates_follow_dolphin_icon_then_generic_order() {
        let mime = fika_core::MimeDatabase::from_maps(
            HashMap::new(),
            HashMap::from([("text/rust".to_string(), "text-x-rust".to_string())]),
            HashMap::from([("text/rust".to_string(), "text-x-source".to_string())]),
        );

        assert_eq!(
            mime_icon_candidates("text/rust", Some("rs"), &mime),
            &[
                "text-rust".to_string(),
                "text-x-rust".to_string(),
                "text-x-rs".to_string()
            ]
        );
        assert_eq!(
            mime_generic_icon_candidates("text/rust", &mime),
            &["text-x-source".to_string()]
        );
    }

    #[test]
    fn mime_icon_candidates_use_generic_when_specific_theme_icon_is_missing() {
        let root = test_dir("icon-theme-generic-fallback");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/text-x-source.svg"),
            test_svg(),
        )
        .unwrap();
        let mut resolver = IconThemeResolver {
            roots: vec![root.clone()],
            themes: vec!["theme".to_string()],
            path_cache: HashMap::new(),
        };
        let mime = fika_core::MimeDatabase::from_maps(
            HashMap::new(),
            HashMap::from([("text/rust".to_string(), "text-x-rust".to_string())]),
            HashMap::from([("text/rust".to_string(), "text-x-source".to_string())]),
        );

        let icon = file_icon_snapshot(
            &FileIconKind::Mime {
                mime: Arc::from("text/rust"),
                extension: Some("rs".to_string()),
            },
            48,
            &mut resolver,
            &mime,
        );

        assert_eq!(icon.icon_name, "text-x-source");
        assert_eq!(
            icon.path,
            Some(root.join("theme/48x48/mimetypes/text-x-source.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preferred_icon_size_dirs_prioritize_nearest_size() {
        let dirs = preferred_icon_size_dirs(40);

        assert_eq!(dirs[0], "48x48");
        assert_eq!(dirs[1], "48");
        assert_eq!(dirs[2], "32x32");
        assert!(dirs.iter().position(|dir| dir == "16x16").unwrap() > 4);
        assert!(dirs.contains(&"scalable".to_string()));
    }

    #[test]
    fn icon_theme_inherits_are_parsed_from_index_theme() {
        assert_eq!(
            parse_icon_theme_inherits(
                "\
[Icon Theme]\n\
Name=Child\n\
Inherits=parent-one, parent-two ,hicolor\n"
            ),
            vec![
                "parent-one".to_string(),
                "parent-two".to_string(),
                "hicolor".to_string()
            ]
        );
    }

    #[test]
    fn configured_icon_themes_parse_kde_and_gtk_settings() {
        assert_eq!(
            parse_configured_icon_theme_names(
                "\
[Icons]\n\
Theme=Papirus\n\
\n\
[Settings]\n\
gtk-icon-theme-name=breeze\n"
            ),
            vec!["Papirus".to_string(), "breeze".to_string()]
        );
        assert_eq!(
            parse_configured_icon_theme_names("gtk-icon-theme-name=\"Papirus-Dark\"\n"),
            vec!["Papirus-Dark".to_string()]
        );
    }

    #[test]
    fn icon_theme_resolver_searches_inherited_themes() {
        let root = test_dir("icon-theme-inherits");
        std::fs::create_dir_all(root.join("child")).unwrap();
        std::fs::create_dir_all(root.join("parent/48x48/mimetypes")).unwrap();
        std::fs::write(root.join("child/index.theme"), "Inherits=parent\n").unwrap();
        std::fs::write(
            root.join("parent/48x48/mimetypes/text-rust.svg"),
            test_svg(),
        )
        .unwrap();
        let mut resolver = IconThemeResolver {
            roots: vec![root.clone()],
            themes: vec!["child".to_string()],
            path_cache: HashMap::new(),
        };

        assert_eq!(
            resolver.find("text-rust", 40),
            Some(root.join("parent/48x48/mimetypes/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_icon_in_theme_supports_breeze_category_size_layout() {
        let root = test_dir("icon-theme-breeze-layout");
        std::fs::create_dir_all(root.join("mimetypes/32")).unwrap();
        std::fs::write(root.join("mimetypes/32/text-rust.svg"), test_svg()).unwrap();

        assert_eq!(
            find_icon_in_theme(&root, "text-rust", 40),
            Some(root.join("mimetypes/32/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_icon_in_theme_chooses_nearest_requested_size() {
        let root = test_dir("icon-theme-size");
        std::fs::create_dir_all(root.join("32x32/mimetypes")).unwrap();
        std::fs::create_dir_all(root.join("48x48/mimetypes")).unwrap();
        std::fs::write(root.join("32x32/mimetypes/text-rust.svg"), test_svg()).unwrap();
        std::fs::write(root.join("48x48/mimetypes/text-rust.svg"), test_svg()).unwrap();

        assert_eq!(
            find_icon_in_theme(&root, "text-rust", 40),
            Some(root.join("48x48/mimetypes/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_icon_snapshot_uses_unknown_theme_icon_before_text_marker() {
        let root = test_dir("icon-theme-unknown");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(root.join("theme/48x48/mimetypes/unknown.svg"), test_svg()).unwrap();
        let mut resolver = IconThemeResolver {
            roots: vec![root.clone()],
            themes: vec!["theme".to_string()],
            path_cache: HashMap::new(),
        };
        let mime =
            fika_core::MimeDatabase::from_maps(HashMap::new(), HashMap::new(), HashMap::new());

        let icon = file_icon_snapshot(&FileIconKind::Directory, 48, &mut resolver, &mime);

        assert_eq!(icon.icon_name, "unknown");
        assert_eq!(
            icon.path,
            Some(root.join("theme/48x48/mimetypes/unknown.svg"))
        );
        assert_eq!(icon.fallback_marker, "DIR");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_icon_cache_is_keyed_by_kind_and_size() {
        let root = test_dir("icon-cache");
        std::fs::create_dir_all(root.join("theme/32x32/mimetypes")).unwrap();
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(root.join("theme/32x32/mimetypes/text-rust.svg"), test_svg()).unwrap();
        std::fs::write(root.join("theme/48x48/mimetypes/text-rust.svg"), test_svg()).unwrap();
        let mut cache = FileIconCache {
            cached: HashMap::new(),
            named_cached: HashMap::new(),
            theme: IconThemeResolver {
                roots: vec![root.clone()],
                themes: vec!["theme".to_string()],
                path_cache: HashMap::new(),
            },
            mime: fika_core::MimeDatabase::from_maps(
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
            ),
        };

        let small = cache.icon_for(
            Path::new("lib.rs"),
            false,
            Some(Arc::from("text/rust")),
            32.0,
        );
        let small_again = cache.icon_for(
            Path::new("main.rs"),
            false,
            Some(Arc::from("text/rust")),
            32.0,
        );
        let large = cache.icon_for(
            Path::new("main.rs"),
            false,
            Some(Arc::from("text/rust")),
            48.0,
        );

        assert_eq!(small, small_again);
        assert_eq!(
            small.path,
            Some(root.join("theme/32x32/mimetypes/text-rust.svg"))
        );
        assert_eq!(
            large.path,
            Some(root.join("theme/48x48/mimetypes/text-rust.svg"))
        );
        assert_eq!(cache.cached.len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-icons-{name}-{}-{nanos}", std::process::id()))
    }

    fn test_svg() -> &'static str {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48"><rect width="48" height="48" fill="#2f6fed"/></svg>"##
    }
}
