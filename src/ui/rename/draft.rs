use std::path::PathBuf;

use fika_core::PaneId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenameDraft {
    pub(crate) pane_id: PaneId,
    pub(crate) original_path: PathBuf,
    pub(crate) draft_name: String,
    pub(crate) error: Option<String>,
}

impl RenameDraft {
    pub(crate) fn extension_warning(&self, is_dir: bool) -> Option<String> {
        if is_dir {
            return None;
        }
        let original_extension = self
            .original_path
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty());
        let draft_name = self.draft_name.trim();
        let draft_extension = std::path::Path::new(draft_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty());
        if original_extension == draft_extension {
            return None;
        }

        Some(match (original_extension, draft_extension) {
            (Some(original), Some(next)) => {
                format!("Extension changes .{original} -> .{next}")
            }
            (Some(original), None) => format!("Extension .{original} will be removed"),
            (None, Some(next)) => format!("Extension .{next} will be added"),
            (None, None) => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_warning_tracks_file_extension_changes() {
        let mut draft = RenameDraft {
            pane_id: PaneId(1),
            original_path: PathBuf::from("/tmp/report.txt"),
            draft_name: "report.md".to_string(),
            error: None,
        };

        assert_eq!(
            draft.extension_warning(false),
            Some("Extension changes .txt -> .md".to_string())
        );

        draft.draft_name = "report.txt".to_string();
        assert_eq!(draft.extension_warning(false), None);

        draft.draft_name = "report".to_string();
        assert_eq!(
            draft.extension_warning(false),
            Some("Extension .txt will be removed".to_string())
        );
    }

    #[test]
    fn extension_warning_ignores_directories() {
        let draft = RenameDraft {
            pane_id: PaneId(1),
            original_path: PathBuf::from("/tmp/archive.d"),
            draft_name: "archive.txt".to_string(),
            error: None,
        };

        assert_eq!(draft.extension_warning(true), None);
    }
}
