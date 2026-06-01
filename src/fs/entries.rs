use crate::FileEntry;
use slint::{Image, SharedString};
use std::cmp::Ordering;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct RawFileEntry {
    pub name: String,
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
    let mut entries = Vec::new();
    let mut dir = tokio::fs::read_dir(path).await?;

    while let Some(item) = dir.next_entry().await? {
        if let Ok(metadata) = item.metadata().await {
            entries.push(to_raw_file_entry(
                item.path(),
                item.file_name().to_string_lossy().trim().to_string(),
                String::new(),
                metadata,
            ));
        }
    }

    entries.sort_by(compare_raw_entries);
    Ok(entries)
}

pub fn to_file_entry(entry: RawFileEntry) -> FileEntry {
    FileEntry {
        name: entry.name.into(),
        path: entry.path.into(),
        group: entry.group.into(),
        location: entry.location.into(),
        kind: entry.kind.into(),
        size: entry.size.into(),
        size_bytes: entry.size_bytes as f32,
        modified: entry.modified.into(),
        modified_age_days: entry.modified_age_days,
        is_dir: entry.is_dir,
        thumbnail_state: 0,
        thumbnail: Image::default(),
    }
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

fn compare_raw_entries(left: &RawFileEntry, right: &RawFileEntry) -> Ordering {
    match (left.is_dir, right.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left
            .name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(format_unix_time(0), "1970-01-01 00:00");
    }
}
