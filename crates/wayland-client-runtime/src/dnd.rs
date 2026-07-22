use std::io::{self, Read};
use std::sync::Arc;

use bitflags::bitflags;
use smithay_client_toolkit::data_device_manager::ReadPipe;

use crate::{LogicalPosition, SurfaceId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DndOfferId(pub(crate) u64);

impl DndOfferId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DndSourceId(pub(crate) u64);

impl DndSourceId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct DndActions: u8 {
        const COPY = 1 << 0;
        const MOVE = 1 << 1;
        const ASK = 1 << 2;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DndAction {
    Copy,
    Move,
    Ask,
}

/// An RGBA drag icon backed by a temporary Wayland SHM surface.
///
/// Pixel dimensions are buffer coordinates. `offset` is expressed in logical
/// surface coordinates relative to the drag hotspot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DndIcon {
    rgba: Arc<[u8]>,
    width: u32,
    height: u32,
    buffer_scale: i32,
    offset: LogicalPosition,
}

impl DndIcon {
    pub fn new(
        rgba: impl Into<Arc<[u8]>>,
        width: u32,
        height: u32,
        buffer_scale: i32,
        offset: LogicalPosition,
    ) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("DnD icon dimensions must be non-zero");
        }
        if width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("DnD icon dimensions exceed Wayland SHM limits");
        }
        if buffer_scale < 1 {
            return Err("DnD icon buffer scale must be at least one");
        }
        if width % buffer_scale as u32 != 0 || height % buffer_scale as u32 != 0 {
            return Err("DnD icon dimensions must be divisible by its buffer scale");
        }
        let rgba = rgba.into();
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("DnD icon byte length overflow")?;
        if rgba.len() != expected {
            return Err("DnD icon RGBA byte length does not match its dimensions");
        }
        Ok(Self {
            rgba,
            width,
            height,
            buffer_scale,
            offset,
        })
    }

    pub fn rgba(&self) -> &Arc<[u8]> {
        &self.rgba
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn buffer_scale(&self) -> i32 {
        self.buffer_scale
    }

    pub const fn offset(&self) -> LogicalPosition {
        self.offset
    }

    pub(crate) fn into_parts(self) -> (Arc<[u8]>, u32, u32, i32, LogicalPosition) {
        (
            self.rgba,
            self.width,
            self.height,
            self.buffer_scale,
            self.offset,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DndMimePayload {
    mime: String,
    bytes: Arc<[u8]>,
}

impl DndMimePayload {
    pub fn new(mime: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Result<Self, &'static str> {
        let mime = mime.into();
        if mime.is_empty() {
            return Err("DnD MIME type must not be empty");
        }
        Ok(Self {
            mime,
            bytes: bytes.into(),
        })
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
}

#[derive(Clone, Debug)]
pub enum DndEvent {
    Enter {
        offer: DndOfferId,
        surface: SurfaceId,
        position: LogicalPosition,
        mime_types: Vec<String>,
        source_actions: DndActions,
    },
    Motion {
        offer: DndOfferId,
        surface: SurfaceId,
        position: LogicalPosition,
    },
    Leave {
        offer: DndOfferId,
        surface: SurfaceId,
    },
    Drop {
        offer: DndOfferId,
        surface: SurfaceId,
        action: Option<DndAction>,
    },
    /// The compositor accepted the drop. The source and drag icon remain alive
    /// until [`DndEvent::SourceFinished`] or [`DndEvent::SourceCancelled`].
    SourceDropped {
        source: DndSourceId,
        action: Option<DndAction>,
    },
    /// The destination completed the transfer and source resources were released.
    SourceFinished {
        source: DndSourceId,
        action: Option<DndAction>,
    },
    /// The drag was cancelled and source resources were released.
    SourceCancelled { source: DndSourceId },
}

/// A readable pipe returned by a Wayland DnD offer.
#[derive(Debug)]
pub struct DndReadPipe(pub(crate) ReadPipe);

impl Read for DndReadPipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_payload_rejects_empty_type_and_keeps_owned_bytes() {
        assert_eq!(
            DndMimePayload::new("", b"value".as_slice()),
            Err("DnD MIME type must not be empty")
        );

        let payload = DndMimePayload::new("text/uri-list", b"file:///tmp/a".as_slice())
            .expect("valid MIME payload");
        assert_eq!(payload.mime(), "text/uri-list");
        assert_eq!(payload.bytes().as_ref(), b"file:///tmp/a");
    }

    #[test]
    fn drag_icon_validates_dimensions_scale_and_rgba_length() {
        let offset = LogicalPosition::new(-8, -4);
        assert!(DndIcon::new(vec![0; 16], 2, 2, 2, offset).is_ok());
        assert_eq!(
            DndIcon::new(vec![], 0, 2, 1, offset),
            Err("DnD icon dimensions must be non-zero")
        );
        assert_eq!(
            DndIcon::new(vec![0; 16], 2, 2, 0, offset),
            Err("DnD icon buffer scale must be at least one")
        );
        assert_eq!(
            DndIcon::new(vec![0; 24], 3, 2, 2, offset),
            Err("DnD icon dimensions must be divisible by its buffer scale")
        );
        assert_eq!(
            DndIcon::new(vec![0; 15], 2, 2, 1, offset),
            Err("DnD icon RGBA byte length does not match its dimensions")
        );
    }
}
