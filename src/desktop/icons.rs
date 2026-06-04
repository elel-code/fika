use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const ICON_EXTENSIONS: [&str; 3] = ["svg", "png", "xpm"];
const ICON_CONTEXTS: [&str; 7] = [
    "actions",
    "apps",
    "places",
    "mimetypes",
    "devices",
    "categories",
    "status",
];
const FALLBACK_THEMES: [&str; 3] = ["hicolor", "breeze", "Adwaita"];

type IconCacheKey = (String, u32);
type IconPathCache = HashMap<IconCacheKey, Option<PathBuf>>;

static ICON_PATH_CACHE: OnceLock<Mutex<IconPathCache>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
struct IconLookup {
    theme_parent_dirs: Vec<PathBuf>,
    pixmap_dirs: Vec<PathBuf>,
    current_themes: Vec<String>,
}

pub(crate) fn resolve_icon_path(icon: &str, size_px: u32) -> Option<PathBuf> {
    let key = (icon.trim().to_string(), size_px);
    if key.0.is_empty() {
        return None;
    }
    let cache = ICON_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(path) = cache.get(&key)
    {
        return path.clone();
    }

    let path = resolve_icon_path_uncached(&key.0, size_px);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, path.clone());
    }
    path
}

fn resolve_icon_path_uncached(icon: &str, size_px: u32) -> Option<PathBuf> {
    resolve_icon_path_with_lookup(icon, size_px, &IconLookup::from_environment())
}

fn resolve_icon_path_with_lookup(icon: &str, size_px: u32, lookup: &IconLookup) -> Option<PathBuf> {
    let icon = icon.trim();
    if icon.is_empty() {
        return None;
    }

    if let Some(path) = direct_icon_file(Path::new(icon)) {
        return Some(path);
    }
    if icon.contains('/') {
        return None;
    }

    let names = themed_icon_names(icon);
    let size_dirs = icon_size_dirs(size_px);
    for theme in lookup.theme_chain() {
        for theme_root in lookup.theme_roots(&theme) {
            if let Some(path) = find_icon_in_theme_root(&theme_root, &names, &size_dirs) {
                return Some(path);
            }
        }
    }

    for pixmap_dir in &lookup.pixmap_dirs {
        for name in &names {
            if let Some(path) = icon_file_in_dir(pixmap_dir, name) {
                return Some(path);
            }
        }
    }

    None
}

impl IconLookup {
    fn from_environment() -> Self {
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".local/share"));
        let data_dirs = split_paths_or_default("XDG_DATA_DIRS", "/usr/local/share:/usr/share");

        let mut theme_parent_dirs = Vec::new();
        theme_parent_dirs.push(home_dir().join(".icons"));
        theme_parent_dirs.push(data_home.join("icons"));
        theme_parent_dirs.extend(data_dirs.iter().map(|dir| dir.join("icons")));

        let mut pixmap_dirs = Vec::new();
        pixmap_dirs.push(
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir().join(".local/share"))
                .join("pixmaps"),
        );
        pixmap_dirs.extend(data_dirs.iter().map(|dir| dir.join("pixmaps")));
        pixmap_dirs.push(PathBuf::from("/usr/share/pixmaps"));

        Self {
            theme_parent_dirs,
            pixmap_dirs,
            current_themes: current_icon_themes(),
        }
    }

    fn theme_roots(&self, theme: &str) -> Vec<PathBuf> {
        self.theme_parent_dirs
            .iter()
            .map(|parent| parent.join(theme))
            .collect()
    }

    fn theme_chain(&self) -> Vec<String> {
        let mut themes = Vec::new();
        let mut seen = HashSet::new();
        for theme in self
            .current_themes
            .iter()
            .map(String::as_str)
            .chain(FALLBACK_THEMES)
        {
            self.add_theme_with_inherits(theme, &mut themes, &mut seen, 0);
        }
        themes
    }

    fn add_theme_with_inherits(
        &self,
        theme: &str,
        themes: &mut Vec<String>,
        seen: &mut HashSet<String>,
        depth: usize,
    ) {
        let theme = theme.trim();
        if theme.is_empty() || depth > 8 || !seen.insert(theme.to_string()) {
            return;
        }
        themes.push(theme.to_string());
        for inherited in self.theme_inherits(theme) {
            self.add_theme_with_inherits(&inherited, themes, seen, depth + 1);
        }
    }

    fn theme_inherits(&self, theme: &str) -> Vec<String> {
        for root in self.theme_roots(theme) {
            let path = root.join("index.theme");
            if let Ok(content) = fs::read_to_string(path)
                && let Some(value) = ini_value(&content, "Icon Theme", "Inherits")
            {
                return value
                    .split(',')
                    .filter_map(|item| {
                        let item = item.trim();
                        (!item.is_empty()).then(|| item.to_string())
                    })
                    .collect();
            }
        }
        Vec::new()
    }
}

