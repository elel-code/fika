use crate::fs::file_ops;
use slint::SharedString;
#[cfg(test)]
use std::cmp::Ordering;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub struct RawFileEntry {
    pub name: String,
    pub name_width_units: f32,
    pub path: String,
    pub group: String,
    pub location: String,
    pub kind: String,
    pub size: String,
    pub size_bytes: u64,
    pub modified: String,
    pub modified_age_days: i32,
    pub is_dir: bool,
}

pub async fn read_entries_async(path: &Path) -> io::Result<Vec<RawFileEntry>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_entries_sync(&path))
        .await
        .map_err(|err| io::Error::other(format!("directory scan task failed: {err}")))?
}

pub fn read_entries_sync(path: &Path) -> io::Result<Vec<RawFileEntry>> {
    let mut entries = Vec::new();
    let dir = std::fs::read_dir(path)?;
    let decorate_trash_metadata = file_ops::is_trash_files_dir(path);

    for item in dir {
        let Ok(item) = item else {
            continue;
        };
        if let Ok(metadata) = item.metadata() {
            let item_path = item.path();
            let mut entry = to_raw_file_entry(
                item_path.clone(),
                item.file_name().to_string_lossy().trim().to_string(),
                String::new(),
                metadata,
            );
            if decorate_trash_metadata {
                decorate_trash_entry(&mut entry, &item_path);
            }
            entries.push(entry);
        }
    }

    sort_entries(&mut entries, decorate_trash_metadata);
    Ok(entries)
}

fn sort_entries(entries: &mut [RawFileEntry], trash: bool) {
    if trash {
        entries.sort_by_cached_key(trash_sort_key);
    } else {
        entries.sort_by_cached_key(raw_sort_key);
    }
}

fn decorate_trash_entry(entry: &mut RawFileEntry, path: &Path) {
    let Ok(metadata) = file_ops::trash_metadata(path) else {
        return;
    };
    entry.group = trash_group_label(&metadata.original_path, metadata.deletion_date.as_deref());
    if let Some(deletion_date) = metadata.deletion_date {
        entry.modified = format_trash_deletion_date(&deletion_date);
        entry.modified_age_days = -1;
    }
}

fn trash_group_label(original_path: &Path, deletion_date: Option<&str>) -> String {
    let original_location = original_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(original_path);
    match deletion_date {
        Some(date) => format!(
            "Original: {} - Deleted: {}",
            original_location.display(),
            format_trash_deletion_date(date)
        ),
        None => format!("Original: {}", original_location.display()),
    }
}

fn format_trash_deletion_date(value: &str) -> String {
    let normalized = value.replace('T', " ");
    normalized
        .strip_suffix(":00")
        .unwrap_or(&normalized)
        .to_string()
}

pub fn format_size(bytes: u64) -> SharedString {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit]).into()
    } else {
        format!("{size:.1} {}", UNITS[unit]).into()
    }
}

#[cfg(test)]
fn compare_raw_entries(left: &RawFileEntry, right: &RawFileEntry) -> Ordering {
    raw_sort_key(left).cmp(&raw_sort_key(right))
}

#[cfg(test)]
fn compare_trash_entries(left: &RawFileEntry, right: &RawFileEntry) -> Ordering {
    trash_sort_key(left).cmp(&trash_sort_key(right))
}

fn raw_sort_key(entry: &RawFileEntry) -> (u8, String) {
    (u8::from(!entry.is_dir), entry.name.to_ascii_lowercase())
}

fn trash_sort_key(entry: &RawFileEntry) -> (u8, std::cmp::Reverse<String>, String) {
    let bucket = trash_sort_bucket(entry);
    let deletion_date = if bucket == 0 {
        std::cmp::Reverse(entry.modified.clone())
    } else {
        std::cmp::Reverse(String::new())
    };
    (bucket, deletion_date, entry.name.to_ascii_lowercase())
}

fn trash_sort_bucket(entry: &RawFileEntry) -> u8 {
    if entry.group.contains("Deleted: ") {
        0
    } else if !entry.group.is_empty() {
        1
    } else {
        2
    }
}

