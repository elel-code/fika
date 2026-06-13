use super::entries::Entry;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

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
    fingerprint: Option<DirectoryCacheFingerprint>,
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

    fn matches_current_directory(&self) -> bool {
        self.fingerprint
            .is_none_or(|fingerprint| fingerprint.matches_path(&self.path))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryCacheStats {
    pub hits: usize,
    pub misses: usize,
    pub stale_invalidations: usize,
    pub evicted_directories: usize,
    pub skipped_large_directories: usize,
    pub cached_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectoryCacheFingerprint {
    modified_nanos: Option<u128>,
    len: u64,
    is_dir: bool,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
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

    pub fn get_fresh(&mut self, path: &Path) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path);
        let snapshot = self.get(&key)?;
        if snapshot.state != DirectoryCacheState::Fresh {
            return None;
        }
        if !snapshot.matches_current_directory() {
            if let Some(snapshot) = self.entries_by_path.get_mut(&key) {
                snapshot.state = DirectoryCacheState::Stale;
            }
            self.stats.stale_invalidations += 1;
            return None;
        }
        Some(snapshot)
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
            fingerprint: DirectoryCacheFingerprint::for_path(&key),
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

impl DirectoryCacheFingerprint {
    fn for_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified_nanos: metadata.modified().ok().and_then(system_time_nanos),
            len: metadata.len(),
            is_dir: metadata.is_dir(),
            #[cfg(unix)]
            dev: metadata.dev(),
            #[cfg(unix)]
            ino: metadata.ino(),
        })
    }

    fn matches_path(self, path: &Path) -> bool {
        Self::for_path(path) == Some(self)
    }
}

fn system_time_nanos(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
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
    use std::process;
    use std::time::Duration;

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

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("fika-cache-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
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
    fn fresh_lookup_marks_cache_stale_when_directory_metadata_changes() {
        let root = temp_root("freshness");
        let mut cache = DirectoryCache::new(2);
        let payload = entries(&["a"]);
        assert!(cache.insert_fresh(&root, Arc::clone(&payload)).is_some());

        assert!(cache.get_fresh(&root).is_some());
        std::thread::sleep(Duration::from_millis(20));
        fs::write(root.join("new.txt"), b"changed").unwrap();

        assert!(cache.get_fresh(&root).is_none());
        let snapshot = cache.get(&root).unwrap();
        assert_eq!(snapshot.state(), DirectoryCacheState::Stale);
        assert!(Arc::ptr_eq(snapshot.entries(), &payload));
        assert_eq!(cache.stats().stale_invalidations, 1);

        let _ = fs::remove_dir_all(root);
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
