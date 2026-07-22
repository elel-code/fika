/// A position in surface-local logical coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct LogicalPosition {
    pub x: i32,
    pub y: i32,
}

impl LogicalPosition {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A size in surface-local logical coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct LogicalSize {
    pub width: u32,
    pub height: u32,
}

/// Per-axis size suggestion from an xdg-toplevel configure.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SuggestedSize {
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl SuggestedSize {
    pub const fn new(width: Option<u32>, height: Option<u32>) -> Self {
        Self { width, height }
    }
}

impl LogicalSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// A rectangle in surface-local logical coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct LogicalRect {
    pub origin: LogicalPosition,
    pub size: LogicalSize,
}

impl LogicalRect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(width, height),
        }
    }

    pub const fn is_empty(self) -> bool {
        self.size.is_empty()
    }
}
