use std::path::PathBuf;

use fika_core::PaneId;

use super::metrics::rename_caret_for_local_x;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenameDraft {
    pub(crate) pane_id: PaneId,
    pub(crate) original_path: PathBuf,
    pub(crate) draft_name: String,
    pub(crate) caret: usize,
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) error: Option<String>,
    pub(crate) privileged: bool,
}

impl RenameDraft {
    pub(crate) fn new(pane_id: PaneId, original_path: PathBuf, draft_name: String) -> Self {
        let (selection_start, selection_end) = default_stem_selection(&draft_name);
        Self {
            pane_id,
            original_path,
            caret: selection_end,
            selection: (selection_start < selection_end)
                .then_some((selection_start, selection_end)),
            draft_name,
            error: None,
            privileged: false,
        }
    }

    pub(crate) fn new_privileged(
        pane_id: PaneId,
        original_path: PathBuf,
        draft_name: String,
    ) -> Self {
        let mut draft = Self::new(pane_id, original_path, draft_name);
        draft.privileged = true;
        draft
    }

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

    pub(crate) fn retarget_original_path(&mut self, original_path: PathBuf) {
        self.original_path = original_path;
    }

    pub(crate) fn move_to_start(&mut self) {
        self.caret = 0;
        self.selection = None;
    }

    pub(crate) fn move_to_end(&mut self) {
        self.caret = self.draft_name.len();
        self.selection = None;
    }

    pub(crate) fn move_backward(&mut self) {
        if let Some((start, _)) = self.normalized_selection() {
            self.caret = start;
            self.selection = None;
            return;
        }
        self.caret = previous_text_boundary(&self.draft_name, self.caret);
        self.selection = None;
    }

    pub(crate) fn move_forward(&mut self) {
        if let Some((_, end)) = self.normalized_selection() {
            self.caret = end;
            self.selection = None;
            return;
        }
        self.caret = next_text_boundary(&self.draft_name, self.caret);
        self.selection = None;
    }

    pub(crate) fn select_all(&mut self) {
        self.caret = self.draft_name.len();
        self.selection = (!self.draft_name.is_empty()).then_some((0, self.draft_name.len()));
    }

    pub(crate) fn select_to_start(&mut self) {
        let anchor = self.selection_anchor();
        self.set_caret_with_anchor(0, anchor);
    }

    pub(crate) fn select_to_end(&mut self) {
        let anchor = self.selection_anchor();
        self.set_caret_with_anchor(self.draft_name.len(), anchor);
    }

    pub(crate) fn select_backward(&mut self) {
        let anchor = self.selection_anchor();
        let caret = previous_text_boundary(&self.draft_name, self.caret);
        self.set_caret_with_anchor(caret, anchor);
    }

    pub(crate) fn select_forward(&mut self) {
        let anchor = self.selection_anchor();
        let caret = next_text_boundary(&self.draft_name, self.caret);
        self.set_caret_with_anchor(caret, anchor);
    }

    pub(crate) fn set_caret(&mut self, caret: usize) {
        self.caret = clamp_text_boundary(&self.draft_name, caret);
        self.selection = None;
    }

    pub(crate) fn set_caret_from_local_x(&mut self, local_x: f32) {
        self.set_caret(rename_caret_for_local_x(&self.draft_name, local_x));
    }

    pub(crate) fn insert(&mut self, text: &str) {
        self.replace_selection_or_delete_none();
        let caret = clamp_text_boundary(&self.draft_name, self.caret);
        self.draft_name.insert_str(caret, text);
        self.caret = caret + text.len();
        self.selection = None;
    }

    pub(crate) fn delete_backward(&mut self) {
        if self.replace_selection_or_delete_none() {
            return;
        }
        let end = clamp_text_boundary(&self.draft_name, self.caret);
        let start = previous_text_boundary(&self.draft_name, end);
        if start == end {
            return;
        }
        self.draft_name.drain(start..end);
        self.caret = start;
    }

    pub(crate) fn delete_forward(&mut self) {
        if self.replace_selection_or_delete_none() {
            return;
        }
        let start = clamp_text_boundary(&self.draft_name, self.caret);
        let end = next_text_boundary(&self.draft_name, start);
        if start == end {
            return;
        }
        self.draft_name.drain(start..end);
        self.caret = start;
    }

    fn replace_selection_or_delete_none(&mut self) -> bool {
        let Some((start, end)) = self.normalized_selection() else {
            return false;
        };
        self.draft_name.drain(start..end);
        self.caret = start;
        self.selection = None;
        true
    }

    fn set_caret_with_anchor(&mut self, caret: usize, anchor: usize) {
        let caret = clamp_text_boundary(&self.draft_name, caret);
        let anchor = clamp_text_boundary(&self.draft_name, anchor);
        self.caret = caret;
        self.selection = (caret != anchor).then_some((anchor, caret));
    }

    fn selection_anchor(&self) -> usize {
        let caret = clamp_text_boundary(&self.draft_name, self.caret);
        let Some((start, end)) = self.normalized_selection() else {
            return caret;
        };
        if caret <= start { end } else { start }
    }

