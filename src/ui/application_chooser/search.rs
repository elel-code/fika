const SEARCH_AVERAGE_UNIT_WIDTH: f32 = 7.0;

pub(crate) fn application_chooser_search_clamped_caret(text: &str, caret: usize) -> usize {
    let mut caret = caret.min(text.len());
    while caret > 0 && !text.is_char_boundary(caret) {
        caret -= 1;
    }
    caret
}

pub(crate) fn application_chooser_search_previous_boundary(text: &str, caret: usize) -> usize {
    let caret = application_chooser_search_clamped_caret(text, caret);
    if caret == 0 {
        return 0;
    }
    text[..caret]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(crate) fn application_chooser_search_next_boundary(text: &str, caret: usize) -> usize {
    let caret = application_chooser_search_clamped_caret(text, caret);
    if caret >= text.len() {
        return text.len();
    }
    text[caret..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| caret + index)
        .unwrap_or(text.len())
}

pub(crate) fn application_chooser_search_parts(
    text: &str,
    caret: usize,
) -> (&str, &str) {
    let caret = application_chooser_search_clamped_caret(text, caret);
    text.split_at(caret)
}

pub(crate) fn application_chooser_search_caret_for_local_x(
    text: &str,
    local_x: f32,
) -> usize {
    if text.is_empty() {
        return 0;
    }

    let local_x = local_x.max(0.0);
    search_byte_positions(text)
        .into_iter()
        .min_by(|left, right| {
            (left.1 - local_x)
                .abs()
                .total_cmp(&(right.1 - local_x).abs())
                .then_with(|| left.0.cmp(&right.0))
        })
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn search_byte_positions(text: &str) -> Vec<(usize, f32)> {
    let mut positions = Vec::with_capacity(text.chars().count() + 1);
    let mut x = 0.0;
    positions.push((0, x));
    for (index, ch) in text.char_indices() {
        x += search_char_width_units(ch) * SEARCH_AVERAGE_UNIT_WIDTH;
        positions.push((index + ch.len_utf8(), x));
    }
    positions
}

fn search_char_width_units(ch: char) -> f32 {
    if ch.is_ascii() { 1.0 } else { 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_caret_clamps_to_utf8_boundary() {
        let text = "ka\u{00df}e";

        assert_eq!(
            application_chooser_search_clamped_caret(text, text.len() + 20),
            text.len()
        );
        assert_eq!(
            application_chooser_search_clamped_caret(text, 3),
            "ka".len()
        );
        assert!(text.is_char_boundary(application_chooser_search_clamped_caret(text, 3)));
    }

    #[test]
    fn search_caret_moves_by_utf8_character() {
        let text = "a\u{76ee}b";
        let after_wide = "a\u{76ee}".len();

        assert_eq!(application_chooser_search_next_boundary(text, 0), 1);
        assert_eq!(application_chooser_search_next_boundary(text, 1), after_wide);
        assert_eq!(
            application_chooser_search_previous_boundary(text, after_wide),
            1
        );
        assert_eq!(application_chooser_search_previous_boundary(text, 1), 0);
    }

    #[test]
    fn search_caret_hit_test_uses_nearest_boundary() {
        assert_eq!(application_chooser_search_caret_for_local_x("kate", -8.0), 0);
        assert_eq!(application_chooser_search_caret_for_local_x("kate", 6.0), 1);
        assert_eq!(
            application_chooser_search_caret_for_local_x("kate", 400.0),
            "kate".len()
        );
    }

    #[test]
    fn search_parts_split_at_clamped_caret() {
        assert_eq!(
            application_chooser_search_parts("writer", 3),
            ("wri", "ter")
        );
        assert_eq!(
            application_chooser_search_parts("\u{76ee}\u{9304}", 4),
            ("\u{76ee}", "\u{9304}")
        );
    }
}
