use super::entries::{Entry, ItemId, ModelEntry, directory_entry_path, entry_name_cmp};
use super::file_ops;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRange {
    pub start: usize,
    pub len: usize,
}

pub type ItemRangeList = Vec<ItemRange>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangedRoles {
    pub metadata: bool,
    pub name: bool,
    pub path: bool,
}

impl ChangedRoles {
    pub const ALL: Self = Self {
        metadata: true,
        name: true,
        path: true,
    };

    pub fn metadata() -> Self {
        Self {
            metadata: true,
            name: false,
            path: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryModelSignal {
    ItemsInserted(ItemRangeList),
    ItemsRemoved(ItemRangeList),
    ItemsChanged(ItemRangeList, ChangedRoles),
    ItemsMoved(Vec<(usize, usize)>),
    GroupsChanged,
    SortChanged,
    ModelReset,
}

#[derive(Debug)]
pub struct DirectoryModel {
    data: DirectoryModelData,
    next_item_id: u64,
    data_generation: u64,
    path_index: RefCell<PathIndexCache>,
}

#[derive(Clone, Debug, Default)]
struct DirectoryModelData {
    directory: PathBuf,
    entries: Vec<ModelEntry>,
}

#[derive(Clone, Debug, Default)]
struct PathIndexCache {
    generation: u64,
    indexed_until: usize,
    index_by_name: HashMap<Arc<str>, usize>,
}

impl Default for DirectoryModel {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryModel {
    pub fn new() -> Self {
        Self::for_directory(PathBuf::new())
    }

    pub fn for_directory(directory: PathBuf) -> Self {
        Self {
            data: DirectoryModelData {
                directory,
                entries: Vec::new(),
            },
            next_item_id: 0,
            data_generation: 0,
            path_index: RefCell::new(PathIndexCache::default()),
        }
    }

    pub fn fork_for_pane(&self) -> Self {
        Self {
            data: self.data.clone(),
            next_item_id: self.next_item_id,
            data_generation: self.data_generation,
            path_index: RefCell::new(PathIndexCache::default()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.data.directory
    }

    pub fn entries(&self) -> &[ModelEntry] {
        &self.data.entries
    }

    pub fn len(&self) -> usize {
        self.data.entries.len()
    }

    pub fn data_generation(&self) -> u64 {
        self.data_generation
    }

    pub fn is_empty(&self) -> bool {
        self.data.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&ModelEntry> {
        self.data.entries.get(index)
    }

    pub fn path_for_index(&self, index: usize) -> Option<PathBuf> {
        self.data
            .entries
            .get(index)
            .map(|entry| self.path_for_entry(entry))
    }

    pub fn path_for_entry(&self, entry: &ModelEntry) -> PathBuf {
        self.data.directory.join(entry.name.as_ref())
    }

    pub fn index_of_path(&self, path: &Path) -> Option<usize> {
        let item_path = directory_entry_path(&self.data.directory, path)?;
        let name = item_path.file_name()?.to_string_lossy();
        self.index_of_name(name.as_ref())
    }

    pub fn index_of_id(&self, id: ItemId) -> Option<usize> {
        self.data.entries.iter().position(|entry| entry.id == id)
    }

    pub fn clear_for_directory(&mut self, directory: PathBuf) -> Vec<DirectoryModelSignal> {
        if self.data.directory == directory && self.data.entries.is_empty() {
            return Vec::new();
        }
        self.replace_data(directory, Vec::new());
        vec![DirectoryModelSignal::ModelReset]
    }

    pub fn replace_listing(
        &mut self,
        directory: PathBuf,
        entries: Arc<Vec<Entry>>,
    ) -> Vec<DirectoryModelSignal> {
        let mut entries = entries
            .iter()
            .cloned()
            .map(ModelEntry::unassigned)
            .collect::<Vec<_>>();
        sort_model_entries(&mut entries, file_ops::is_trash_files_dir(&directory));
        self.assign_listing_identity(&directory, &mut entries);
        if self.same_listing(&directory, &entries) {
            self.replace_data(directory, entries);
            return vec![DirectoryModelSignal::ItemsChanged(
                range_all(self.data.entries.len()),
                ChangedRoles::metadata(),
            )];
        }
        self.replace_data(directory, entries);
        vec![DirectoryModelSignal::ModelReset]
    }

    pub fn apply_items_added(&mut self, entries: Vec<Entry>) -> Vec<DirectoryModelSignal> {
        if entries.is_empty() {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let mut added = Vec::new();
        for entry in entries {
            if let Some(index) = self.index_of_entry_name(entry.name.as_ref()) {
                let mut entry = ModelEntry::unassigned(entry);
                self.assign_identity_from_index(&mut entry, index);
                self.data_mut().entries[index] = entry;
                changed.push(index);
            } else {
                let mut entry = ModelEntry::unassigned(entry);
                self.assign_new_identity(&mut entry);
                added.push(entry);
            }
        }
        {
            let data = self.data_mut();
            data.entries.extend(added);
            sort_model_entries(
                &mut data.entries,
                file_ops::is_trash_files_dir(&data.directory),
            );
        }
        self.mark_data_changed();

        let mut signals = Vec::new();
        if !changed.is_empty() {
            signals.push(DirectoryModelSignal::ItemsChanged(
                ranges_from_indexes(changed),
                ChangedRoles::ALL,
            ));
        }
        signals.push(DirectoryModelSignal::ModelReset);
        signals
    }

    pub fn apply_items_deleted(&mut self, paths: &[PathBuf]) -> Vec<DirectoryModelSignal> {
        if paths.is_empty() {
            return Vec::new();
        }
        let removed_indexes = paths
            .iter()
            .filter_map(|path| self.index_of_path(path))
            .collect::<Vec<_>>();
        if removed_indexes.is_empty() {
            return Vec::new();
        }
        let deleted = removed_indexes.iter().copied().collect::<BTreeSet<_>>();
        let mut index = 0usize;
        self.data_mut().entries.retain(|_| {
            let keep = !deleted.contains(&index);
            index += 1;
            keep
        });
        self.mark_data_changed();
        vec![DirectoryModelSignal::ItemsRemoved(ranges_from_indexes(
            removed_indexes,
        ))]
    }

    pub fn apply_items_refreshed(
        &mut self,
        pairs: Vec<super::directory::RefreshPair>,
    ) -> Vec<DirectoryModelSignal> {
        if pairs.is_empty() {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let mut removed = Vec::new();
        let mut added = Vec::new();

        for pair in pairs {
            match pair.entry {
                Some(entry) => {
                    if let Some(index) = self.index_of_path(&pair.old_path) {
                        let mut entry = ModelEntry::unassigned(entry);
                        self.assign_identity_from_index(&mut entry, index);
                        self.data_mut().entries[index] = entry;
                        changed.push(index);
                    } else {
                        added.push(entry);
                    }
                }
                None => {
                    removed.push(pair.old_path);
                }
            }
        }

        let mut signals = Vec::new();
        if !removed.is_empty() {
            signals.extend(self.apply_items_deleted(&removed));
        }
        if !added.is_empty() {
            signals.extend(self.apply_items_added(added));
        }
        if !changed.is_empty() {
            let trash = file_ops::is_trash_files_dir(&self.data.directory);
            let old_order = self
                .data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            sort_model_entries(&mut self.data_mut().entries, trash);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            self.mark_data_changed();
            if order_changed {
                signals.push(DirectoryModelSignal::ModelReset);
            } else {
                signals.push(DirectoryModelSignal::ItemsChanged(
                    ranges_from_indexes(changed),
                    ChangedRoles::ALL,
                ));
            }
        }
        signals
    }

    fn same_listing(&self, directory: &Path, entries: &[ModelEntry]) -> bool {
        self.data.directory == directory
            && self.data.entries.len() == entries.len()
            && self
                .data
                .entries
                .iter()
                .zip(entries)
                .all(|(left, right)| left.name == right.name && left.is_dir == right.is_dir)
    }

    fn assign_listing_identity(&mut self, directory: &Path, entries: &mut [ModelEntry]) {
        if self.data.directory != directory {
            for entry in entries {
                self.assign_new_identity(entry);
            }
            return;
        }

        if file_ops::is_trash_files_dir(directory) {
            self.assign_listing_identity_by_name(entries);
            return;
        }

        let mut old_index = 0usize;
        for entry in entries {
            if entry.id.is_assigned() {
                self.next_item_id = self.next_item_id.max(entry.id.0);
                continue;
            }

            let mut reused_id = None;
            while let Some(old_entry) = self.data.entries.get(old_index) {
                match identity_sort_cmp(old_entry, entry) {
                    std::cmp::Ordering::Less => old_index += 1,
                    std::cmp::Ordering::Equal => {
                        reused_id = Some(old_entry.id);
                        old_index += 1;
                        break;
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }

            entry.id = reused_id.unwrap_or_else(|| self.allocate_item_id());
        }
    }

    fn assign_listing_identity_by_name(&mut self, entries: &mut [ModelEntry]) {
        let mut old_indexes = (0..self.data.entries.len()).collect::<Vec<_>>();
        old_indexes.sort_by(|left, right| {
            entry_name_cmp(
                &self.data.entries[*left].name,
                &self.data.entries[*right].name,
            )
        });
        let mut new_indexes = (0..entries.len()).collect::<Vec<_>>();
        new_indexes
            .sort_by(|left, right| entry_name_cmp(&entries[*left].name, &entries[*right].name));

        let mut old_cursor = 0usize;
        for new_index in new_indexes {
            if entries[new_index].id.is_assigned() {
                self.next_item_id = self.next_item_id.max(entries[new_index].id.0);
                continue;
            }

            let mut reused_id = None;
            while let Some(old_index) = old_indexes.get(old_cursor).copied() {
                match entry_name_cmp(&self.data.entries[old_index].name, &entries[new_index].name) {
                    std::cmp::Ordering::Less => old_cursor += 1,
                    std::cmp::Ordering::Equal => {
                        reused_id = Some(self.data.entries[old_index].id);
                        old_cursor += 1;
                        break;
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }
            entries[new_index].id = reused_id.unwrap_or_else(|| self.allocate_item_id());
        }
    }

    fn assign_identity_from_index(&mut self, entry: &mut ModelEntry, index: usize) {
        if entry.id.is_assigned() {
            self.next_item_id = self.next_item_id.max(entry.id.0);
        } else {
            entry.id = self
                .data
                .entries
                .get(index)
                .map(|entry| entry.id)
                .unwrap_or_else(|| self.allocate_item_id());
        }
    }

    fn assign_new_identity(&mut self, entry: &mut ModelEntry) {
        if entry.id.is_assigned() {
            self.next_item_id = self.next_item_id.max(entry.id.0);
        } else {
            entry.id = self.allocate_item_id();
        }
    }

    fn replace_data(&mut self, directory: PathBuf, entries: Vec<ModelEntry>) {
        self.data = DirectoryModelData { directory, entries };
        self.data_generation = self.data_generation.wrapping_add(1);
        self.reset_path_index();
    }

    fn data_mut(&mut self) -> &mut DirectoryModelData {
        &mut self.data
    }

    fn mark_data_changed(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.reset_path_index();
    }

    fn index_of_entry_name(&self, name: &str) -> Option<usize> {
        self.index_of_name(name)
    }

    fn index_of_name(&self, name: &str) -> Option<usize> {
        const INDEX_BLOCK_SIZE: usize = 1000;

        let mut cache = self.path_index.borrow_mut();
        cache.prepare(self.data_generation, self.data.entries.len());

        loop {
            if let Some(index) = cache.index_by_name.get(name).copied() {
                return Some(index);
            }

            if cache.indexed_until >= self.data.entries.len() {
                return None;
            }

            let end = (cache.indexed_until + INDEX_BLOCK_SIZE).min(self.data.entries.len());
            for index in cache.indexed_until..end {
                cache
                    .index_by_name
                    .insert(Arc::clone(&self.data.entries[index].name), index);
            }
            cache.indexed_until = end;
        }
    }

    fn reset_path_index(&self) {
        self.path_index.borrow_mut().reset();
    }

    fn allocate_item_id(&mut self) -> ItemId {
        self.next_item_id += 1;
        ItemId(self.next_item_id)
    }
}

impl PathIndexCache {
    fn prepare(&mut self, generation: u64, len: usize) {
        if self.generation == generation {
            return;
        }

        self.generation = generation;
        self.indexed_until = 0;
        if self.index_by_name.capacity() > len.saturating_mul(2).max(2048) {
            self.index_by_name = std::collections::HashMap::new();
        } else {
            self.index_by_name.clear();
        }
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.indexed_until = 0;
        if self.index_by_name.capacity() > 2048 {
            self.index_by_name = std::collections::HashMap::new();
        } else {
            self.index_by_name.clear();
        }
    }
}

fn identity_sort_cmp(left: &ModelEntry, right: &ModelEntry) -> std::cmp::Ordering {
    match right.is_dir.cmp(&left.is_dir) {
        std::cmp::Ordering::Equal => entry_name_cmp(&left.name, &right.name),
        ordering => ordering,
    }
}

fn sort_model_entries(entries: &mut [ModelEntry], trash: bool) {
    if trash {
        entries.sort_by(trash_sort_cmp);
    } else {
        entries.sort_by(ModelEntry::sort_cmp);
    }
}

fn trash_sort_cmp(left: &ModelEntry, right: &ModelEntry) -> std::cmp::Ordering {
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

fn trash_sort_bucket(entry: &ModelEntry) -> u8 {
    if entry.trash_deletion_time.is_some() {
        0
    } else if entry.trash_original_path.is_some() {
        1
    } else {
        2
    }
}

fn range_all(len: usize) -> ItemRangeList {
    if len == 0 {
        Vec::new()
    } else {
        vec![ItemRange { start: 0, len }]
    }
}

fn ranges_from_indexes(mut indexes: Vec<usize>) -> ItemRangeList {
    indexes.sort_unstable();
    indexes.dedup();
    let mut ranges: ItemRangeList = Vec::new();
    for index in indexes {
        if let Some(last) = ranges.last_mut()
            && last.start + last.len == index
        {
            last.len += 1;
            continue;
        }
        ranges.push(ItemRange {
            start: index,
            len: 1,
        });
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entries::EntryData;

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn trash_entry(name: &str, original_path: &str, deletion_time: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            trash_original_path: Some(PathBuf::from(original_path)),
            trash_deletion_time: Some(Arc::from(deletion_time)),
            is_dir: false,
        })
    }

    fn listing(entries: Vec<Entry>) -> Arc<Vec<Entry>> {
        Arc::new(entries)
    }

    #[test]
    fn listing_reset_rebuilds_path_index() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        let signals = model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("b.txt", false), entry("a", true)]),
        );

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        assert_eq!(model.entries()[0].name.as_ref(), "a");
        assert_eq!(model.path_for_index(1), Some(PathBuf::from("/tmp/b.txt")));
        assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
    }

    #[test]
    fn path_index_reuses_entry_name_storage() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("b.txt", false), entry("a", true)]),
        );

        assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));

        let cache = model.path_index.borrow();
        let indexed_name = cache
            .index_by_name
            .keys()
            .find(|name| name.as_ref() == "b.txt")
            .expect("indexed file name missing");
        assert!(Arc::ptr_eq(indexed_name, &model.entries()[1].name));
    }

    #[test]
    fn delete_emits_removed_ranges() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry("a", false),
                entry("b", false),
                entry("c", false),
            ]),
        );