    fn normalized_selection(&self) -> Option<(usize, usize)> {
        let (raw_start, raw_end) = self.selection?;
        let start = clamp_text_boundary(&self.draft_name, raw_start.min(raw_end));
        let end = clamp_text_boundary(&self.draft_name, raw_start.max(raw_end));
        (start < end).then_some((start, end))
    }
}

fn default_stem_selection(name: &str) -> (usize, usize) {
    match name.rfind('.') {
        Some(index) if index > 0 && index < name.len() - 1 => (0, index),
        _ => (0, name.len()),
    }
}

fn clamp_text_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn previous_text_boundary(text: &str, index: usize) -> usize {
    let index = clamp_text_boundary(text, index);
    if index == 0 {
        return 0;
    }
    text[..index]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_text_boundary(text: &str, index: usize) -> usize {
    let index = clamp_text_boundary(text, index);
    if index >= text.len() {
        return text.len();
    }
    text[index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| index + offset)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_warning_tracks_file_extension_changes() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/report.txt"),
            "report.md".to_string(),
        );

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
        let draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/archive.d"),
            "archive.txt".to_string(),
        );

        assert_eq!(draft.extension_warning(true), None);
    }

    #[test]
    fn rename_starts_with_stem_selected_and_replaces_selection() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/report.txt"),
            "report.txt".to_string(),
        );

        assert_eq!(draft.selection, Some((0, "report".len())));
        assert_eq!(draft.caret, "report".len());

        draft.insert("summary");

        assert_eq!(draft.draft_name, "summary.txt");
        assert_eq!(draft.caret, "summary".len());
        assert_eq!(draft.selection, None);
    }

    #[test]
    fn rename_selection_collapse_and_extend_behaves_like_text_input() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/report.txt"),
            "report.txt".to_string(),
        );

        draft.move_backward();
        assert_eq!(draft.caret, 0);
        assert_eq!(draft.selection, None);

        draft.selection = Some((0, "report".len()));
        draft.caret = "report".len();
        draft.move_forward();
        assert_eq!(draft.caret, "report".len());
        assert_eq!(draft.selection, None);

        draft.select_forward();
        assert_eq!(draft.caret, "report.".len());
        assert_eq!(draft.selection, Some(("report".len(), "report.".len())));

        draft.select_to_start();
        assert_eq!(draft.caret, 0);
        assert_eq!(draft.selection, Some(("report".len(), 0)));

        draft.select_all();
        assert_eq!(draft.caret, "report.txt".len());
        assert_eq!(draft.selection, Some((0, "report.txt".len())));
    }

    #[test]
    fn rename_shift_selection_respects_utf8_boundaries() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/目录.txt"),
            "目录.txt".to_string(),
        );
        draft.move_to_start();

        draft.select_forward();
        assert_eq!(draft.caret, "目".len());
        assert_eq!(draft.selection, Some((0, "目".len())));

        draft.select_forward();
        assert_eq!(draft.caret, "目录".len());
        assert_eq!(draft.selection, Some((0, "目录".len())));
        assert!(draft.draft_name.is_char_boundary(draft.caret));
    }

    #[test]
    fn rename_delete_handles_selection_and_utf8_boundaries() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/目录.txt"),
            "目录.txt".to_string(),
        );
        draft.move_to_end();
        draft.delete_backward();

        assert_eq!(draft.draft_name, "目录.tx");

        draft.selection = Some((0, "目".len()));
        draft.delete_forward();

        assert_eq!(draft.draft_name, "录.tx");
        assert_eq!(draft.caret, 0);
    }

    #[test]
    fn rename_click_caret_clears_selection_and_uses_utf8_boundary() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/alpha.txt"),
            "alpha.txt".to_string(),
        );

        draft.set_caret_from_local_x(6.0);

        assert_eq!(draft.caret, 1);
        assert_eq!(draft.selection, None);

        let mut unicode = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/目录.txt"),
            "目录.txt".to_string(),
        );
        unicode.set_caret_from_local_x(22.0);

        assert_eq!(unicode.caret, "目录".len());
        assert!(unicode.draft_name.is_char_boundary(unicode.caret));
        assert_eq!(unicode.selection, None);
    }

    #[test]
    fn rename_retarget_preserves_edit_state() {
        let mut draft = RenameDraft::new(
            PaneId(1),
            PathBuf::from("/tmp/old.txt"),
            "old.txt".to_string(),
        );
        draft.draft_name = "custom.txt".to_string();
        draft.caret = 3;
        draft.selection = Some((1, 5));
        draft.error = Some("pending".to_string());

        draft.retarget_original_path(PathBuf::from("/tmp/new.txt"));

        assert_eq!(draft.original_path, PathBuf::from("/tmp/new.txt"));
        assert_eq!(draft.draft_name, "custom.txt");
        assert_eq!(draft.caret, 3);
        assert_eq!(draft.selection, Some((1, 5)));
        assert_eq!(draft.error.as_deref(), Some("pending"));
    }
}
