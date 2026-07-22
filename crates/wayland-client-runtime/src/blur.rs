use crate::LogicalRect;

/// The portion of a surface for which compositor background blur is requested.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlurRegion {
    /// Blur behind the complete surface.
    EntireSurface,
    /// Blur only behind the union of these surface-local rectangles.
    Rectangles(Vec<LogicalRect>),
}

/// Current blur request for a surface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BlurState {
    #[default]
    Disabled,
    Enabled(BlurRegion),
}
