pub(crate) type UiColor = [f32; 4];

pub(crate) const BREEZE_ITEM_ROUNDNESS: f32 = 5.0;

const BREEZE_HIGHLIGHT: UiColor = [0.239, 0.502, 0.710, 1.0];
const BREEZE_TEXT: UiColor = [0.188, 0.220, 0.259, 1.0];
const VIEW_BASE: UiColor = [0.973, 0.976, 0.984, 1.0];
const VIEW_ALTERNATE_BASE: UiColor = [0.949, 0.957, 0.969, 1.0];

pub(crate) fn item_background_color(selected: bool, hovered: bool) -> UiColor {
    match (selected, hovered) {
        (true, true) => with_alpha(BREEZE_HIGHLIGHT, 0.40),
        (true, false) => with_alpha(BREEZE_HIGHLIGHT, 0.32),
        (false, true) => with_alpha(BREEZE_TEXT, 0.06),
        (false, false) => transparent(),
    }
}

pub(crate) fn details_row_background_color(
    selected: bool,
    hovered: bool,
    alternate: bool,
) -> UiColor {
    match (selected, hovered, alternate) {
        (true, _, _) | (false, true, _) => item_background_color(selected, hovered),
        (false, false, true) => VIEW_ALTERNATE_BASE,
        (false, false, false) => VIEW_BASE,
    }
}

pub(crate) fn place_row_background_color(active: bool, hovered: bool) -> UiColor {
    item_background_color(active, hovered)
}

const fn with_alpha(mut color: UiColor, alpha: f32) -> UiColor {
    color[3] = alpha;
    color
}

const fn transparent() -> UiColor {
    [0.0, 0.0, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breeze_item_background_uses_dolphin_alpha_levels() {
        assert_eq!(
            item_background_color(true, false),
            [0.239, 0.502, 0.710, 0.32]
        );
        assert_eq!(
            item_background_color(true, true),
            [0.239, 0.502, 0.710, 0.40]
        );
        assert_eq!(
            item_background_color(false, true),
            [0.188, 0.220, 0.259, 0.06]
        );
        assert_eq!(item_background_color(false, false), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn details_rows_keep_base_colors_when_not_interactive() {
        assert_eq!(details_row_background_color(false, false, false), VIEW_BASE);
        assert_eq!(
            details_row_background_color(false, false, true),
            VIEW_ALTERNATE_BASE
        );
    }
}
