use super::entries::{RawFileEntry, to_raw_file_entry};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchProgress {
    pub directories_scanned: usize,
    pub matches_found: usize,
}

#[cfg(test)]
pub async fn search_recursive(
    root: &Path,
    query: &str,
    cancel: Arc<AtomicBool>,
) -> io::Result<Vec<RawFileEntry>> {
    search_recursive_with_progress(root, query, cancel, |_| {}).await
}

pub async fn search_recursive_with_progress<F>(
    root: &Path,
    query: &str,
    cancel: Arc<AtomicBool>,
    mut progress: F,
) -> io::Result<Vec<RawFileEntry>>
where
    F: FnMut(SearchProgress) + Send,
{
    let query = query.to_ascii_lowercase();
    let mut results = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let mut progress_state = SearchProgress::default();

    progress(progress_state);

    while let Some(dir) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "recursive search cancelled",
            ));
        }

        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        while let Some(entry) = entries.next_entry().await? {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "recursive search cancelled",
                ));
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().trim().to_string();
            let metadata = match entry.metadata().await {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            if metadata.is_dir() {
                pending.push(path.clone());
            }

            let path_text = path.to_string_lossy();
            if name.to_ascii_lowercase().contains(&query)
                || path_text.to_ascii_lowercase().contains(&query)
            {
                let location = path
                    .parent()
                    .map(|parent| display_relative_location(root, parent))
                    .unwrap_or_default();
                results.push(to_raw_file_entry(path, name, location, metadata));
                progress_state.matches_found += 1;
            }
        }

        progress_state.directories_scanned += 1;
        if should_report_progress(progress_state) {
            progress(progress_state);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "recursive search cancelled",
        ));
    }

    results.sort_by(|left, right| {
        left.location
            .to_ascii_lowercase()
            .cmp(&right.location.to_ascii_lowercase())
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
    });
    annotate_location_groups(&mut results);
    progress(progress_state);
    Ok(results)
}

fn should_report_progress(progress: SearchProgress) -> bool {
    progress.directories_scanned == 1 || progress.directories_scanned.is_multiple_of(8)
}

fn annotate_location_groups(results: &mut [RawFileEntry]) {
    let mut previous_location: Option<&str> = None;

    for entry in results {
        if previous_location != Some(entry.location.as_str()) {
            entry.group = search_group_label(&entry.location);
            previous_location = Some(entry.location.as_str());
        } else {
            entry.group.clear();
        }
    }
}

fn search_group_label(location: &str) -> String {
    if location == "." {
        "Current folder".to_string()
    } else if location.is_empty() {
        "Unknown location".to_string()
    } else {
        location.to_string()
    }
}

fn display_relative_location(root: &Path, parent: &Path) -> String {
    if parent == root {
        ".".to_string()
    } else {
        parent
            .strip_prefix(root)
            .ok()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| parent.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn formats_root_relative_location() {
        assert_eq!(
            display_relative_location(&PathBuf::from("/tmp/root"), &PathBuf::from("/tmp/root/a")),
            "a"
        );
        assert_eq!(
            display_relative_location(&PathBuf::from("/tmp/root"), &PathBuf::from("/tmp/root")),
            "."
        );
    }

    #[test]
    fn annotates_first_result_in_each_location_group() {
        let mut results = vec![
            RawFileEntry {
                name: "alpha".to_string(),
                path: "/tmp/root/alpha".to_string(),
                group: String::new(),
                location: ".".to_string(),
                kind: "File".to_string(),
                size: "0 B".to_string(),
                size_bytes: 0,
                modified: "-".to_string(),
                modified_age_days: -1,
                is_dir: false,
            },
            RawFileEntry {
                name: "beta".to_string(),
                path: "/tmp/root/beta".to_string(),
                group: String::new(),
                location: ".".to_string(),
                kind: "File".to_string(),
                size: "0 B".to_string(),
                size_bytes: 0,
                modified: "-".to_string(),
                modified_age_days: -1,
                is_dir: false,
            },
            RawFileEntry {
                name: "gamma".to_string(),
                path: "/tmp/root/docs/gamma".to_string(),
                group: String::new(),
                location: "docs".to_string(),
                kind: "File".to_string(),
                size: "0 B".to_string(),
                size_bytes: 0,
                modified: "-".to_string(),
                modified_age_days: -1,
                is_dir: false,
            },
        ];

        annotate_location_groups(&mut results);

        assert_eq!(results[0].group, "Current folder");
        assert_eq!(results[1].group, "");
        assert_eq!(results[2].group, "docs");
    }

    #[test]
    fn progress_reports_first_and_periodic_directory_batches() {
        assert!(should_report_progress(SearchProgress {
            directories_scanned: 1,
            matches_found: 0,
        }));
        assert!(should_report_progress(SearchProgress {
            directories_scanned: 8,
            matches_found: 3,
        }));
        assert!(!should_report_progress(SearchProgress {
            directories_scanned: 7,
            matches_found: 3,
        }));
    }

    #[test]
    fn recursive_search_stops_when_cancelled_before_scan() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(true));

        let result = runtime.block_on(search_recursive(Path::new("/tmp"), "anything", cancel));

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
}