fn find_icon_in_theme_root(
    theme_root: &Path,
    names: &[String],
    size_dirs: &[String],
) -> Option<PathBuf> {
    for name in names {
        for size_dir in size_dirs {
            for context in ICON_CONTEXTS {
                for dir in [
                    theme_root.join(size_dir).join(context),
                    theme_root.join(context).join(size_dir),
                ] {
                    if let Some(path) = icon_file_in_dir(&dir, name) {
                        return Some(path);
                    }
                }
            }
        }

        for context in ICON_CONTEXTS {
            if let Some(path) = icon_file_in_dir(&theme_root.join(context), name) {
                return Some(path);
            }
        }
    }
    None
}

fn direct_icon_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.extension().is_some() {
        return None;
    }
    ICON_EXTENSIONS
        .iter()
        .map(|extension| path.with_extension(extension))
        .find(|candidate| candidate.is_file())
}

fn icon_file_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let path = dir.join(name);
    direct_icon_file(&path)
}

fn themed_icon_names(icon: &str) -> Vec<String> {
    let path = Path::new(icon);
    let mut names = Vec::new();
    names.push(icon.to_string());
    if path.extension().is_some()
        && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
        && stem != icon
    {
        names.push(stem.to_string());
    }
    names
}

fn icon_size_dirs(size_px: u32) -> Vec<String> {
    let mut sizes = [16_u32, 22, 24, 32, 48, 64, 96, 128, 256];
    sizes.sort_by_key(|size| size.abs_diff(size_px));
    let mut dirs = Vec::new();
    for size in sizes {
        dirs.push(format!("{size}x{size}"));
        dirs.push(size.to_string());
    }
    dirs.push("scalable".to_string());
    dirs.push("symbolic".to_string());
    dirs
}

fn current_icon_themes() -> Vec<String> {
    let mut themes = Vec::new();
    if let Ok(theme) = env::var("FIKA_ICON_THEME") {
        push_theme(&mut themes, &theme);
    }
    for path in icon_config_paths() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(theme) = ini_value(&content, "Icons", "Theme") {
                push_theme(&mut themes, &theme);
            }
            if let Some(theme) = ini_value(&content, "Settings", "gtk-icon-theme-name") {
                push_theme(&mut themes, &theme);
            }
        }
    }
    themes
}

fn icon_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in config_dirs() {
        paths.push(dir.join("kdeglobals"));
        paths.push(dir.join("gtk-3.0").join("settings.ini"));
        paths.push(dir.join("gtk-4.0").join("settings.ini"));
    }
    paths
}

fn push_theme(themes: &mut Vec<String>, theme: &str) {
    let theme = theme.trim();
    if !theme.is_empty() && !themes.iter().any(|existing| existing == theme) {
        themes.push(theme.to_string());
    }
}

fn ini_value(content: &str, section_name: &str, key_name: &str) -> Option<String> {
    let mut in_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line[1..line.len() - 1].trim() == section_name;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == key_name
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config")),
    );
    dirs.extend(split_paths_or_default("XDG_CONFIG_DIRS", "/etc/xdg"));
    dirs
}

fn split_paths_or_default(var: &str, default: &str) -> Vec<PathBuf> {
    let value = env::var_os(var).unwrap_or_else(|| default.into());
    env::split_paths(&value).collect()
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn resolves_breeze_category_before_size_icons() {
        let temp = test_dir("breeze-category-size");
        let icon_path = temp
            .join("icons")
            .join("breeze")
            .join("actions")
            .join("16")
            .join("edit-copy.svg");
        fs::create_dir_all(icon_path.parent().unwrap()).unwrap();
        fs::write(&icon_path, "").unwrap();

        let lookup = test_lookup(&temp, &["breeze"]);

        assert_eq!(
            resolve_icon_path_with_lookup("edit-copy", 18, &lookup),
            Some(icon_path)
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn follows_icon_theme_inherits() {
        let temp = test_dir("theme-inherits");
        let custom = temp.join("icons").join("custom");
        fs::create_dir_all(&custom).unwrap();
        fs::write(
            custom.join("index.theme"),
            "[Icon Theme]\nInherits=hicolor\n",
        )
        .unwrap();
        let icon_path = temp
            .join("icons")
            .join("hicolor")
            .join("16x16")
            .join("apps")
            .join("fika-test.png");
        fs::create_dir_all(icon_path.parent().unwrap()).unwrap();
        fs::write(&icon_path, "").unwrap();

        let lookup = test_lookup(&temp, &["custom"]);

        assert_eq!(
            resolve_icon_path_with_lookup("fika-test", 16, &lookup),
            Some(icon_path)
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn resolves_pixmap_icons() {
        let temp = test_dir("pixmaps");
        let icon_path = temp.join("pixmaps").join("tool.xpm");
        fs::create_dir_all(icon_path.parent().unwrap()).unwrap();
        fs::write(&icon_path, "").unwrap();

        let lookup = test_lookup(&temp, &[]);

        assert_eq!(
            resolve_icon_path_with_lookup("tool", 22, &lookup),
            Some(icon_path)
        );

        let _ = fs::remove_dir_all(temp);
    }

    fn test_lookup(temp: &Path, themes: &[&str]) -> IconLookup {
        IconLookup {
            theme_parent_dirs: vec![temp.join("icons")],
            pixmap_dirs: vec![temp.join("pixmaps")],
            current_themes: themes.iter().map(|theme| theme.to_string()).collect(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "fika-icons-{name}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