        let signals =
            model.apply_items_deleted(&[PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsRemoved(vec![ItemRange {
                start: 0,
                len: 2
            }])]
        );
        assert_eq!(model.entries()[0].name.as_ref(), "c");
    }

    #[test]
    fn full_reload_retains_item_identity_for_same_path() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("a.txt", false), entry("b.txt", false)]),
        );
        let original_a = model.entries()[0].id;
        let original_b = model.entries()[1].id;

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("a.txt", false), entry("b.txt", false)]),
        );

        assert_eq!(model.entries()[0].id, original_a);
        assert_eq!(model.entries()[1].id, original_b);
        assert_eq!(model.index_of_id(original_a), Some(0));
        assert_eq!(model.index_of_id(original_b), Some(1));
    }

    #[test]
    fn split_models_share_listing_payload_without_entry_level_identity() {
        let listing = listing(vec![entry("shared.txt", false)]);
        let mut first = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        let mut second = DirectoryModel::for_directory(PathBuf::from("/tmp"));

        first.replace_listing(PathBuf::from("/tmp"), Arc::clone(&listing));
        second.replace_listing(PathBuf::from("/tmp"), Arc::clone(&listing));

        assert!(first.entries()[0].id.is_assigned());
        assert!(second.entries()[0].id.is_assigned());
        assert!(Entry::ptr_eq(
            &first.entries()[0].entry,
            &second.entries()[0].entry
        ));
    }

    #[test]
    fn fork_for_pane_shares_payload_but_not_model_entries() {
        let mut source = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        source.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("a.txt", false), entry("b.txt", false)]),
        );

        let mut fork = source.fork_for_pane();
        assert_eq!(fork.len(), source.len());
        assert!(Entry::ptr_eq(
            &source.entries()[0].entry,
            &fork.entries()[0].entry
        ));
        assert!(Entry::ptr_eq(
            &source.entries()[1].entry,
            &fork.entries()[1].entry
        ));

        fork.apply_items_deleted(&[PathBuf::from("/tmp/a.txt")]);

        assert_eq!(source.len(), 2);
        assert_eq!(fork.len(), 1);
        assert_eq!(source.index_of_path(Path::new("/tmp/a.txt")), Some(0));
        assert_eq!(fork.index_of_path(Path::new("/tmp/a.txt")), None);
        assert!(Entry::ptr_eq(
            &source.entries()[1].entry,
            &fork.entries()[0].entry
        ));
    }

    #[test]
    fn refresh_rename_retains_item_identity_from_old_path() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("old.txt", false)]),
        );
        let original = model.entries()[0].id;

        model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
            old_path: PathBuf::from("/tmp/old.txt"),
            entry: Some(entry("new.txt", false)),
        }]);

        assert_eq!(model.entries()[0].id, original);
        assert_eq!(model.path_for_index(0), Some(PathBuf::from("/tmp/new.txt")));
        assert_eq!(model.index_of_path(Path::new("/tmp/old.txt")), None);
        assert_eq!(model.index_of_id(original), Some(0));
    }

    #[test]
    fn trash_listing_sorts_by_deletion_time_and_retains_identity_after_reload() {
        let trash_dir = file_ops::trash_files_dir();
        let mut model = DirectoryModel::for_directory(trash_dir.clone());
        model.replace_listing(
            trash_dir.clone(),
            listing(vec![
                trash_entry("old.txt", "/tmp/old.txt", "2026-06-01T10:00:00"),
                trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
            ]),
        );
        let new_id = model.entries()[0].id;
        let old_id = model.entries()[1].id;

        assert_eq!(model.entries()[0].name.as_ref(), "new.txt");
        assert_eq!(model.entries()[1].name.as_ref(), "old.txt");

        let signals = model.replace_listing(
            trash_dir.clone(),
            listing(vec![
                trash_entry("old.txt", "/tmp/old.txt", "2026-06-05T10:00:00"),
                trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
            ]),
        );

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        assert_eq!(model.entries()[0].name.as_ref(), "old.txt");
        assert_eq!(model.entries()[0].id, old_id);
        assert_eq!(model.entries()[1].name.as_ref(), "new.txt");
        assert_eq!(model.entries()[1].id, new_id);
    }

    #[test]
    fn trash_metadata_refresh_resorts_and_keeps_item_identity() {
        let trash_dir = file_ops::trash_files_dir();
        let mut model = DirectoryModel::for_directory(trash_dir.clone());
        model.replace_listing(
            trash_dir.clone(),
            listing(vec![
                trash_entry("old.txt", "/tmp/old.txt", "2026-06-01T10:00:00"),
                trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
            ]),
        );
        let old_id = model.entries()[1].id;

        let signals = model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
            old_path: trash_dir.join("old.txt"),
            entry: Some(trash_entry(
                "old.txt",
                "/tmp/old.txt",
                "2026-06-05T10:00:00",
            )),
        }]);

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        assert_eq!(model.entries()[0].name.as_ref(), "old.txt");
        assert_eq!(model.entries()[0].id, old_id);
        assert_eq!(
            model.entries()[0].trash_deletion_time.as_deref(),
            Some("2026-06-05T10:00:00")
        );
    }
}
