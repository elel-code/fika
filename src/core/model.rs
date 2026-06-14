use super::entries::{
    Entry, EntryMetadataRole, ItemId, ModelEntry, directory_entry_path, entry_name_cmp,
};
use super::file_ops;
use super::mime::mime_magic_resolution_required;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::ptr;
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SortRole {
    Name,
    Modified,
    Size,
    TrashOriginalPath,
    TrashDeletionTime,
}

impl SortRole {
    pub fn default_order(self) -> SortOrder {
        match self {
            Self::Name | Self::TrashOriginalPath => SortOrder::Ascending,
            Self::Modified | Self::Size | Self::TrashDeletionTime => SortOrder::Descending,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortDescriptor {
    pub role: SortRole,
    pub order: SortOrder,
    pub folders_first: bool,
    pub hidden_last: bool,
}

impl SortDescriptor {
    pub fn for_directory(directory: &Path) -> Self {
        if file_ops::is_trash_files_dir(directory) {
            Self {
                role: SortRole::TrashDeletionTime,
                order: SortOrder::Descending,
                folders_first: true,
                hidden_last: false,
            }
        } else {
            Self::default()
        }
    }
}

impl Default for SortDescriptor {
    fn default() -> Self {
        Self {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: false,
        }
    }
}

#[derive(Debug)]
pub struct DirectoryModel {
    data: DirectoryModelData,
    next_item_id: u64,
    data_generation: u64,
    index_generation: u64,
    path_index: RefCell<PathIndexCache>,
    id_index: RefCell<ItemIdIndexCache>,
}

#[derive(Clone, Debug, Default)]
struct DirectoryModelData {
    directory: PathBuf,
    entries: Vec<ModelEntry>,
    sort: SortDescriptor,
}

#[derive(Clone, Debug, Default)]
struct PathIndexCache {
    generation: u64,
    indexed_until: usize,
    index_by_name: HashMap<Arc<str>, usize>,
}

#[derive(Clone, Debug, Default)]
struct ItemIdIndexCache {
    generation: u64,
    indexed_until: usize,
    index_by_id: HashMap<ItemId, usize>,
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
        let sort = SortDescriptor::for_directory(&directory);
        Self {
            data: DirectoryModelData {
                directory,
                entries: Vec::new(),
                sort,
            },
            next_item_id: 0,
            data_generation: 0,
            index_generation: 0,
            path_index: RefCell::new(PathIndexCache::default()),
            id_index: RefCell::new(ItemIdIndexCache::default()),
        }
    }

    pub fn fork_for_pane(&self) -> Self {
        Self {
            data: self.data.clone(),
            next_item_id: self.next_item_id,
            data_generation: self.data_generation,
            index_generation: self.index_generation,
            path_index: RefCell::new(PathIndexCache::default()),
            id_index: RefCell::new(ItemIdIndexCache::default()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.data.directory
    }

    pub fn entries(&self) -> &[ModelEntry] {
        &self.data.entries
    }

    pub fn listing_snapshot(&self) -> Arc<Vec<Entry>> {
        Arc::new(
            self.data
                .entries
                .iter()
                .map(|entry| entry.entry.clone())
                .collect(),
        )
    }

    pub fn sort_descriptor(&self) -> SortDescriptor {
        self.data.sort
    }

    pub fn len(&self) -> usize {
        self.data.entries.len()
    }

    pub fn data_generation(&self) -> u64 {
        self.data_generation
    }

    pub fn structure_generation(&self) -> u64 {
        self.index_generation
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
        if !id.is_assigned() {
            return None;
        }
        self.index_of_item_id(id)
    }

    pub fn set_thumbnail_path(
        &mut self,
        id: ItemId,
        thumbnail_path: Option<PathBuf>,
    ) -> Vec<DirectoryModelSignal> {
        let Some(index) = self.index_of_id(id) else {
            return Vec::new();
        };
        if self.data.entries[index].thumbnail_path == thumbnail_path {
            return Vec::new();
        }

        self.data.entries[index].thumbnail_path = thumbnail_path;
        self.mark_metadata_changed();
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange {
                start: index,
                len: 1,
            }],
            ChangedRoles::metadata(),
        )]
    }

    pub fn set_metadata_role(
        &mut self,
        id: ItemId,
        path: &Path,
        role: EntryMetadataRole,
    ) -> Vec<DirectoryModelSignal> {
        let Some(index) = self.index_of_id(id) else {
            return Vec::new();
        };
        if self.path_for_index(index).as_deref() != Some(path) {
            return Vec::new();
        }

        if self.data.entries[index].effective_metadata_role() == role {
            let normalized_role =
                (!metadata_role_matches_base_entry(&self.data.entries[index], &role))
                    .then_some(role);
            if self.data.entries[index].metadata_refresh_pending
                || self.data.entries[index].metadata_role != normalized_role
            {
                self.data.entries[index].metadata_role = normalized_role;
                self.data.entries[index].metadata_refresh_pending = false;
                self.mark_metadata_changed();
            }
            return Vec::new();
        }

        let old_order = metadata_sort_role_needs_resort(self.data.sort).then(|| {
            self.data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>()
        });
        let had_reusable_thumbnail = self.data.entries[index].thumbnail_path.is_some();
        let old_size = self.data.entries[index].effective_size_bytes();
        let old_modified = self.data.entries[index].effective_modified_secs();
        self.data.entries[index].metadata_role =
            (!metadata_role_matches_base_entry(&self.data.entries[index], &role))
                .then_some(role.clone());
        self.data.entries[index].metadata_refresh_pending = false;
        if had_reusable_thumbnail
            && (old_size != self.data.entries[index].effective_size_bytes()
                || old_modified != self.data.entries[index].effective_modified_secs())
        {
            self.data.entries[index].thumbnail_path = None;
        }

        if let Some(old_order) = old_order {
            let sort = self.data.sort;
            sort_model_entries(&mut self.data.entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if order_changed {
                self.mark_structure_changed();
                return vec![DirectoryModelSignal::ModelReset];
            }
        }

        let index = self.index_of_id(id).unwrap_or(index);
        self.mark_metadata_changed();
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange {
                start: index,
                len: 1,
            }],
            ChangedRoles::metadata(),
        )]
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
        let sort = if self.data.directory == directory {
            self.data.sort
        } else {
            SortDescriptor::for_directory(&directory)
        };
        self.assign_listing_identity(&directory, &mut entries);
        sort_model_entries(&mut entries, sort);
        if self.same_listing(&directory, &entries) {
            self.replace_same_listing_metadata(entries);
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
        let mut changed_structure = false;
        let mut added = Vec::new();
        for entry in entries {
            if let Some(index) = self.index_of_entry_name(entry.name.as_ref()) {
                let mut entry = ModelEntry::unassigned(entry);
                let old = self.data.entries[index].clone();
                self.assign_identity_from_index(&mut entry, index);
                changed_structure |= old.is_dir != entry.is_dir;
                self.data_mut().entries[index] = entry;
                changed.push(index);
            } else {
                let mut entry = ModelEntry::unassigned(entry);
                self.assign_new_identity(&mut entry);
                added.push(entry);
            }
        }
        if !added.is_empty() {
            let sort = self.data.sort;
            if !changed.is_empty() {
                let data = self.data_mut();
                data.entries.extend(added);
                sort_model_entries(&mut data.entries, sort);
                self.mark_structure_changed();
                return vec![DirectoryModelSignal::ModelReset];
            }
            sort_model_entries(&mut added, sort);
            let item_ranges = self.insert_sorted_model_entries(added);
            self.mark_structure_changed();
            return vec![DirectoryModelSignal::ItemsInserted(item_ranges)];
        }

        if !changed.is_empty() {
            let sort = self.data.sort;
            let old_order = self
                .data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            sort_model_entries(&mut self.data_mut().entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if changed_structure || order_changed {
                self.mark_structure_changed();
                vec![DirectoryModelSignal::ModelReset]
            } else {
                self.mark_metadata_changed();
                vec![DirectoryModelSignal::ItemsChanged(
                    ranges_from_indexes(changed),
                    ChangedRoles::ALL,
                )]
            }
        } else {
            Vec::new()
        }
    }

    fn insert_sorted_model_entries(&mut self, added: Vec<ModelEntry>) -> ItemRangeList {
        if added.is_empty() {
            return Vec::new();
        }

        if self.data.entries.is_empty() {
            let len = added.len();
            self.data.entries = added;
            return vec![ItemRange { start: 0, len }];
        }

        let sort = self.data.sort;
        merge_sorted_model_entries_in_place(&mut self.data.entries, added, sort)
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
        self.mark_structure_changed();
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
        let mut changed_structure = false;
        let mut removed = Vec::new();
        let mut added = Vec::new();

        for pair in pairs {
            match pair.entry {
                Some(entry) => {
                    if let Some(index) = self.index_of_path(&pair.old_path) {
                        let mut entry = ModelEntry::unassigned(entry);
                        let old = self.data.entries[index].clone();
                        self.assign_identity_from_index(&mut entry, index);
                        changed_structure |= old.name != entry.name || old.is_dir != entry.is_dir;
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
            let sort = self.data.sort;
            let old_order = self
                .data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            sort_model_entries(&mut self.data_mut().entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if changed_structure || order_changed {
                self.mark_structure_changed();
                signals.push(DirectoryModelSignal::ModelReset);
            } else {
                self.mark_metadata_changed();
                signals.push(DirectoryModelSignal::ItemsChanged(
                    ranges_from_indexes(changed),
                    ChangedRoles::ALL,
                ));
            }
        }
        signals
    }

    pub fn set_sort(&mut self, sort: SortDescriptor) -> Vec<DirectoryModelSignal> {
        if self.data.sort == sort {
            return Vec::new();
        }
        self.data.sort = sort;
        sort_model_entries(&mut self.data.entries, sort);
        self.mark_structure_changed();
        vec![DirectoryModelSignal::SortChanged]
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

        self.assign_listing_identity_by_name(entries);
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

            let mut reused_index = None;
            while let Some(old_index) = old_indexes.get(old_cursor).copied() {
                match entry_name_cmp(&self.data.entries[old_index].name, &entries[new_index].name) {
                    std::cmp::Ordering::Less => old_cursor += 1,
                    std::cmp::Ordering::Equal => {
                        reused_index = Some(old_index);
                        old_cursor += 1;
                        break;
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }
            if let Some(old_index) = reused_index {
                entries[new_index].id = self.data.entries[old_index].id;
                preserve_refreshed_entry_roles(
                    &self.data.entries[old_index],
                    &mut entries[new_index],
                );
            } else {
                entries[new_index].id = self.allocate_item_id();
            }
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
        if let Some(old) = self.data.entries.get(index) {
            preserve_refreshed_entry_roles(old, entry);
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
        let sort = if self.data.directory == directory {
            self.data.sort
        } else {
            SortDescriptor::for_directory(&directory)
        };
        self.data = DirectoryModelData {
            directory,
            entries,
            sort,
        };
        self.data_generation = self.data_generation.wrapping_add(1);
        self.index_generation = self.index_generation.wrapping_add(1);
        self.reset_indexes();
    }

    fn replace_same_listing_metadata(&mut self, mut entries: Vec<ModelEntry>) {
        for (old, new) in self.data.entries.iter().zip(&mut entries) {
            let mut data = (*new.entry).clone();
            data.name = Arc::clone(&old.name);
            data.name_width_units = old.name_width_units;
            new.entry = Entry::new(data);
            new.id = old.id;
            preserve_refreshed_entry_roles(old, new);
        }
        self.data.entries = entries;
        self.mark_metadata_changed();
    }

    fn data_mut(&mut self) -> &mut DirectoryModelData {
        &mut self.data
    }

    fn mark_metadata_changed(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
    }

    fn mark_structure_changed(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.index_generation = self.index_generation.wrapping_add(1);
        self.reset_indexes();
    }

    fn index_of_entry_name(&self, name: &str) -> Option<usize> {
        self.index_of_name(name)
    }

    fn index_of_name(&self, name: &str) -> Option<usize> {
        const INDEX_BLOCK_SIZE: usize = 1000;

        let mut cache = self.path_index.borrow_mut();
        cache.prepare(self.index_generation, self.data.entries.len());

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

    fn index_of_item_id(&self, id: ItemId) -> Option<usize> {
        const INDEX_BLOCK_SIZE: usize = 1000;

        let mut cache = self.id_index.borrow_mut();
        cache.prepare(self.index_generation, self.data.entries.len());

        loop {
            if let Some(index) = cache.index_by_id.get(&id).copied() {
                return Some(index);
            }

            if cache.indexed_until >= self.data.entries.len() {
                return None;
            }

            let end = (cache.indexed_until + INDEX_BLOCK_SIZE).min(self.data.entries.len());
            for index in cache.indexed_until..end {
                let item_id = self.data.entries[index].id;
                if item_id.is_assigned() {
                    cache.index_by_id.insert(item_id, index);
                }
            }
            cache.indexed_until = end;
        }
    }

    fn reset_indexes(&self) {
        self.path_index.borrow_mut().reset();
        self.id_index.borrow_mut().reset();
    }

    fn allocate_item_id(&mut self) -> ItemId {
        self.next_item_id += 1;
        ItemId(self.next_item_id)
    }
}

fn reusable_thumbnail_path(old: &ModelEntry, new: &ModelEntry) -> Option<PathBuf> {
    if old.is_dir
        || new.is_dir
        || old.name != new.name
        || !old.effective_metadata_complete()
        || !new.effective_metadata_complete()
        || old.effective_size_bytes() != new.effective_size_bytes()
        || old.effective_modified_secs().is_none()
        || old.effective_modified_secs() != new.effective_modified_secs()
    {
        None
    } else {
        old.thumbnail_path.clone()
    }
}

fn preserve_refreshed_entry_roles(old: &ModelEntry, new: &mut ModelEntry) {
    preserve_pending_entry_metadata_role(old, new);
    new.thumbnail_path = reusable_thumbnail_path(old, new);
}

fn preserve_pending_entry_metadata_role(old: &ModelEntry, new: &mut ModelEntry) {
    if old.name != new.name
        || old.is_dir != new.is_dir
        || !old.effective_metadata_complete()
        || !new_requires_async_metadata_role(new)
    {
        return;
    }

    let old_role = old.effective_metadata_role();
    if !old_role.mime_magic_checked {
        return;
    }

    let role = base_metadata_role(new);
    if old_role.size_bytes == role.size_bytes && old_role.modified_secs == role.modified_secs {
        new.metadata_role = (!metadata_role_matches_base_entry(new, &old_role)).then_some(old_role);
    }
}

fn new_requires_async_metadata_role(entry: &ModelEntry) -> bool {
    !entry.is_dir
        && (!entry.entry.metadata_complete
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.entry.size_bytes,
                entry.entry.mime_type.as_deref(),
                entry.entry.mime_magic_checked,
            ))
}

fn base_metadata_role(entry: &ModelEntry) -> EntryMetadataRole {
    EntryMetadataRole {
        size_bytes: entry.entry.size_bytes,
        modified_secs: entry.entry.modified_secs,
        mime_type: entry.entry.mime_type.clone(),
        mime_magic_checked: entry.entry.mime_magic_checked,
    }
}

fn metadata_role_matches_base_entry(entry: &ModelEntry, role: &EntryMetadataRole) -> bool {
    entry.entry.metadata_complete && base_metadata_role(entry) == *role
}

fn metadata_sort_role_needs_resort(sort: SortDescriptor) -> bool {
    matches!(sort.role, SortRole::Modified | SortRole::Size)
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

impl ItemIdIndexCache {
    fn prepare(&mut self, generation: u64, len: usize) {
        if self.generation == generation {
            return;
        }

        self.generation = generation;
        self.indexed_until = 0;
        if self.index_by_id.capacity() > len.saturating_mul(2).max(2048) {
            self.index_by_id = std::collections::HashMap::new();
        } else {
            self.index_by_id.clear();
        }
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.indexed_until = 0;
        if self.index_by_id.capacity() > 2048 {
            self.index_by_id = std::collections::HashMap::new();
        } else {
            self.index_by_id.clear();
        }
    }
}

fn sort_model_entries(entries: &mut [ModelEntry], sort: SortDescriptor) {
    entries.sort_by(|left, right| sort_cmp(left, right, sort));
}

fn merge_sorted_model_entries_in_place(
    entries: &mut Vec<ModelEntry>,
    added: Vec<ModelEntry>,
    sort: SortDescriptor,
) -> ItemRangeList {
    debug_assert!(!added.is_empty());
    debug_assert!(!entries.is_empty());

    let existing_len = entries.len();
    let added_len = added.len();
    let total_len = existing_len + added_len;
    let mut inserted_ranges = Vec::with_capacity(added_len.min(existing_len + 1));

    entries.reserve(added_len);
    let added = ManuallyDrop::new(added);

    unsafe {
        // SAFETY: `entries` has enough spare capacity for `added_len` items.
        // Existing items are moved from the initialized prefix toward the tail
        // with `ptr::read` + `ptr::write`; every vacated source slot is either
        // overwritten by a later write or stays outside the final untouched
        // prefix. Every `added` item is read exactly once before `entries.len`
        // is extended to `total_len`, so both vectors have one owner per item.
        let entries_ptr = entries.as_mut_ptr();
        let added_ptr = added.as_ptr();
        let mut target = total_len;
        let mut existing = existing_len;
        let mut new = added_len;

        while new > 0 {
            let take_existing = existing > 0
                && sort_cmp(
                    &*added_ptr.add(new - 1),
                    &*entries_ptr.add(existing - 1),
                    sort,
                ) == Ordering::Less;

            target -= 1;
            if take_existing {
                existing -= 1;
                ptr::write(
                    entries_ptr.add(target),
                    ptr::read(entries_ptr.add(existing)),
                );
            } else {
                new -= 1;
                record_reverse_inserted_range(&mut inserted_ranges, target);
                ptr::write(entries_ptr.add(target), ptr::read(added_ptr.add(new)));
            }
        }

        entries.set_len(total_len);
    }

    inserted_ranges.reverse();
    inserted_ranges
}

fn record_reverse_inserted_range(ranges: &mut ItemRangeList, index: usize) {
    if let Some(range) = ranges.last_mut()
        && range.start == index + 1
    {
        range.start = index;
        range.len += 1;
        return;
    }
    ranges.push(ItemRange {
        start: index,
        len: 1,
    });
}

fn sort_cmp(left: &ModelEntry, right: &ModelEntry, sort: SortDescriptor) -> Ordering {
    if sort.hidden_last {
        match left_is_hidden(left).cmp(&left_is_hidden(right)) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    if sort.folders_first || sort.role == SortRole::Size {
        match right.is_dir.cmp(&left.is_dir) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    match sort.role {
        SortRole::TrashOriginalPath => trash_original_path_sort_cmp(left, right, sort.order),
        SortRole::TrashDeletionTime => trash_deletion_sort_cmp(left, right, sort.order),
        role => apply_sort_order(role_sort_cmp(left, right, role), sort.order)
            .then_with(|| entry_name_cmp(&left.name, &right.name))
            .then_with(|| {
                left.effective_size_bytes()
                    .cmp(&right.effective_size_bytes())
            }),
    }
}

fn left_is_hidden(entry: &ModelEntry) -> bool {
    entry.name.starts_with('.')
}

fn role_sort_cmp(left: &ModelEntry, right: &ModelEntry, role: SortRole) -> Ordering {
    match role {
        SortRole::Name => entry_name_cmp(&left.name, &right.name),
        SortRole::Modified => left
            .effective_modified_secs()
            .unwrap_or_default()
            .cmp(&right.effective_modified_secs().unwrap_or_default()),
        SortRole::Size => left
            .effective_size_bytes()
            .cmp(&right.effective_size_bytes()),
        SortRole::TrashOriginalPath => Ordering::Equal,
        SortRole::TrashDeletionTime => Ordering::Equal,
    }
}

fn apply_sort_order(ordering: Ordering, order: SortOrder) -> Ordering {
    match order {
        SortOrder::Ascending => ordering,
        SortOrder::Descending => ordering.reverse(),
    }
}

fn trash_deletion_sort_cmp(left: &ModelEntry, right: &ModelEntry, order: SortOrder) -> Ordering {
    trash_sort_bucket(left)
        .cmp(&trash_sort_bucket(right))
        .then_with(|| {
            let left = left.trash_deletion_time.as_deref().unwrap_or_default();
            let right = right.trash_deletion_time.as_deref().unwrap_or_default();
            apply_sort_order(left.cmp(right), order)
        })
        .then_with(|| left.sort_cmp(right))
}

fn trash_original_path_sort_cmp(
    left: &ModelEntry,
    right: &ModelEntry,
    order: SortOrder,
) -> Ordering {
    trash_sort_bucket(left)
        .cmp(&trash_sort_bucket(right))
        .then_with(|| {
            let left = trash_original_path_key(left);
            let right = trash_original_path_key(right);
            apply_sort_order(entry_name_cmp(left.as_ref(), right.as_ref()), order)
        })
        .then_with(|| left.sort_cmp(right))
}

fn trash_original_path_key(entry: &ModelEntry) -> std::borrow::Cow<'_, str> {
    let Some(path) = entry.trash_original_path.as_deref() else {
        return std::borrow::Cow::Borrowed("");
    };
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(path)
        .to_string_lossy()
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
    use crate::core::mime::GENERIC_BINARY_MIME;

    fn entry(name: &str, is_dir: bool) -> Entry {
        entry_with_metadata(name, is_dir, 0, None)
    }

    fn entry_with_metadata(
        name: &str,
        is_dir: bool,
        size_bytes: u64,
        modified_secs: Option<u64>,
    ) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes,
            modified_secs,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn entry_with_metadata_state(
        name: &str,
        is_dir: bool,
        size_bytes: u64,
        modified_secs: Option<u64>,
        metadata_complete: bool,
    ) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes,
            modified_secs,
            metadata_complete,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn entry_with_mime_state(
        name: &str,
        size_bytes: u64,
        modified_secs: Option<u64>,
        mime_type: &str,
        mime_magic_checked: bool,
    ) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes,
            modified_secs,
            metadata_complete: true,
            mime_type: Some(Arc::from(mime_type)),
            mime_magic_checked,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn trash_entry(name: &str, original_path: &str, deletion_time: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
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
    fn listing_snapshot_exports_entry_payload_without_item_identity() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("b.txt", false), entry("a", true)]),
        );
        let first_id = model.entries()[0].id;

        let snapshot = model.listing_snapshot();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].name.as_ref(), "a");
        assert_eq!(snapshot[1].name.as_ref(), "b.txt");
        assert!(Entry::ptr_eq(&snapshot[0], &model.entries()[0].entry));
        assert_eq!(model.entries()[0].id, first_id);
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
    fn item_id_index_survives_metadata_role_updates() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry("a.txt", false),
                entry("b.txt", false),
                entry("c.txt", false),
            ]),
        );
        let item_id = model.entries()[1].id;

        assert_eq!(model.index_of_id(item_id), Some(1));
        let indexed_until = model.id_index.borrow().indexed_until;
        let index_generation = model.index_generation;
        let data_generation = model.data_generation;

        model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/b.png")));

        assert!(model.data_generation > data_generation);
        assert_eq!(model.index_generation, index_generation);
        assert_eq!(model.id_index.borrow().indexed_until, indexed_until);
        assert_eq!(model.index_of_id(item_id), Some(1));
    }

    #[test]
    fn item_id_index_rebuilds_after_structural_changes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry("a.txt", false),
                entry("b.txt", false),
                entry("c.txt", false),
            ]),
        );
        let first_id = model.entries()[0].id;
        let second_id = model.entries()[1].id;

        assert_eq!(model.index_of_id(second_id), Some(1));
        assert!(!model.id_index.borrow().index_by_id.is_empty());

        model.apply_items_deleted(&[PathBuf::from("/tmp/a.txt")]);

        assert!(model.id_index.borrow().index_by_id.is_empty());
        assert_eq!(model.index_of_id(first_id), None);
        assert_eq!(model.index_of_id(second_id), Some(0));
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
    fn added_items_merge_into_sorted_model_with_inserted_ranges() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("b.txt", false), entry("d.txt", false)]),
        );

        let signals = model.apply_items_added(vec![
            entry("a.txt", false),
            entry("c.txt", false),
            entry("e.txt", false),
        ]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsInserted(vec![
                ItemRange { start: 0, len: 1 },
                ItemRange { start: 2, len: 1 },
                ItemRange { start: 4, len: 1 },
            ])]
        );
        let names = model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
    }

    #[test]
    fn first_added_batch_uses_inserted_range_without_model_reset() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));

        let signals = model.apply_items_added(vec![entry("b.txt", false), entry("a.txt", false)]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsInserted(vec![ItemRange {
                start: 0,
                len: 2,
            }])]
        );
        let names = model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn tail_added_batch_uses_single_final_inserted_range() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(PathBuf::from("/tmp"), listing(vec![entry("a.txt", false)]));

        let signals = model.apply_items_added(vec![entry("c.txt", false), entry("b.txt", false)]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsInserted(vec![ItemRange {
                start: 1,
                len: 2,
            }])]
        );
        let names = model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
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
    fn same_listing_reload_updates_metadata_without_rebuilding_indexes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("a.txt", false, 1, Some(10)),
                entry_with_metadata("b.txt", false, 2, Some(20)),
            ]),
        );
        let b_id = model.entries()[1].id;
        assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
        assert_eq!(model.index_of_id(b_id), Some(1));
        let indexed_name = model
            .path_index
            .borrow()
            .index_by_name
            .keys()
            .find(|name| name.as_ref() == "b.txt")
            .cloned()
            .expect("indexed file name missing");
        let path_indexed_until = model.path_index.borrow().indexed_until;
        let id_indexed_until = model.id_index.borrow().indexed_until;
        let index_generation = model.index_generation;

        let signals = model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("a.txt", false, 10, Some(100)),
                entry_with_metadata("b.txt", false, 20, Some(200)),
            ]),
        );

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 2 }],
                ChangedRoles::metadata(),
            )]
        );
        assert_eq!(model.index_generation, index_generation);
        assert_eq!(model.path_index.borrow().indexed_until, path_indexed_until);
        assert_eq!(model.id_index.borrow().indexed_until, id_indexed_until);
        assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
        assert_eq!(model.index_of_id(b_id), Some(1));
        assert_eq!(model.entries()[1].effective_size_bytes(), 20);
        assert_eq!(model.entries()[1].effective_modified_secs(), Some(200));
        assert!(Arc::ptr_eq(&indexed_name, &model.entries()[1].name));
    }

    #[test]
    fn same_listing_lightweight_reload_drops_stale_visible_metadata() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata(
                "notes.txt",
                false,
                512,
                Some(100),
            )]),
        );
        let item_id = model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/thumbs/notes.png");
        model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata_state(
                "notes.txt",
                false,
                0,
                None,
                false,
            )]),
        );

        let entry = &model.entries()[0];
        assert_eq!(entry.id, item_id);
        assert!(!entry.entry.metadata_complete);
        assert!(!entry.effective_metadata_complete());
        assert!(!entry.metadata_refresh_pending);
        assert_eq!(entry.effective_size_bytes(), 0);
        assert_eq!(entry.effective_modified_secs(), None);
        assert!(entry.thumbnail_path.is_none());
    }

    #[test]
    fn same_listing_directory_reload_does_not_enter_metadata_refresh() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("Documents", true, 0, Some(100))]),
        );
        let item_id = model.entries()[0].id;

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata_state(
                "Documents",
                true,
                0,
                None,
                false,
            )]),
        );

        let entry = &model.entries()[0];
        assert_eq!(entry.id, item_id);
        assert!(entry.is_dir);
        assert!(!entry.metadata_refresh_pending);
        assert!(entry.metadata_role.is_none());
        assert_eq!(entry.effective_size_bytes(), 0);
    }

    #[test]
    fn size_sorted_lightweight_reload_drops_stale_visible_metadata() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("small.txt", false, 1, Some(10)),
                entry_with_metadata("large.txt", false, 100, Some(20)),
            ]),
        );
        model.set_sort(SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Descending,
            folders_first: true,
            hidden_last: false,
        });
        let large_id = model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/thumbs/large.png");
        model.set_thumbnail_path(large_id, Some(thumbnail_path.clone()));
        let index_generation = model.index_generation;

        let signals = model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata_state("small.txt", false, 0, None, false),
                entry_with_metadata_state("large.txt", false, 0, None, false),
            ]),
        );

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 2 }],
                ChangedRoles::metadata(),
            )]
        );
        assert_eq!(model.index_generation, index_generation);
        let large = model
            .entries()
            .iter()
            .find(|entry| entry.id == large_id)
            .unwrap();
        assert_eq!(large.name.as_ref(), "large.txt");
        assert!(!large.entry.metadata_complete);
        assert!(!large.effective_metadata_complete());
        assert!(!large.metadata_refresh_pending);
        assert_eq!(large.effective_size_bytes(), 0);
        assert_eq!(large.effective_modified_secs(), None);
        assert!(large.thumbnail_path.is_none());
    }

    #[test]
    fn refreshed_item_lightweight_update_drops_stale_visible_metadata() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata(
                "notes.txt",
                false,
                512,
                Some(100),
            )]),
        );
        let item_id = model.entries()[0].id;
        let index_generation = model.index_generation;

        let signals = model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
            old_path: PathBuf::from("/tmp/notes.txt"),
            entry: Some(entry_with_metadata_state(
                "notes.txt",
                false,
                0,
                None,
                false,
            )),
        }]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 1 }],
                ChangedRoles::ALL,
            )]
        );
        assert_eq!(model.index_generation, index_generation);
        assert_eq!(model.entries()[0].id, item_id);
        assert!(!model.entries()[0].entry.metadata_complete);
        assert!(!model.entries()[0].effective_metadata_complete());
        assert!(!model.entries()[0].metadata_refresh_pending);
        assert_eq!(model.entries()[0].effective_size_bytes(), 0);
        assert_eq!(model.entries()[0].effective_modified_secs(), None);
    }

    #[test]
    fn metadata_role_update_is_item_and_path_guarded() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("payload", false, 0, None)]),
        );
        let item_id = model.entries()[0].id;
        let role = EntryMetadataRole {
            size_bytes: 99,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        };

        assert!(
            model
                .set_metadata_role(item_id, Path::new("/tmp/other"), role.clone())
                .is_empty()
        );
        assert_eq!(model.entries()[0].effective_size_bytes(), 0);

        let signals = model.set_metadata_role(item_id, Path::new("/tmp/payload"), role);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 1 }],
                ChangedRoles::metadata(),
            )]
        );
        assert!(model.entries()[0].effective_metadata_complete());
        assert_eq!(model.entries()[0].effective_size_bytes(), 99);
        assert_eq!(model.entries()[0].effective_modified_secs(), Some(42));
        assert_eq!(
            model.entries()[0].effective_mime_type().map(Arc::as_ref),
            Some("text/plain")
        );
    }

    #[test]
    fn metadata_role_update_clears_stale_thumbnail_when_identity_changes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 12, Some(10))]),
        );
        let item_id = model.entries()[0].id;
        model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/image.png")));

        let signals = model.set_metadata_role(
            item_id,
            Path::new("/tmp/image.png"),
            EntryMetadataRole {
                size_bytes: 13,
                modified_secs: Some(11),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            },
        );

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 1 }],
                ChangedRoles::metadata(),
            )]
        );
        assert!(model.entries()[0].thumbnail_path.is_none());
    }

    #[test]
    fn metadata_role_update_resorts_size_sorted_model() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("small.txt", false, 1, Some(10)),
                entry_with_metadata("large.txt", false, 10, Some(10)),
            ]),
        );
        model.set_sort(SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: false,
        });
        let small_id = model
            .entries()
            .iter()
            .find(|entry| entry.name.as_ref() == "small.txt")
            .unwrap()
            .id;

        let signals = model.set_metadata_role(
            small_id,
            Path::new("/tmp/small.txt"),
            EntryMetadataRole {
                size_bytes: 20,
                modified_secs: Some(20),
                mime_type: Some(Arc::from("text/plain")),
                mime_magic_checked: true,
            },
        );

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        assert_eq!(model.entries()[1].id, small_id);
        assert_eq!(model.entries()[1].effective_size_bytes(), 20);
    }

    #[test]
    fn thumbnail_role_update_keeps_item_identity_and_emits_metadata_change() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry("image.png", false)]),
        );
        let item_id = model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");

        let signals = model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 1 }],
                ChangedRoles::metadata(),
            )]
        );
        assert_eq!(model.entries()[0].id, item_id);
        assert_eq!(
            model.entries()[0].thumbnail_path.as_deref(),
            Some(thumbnail_path.as_path())
        );
        assert!(
            model
                .set_thumbnail_path(item_id, Some(thumbnail_path))
                .is_empty()
        );
        assert!(
            model
                .set_thumbnail_path(ItemId(999), Some(PathBuf::from("/tmp/missing.png")))
                .is_empty()
        );
    }

    #[test]
    fn same_listing_reload_keeps_resolved_mime_as_finished_role_when_metadata_matches() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_mime_state(
                "payload",
                12,
                Some(42),
                "text/plain",
                true,
            )]),
        );
        let item_id = model.entries()[0].id;

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_mime_state(
                "payload",
                12,
                Some(42),
                GENERIC_BINARY_MIME,
                false,
            )]),
        );

        let entry = &model.entries()[0];
        assert_eq!(entry.id, item_id);
        assert_eq!(entry.entry.mime_type.as_deref(), Some(GENERIC_BINARY_MIME));
        assert!(!entry.entry.mime_magic_checked);
        assert!(!entry.metadata_refresh_pending);
        assert_eq!(
            entry.effective_mime_type().map(Arc::as_ref),
            Some("text/plain")
        );
        assert!(entry.effective_mime_magic_checked());
    }

    #[test]
    fn same_listing_reload_drops_resolved_mime_when_metadata_changes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_mime_state(
                "payload",
                12,
                Some(42),
                "text/plain",
                true,
            )]),
        );
        let item_id = model.entries()[0].id;

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_mime_state(
                "payload",
                13,
                Some(43),
                GENERIC_BINARY_MIME,
                false,
            )]),
        );

        let entry = &model.entries()[0];
        assert_eq!(entry.id, item_id);
        assert!(!entry.metadata_refresh_pending);
        assert_eq!(
            entry.effective_mime_type().map(Arc::as_ref),
            Some(GENERIC_BINARY_MIME)
        );
        assert!(!entry.effective_mime_magic_checked());
    }

    #[test]
    fn same_listing_reload_preserves_matching_thumbnail_role() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
        );
        let item_id = model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");
        model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
        );

        assert_eq!(model.entries()[0].id, item_id);
        assert_eq!(
            model.entries()[0].thumbnail_path.as_deref(),
            Some(thumbnail_path.as_path())
        );
    }

    #[test]
    fn metadata_change_clears_thumbnail_role() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
        );
        let item_id = model.entries()[0].id;
        model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/image.png")));

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 13, Some(101))]),
        );

        assert_eq!(model.entries()[0].id, item_id);
        assert!(model.entries()[0].thumbnail_path.is_none());
    }

    #[test]
    fn incomplete_metadata_reload_keeps_thumbnail_role_until_refresh_finishes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
        );
        let item_id = model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");
        model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![entry_with_metadata_state(
                "image.png",
                false,
                12,
                Some(100),
                false,
            )]),
        );

        assert_eq!(model.entries()[0].id, item_id);
        assert_eq!(
            model.entries()[0].thumbnail_path.as_deref(),
            Some(thumbnail_path.as_path())
        );
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
    fn set_sort_by_size_keeps_directories_first_and_preserves_item_identity() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("small.txt", false, 1, None),
                entry_with_metadata("folder-b", true, 0, None),
                entry_with_metadata("big.txt", false, 100, None),
                entry_with_metadata("folder-a", true, 0, None),
            ]),
        );
        let big_id = model
            .entries()
            .iter()
            .find(|entry| entry.name.as_ref() == "big.txt")
            .unwrap()
            .id;

        let signals = model.set_sort(SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Descending,
            ..SortDescriptor::default()
        });

        assert_eq!(signals, vec![DirectoryModelSignal::SortChanged]);
        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["folder-a", "folder-b", "big.txt", "small.txt"]
        );
        assert_eq!(model.index_of_id(big_id), Some(2));
    }

    #[test]
    fn set_sort_by_modified_uses_model_role_order() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("new.txt", false, 0, Some(30)),
                entry_with_metadata("unknown.txt", false, 0, None),
                entry_with_metadata("old.txt", false, 0, Some(10)),
            ]),
        );

        model.set_sort(SortDescriptor {
            role: SortRole::Modified,
            order: SortOrder::Ascending,
            ..SortDescriptor::default()
        });

        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["unknown.txt", "old.txt", "new.txt"]
        );
    }

    #[test]
    fn folders_first_can_be_disabled_for_name_sorting() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry("z-dir", true),
                entry("b-file.txt", false),
                entry("a-dir", true),
                entry("a-file.txt", false),
            ]),
        );

        model.set_sort(SortDescriptor {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            folders_first: false,
            hidden_last: false,
        });

        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["a-dir", "a-file.txt", "b-file.txt", "z-dir"]
        );
    }

    #[test]
    fn size_sort_keeps_directories_first_even_when_folders_first_is_disabled() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry_with_metadata("large-file.txt", false, 100, None),
                entry_with_metadata("folder-b", true, 0, None),
                entry_with_metadata("small-file.txt", false, 1, None),
                entry_with_metadata("folder-a", true, 0, None),
            ]),
        );

        model.set_sort(SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Descending,
            folders_first: false,
            hidden_last: false,
        });

        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["folder-a", "folder-b", "large-file.txt", "small-file.txt"]
        );
    }

    #[test]
    fn hidden_last_sorts_hidden_entries_after_visible_entries() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            listing(vec![
                entry(".hidden-file.txt", false),
                entry("visible-file.txt", false),
                entry(".hidden-folder", true),
                entry("visible-folder", true),
            ]),
        );

        model.set_sort(SortDescriptor {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: true,
        });

        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec![
                "visible-folder",
                "visible-file.txt",
                ".hidden-folder",
                ".hidden-file.txt"
            ]
        );
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
    fn trash_listing_can_sort_by_original_path_role() {
        let trash_dir = file_ops::trash_files_dir();
        let mut model = DirectoryModel::for_directory(trash_dir.clone());
        model.replace_listing(
            trash_dir,
            listing(vec![
                trash_entry("beta.txt", "/tmp/beta/beta.txt", "2026-06-03T10:00:00"),
                trash_entry("alpha.txt", "/tmp/alpha/alpha.txt", "2026-06-01T10:00:00"),
                trash_entry("gamma.txt", "/tmp/gamma/gamma.txt", "2026-06-02T10:00:00"),
            ]),
        );

        let signals = model.set_sort(SortDescriptor {
            role: SortRole::TrashOriginalPath,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: false,
        });

        assert_eq!(signals, vec![DirectoryModelSignal::SortChanged]);
        assert_eq!(
            model
                .entries()
                .iter()
                .map(|entry| entry.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["alpha.txt", "beta.txt", "gamma.txt"]
        );
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
