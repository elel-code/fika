use bitflags::bitflags;

use crate::{
    ActivationEvent, DndEvent, InputSerial, LogicalPosition, LogicalSize, SuggestedSize, SurfaceId,
};

bitflags! {
    /// State flags reported by an xdg-toplevel configure.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct ToplevelState: u16 {
        const MAXIMIZED = 1 << 0;
        const FULLSCREEN = 1 << 1;
        const RESIZING = 1 << 2;
        const ACTIVATED = 1 << 3;
        const TILED_LEFT = 1 << 4;
        const TILED_RIGHT = 1 << 5;
        const TILED_TOP = 1 << 6;
        const TILED_BOTTOM = 1 << 7;
        const SUSPENDED = 1 << 8;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PopupConfigureKind {
    Initial,
    Reactive,
    Reposition { token: u32 },
}

#[derive(Clone, Debug)]
pub enum SurfaceEvent {
    Configure {
        surface: SurfaceId,
        suggested_size: SuggestedSize,
        state: ToplevelState,
        serial: u32,
    },
    PopupConfigure {
        surface: SurfaceId,
        position: LogicalPosition,
        size: LogicalSize,
        serial: u32,
        kind: PopupConfigureKind,
    },
    CloseRequested {
        surface: SurfaceId,
    },
    PopupDone {
        surface: SurfaceId,
    },
    Frame {
        surface: SurfaceId,
        time: u32,
    },
    ScaleFactorChanged {
        surface: SurfaceId,
        /// Preferred compositor scale. Fractional values are reported when
        /// wp-fractional-scale-v1 is active for the surface.
        factor: f64,
    },
}

#[derive(Clone, Debug)]
pub enum PointerEventKind {
    Enter {
        serial: InputSerial,
    },
    Leave,
    Motion {
        time: u32,
    },
    Press {
        time: u32,
        button: u32,
        serial: InputSerial,
    },
    Release {
        time: u32,
        button: u32,
        serial: InputSerial,
    },
    Axis {
        time: u32,
        horizontal: f64,
        vertical: f64,
    },
}

#[derive(Clone, Debug)]
pub struct PointerEvent {
    pub surface: SurfaceId,
    pub position: (f64, f64),
    pub kind: PointerEventKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub caps_lock: bool,
    pub logo: bool,
    pub num_lock: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyState {
    Pressed,
    Repeated,
    Released,
}

#[derive(Clone, Debug)]
pub enum KeyboardEvent {
    Enter {
        surface: SurfaceId,
        serial: InputSerial,
        pressed_raw_codes: Vec<u32>,
    },
    Leave {
        surface: SurfaceId,
    },
    Key {
        surface: SurfaceId,
        state: KeyState,
        time: u32,
        raw_code: u32,
        keysym: u32,
        text: Option<String>,
        serial: InputSerial,
    },
    Modifiers {
        surface: SurfaceId,
        modifiers: Modifiers,
    },
}

#[derive(Clone, Debug)]
pub enum TouchEventKind {
    Down {
        time: u32,
        id: i32,
        position: (f64, f64),
        serial: InputSerial,
    },
    Up {
        time: u32,
        id: i32,
        serial: InputSerial,
    },
    Motion {
        time: u32,
        id: i32,
        position: (f64, f64),
    },
    Shape {
        id: i32,
        major: f64,
        minor: f64,
    },
    Orientation {
        id: i32,
        degrees: f64,
    },
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct TouchEvent {
    /// Surface associated with this point or cancellation. This is `None` only
    /// for an unmatched up or a cancellation with no tracked live point.
    pub surface: Option<SurfaceId>,
    pub kind: TouchEventKind,
}

#[derive(Clone, Debug)]
pub enum Event {
    Surface(SurfaceEvent),
    Activation(ActivationEvent),
    Pointer(PointerEvent),
    Keyboard(KeyboardEvent),
    Touch(TouchEvent),
    Dnd(DndEvent),
}
