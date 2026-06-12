#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocationEditMetrics {
    pub(crate) value: String,
    pub(crate) origin_x: f32,
    pub(crate) scroll_x: f32,
    pub(crate) visible_width: f32,
    pub(crate) byte_positions: Vec<(usize, f32)>,
}
