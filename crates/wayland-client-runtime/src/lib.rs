//! A Wayland-native client runtime built on Smithay Client Toolkit.
//!
//! The crate deliberately exposes Wayland concepts instead of reproducing a
//! cross-platform window API. Protocol objects and their parent/child ordering
//! are owned by [`Runtime`]; renderers receive [`SurfaceHandle`] values that
//! implement raw-window-handle 0.6 for both wgpu and direct Vulkan use.

mod activation;
mod blur;
pub mod clipboard;
pub mod data_transfer;
mod dnd;
mod event;
mod fractional_scale;
mod geometry;
mod input;
mod runtime;
mod shm_format;
mod surface;
mod toplevel_icon;
mod touch;

pub use activation::{
    ActivationEvent, ActivationRequestId, ActivationToken, ActivationTokenAttributes,
};
pub use blur::{BlurRegion, BlurState};
pub use data_transfer::{MimePayload, TransferContent, TransferError, TransferReadPipe};
pub use dnd::{
    DndAction, DndActions, DndEvent, DndIcon, DndMimePayload, DndOfferId, DndReadPipe, DndSourceId,
};
pub use event::{
    Event, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind, PopupConfigureKind,
    SurfaceEvent, ToplevelState, TouchEvent, TouchEventKind,
};
pub use geometry::{LogicalPosition, LogicalRect, LogicalSize, SuggestedSize};
pub use input::{CursorIcon, InputSerial, InputSerialSource};
pub use runtime::{Runtime, RuntimeCapabilities, RuntimeError, RuntimeOptions, WakeHandle};
pub use surface::{
    ConstraintAdjustments, DecorationPreference, DialogAttributes, Gravity, PopupAnchor,
    PopupAttributes, PopupPositioner, SurfaceHandle, SurfaceId, SurfaceKind, ToplevelAttributes,
};
pub use toplevel_icon::{ToplevelIcon, ToplevelIconBuffer, ToplevelIconError};
