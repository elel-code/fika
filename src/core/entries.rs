use super::file_ops;
use std::cmp::Ordering;
use std::fs::Metadata;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemId(pub u64);

impl ItemId {
    pub const UNASSIGNED: Self = Self(0);

    pub fn is_assigned(self) -> bool {
        self != Self::UNASSIGNED
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryData {
    pub name: Arc<str>,
    pub name_width_units: u16,
    pub size_bytes: u64,
    pub modified_secs: Option<u64>,
    pub trash_group: Option<Arc<str>>,
    pub trash_deletion_label: Option<Arc<str>>,
    pub is_dir: bool,
}

impl Entry {
    pub fn new(data: EntryData) -> Self {
        Self(Arc::new(data))
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }

    pub(crate) fn sort_cmp(&self, other: &Self) -> Ordering {
        self.0.sort_cmp(&other.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry(Arc<EntryData>);

impl Deref for Entry {
    type Target = EntryData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EntryData {
    pub(crate) fn sort_cmp(&self, other: &Self) -> Ordering {
        match other.is_dir.cmp(&self.is_dir) {
            Ordering::Equal => entry_name_cmp(&self.name, &other.name)
                .then_with(|| self.size_bytes.cmp(&other.size_bytes)),
            ordering => ordering,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelEntry {
    pub id: ItemId,
    pub entry: Entry,
}

impl ModelEntry {
    pub(crate) fn unassigned(entry: Entry) -> Self {
        Self {
            id: ItemId::UNASSIGNED,
            entry,
        }
    }

    pub(crate) fn sort_cmp(&self, other: &Self) -> Ordering {
        self.entry.sort_cmp(&other.entry)
    }
}

impl Deref for ModelEntry {
    type Target = EntryData;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

pub fn read_entries_sync(path: &Path) -> io::Result<Vec<Entry>> {
    Ok(read_entries_sync_cancellable(path, || false)?.unwrap_or_default())
}

pub(crate) fn read_entries_sync_cancellable(
    path: &Path,
    mut is_cancelled: impl FnMut() -> bool,
) -> io::Result<Option<Vec<Entry>>> {
    if is_cancelled() {
        return Ok(None);
    }

    let mut entries = Vec::new();
    let decorate_trash_metadata = file_ops::is_trash_files_dir(path);

    for (index, item) in std::fs::read_dir(path)?.enumerate() {
        if index % 64 == 0 && is_cancelled() {
            return Ok(None);
        }
        let Ok(item) = item else {
            continue;
        };
        if let Ok(metadata) = item.metadata() {
            let item_path = item.path();
            let name = item.file_name().to_string_lossy().trim().to_string();
            if name.is_empty() {
                continue;
            }
            let mut data = to_entry_data(name, metadata);
            if decorate_trash_metadata {
                decorate_trash_entry(&mut data, &item_path);
            }
            entries.push(Entry::new(data));
        }
    }

    if is_cancelled() {
        return Ok(None);
    }
    sort_entries(&mut entries, decorate_trash_metadata);
    Ok(Some(entries))
}

pub fn read_entry_sync(directory: &Path, path: &Path) -> io::Result<Entry> {
    let decorate_trash_metadata = file_ops::is_trash_files_dir(directory);
    let item_path = directory_entry_path(directory, path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "directory item is outside directory",
        )
    })?;
    let metadata = std::fs::metadata(&item_path)?;
    let name = item_path
        .file_name()
        .map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "directory item has no name"))?;
    let mut data = to_entry_data(name, metadata);
    if decorate_trash_metadata {
        decorate_trash_entry(&mut data, &item_path);
    }
    Ok(Entry::new(data))
}

pub fn sort_entries(entries: &mut [Entry], trash: bool) {
    if trash {
        entries.sort_by(trash_sort_cmp);
    } else {
        entries.sort_by(Entry::sort_cmp);
    }
}

pub fn directory_entry_path(directory: &Path, path: &Path) -> Option<PathBuf> {
    if file_ops::is_trash_files_dir(directory)
        && path
            .parent()
            .is_some_and(|parent| parent == file_ops::trash_info_dir())
        && path.extension().and_then(|extension| extension.to_str()) == Some("trashinfo")
    {
        let stem = path.file_stem()?;
        return Some(file_ops::trash_files_dir().join(stem));
    }

    path.parent()
        .is_some_and(|parent| parent == directory)
        .then(|| path.to_path_buf())
}

fn decorate_trash_entry(entry: &mut EntryData, path: &Path) {
    let Ok(metadata) = file_ops::trash_metadata(path) else {
        return;
    };
    entry.trash_group = Some(Arc::from(trash_group_label(
        &metadata.original_path,
        metadata.deletion_date.as_deref(),
    )));
    if let Some(deletion_date) = metadata.deletion_date {
        entry.trash_deletion_label = Some(Arc::from(format_trash_deletion_date(&deletion_date)));
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

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn trash_sort_cmp(left: &Entry, right: &Entry) -> Ordering {
    trash_sort_bucket(left)
        .cmp(&trash_sort_bucket(right))
        .then_with(|| {
            right
                .trash_deletion_label
                .as_deref()
                .unwrap_or_default()
                .cmp(left.trash_deletion_label.as_deref().unwrap_or_default())
        })
        .then_with(|| left.sort_cmp(right))
}

pub(crate) fn entry_name_cmp(left: &str, right: &str) -> Ordering {
    ascii_case_insensitive_cmp(left, right).then_with(|| left.cmp(right))
}

fn trash_sort_bucket(entry: &Entry) -> u8 {
    if entry.trash_deletion_label.is_some() {
        0
    } else if entry.trash_group.is_some() {
        1
    } else {
        2
    }
}

fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    let mut left_bytes = left.bytes();
    let mut right_bytes = right.bytes();
    loop {
        match (left_bytes.next(), right_bytes.next()) {
            (Some(left), Some(right)) => {
                let ordering = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn name_width_units(name: &str) -> u16 {
    name.chars()
        .map(|ch| if ch.is_ascii() { 1u32 } else { 2u32 })
        .sum::<u32>()
        .min(u16::MAX as u32) as u16
}

fn to_entry_data(name: String, metadata: Metadata) -> EntryData {
    let is_dir = metadata.is_dir();
    let size_bytes = if is_dir { 0 } else { metadata.len() };
    let modified_secs = metadata.modified().ok().map(system_time_secs);
    let name_width_units = name_width_units(&name);

    EntryData {
        name: Arc::from(name),
        name_width_units,
        size_bytes,
        modified_secs,
        trash_group: None,
        trash_deletion_label: None,
        is_dir,
    }
}

fn system_time_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub fn format_modified_secs(secs: Option<u64>) -> String {
    let Some(secs) = secs else {
        return "-".to_string();
    };
    format_unix_time(secs)
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
    fn formats_file_sizes_without_ui_types() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn cancellable_listing_returns_none_before_touching_directory() {
        let result =
            read_entries_sync_cancellable(Path::new("/definitely/missing/fika"), || true).unwrap();

        assert!(result.is_none());
    }
}
