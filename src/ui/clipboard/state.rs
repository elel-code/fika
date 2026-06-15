use std::path::PathBuf;

use fika_core::{FileClipboardRole, decode_file_clipboard_text, encode_file_clipboard_text};
use gpui::{ClipboardEntry, ClipboardItem};

use crate::ui::drag_drop::FileTransferMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClipboardMode {
    Copy,
    Cut,
}

impl ClipboardMode {
    pub(crate) fn transfer_mode(self) -> FileTransferMode {
        match self {
            Self::Copy => FileTransferMode::Copy,
            Self::Cut => FileTransferMode::Move,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Cut => "Move",
        }
    }

    fn file_clipboard_role(self) -> FileClipboardRole {
        match self {
            Self::Copy => FileClipboardRole::Copy,
            Self::Cut => FileClipboardRole::Cut,
        }
    }

    fn from_file_clipboard_role(role: FileClipboardRole) -> Self {
        match role {
            FileClipboardRole::Copy => Self::Copy,
            FileClipboardRole::Cut => Self::Cut,
        }
    }

    fn metadata_tag(self) -> &'static str {
        match self {
            Self::Copy => "fika-file-clipboard:copy",
            Self::Cut => "fika-file-clipboard:cut",
        }
    }

    fn from_metadata_tag(tag: &str) -> Option<Self> {
        match tag {
            "fika-file-clipboard:copy" => Some(Self::Copy),
            "fika-file-clipboard:cut" => Some(Self::Cut),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardState {
    pub(crate) mode: ClipboardMode,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) text: Option<String>,
}

impl ClipboardState {
    pub(crate) fn files(mode: ClipboardMode, paths: Vec<PathBuf>) -> Self {
        Self {
            mode,
            paths,
            text: None,
        }
    }

    pub(crate) fn text(text: String) -> Option<Self> {
        (!text.is_empty()).then_some(Self {
            mode: ClipboardMode::Copy,
            paths: Vec::new(),
            text: Some(text),
        })
    }

    pub(crate) fn to_clipboard_item(&self) -> ClipboardItem {
        if let Some(text) = &self.text {
            return ClipboardItem::new_string(text.clone());
        }
        ClipboardItem::new_string_with_metadata(
            encode_file_clipboard_text(self.mode.file_clipboard_role(), &self.paths),
            self.mode.metadata_tag().to_string(),
        )
    }

    pub(crate) fn from_clipboard_item(item: &ClipboardItem) -> Option<Self> {
        let metadata_mode = item
            .metadata()
            .and_then(|tag| ClipboardMode::from_metadata_tag(tag.as_str()));
        let external_paths = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths()),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if !external_paths.is_empty() {
            return Some(Self {
                mode: metadata_mode.unwrap_or(ClipboardMode::Copy),
                paths: external_paths,
                text: None,
            });
        }

        let text = item.text()?;
        if let Some(payload) = decode_file_clipboard_text(&text) {
            return Some(Self {
                mode: metadata_mode
                    .unwrap_or_else(|| ClipboardMode::from_file_clipboard_role(payload.role)),
                paths: payload.paths,
                text: None,
            });
        }

        Self::text(text)
    }

    #[allow(dead_code)]
    fn item_count(&self) -> usize {
        if self.text.is_some() {
            1
        } else {
            self.paths.len()
        }
    }

    pub(crate) fn action_label(&self) -> &'static str {
        if self.text.is_some() {
            "Paste"
        } else {
            self.mode.label()
        }
    }

    #[allow(dead_code)]
    pub(crate) fn progress_label(&self) -> String {
        if self.text.is_some() {
            "Pasting text".to_string()
        } else {
            self.mode.transfer_mode().progress_label(self.item_count())
        }
    }
}

pub(crate) fn standard_paste_clipboard_state(
    clipboard: Option<&ClipboardItem>,
    primary: Option<&ClipboardItem>,
) -> Option<ClipboardState> {
    clipboard
        .and_then(ClipboardState::from_clipboard_item)
        .or_else(|| primary.and_then(ClipboardState::from_clipboard_item))
}

pub(crate) fn primary_paste_clipboard_state(
    primary: Option<&ClipboardItem>,
) -> Option<ClipboardState> {
    primary.and_then(ClipboardState::from_clipboard_item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_state_round_trips_file_clipboard_item_metadata() {
        let paths = vec![
            PathBuf::from("/tmp/fika clipboard/one.txt"),
            PathBuf::from("/tmp/fika clipboard/two.txt"),
        ];
        let clipboard = ClipboardState::files(ClipboardMode::Cut, paths.clone());
        let item = clipboard.to_clipboard_item();

        assert_eq!(
            ClipboardState::from_clipboard_item(&item),
            Some(ClipboardState::files(ClipboardMode::Cut, paths))
        );
    }

    #[test]
    fn clipboard_state_imports_uri_list_text_and_plain_text() {
        let uri_list =
            ClipboardItem::new_string("copy\nfile:///tmp/fika%20clipboard.txt\n".to_string());
        assert_eq!(
            ClipboardState::from_clipboard_item(&uri_list),
            Some(ClipboardState::files(
                ClipboardMode::Copy,
                vec![PathBuf::from("/tmp/fika clipboard.txt")]
            ))
        );

        let plain = ClipboardItem::new_string("hello from clipboard".to_string());
        assert_eq!(
            ClipboardState::from_clipboard_item(&plain),
            ClipboardState::text("hello from clipboard".to_string())
        );
    }

    #[test]
    fn standard_paste_clipboard_state_prefers_clipboard_over_primary() {
        let clipboard = ClipboardItem::new_string("regular clipboard".to_string());
        let primary = ClipboardItem::new_string("primary selection".to_string());

        assert_eq!(
            standard_paste_clipboard_state(Some(&clipboard), Some(&primary)),
            ClipboardState::text("regular clipboard".to_string())
        );
    }

    #[test]
    fn primary_paste_clipboard_state_reads_only_primary_selection() {
        let primary = ClipboardItem::new_string("primary selection".to_string());

        assert_eq!(
            primary_paste_clipboard_state(Some(&primary)),
            ClipboardState::text("primary selection".to_string())
        );
        assert_eq!(primary_paste_clipboard_state(None), None);
    }
}
