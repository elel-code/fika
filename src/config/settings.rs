use super::paths::home_dir;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppSettings {
    pub dark_mode: Option<bool>,
    pub sidebar_width_px: Option<f32>,
    pub split_pane_ratio: Option<f32>,
    pub icon_zoom_level: Option<i32>,
    pub window_width_px: Option<f32>,
    pub window_height_px: Option<f32>,
    pub last_dir: Option<PathBuf>,
}

pub fn load_settings() -> AppSettings {
    let Ok(contents) = fs::read_to_string(settings_path()) else {
        return AppSettings::default();
    };

    let mut settings = AppSettings::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('\t') else {
            continue;
        };
        match key {
            "dark_mode" => settings.dark_mode = value.parse().ok(),
            "sidebar_width_px" => settings.sidebar_width_px = value.parse().ok(),
            "split_pane_ratio" => settings.split_pane_ratio = value.parse().ok(),
            "icon_zoom_level" => settings.icon_zoom_level = value.parse().ok(),
            "window_width_px" => settings.window_width_px = value.parse().ok(),
            "window_height_px" => settings.window_height_px = value.parse().ok(),
            "last_dir" if !value.is_empty() => settings.last_dir = Some(PathBuf::from(value)),
            _ => {}
        }
    }
    settings
}

pub fn save_settings(settings: &AppSettings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut lines = Vec::new();
    if let Some(dark_mode) = settings.dark_mode {
        lines.push(format!("dark_mode\t{dark_mode}"));
    }
    if let Some(sidebar_width_px) = settings.sidebar_width_px {
        lines.push(format!("sidebar_width_px\t{sidebar_width_px}"));
    }
    if let Some(split_pane_ratio) = settings.split_pane_ratio {
        lines.push(format!("split_pane_ratio\t{split_pane_ratio}"));
    }
    if let Some(icon_zoom_level) = settings.icon_zoom_level {
        lines.push(format!("icon_zoom_level\t{icon_zoom_level}"));
    }
    if let Some(window_width_px) = settings.window_width_px {
        lines.push(format!("window_width_px\t{window_width_px}"));
    }
    if let Some(window_height_px) = settings.window_height_px {
        lines.push(format!("window_height_px\t{window_height_px}"));
    }
    if let Some(last_dir) = &settings.last_dir {
        lines.push(format!("last_dir\t{}", last_dir.display()));
    }

    let _ = fs::write(path, lines.join("\n"));
}

fn settings_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("fika")
        .join("settings.tsv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_corrupt_settings_values() {
        let mut settings = AppSettings::default();
        for line in [
            "dark_mode\tmaybe",
            "sidebar_width_px\twide",
            "split_pane_ratio\tratio",
            "icon_zoom_level\tlarge",
            "window_width_px\twide",
            "window_height_px\ttall",
            "last_dir\t/tmp",
        ] {
            let (key, value) = line.split_once('\t').unwrap();
            match key {
                "dark_mode" => settings.dark_mode = value.parse().ok(),
                "sidebar_width_px" => settings.sidebar_width_px = value.parse().ok(),
                "split_pane_ratio" => settings.split_pane_ratio = value.parse().ok(),
                "icon_zoom_level" => settings.icon_zoom_level = value.parse().ok(),
                "window_width_px" => settings.window_width_px = value.parse().ok(),
                "window_height_px" => settings.window_height_px = value.parse().ok(),
                "last_dir" if !value.is_empty() => settings.last_dir = Some(PathBuf::from(value)),
                _ => {}
            }
        }

        assert_eq!(settings.dark_mode, None);
        assert_eq!(settings.sidebar_width_px, None);
        assert_eq!(settings.split_pane_ratio, None);
        assert_eq!(settings.icon_zoom_level, None);
        assert_eq!(settings.window_width_px, None);
        assert_eq!(settings.window_height_px, None);
        assert_eq!(settings.last_dir, Some(PathBuf::from("/tmp")));
    }
}
