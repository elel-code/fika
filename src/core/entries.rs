use super::{
    file_ops,
    mime::{GENERIC_BINARY_MIME, MimeDatabase, mime_magic_resolution_required, read_mime_magic},
};
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
    pub metadata_complete: bool,
    pub mime_type: Option<Arc<str>>,
    pub mime_magic_checked: bool,
    pub trash_original_path: Option<PathBuf>,
    pub trash_deletion_time: Option<Arc<str>>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryMetadataRole {
    pub size_bytes: u64,
    pub modified_secs: Option<u64>,
    pub mime_type: Option<Arc<str>>,
    pub mime_magic_checked: bool,
}

impl EntryMetadataRole {
    pub fn from_metadata(
        name: &str,
        is_dir: bool,
        metadata: &Metadata,
        mime: &MimeDatabase,
    ) -> Self {
        let size_bytes = if is_dir { 0 } else { metadata.len() };
        let modified_secs = metadata.modified().ok().map(system_time_secs);
        let mime_type = Some(mime.mime_for_name(name, is_dir, None));
        let mime_magic_checked =
            is_dir || size_bytes == 0 || mime_type.as_deref() != Some(GENERIC_BINARY_MIME);
        Self {
            size_bytes,
            modified_secs,
            mime_type,
            mime_magic_checked,
        }
    }

