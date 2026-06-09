use super::entries::{Entry, sort_entries};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Default)]
pub struct DirectoryModel {
    entries: Vec<Entry>,
    index_by_path: HashMap<PathBuf, usize>,
}

impl DirectoryModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn index_of_path(&self, path: &Path) -> Option<usize> {
        self.index_by_path.get(path).copied()
    }

    pub fn replace_listing(&mut self, mut entries: Vec<Entry>) -> Vec<DirectoryModelSignal> {
        sort_entries(&mut entries, false);
        if self.same_listing(&entries) {
            self.entries = entries;
            self.rebuild_index();
            return vec![DirectoryModelSignal::ItemsChanged(
                range_all(self.entries.len()),
                ChangedRoles::metadata(),
            )];
        }
        self.entries = entries;
        self.rebuild_index();
        vec![DirectoryModelSignal::ModelReset]
    }

    pub fn apply_items_added(&mut self, mut entries: Vec<Entry>) -> Vec<DirectoryModelSignal> {
        if entries.is_empty() {
            return Vec::new();
        }

        let mut changed = Vec::new();
        entries.retain(|entry| {
            if let Some(index) = self.index_of_path(&entry.path) {
                self.entries[index] = entry.clone();
                changed.push(index);
                false
            } else {
                true
            }
        });
        self.entries.extend(entries);
        sort_entries(&mut self.entries, false);
        self.rebuild_index();

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
        let deleted = paths.iter().collect::<BTreeSet<_>>();
        let mut removed_indexes = Vec::new();
        self.entries.retain(|entry| {
            let remove = deleted.contains(&entry.path);
            if remove && let Some(index) = self.index_by_path.get(&entry.path) {
                removed_indexes.push(*index);
            }
            !remove
        });
        if removed_indexes.is_empty() {
            return Vec::new();
        }
        self.rebuild_index();
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
                        self.entries[index] = entry;
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
            sort_entries(&mut self.entries, false);
            self.rebuild_index();
            signals.push(DirectoryModelSignal::ItemsChanged(
                ranges_from_indexes(changed),
                ChangedRoles::ALL,
            ));
        }
        signals
    }

    fn same_listing(&self, entries: &[Entry]) -> bool {
        self.entries.len() == entries.len()
            && self
                .entries
                .iter()
                .zip(entries)
                .all(|(left, right)| left.path == right.path && left.name == right.name)
    }

    fn rebuild_index(&mut self) {
        self.index_by_path.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.index_by_path.insert(entry.path.clone(), index);
        }
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

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            group: String::new(),
            location: String::new(),
            kind: if is_dir { "Folder" } else { "File" }.to_string(),
            size: "-".to_string(),
            size_bytes: 0,
            modified: "-".to_string(),
            modified_age_days: -1,
            is_dir,
        }
    }

    #[test]
    fn listing_reset_rebuilds_path_index() {
        let mut model = DirectoryModel::new();
        let signals = model.replace_listing(vec![entry("b.txt", false), entry("a", true)]);

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        assert_eq!(model.entries()[0].name, "a");
        assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
    }

    #[test]
    fn delete_emits_removed_ranges() {
        let mut model = DirectoryModel::new();
        model.replace_listing(vec![
            entry("a", false),
            entry("b", false),
            entry("c", false),
        ]);

        let signals =
            model.apply_items_deleted(&[PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);

        assert_eq!(
            signals,
            vec![DirectoryModelSignal::ItemsRemoved(vec![ItemRange {
                start: 0,
                len: 2
            }])]
        );
        assert_eq!(model.entries()[0].name, "c");
    }
}
