pub(crate) const RENAME_TEXT_INSET_X: f32 = 4.0;
const RENAME_AVERAGE_UNIT_WIDTH: f32 = 7.0;

pub(crate) fn rename_caret_for_local_x(text: &str, local_x: f32) -> usize {
    if text.is_empty() {
        return 0;
    }

    let local_x = local_x.max(0.0);
    rename_byte_positions(text)
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

fn rename_byte_positions(text: &str) -> Vec<(usize, f32)> {
    let mut positions = Vec::with_capacity(text.chars().count() + 1);
    let mut x = 0.0;
    positions.push((0, x));
    for (index, ch) in text.char_indices() {
        x += rename_char_width_units(ch) * RENAME_AVERAGE_UNIT_WIDTH;
        positions.push((index + ch.len_utf8(), x));
    }
    positions
}

fn rename_char_width_units(ch: char) -> f32 {
    if ch.is_ascii() { 1.0 } else { 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_hit_test_uses_nearest_byte_position() {
        assert_eq!(rename_caret_for_local_x("alpha.txt", -10.0), 0);
        assert_eq!(rename_caret_for_local_x("alpha.txt", 1.0), 0);
        assert_eq!(rename_caret_for_local_x("alpha.txt", 6.0), 1);
        assert_eq!(rename_caret_for_local_x("alpha.txt", 18.0), 3);
        assert_eq!(
            rename_caret_for_local_x("alpha.txt", 400.0),
            "alpha.txt".len()
        );
    }

    #[test]
    fn caret_hit_test_keeps_utf8_boundaries() {
        let text = "\u{76ee}\u{9304}.txt";
        assert_eq!(rename_caret_for_local_x(text, 8.0), "\u{76ee}".len());
        assert_eq!(
            rename_caret_for_local_x(text, 22.0),
            "\u{76ee}\u{9304}".len()
        );
        assert!(text.is_char_boundary(rename_caret_for_local_x(text, 22.0)));
    }
}
