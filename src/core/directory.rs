use super::entries::{Entry, read_entries_sync_cancellable, read_entry_sync};
use super::model::{DirectoryModel, DirectoryModelSignal};
use super::pane::{Generation, PaneId, RequestSerial};
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadMode {
    Load,
    Reload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshPair {
    pub old_path: PathBuf,
    pub entry: Option<Entry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryListerEvent {
    LoadingStarted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
    },
    ItemsAdded {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        entries: Vec<Entry>,
    },
    ItemsDeleted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        paths: Vec<PathBuf>,
    },
    ItemsRefreshed {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        pairs: Vec<RefreshPair>,
    },
    ListingRefreshed {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        entries: Arc<Vec<Entry>>,
    },
    ListingCompleted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
    },
    CurrentDirectoryRemoved {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
    },
    Error {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        message: String,
    },
}

impl DirectoryListerEvent {
    pub fn pane_id(&self) -> PaneId {
        match self {
            Self::LoadingStarted { pane_id, .. }
            | Self::ItemsAdded { pane_id, .. }
            | Self::ItemsDeleted { pane_id, .. }
            | Self::ItemsRefreshed { pane_id, .. }
            | Self::ListingRefreshed { pane_id, .. }
            | Self::ListingCompleted { pane_id, .. }
            | Self::CurrentDirectoryRemoved { pane_id, .. }
            | Self::Error { pane_id, .. } => *pane_id,
        }
    }

    pub fn generation(&self) -> Generation {
        match self {
            Self::LoadingStarted { generation, .. }
            | Self::ItemsAdded { generation, .. }
            | Self::ItemsDeleted { generation, .. }
            | Self::ItemsRefreshed { generation, .. }
            | Self::ListingRefreshed { generation, .. }
            | Self::ListingCompleted { generation, .. }
            | Self::CurrentDirectoryRemoved { generation, .. }
            | Self::Error { generation, .. } => *generation,
        }
    }

    pub fn request_serial(&self) -> RequestSerial {
        match self {
            Self::LoadingStarted { request_serial, .. }
            | Self::ItemsAdded { request_serial, .. }
            | Self::ItemsDeleted { request_serial, .. }
            | Self::ItemsRefreshed { request_serial, .. }
            | Self::ListingRefreshed { request_serial, .. }
            | Self::ListingCompleted { request_serial, .. }
            | Self::CurrentDirectoryRemoved { request_serial, .. }
            | Self::Error { request_serial, .. } => *request_serial,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::LoadingStarted { path, .. }
            | Self::ItemsAdded { path, .. }
            | Self::ItemsDeleted { path, .. }
            | Self::ItemsRefreshed { path, .. }
            | Self::ListingRefreshed { path, .. }
            | Self::ListingCompleted { path, .. }
            | Self::CurrentDirectoryRemoved { path, .. }
            | Self::Error { path, .. } => path,
        }
    }

    pub fn matches_target(&self, pane_id: PaneId, generation: Generation, path: &Path) -> bool {
        self.pane_id() == pane_id && self.generation() == generation && self.path() == path
    }
}

#[derive(Debug)]
pub struct DirectoryLister {
    pane_id: PaneId,
    generation: Generation,
    path: PathBuf,
    request_serial: u64,
    watcher: Option<notify::RecommendedWatcher>,
    watcher_rx: Option<Receiver<notify::Result<Event>>>,
}

impl DirectoryLister {
    pub fn new(pane_id: PaneId, path: PathBuf, generation: Generation) -> Self {
        Self {
            pane_id,
            generation,
            path,
            request_serial: 0,
            watcher: None,
            watcher_rx: None,
        }
    }

