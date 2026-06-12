use fika_core::PaneId;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocationDraft {
    pub(crate) pane_id: PaneId,
    pub(crate) value: String,
    pub(crate) caret: usize,
    pub(crate) scroll_x: f32,
}

impl LocationDraft {
    pub(crate) fn new(pane_id: PaneId, value: String) -> Self {
        let caret = value.len();
        Self {
            pane_id,
            value,
            caret,
            scroll_x: 0.0,
        }
    }

    pub(crate) fn snapshot(&self) -> LocationDraftSnapshot {
        LocationDraftSnapshot {
            value: self.value.clone(),
            caret: self.caret,
            scroll_x: self.scroll_x,
        }
    }

    pub(crate) fn set_caret(&mut self, caret: usize) {
        self.caret = clamp_text_boundary(&self.value, caret);
    }

    pub(crate) fn move_to_start(&mut self) {
        self.caret = 0;
        self.scroll_x = 0.0;
    }

    pub(crate) fn move_to_end(&mut self) {
        self.caret = self.value.len();
    }

    pub(crate) fn move_backward(&mut self) {
        self.caret = previous_text_boundary(&self.value, self.caret);
    }

    pub(crate) fn move_forward(&mut self) {
        self.caret = next_text_boundary(&self.value, self.caret);
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let caret = clamp_text_boundary(&self.value, self.caret);
        self.value.insert_str(caret, text);
        self.caret = caret + text.len();
    }

    pub(crate) fn delete_backward(&mut self) {
        let end = clamp_text_boundary(&self.value, self.caret);
        let start = previous_text_boundary(&self.value, end);
        if start == end {
            return;
        }
        self.value.drain(start..end);
        self.caret = start;
    }

    pub(crate) fn delete_forward(&mut self) {
        let start = clamp_text_boundary(&self.value, self.caret);
        let end = next_text_boundary(&self.value, start);
        if start == end {
            return;
        }
        self.value.drain(start..end);
        self.caret = start;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocationDraftSnapshot {
    pub(crate) value: String,
    pub(crate) caret: usize,
    pub(crate) scroll_x: f32,
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
    text[..index]
        .char_indices()
        .last()
        .map(|(boundary, _)| boundary)
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
    use super::LocationDraft;
    use fika_core::PaneId;

    #[test]
    fn location_draft_edits_at_caret_and_preserves_utf8_boundaries() {
        let mut draft = LocationDraft::new(PaneId(7), "/tmp/\u{76ee}\u{5f55}".to_string());
        draft.set_caret(6);
        draft.insert("a");
        assert_eq!(draft.value, "/tmp/a\u{76ee}\u{5f55}");
        assert_eq!(draft.caret, "/tmp/a".len());

        draft.delete_backward();
        assert_eq!(draft.value, "/tmp/\u{76ee}\u{5f55}");
        assert_eq!(draft.caret, 5);

        draft.set_caret(6);
        assert_eq!(draft.caret, 5);
        draft.move_forward();
        assert_eq!(draft.caret, "/tmp/\u{76ee}".len());
        draft.delete_forward();
        assert_eq!(draft.value, "/tmp/\u{76ee}");
        draft.move_to_start();
        assert_eq!(draft.caret, 0);
        draft.move_to_end();
        assert_eq!(draft.caret, draft.value.len());
    }
}
