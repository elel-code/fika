use std::error::Error;
use std::path::PathBuf;

use fika_core::{Entry, ViewMode, read_entries_sync};

pub(crate) struct SctkScene {
    path: PathBuf,
    view_mode: ViewMode,
    entries: Vec<Entry>,
    dir_count: usize,
}

impl SctkScene {
    pub(crate) fn load(path: PathBuf, view_mode: ViewMode) -> Result<Self, Box<dyn Error>> {
        let entries = read_entries_sync(&path)?;
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        Ok(Self {
            path,
            view_mode,
            entries,
            dir_count,
        })
    }

    pub(crate) fn log_startup(&self) {
        eprintln!(
            "[fika-sctk] path={} view={} entries={} dirs={} files={}",
            self.path.display(),
            self.view_mode.as_str(),
            self.entries.len(),
            self.dir_count,
            self.file_count()
        );
    }

    pub(crate) fn path(&self) -> &PathBuf {
        &self.path
    }

    pub(crate) fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn dir_count(&self) -> usize {
        self.dir_count
    }

    pub(crate) fn file_count(&self) -> usize {
        self.entries.len().saturating_sub(self.dir_count)
    }
}
