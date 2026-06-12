use super::entries::Entry;
use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryCacheState {
    Fresh,
    Stale,
}

#[derive(Clone, Debug)]
pub struct DirectoryCacheSnapshot {
    path: PathBuf,
    entries: Arc<Vec<Entry>>,
    state: DirectoryCacheState,
    loaded_at: u64,
}

impl DirectoryCacheSnapshot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &Arc<Vec<Entry>> {
        &self.entries
    }

    pub fn state(&self) -> DirectoryCacheState {
        self.state
    }

    pub fn loaded_at(&self) -> u64 {
        self.loaded_at
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryCacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evicted_directories: usize,
    pub skipped_large_directories: usize,
    pub cached_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryCacheLimits {
    pub max_dirs: usize,
    pub max_entries: usize,
    pub max_entries_per_dir: usize,
}

impl Default for DirectoryCacheLimits {
    fn default() -> Self {
        Self {
            max_dirs: 32,
            max_entries: 50_000,
            max_entries_per_dir: 10_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryCache {
    limits: DirectoryCacheLimits,
    clock: u64,
    stats: DirectoryCacheStats,
    cached_entries: usize,
    entries_by_path: HashMap<PathBuf, DirectoryCacheSnapshot>,
    lru: VecDeque<PathBuf>,
}

impl Default for DirectoryCache {
    fn default() -> Self {
        Self::with_limits(DirectoryCacheLimits::default())
    }
}

impl DirectoryCache {
    pub fn new(max_dirs: usize) -> Self {
        Self::with_limits(DirectoryCacheLimits {
            max_dirs,
            ..DirectoryCacheLimits::default()
        })
    }

    pub fn with_limits(limits: DirectoryCacheLimits) -> Self {
        Self {
            limits: DirectoryCacheLimits {
                max_dirs: limits.max_dirs.max(1),
                max_entries: limits.max_entries.max(1),
                max_entries_per_dir: limits.max_entries_per_dir.max(1),
            },
            clock: 0,
            stats: DirectoryCacheStats::default(),
            cached_entries: 0,
            entries_by_path: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries_by_path.len()
    }

    pub fn cached_entry_count(&self) -> usize {
        self.cached_entries
    }

    pub fn stats(&self) -> DirectoryCacheStats {
        DirectoryCacheStats {
            cached_entries: self.cached_entries,
            ..self.stats
        }
    }

    pub fn get(&mut self, path: &Path) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path);
        let snapshot = self.entries_by_path.get(&key).cloned();
        if snapshot.is_some() {
            self.stats.hits += 1;
            self.touch(&key);
        } else {
            self.stats.misses += 1;
        }
        snapshot
    }

    pub fn insert_fresh(
        &mut self,
        path: impl AsRef<Path>,
        entries: Arc<Vec<Entry>>,
    ) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path.as_ref());
        if entries.len() > self.limits.max_entries_per_dir
            || entries.len() > self.limits.max_entries
        {
            self.remove_normalized(&key);
            self.stats.skipped_large_directories += 1;
            return None;
        }

        self.clock = self.clock.wrapping_add(1);
        let snapshot = DirectoryCacheSnapshot {
            path: key.clone(),
            entries,
            state: DirectoryCacheState::Fresh,
            loaded_at: self.clock,
        };
        self.remove_normalized(&key);
        self.cached_entries += snapshot.entry_count();
        self.entries_by_path.insert(key.clone(), snapshot.clone());
        self.touch(&key);
        self.evict_oldest();
        Some(snapshot)
    }

    pub fn mark_stale(&mut self, path: &Path) -> bool {
        let key = normalize_cache_path(path);
        let Some(snapshot) = self.entries_by_path.get_mut(&key) else {
            return false;
        };
        snapshot.state = DirectoryCacheState::Stale;
        true
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let key = normalize_cache_path(path);
        self.remove_normalized(&key)
    }

    fn remove_normalized(&mut self, key: &Path) -> bool {
        self.lru.retain(|candidate| candidate.as_path() != key);
        let Some(removed) = self.entries_by_path.remove(key) else {
            return false;
        };
        self.cached_entries = self.cached_entries.saturating_sub(removed.entry_count());
        true
    }

    fn touch(&mut self, key: &Path) {
        self.lru.retain(|candidate| candidate.as_path() != key);
        self.lru.push_back(key.to_path_buf());
    }

    fn evict_oldest(&mut self) {
        while self.entries_by_path.len() > self.limits.max_dirs
            || self.cached_entries > self.limits.max_entries
        {
            let Some(path) = self.lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries_by_path.remove(&path) {
                self.cached_entries = self.cached_entries.saturating_sub(removed.entry_count());
                self.stats.evicted_directories += 1;
            }
        }
    }
}

pub fn normalize_cache_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entries::EntryData;

    fn entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            mime_type: None,
            thumbnail_path: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn entries(names: &[&str]) -> Arc<Vec<Entry>> {
        Arc::new(names.iter().map(|name| entry(name)).collect())
    }

    #[test]
    fn cache_normalizes_paths_for_lookup() {
        let mut cache = DirectoryCache::new(4);
        let payload = entries(&["a"]);

        assert!(
            cache
                .insert_fresh("/tmp/fika/../fika", Arc::clone(&payload))
                .is_some()
        );
        let snapshot = cache.get(Path::new("/tmp/fika")).unwrap();

        assert!(Arc::ptr_eq(snapshot.entries(), &payload));
        assert_eq!(snapshot.path(), Path::new("/tmp/fika"));
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 0);
    }

    #[test]
    fn cache_evicts_least_recently_used_directory() {
        let mut cache = DirectoryCache::new(2);

        assert!(cache.insert_fresh("/tmp/a", entries(&["a"])).is_some());
        assert!(cache.insert_fresh("/tmp/b", entries(&["b"])).is_some());
        assert!(cache.get(Path::new("/tmp/a")).is_some());
        assert!(cache.insert_fresh("/tmp/c", entries(&["c"])).is_some());

        assert!(cache.get(Path::new("/tmp/a")).is_some());
        assert!(cache.get(Path::new("/tmp/b")).is_none());
        assert!(cache.get(Path::new("/tmp/c")).is_some());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_marks_directory_stale_without_dropping_payload() {
        let mut cache = DirectoryCache::new(2);
        let payload = entries(&["a"]);
        assert!(cache.insert_fresh("/tmp/a", Arc::clone(&payload)).is_some());

        assert!(cache.mark_stale(Path::new("/tmp/a")));
        let snapshot = cache.get(Path::new("/tmp/a")).unwrap();

        assert_eq!(snapshot.state(), DirectoryCacheState::Stale);
        assert!(Arc::ptr_eq(snapshot.entries(), &payload));
    }

    #[test]
    fn cache_evicts_to_total_entry_budget() {
        let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
            max_dirs: 8,
            max_entries: 3,
            max_entries_per_dir: 8,
        });

        assert!(
            cache
                .insert_fresh("/tmp/a", entries(&["a1", "a2"]))
                .is_some()
        );
        assert_eq!(cache.cached_entry_count(), 2);
        assert!(
            cache
                .insert_fresh("/tmp/b", entries(&["b1", "b2"]))
                .is_some()
        );

        assert!(cache.get(Path::new("/tmp/a")).is_none());
        assert!(cache.get(Path::new("/tmp/b")).is_some());
        assert_eq!(cache.cached_entry_count(), 2);
        assert_eq!(cache.stats().evicted_directories, 1);
    }

    #[test]
    fn cache_does_not_retain_large_directory_payloads() {
        let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
            max_dirs: 8,
            max_entries: 16,
            max_entries_per_dir: 2,
        });

        assert!(
            cache
                .insert_fresh("/tmp/large", entries(&["a", "b", "c"]))
                .is_none()
        );

        assert!(cache.get(Path::new("/tmp/large")).is_none());
        assert_eq!(cache.cached_entry_count(), 0);
        assert_eq!(cache.stats().skipped_large_directories, 1);
    }
}
