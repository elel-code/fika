use crate::{InputSerial, SurfaceId};

/// A semantic touchpad gesture reported for a pointer.
#[derive(Clone, Debug)]
pub enum PointerGestureEvent {
    Swipe(PointerSwipeEvent),
    Pinch(PointerPinchEvent),
    Hold(PointerHoldEvent),
}

impl PointerGestureEvent {
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Swipe(event) => event.surface(),
            Self::Pinch(event) => event.surface(),
            Self::Hold(event) => event.surface(),
        }
    }

    /// Input serial carried by begin and end events.
    pub fn serial(&self) -> Option<&InputSerial> {
        match self {
            Self::Swipe(event) => event.serial(),
            Self::Pinch(event) => event.serial(),
            Self::Hold(event) => event.serial(),
        }
    }
}

/// Multi-finger translation in which all fingers move in the same direction.
#[derive(Clone, Debug)]
pub enum PointerSwipeEvent {
    Begin {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        fingers: u32,
    },
    /// Surface-coordinate delta since the previous update.
    Update {
        surface: SurfaceId,
        time: u32,
        delta: (f64, f64),
    },
    End {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        cancelled: bool,
    },
}

impl PointerSwipeEvent {
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Begin { surface, .. }
            | Self::Update { surface, .. }
            | Self::End { surface, .. } => *surface,
        }
    }

    pub fn serial(&self) -> Option<&InputSerial> {
        match self {
            Self::Begin { serial, .. } | Self::End { serial, .. } => Some(serial),
            Self::Update { .. } => None,
        }
    }
}

/// Multi-finger gesture combining translation, scale, and rotation.
#[derive(Clone, Debug)]
pub enum PointerPinchEvent {
    Begin {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        fingers: u32,
    },
    Update {
        surface: SurfaceId,
        time: u32,
        /// Surface-coordinate movement of the logical center since the
        /// previous update.
        delta: (f64, f64),
        /// Absolute scale relative to the finger positions at `Begin`.
        scale: f64,
        /// Clockwise rotation in degrees since the previous gesture event.
        rotation_degrees_cw: f64,
    },
    End {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        cancelled: bool,
    },
}

impl PointerPinchEvent {
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Begin { surface, .. }
            | Self::Update { surface, .. }
            | Self::End { surface, .. } => *surface,
        }
    }

    pub fn serial(&self) -> Option<&InputSerial> {
        match self {
            Self::Begin { serial, .. } | Self::End { serial, .. } => Some(serial),
            Self::Update { .. } => None,
        }
    }
}

/// One or more fingers held without significant movement.
#[derive(Clone, Debug)]
pub enum PointerHoldEvent {
    Begin {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        fingers: u32,
    },
    End {
        surface: SurfaceId,
        serial: InputSerial,
        time: u32,
        cancelled: bool,
    },
}

impl PointerHoldEvent {
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Begin { surface, .. } | Self::End { surface, .. } => *surface,
        }
    }

    pub fn serial(&self) -> Option<&InputSerial> {
        match self {
            Self::Begin { serial, .. } | Self::End { serial, .. } => Some(serial),
        }
    }
}