    pub fn resolved_from_path(
        name: &str,
        path: &Path,
        is_dir: bool,
        metadata: &Metadata,
        mime: &MimeDatabase,
    ) -> Self {
        let mut role = Self::from_metadata(name, is_dir, metadata, mime);
        if mime_magic_resolution_required(
            is_dir,
            role.size_bytes,
            role.mime_type.as_deref(),
            role.mime_magic_checked,
        ) {
            role.mime_type = read_mime_magic(path)
                .ok()
                .flatten()
                .and_then(|magic| mime.mime_for_path(path, is_dir, Some(&magic)))
                .or(role.mime_type);
            role.mime_magic_checked = true;
        }
        role
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
    pub metadata_refresh_pending: bool,
    pub icon_name: Option<Arc<str>>,
    pub thumbnail_path: Option<PathBuf>,
}

impl ModelEntry {
    pub(crate) fn unassigned(entry: Entry) -> Self {
        Self {
            id: ItemId::UNASSIGNED,
            entry,
            metadata_refresh_pending: false,
            icon_name: None,
            thumbnail_path: None,
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
    let mime_database = MimeDatabase::shared();

    for (index, item) in std::fs::read_dir(path)?.enumerate() {
        if index % 64 == 0 && is_cancelled() {
            return Ok(None);
        }
        let Ok(item) = item else {
            continue;
        };
        let name = item.file_name().to_string_lossy().trim().to_string();
        if name.is_empty() {
            continue;
        }

        if decorate_trash_metadata {
            if let Ok(metadata) = item.metadata() {
                let item_path = item.path();
                let mut data = complete_entry_data(name, &item_path, metadata, mime_database);
                decorate_trash_entry(&mut data, &item_path);
                entries.push(Entry::new(data));
            }
        } else {
            let is_dir = item
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false);
            entries.push(Entry::new(incomplete_entry_data(
                name,
                is_dir,
                mime_database,
            )));
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
    let mut data = complete_entry_data(name, &item_path, metadata, MimeDatabase::shared());
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
    entry.trash_original_path = Some(metadata.original_path);
    entry.trash_deletion_time = metadata.deletion_date.map(Arc::from);
}

pub fn format_trash_original_location(original_path: &Path, deletion_time: Option<&str>) -> String {
    let original_location = original_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(original_path);
    match deletion_time {
        Some(date) => format!(
            "Original: {} - Deleted: {}",
            original_location.display(),
            format_trash_deletion_time(date)
        ),
        None => format!("Original: {}", original_location.display()),
    }
}

pub fn format_trash_deletion_time(value: &str) -> String {
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
                .trash_deletion_time
                .as_deref()
                .unwrap_or_default()
                .cmp(left.trash_deletion_time.as_deref().unwrap_or_default())
        })
        .then_with(|| left.sort_cmp(right))
}

pub(crate) fn entry_name_cmp(left: &str, right: &str) -> Ordering {
    ascii_case_insensitive_cmp(left, right).then_with(|| left.cmp(right))
}

fn trash_sort_bucket(entry: &Entry) -> u8 {
    if entry.trash_deletion_time.is_some() {
        0
    } else if entry.trash_original_path.is_some() {
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

fn incomplete_entry_data(name: String, is_dir: bool, mime: &MimeDatabase) -> EntryData {
    let name_width_units = name_width_units(&name);
    let mime_type = Some(mime.mime_for_name(&name, is_dir, None));
    let mime_magic_checked = is_dir || mime_type.as_deref() != Some(GENERIC_BINARY_MIME);

    EntryData {
        name: Arc::from(name),
        name_width_units,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: false,
        mime_type,
        mime_magic_checked,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir,
    }
}

fn complete_entry_data(
    name: String,
    path: &Path,
    metadata: Metadata,
    mime: &MimeDatabase,
) -> EntryData {
    let is_dir = metadata.is_dir();
    let name_width_units = name_width_units(&name);
    let role = EntryMetadataRole::resolved_from_path(&name, path, is_dir, &metadata, mime);

    EntryData {
        name: Arc::from(name),
        name_width_units,
        size_bytes: role.size_bytes,
        modified_secs: role.modified_secs,
        metadata_complete: true,
        mime_type: role.mime_type,
        mime_magic_checked: role.mime_magic_checked,
        trash_original_path: None,
        trash_deletion_time: None,
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
    use std::fs;

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

    #[test]
    fn extensionless_entry_resolves_magic_mime_in_metadata_role() {
        let dir = std::env::temp_dir().join(format!(
            "fika-entry-mime-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _guard = DirGuard(dir.clone());
        let path = dir.join("payload");
        fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();

        let entry = read_entry_sync(&dir, &path).unwrap();

        assert_eq!(entry.mime_type.as_deref(), Some("image/png"));
        assert!(entry.mime_magic_checked);
        assert!(entry.metadata_complete);
    }

    #[test]
    fn model_entry_defers_thumbnail_path_to_visible_role_update() {
        let dir = std::env::temp_dir().join(format!(
            "fika-entry-thumbnail-deferred-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _guard = DirGuard(dir.clone());
        let path = dir.join("image.png");
        fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();

        let entry = Entry::new(complete_entry_data(
            "image.png".to_string(),
            &path,
            fs::metadata(&path).unwrap(),
            MimeDatabase::shared(),
        ));
        let model_entry = ModelEntry::unassigned(entry);

        assert_eq!(model_entry.thumbnail_path, None);
    }

    #[test]
    fn ordinary_listing_defers_full_metadata_to_visible_role_update() {
        let dir = std::env::temp_dir().join(format!(
            "fika-entry-lazy-metadata-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _guard = DirGuard(dir.clone());
        fs::write(dir.join("payload.txt"), b"payload").unwrap();
        fs::create_dir(dir.join("folder")).unwrap();

        let entries = read_entries_sync(&dir).unwrap();
        let file = entries
            .iter()
            .find(|entry| entry.name.as_ref() == "payload.txt")
            .unwrap();
        let folder = entries
            .iter()
            .find(|entry| entry.name.as_ref() == "folder")
            .unwrap();

        assert!(!file.metadata_complete);
        assert_eq!(file.size_bytes, 0);
        assert_eq!(file.modified_secs, None);
        assert_eq!(file.mime_type.as_deref(), Some("text/plain"));
        assert!(file.mime_magic_checked);
        assert!(!folder.metadata_complete);
        assert!(folder.is_dir);
        assert_eq!(folder.mime_type.as_deref(), Some("inode/directory"));
    }

    #[test]
    fn trash_listing_reads_original_path_and_deletion_time_from_trashinfo() {
        let files_dir = file_ops::trash_files_dir();
        let info_dir = file_ops::trash_info_dir();
        file_ops::ensure_trash_dirs().unwrap();

        let unique = format!(
            "fika-trash-metadata-{}-{}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let trash_path = files_dir.join(&unique);
        let info_path = info_dir.join(format!("{unique}.trashinfo"));
        struct TrashTestGuard {
            trash_path: PathBuf,
            info_path: PathBuf,
        }
        impl Drop for TrashTestGuard {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.trash_path);
                let _ = fs::remove_file(&self.info_path);
            }
        }
        let _guard = TrashTestGuard {
            trash_path: trash_path.clone(),
            info_path: info_path.clone(),
        };
        let original_path = PathBuf::from(format!("/tmp/fika original {unique}"));
        fs::write(&trash_path, "trashed").unwrap();
        fs::write(
            &info_path,
            format!(
                "[Trash Info]\nPath=/tmp/fika%20original%20{unique}\nDeletionDate=2026-06-02T10:11:12\n"
            ),
        )
        .unwrap();

        let entry = read_entry_sync(&files_dir, &trash_path).unwrap();

        assert_eq!(entry.name.as_ref(), unique);
        assert_eq!(
            entry.trash_original_path.as_deref(),
            Some(original_path.as_path())
        );
        assert_eq!(
            entry.trash_deletion_time.as_deref(),
            Some("2026-06-02T10:11:12")
        );
    }
}