    pub fn set_target(&mut self, pane_id: PaneId, path: PathBuf, generation: Generation) {
        self.pane_id = pane_id;
        self.generation = generation;
        self.path = path;
        self.drop_watcher();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn load_directory(&mut self, mode: LoadMode) -> DirectoryListerEvent {
        let serial = self.next_serial();
        self.loading_started(mode, serial)
    }

    pub fn read_listing(&mut self, mode: LoadMode) -> Vec<DirectoryListerEvent> {
        let serial = self.next_serial();
        read_listing_events(
            self.pane_id,
            self.generation,
            serial,
            self.path.clone(),
            mode,
        )
    }

    pub fn read_listing_events(
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
    ) -> Vec<DirectoryListerEvent> {
        read_listing_events(pane_id, generation, request_serial, path, mode)
    }

    pub fn read_listing_events_cancellable(
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
        is_cancelled: impl FnMut() -> bool,
    ) -> Option<Vec<DirectoryListerEvent>> {
        read_listing_events_cancellable(
            pane_id,
            generation,
            request_serial,
            path,
            mode,
            is_cancelled,
        )
    }

    pub fn start_watcher(&mut self) -> Result<(), String> {
        self.drop_watcher();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .map_err(|err| err.to_string())?;
        watcher
            .watch(&self.path, RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;
        self.watcher = Some(watcher);
        self.watcher_rx = Some(rx);
        Ok(())
    }

    pub fn drain_watcher_events(&mut self) -> Vec<DirectoryListerEvent> {
        let Some(rx) = self.watcher_rx.take() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(self.classify_notify_result(event));
        }
        self.watcher_rx = Some(rx);
        events
    }

    pub fn classify_watcher_delta(&mut self, delta: WatcherDelta) -> DirectoryListerEvent {
        let serial = self.next_serial();
        self.classify_delta_with_serial(delta, serial)
    }

    pub fn apply_event_to_model(
        &mut self,
        event: DirectoryListerEvent,
        model: &mut DirectoryModel,
    ) -> Vec<DirectoryModelSignal> {
        if event.request_serial() < RequestSerial(self.request_serial) {
            return Vec::new();
        }

        match event {
            DirectoryListerEvent::LoadingStarted { path, mode, .. } => match mode {
                LoadMode::Load => model.clear_for_directory(path),
                LoadMode::Reload if model.directory() != path => model.clear_for_directory(path),
                LoadMode::Reload => Vec::new(),
            },
            DirectoryListerEvent::ListingCompleted { .. }
            | DirectoryListerEvent::CurrentDirectoryRemoved { .. }
            | DirectoryListerEvent::Error { .. } => Vec::new(),
            DirectoryListerEvent::ItemsAdded { entries, .. } => model.apply_items_added(entries),
            DirectoryListerEvent::ItemsDeleted { paths, .. } => model.apply_items_deleted(&paths),
            DirectoryListerEvent::ItemsRefreshed { pairs, .. } => {
                model.apply_items_refreshed(pairs)
            }
            DirectoryListerEvent::ListingRefreshed { path, entries, .. } => {
                let entries = Arc::try_unwrap(entries).unwrap_or_else(|entries| (*entries).clone());
                model.replace_listing(path, entries)
            }
        }
    }

    fn classify_notify_result(&mut self, event: notify::Result<Event>) -> DirectoryListerEvent {
        match event {
            Ok(event) => {
                self.classify_watcher_delta(WatcherDelta::from_notify_event(&self.path, event))
            }
            Err(err) => {
                let serial = self.next_serial();
                DirectoryListerEvent::Error {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    message: err.to_string(),
                }
            }
        }
    }

    fn classify_delta_with_serial(
        &self,
        delta: WatcherDelta,
        serial: RequestSerial,
    ) -> DirectoryListerEvent {
        match classify_watcher_delta(&self.path, delta) {
            ClassifiedWatcherDelta::ItemsAdded(paths) => {
                let entries = paths
                    .iter()
                    .filter_map(|path| read_entry_sync(&self.path, path).ok())
                    .collect();
                DirectoryListerEvent::ItemsAdded {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    entries,
                }
            }
            ClassifiedWatcherDelta::ItemsDeleted(paths) => DirectoryListerEvent::ItemsDeleted {
                pane_id: self.pane_id,
                generation: self.generation,
                request_serial: serial,
                path: self.path.clone(),
                paths,
            },
            ClassifiedWatcherDelta::ItemsRefreshed(paths) => {
                let pairs = paths
                    .into_iter()
                    .map(|path| RefreshPair {
                        entry: read_entry_sync(&self.path, &path).ok(),
                        old_path: path,
                    })
                    .collect();
                DirectoryListerEvent::ItemsRefreshed {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    pairs,
                }
            }
            ClassifiedWatcherDelta::Renamed { from, to } => {
                let entry = read_entry_sync(&self.path, &to).ok();
                DirectoryListerEvent::ItemsRefreshed {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    pairs: vec![RefreshPair {
                        old_path: from,
                        entry,
                    }],
                }
            }
            ClassifiedWatcherDelta::FullReload => self.loading_started(LoadMode::Reload, serial),
            ClassifiedWatcherDelta::CurrentDirectoryRemoved => {
                DirectoryListerEvent::CurrentDirectoryRemoved {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                }
            }
        }
    }

    fn loading_started(&self, mode: LoadMode, serial: RequestSerial) -> DirectoryListerEvent {
        DirectoryListerEvent::LoadingStarted {
            pane_id: self.pane_id,
            generation: self.generation,
            request_serial: serial,
            path: self.path.clone(),
            mode,
        }
    }

    fn next_serial(&mut self) -> RequestSerial {
        self.request_serial += 1;
        RequestSerial(self.request_serial)
    }

    fn drop_watcher(&mut self) {
        self.watcher = None;
        self.watcher_rx = None;
    }
}

fn read_listing_events(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
) -> Vec<DirectoryListerEvent> {
    read_listing_events_cancellable(pane_id, generation, request_serial, path, mode, || false)
        .unwrap_or_default()
}

fn read_listing_events_cancellable(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<Vec<DirectoryListerEvent>> {
    if is_cancelled() {
        return None;
    }

    let result = match read_entries_sync_cancellable(&path, &mut is_cancelled) {
        Ok(Some(entries)) => DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            path: path.clone(),
            entries: Arc::new(entries),
        },
        Ok(None) => return None,
        Err(err) => {
            if mode == LoadMode::Reload && !path.exists() {
                DirectoryListerEvent::CurrentDirectoryRemoved {
                    pane_id,
                    generation,
                    request_serial,
                    path: path.clone(),
                }
            } else {
                DirectoryListerEvent::Error {
                    pane_id,
                    generation,
                    request_serial,
                    path: path.clone(),
                    message: err.to_string(),
                }
            }
        }
    };
    if is_cancelled() {
        return None;
    }
    Some(vec![
        result,
        DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation,
            request_serial,
            path,
        },
    ])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatcherDelta {
    pub kind: WatcherDeltaKind,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatcherDeltaKind {
    Create,
    Remove,
    Rename,
    Modify,
    Rescan,
}

impl WatcherDelta {
    pub fn from_notify_event(root: &Path, event: Event) -> Self {
        let kind = match event.kind {
            EventKind::Create(
                CreateKind::Any | CreateKind::File | CreateKind::Folder | CreateKind::Other,
            ) => WatcherDeltaKind::Create,
            EventKind::Remove(
                RemoveKind::Any | RemoveKind::File | RemoveKind::Folder | RemoveKind::Other,
            ) => WatcherDeltaKind::Remove,
            EventKind::Modify(ModifyKind::Name(
                RenameMode::Any
                | RenameMode::Both
                | RenameMode::From
                | RenameMode::To
                | RenameMode::Other,
            )) => WatcherDeltaKind::Rename,
            EventKind::Modify(
                ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_) | ModifyKind::Other,
            ) => WatcherDeltaKind::Modify,
            _ => WatcherDeltaKind::Rescan,
        };
        let paths = event
            .paths
            .into_iter()
            .filter(|path| path == root || path.parent() == Some(root))
            .collect();
        Self { kind, paths }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassifiedWatcherDelta {
    ItemsAdded(Vec<PathBuf>),
    ItemsDeleted(Vec<PathBuf>),
    ItemsRefreshed(Vec<PathBuf>),
    Renamed { from: PathBuf, to: PathBuf },
    FullReload,
    CurrentDirectoryRemoved,
}

pub fn classify_watcher_delta(root: &Path, delta: WatcherDelta) -> ClassifiedWatcherDelta {
    if delta.paths.iter().any(|path| path == root) && matches!(delta.kind, WatcherDeltaKind::Remove)
    {
        return ClassifiedWatcherDelta::CurrentDirectoryRemoved;
    }

    let child_paths = delta
        .paths
        .into_iter()
        .filter(|path| path.parent() == Some(root))
        .collect::<Vec<_>>();
    if child_paths.is_empty() {
        return ClassifiedWatcherDelta::FullReload;
    }

    match delta.kind {
        WatcherDeltaKind::Create => ClassifiedWatcherDelta::ItemsAdded(child_paths),
        WatcherDeltaKind::Remove => ClassifiedWatcherDelta::ItemsDeleted(child_paths),
        WatcherDeltaKind::Modify => ClassifiedWatcherDelta::ItemsRefreshed(child_paths),
        WatcherDeltaKind::Rename if child_paths.len() == 2 => ClassifiedWatcherDelta::Renamed {
            from: child_paths[0].clone(),
            to: child_paths[1].clone(),
        },
        WatcherDeltaKind::Rename => ClassifiedWatcherDelta::FullReload,
        WatcherDeltaKind::Rescan => ClassifiedWatcherDelta::FullReload,
    }
}

pub fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut cursor = Some(path);
    while let Some(path) = cursor {
        if path.exists() {
            return Some(path.to_path_buf());
        }
        cursor = path.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_create_maps_to_items_added() {
        let root = Path::new("/tmp/root");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Create,
            paths: vec![root.join("new.txt")],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::ItemsAdded(vec![root.join("new.txt")])
        );
    }

    #[test]
    fn watcher_root_remove_maps_to_current_directory_removed() {
        let root = Path::new("/tmp/root");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Remove,
            paths: vec![root.to_path_buf()],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::CurrentDirectoryRemoved
        );
    }

    #[test]
    fn watcher_child_remove_maps_to_items_deleted() {
        let root = Path::new("/tmp/root");
        let path = root.join("old.txt");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Remove,
            paths: vec![path.clone()],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::ItemsDeleted(vec![path])
        );
    }

    #[test]
    fn watcher_modify_maps_to_items_refreshed() {
        let root = Path::new("/tmp/root");
        let path = root.join("changed.txt");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Modify,
            paths: vec![path.clone()],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::ItemsRefreshed(vec![path])
        );
    }

    #[test]
    fn watcher_two_path_rename_maps_to_refresh_pair() {
        let root = Path::new("/tmp/root");
        let from = root.join("before.txt");
        let to = root.join("after.txt");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Rename,
            paths: vec![from.clone(), to.clone()],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::Renamed { from, to }
        );
    }

    #[test]
    fn watcher_partial_rename_uses_full_reload() {
        let root = Path::new("/tmp/root");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Rename,
            paths: vec![root.join("only-one-side.txt")],
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::FullReload
        );
    }

    #[test]
    fn unclassified_watcher_delta_uses_full_reload() {
        let root = Path::new("/tmp/root");
        let delta = WatcherDelta {
            kind: WatcherDeltaKind::Rescan,
            paths: Vec::new(),
        };

        assert_eq!(
            classify_watcher_delta(root, delta),
            ClassifiedWatcherDelta::FullReload
        );
    }
}