pub(crate) fn to_raw_file_entry(
    path: PathBuf,
    name: String,
    location: String,
    metadata: Metadata,
) -> RawFileEntry {
    let is_dir = metadata.is_dir();
    let size_bytes = if is_dir { 0 } else { metadata.len() };
    let modified = metadata.modified().ok();

    RawFileEntry {
        name_width_units: raw_name_width_units(&name),
        name,
        path: path.display().to_string(),
        group: String::new(),
        location,
        kind: if is_dir { "Folder" } else { "File" }.to_string(),
        size: if is_dir {
            "-".to_string()
        } else {
            format_size(size_bytes).to_string()
        },
        size_bytes,
        modified: modified
            .map(format_system_time)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        modified_age_days: modified.map(modified_age_days).unwrap_or(-1),
        is_dir,
    }
}

fn modified_age_days(time: SystemTime) -> i32 {
    match SystemTime::now().duration_since(time) {
        Ok(duration) => (duration.as_secs() / 86_400).min(i32::MAX as u64) as i32,
        Err(_) => 0,
    }
}

fn format_system_time(time: SystemTime) -> SharedString {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format_unix_time(secs).into()
}

fn format_unix_time(secs: u64) -> String {
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let seconds_of_day = secs % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

fn raw_name_width_units(text: &str) -> f32 {
    text.chars()
        .map(|ch| {
            if ch.is_whitespace() {
                0.35
            } else if ch.is_ascii() {
                match ch {
                    'i' | 'l' | 'I' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' | '`' => 0.32,
                    'm' | 'w' | 'M' | 'W' | '@' | '#' | '%' | '&' => 0.82,
                    _ => 0.58,
                }
            } else {
                1.0
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(format_unix_time(0), "1970-01-01 00:00");
    }

    #[test]
    fn regular_directory_sort_keeps_folders_before_files_by_name() {
        let mut entries = vec![
            test_entry("zeta.txt", false, "", "2026-06-01 10:00"),
            test_entry("beta", true, "", "2026-06-01 10:00"),
            test_entry("alpha.txt", false, "", "2026-06-01 10:00"),
            test_entry("Alpha", true, "", "2026-06-01 10:00"),
        ];

        entries.sort_by(compare_raw_entries);

        assert_eq!(
            entry_names(&entries),
            vec!["Alpha", "beta", "alpha.txt", "zeta.txt"]
        );
    }

    #[test]
    fn trash_sort_prefers_newer_deletion_dates_then_metadata_then_unknowns() {
        let mut entries = vec![
            test_entry("unknown.txt", false, "", "2026-06-02 11:00"),
            test_entry(
                "old.txt",
                false,
                "Original: /tmp - Deleted: 2026-05-30 12:00",
                "2026-05-30 12:00",
            ),
            test_entry(
                "new.txt",
                false,
                "Original: /tmp - Deleted: 2026-06-02 09:00",
                "2026-06-02 09:00",
            ),
            test_entry(
                "metadata-no-date.txt",
                false,
                "Original: /tmp",
                "2026-06-03 09:00",
            ),
            test_entry(
                "same-date-a.txt",
                false,
                "Original: /tmp - Deleted: 2026-06-02 09:00",
                "2026-06-02 09:00",
            ),
        ];

        entries.sort_by(compare_trash_entries);

        assert_eq!(
            entry_names(&entries),
            vec![
                "new.txt",
                "same-date-a.txt",
                "old.txt",
                "metadata-no-date.txt",
                "unknown.txt"
            ]
        );
    }

    #[test]
    fn read_regular_directory_keeps_existing_sort_order() {
        let temp = test_dir("regular-sort");
        fs::create_dir_all(temp.join("beta")).unwrap();
        fs::create_dir_all(temp.join("Alpha")).unwrap();
        fs::write(temp.join("zeta.txt"), b"zeta").unwrap();
        fs::write(temp.join("alpha.txt"), b"alpha").unwrap();

        let entries = read_entries_sync(&temp).unwrap();

        assert_eq!(
            entry_names(&entries),
            vec!["Alpha", "beta", "alpha.txt", "zeta.txt"]
        );
        let _ = fs::remove_dir_all(temp);
    }

    fn test_entry(name: &str, is_dir: bool, group: &str, modified: &str) -> RawFileEntry {
        RawFileEntry {
            name: name.to_string(),
            name_width_units: raw_name_width_units(name),
            path: format!("/tmp/{name}"),
            group: group.to_string(),
            location: String::new(),
            kind: if is_dir { "Folder" } else { "File" }.to_string(),
            size: if is_dir { "-" } else { "1 B" }.to_string(),
            size_bytes: u64::from(!is_dir),
            modified: modified.to_string(),
            modified_age_days: -1,
            is_dir,
        }
    }

    fn entry_names(entries: &[RawFileEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "fika-entries-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
