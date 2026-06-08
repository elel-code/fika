pub(crate) const ZOOM_LEVEL_MIN: i32 = 0;
pub(crate) const ZOOM_LEVEL_MAX: i32 = 16;

const ICON_SIZE_SMALL: u32 = 16;
const ICON_SIZE_SMALL_MEDIUM: u32 = 22;
const ICON_SIZE_MEDIUM: u32 = 32;
const ICON_SIZE_LARGE: u32 = 48;
const ICON_SIZE_HUGE: u32 = 64;

pub(crate) fn clamp_zoom_level(level: i32) -> i32 {
    level.clamp(ZOOM_LEVEL_MIN, ZOOM_LEVEL_MAX)
}

pub(crate) fn icon_size_for_zoom_level(level: i32) -> u32 {
    match clamp_zoom_level(level) {
        0 => ICON_SIZE_SMALL,
        1 => ICON_SIZE_SMALL_MEDIUM,
        2 => ICON_SIZE_MEDIUM,
        3 => ICON_SIZE_LARGE,
        4 => ICON_SIZE_HUGE,
        level => ICON_SIZE_HUGE + ((level - 4) as u32 * 16),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_size_for_zoom_level_matches_dolphin_zoom_level_info() {
        assert_eq!(icon_size_for_zoom_level(0), 16);
        assert_eq!(icon_size_for_zoom_level(1), 22);
        assert_eq!(icon_size_for_zoom_level(2), 32);
        assert_eq!(icon_size_for_zoom_level(3), 48);
        assert_eq!(icon_size_for_zoom_level(4), 64);
        assert_eq!(icon_size_for_zoom_level(5), 80);
        assert_eq!(icon_size_for_zoom_level(16), 256);
    }

    #[test]
    fn zoom_level_is_clamped_to_dolphin_range() {
        assert_eq!(clamp_zoom_level(-1), ZOOM_LEVEL_MIN);
        assert_eq!(clamp_zoom_level(20), ZOOM_LEVEL_MAX);
    }
}
