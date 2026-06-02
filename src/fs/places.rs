use crate::PlaceEntry;
use crate::config::paths::home_dir;
use crate::fs::file_ops::trash_files_dir;
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn default_places() -> Vec<PlaceEntry> {
    if let Ok(contents) = fs::read_to_string(places_config_path()) {
        let mut places: Vec<_> = contents
            .lines()
            .filter_map(parse_place_line)
            .filter(|place| !place.path.is_empty())
            .collect();
        if !places.is_empty() {
            append_missing_builtin_places(&mut places);
            return places;
        }
    }

    builtin_places()
}

pub fn builtin_places() -> Vec<PlaceEntry> {
    let home = home_dir();
    [
        ("Home", home.clone(), "~"),
        ("Desktop", home.join("Desktop"), "D"),
        ("Documents", home.join("Documents"), "O"),
        ("Downloads", home.join("Downloads"), "v"),
        ("Music", home.join("Music"), "M"),
        ("Pictures", home.join("Pictures"), "P"),
        ("Videos", home.join("Videos"), "V"),
        ("Trash", trash_files_dir(), "T"),
    ]
    .into_iter()
    .map(|(label, path, marker)| place_entry_with_kind(label, path, marker, true))
    .collect()
}

pub fn place_entry(label: &str, path: PathBuf, marker: &str) -> PlaceEntry {
    place_entry_with_kind(label, path, marker, false)
}

pub fn place_entry_with_kind(
    label: &str,
    path: PathBuf,
    marker: &str,
    is_builtin: bool,
) -> PlaceEntry {
    PlaceEntry {
        label: label.into(),
        path: path.display().to_string().into(),
        marker: marker.into(),
        is_builtin,
    }
}

pub fn save_places(places: &[PlaceEntry]) {
    let path = places_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let contents = places
        .iter()
        .map(|place| {
            format!(
                "{}\t{}\t{}\t{}",
                place.label, place.path, place.marker, place.is_builtin
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(path, contents);
}

fn places_config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("fika")
        .join("places.tsv")
}

fn parse_place_line(line: &str) -> Option<PlaceEntry> {
    let mut parts = line.splitn(4, '\t');
    let label = parts.next()?;
    let path = parts.next()?;
    let marker = parts.next().unwrap_or("+");
    let is_builtin = parts
        .next()
        .map(str::parse)
        .and_then(Result::ok)
        .unwrap_or_else(|| is_default_place(label, path));
    Some(PlaceEntry {
        label: label.into(),
        path: path.into(),
        marker: marker.into(),
        is_builtin,
    })
}

fn is_default_place(label: &str, path: &str) -> bool {
    builtin_places()
        .iter()
        .any(|place| place.label == label && place.path == path)
}

fn append_missing_builtin_places(places: &mut Vec<PlaceEntry>) {
    for builtin in builtin_places() {
        if !places
            .iter()
            .any(|place| place.label == builtin.label || place.path == builtin.path)
        {
            places.push(builtin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(places: &[PlaceEntry]) -> Vec<String> {
        places.iter().map(|place| place.label.to_string()).collect()
    }

    #[test]
    fn builtin_places_include_trash_location() {
        let places = builtin_places();
        let trash = places
            .iter()
            .find(|place| place.label == "Trash")
            .expect("Trash place should be built in");

        assert_eq!(trash.path, trash_files_dir().display().to_string());
        assert_eq!(trash.marker, "T");
        assert!(trash.is_builtin);
    }

    #[test]
    fn missing_builtin_places_are_appended_to_existing_config() {
        let mut places = vec![place_entry_with_kind("Home", home_dir(), "~", true)];

        append_missing_builtin_places(&mut places);

        assert!(labels(&places).contains(&"Trash".to_string()));
        assert_eq!(labels(&places).first(), Some(&"Home".to_string()));
    }
}
