use std::error::Error;
use std::path::PathBuf;

use fika_core::{Entry, read_entries_sync};

pub(crate) struct SctkScene {
    path: PathBuf,
    entries: Vec<Entry>,
    dir_count: usize,
}

impl SctkScene {
    pub(crate) fn load(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let entries = read_entries_sync(&path)?;
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        Ok(Self {
            path,
            entries,
            dir_count,
        })
    }

    pub(crate) fn log_startup(&self) {
        eprintln!(
            "[fika-sctk] path={} entries={} dirs={} files={}",
            self.path.display(),
            self.entries.len(),
            self.dir_count,
            self.file_count()
        );
    }

    pub(crate) fn path(&self) -> &PathBuf {
        &self.path
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
