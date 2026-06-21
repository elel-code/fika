use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

pub(crate) enum ShellClipboard {
    Wayland(smithay_clipboard::Clipboard),
}

impl ShellClipboard {
    pub(crate) fn from_window(window: &dyn Window) -> Result<Option<Self>, String> {
        let display = window
            .display_handle()
            .map_err(|error| format!("display handle: {error}"))?;
        match display.as_raw() {
            RawDisplayHandle::Wayland(handle) => {
                let clipboard = unsafe {
                    // The pointer comes from winit's live Wayland display handle.
                    // Fika drops ShellClipboard before dropping the window.
                    smithay_clipboard::Clipboard::new(handle.display.as_ptr())
                };
                Ok(Some(Self::Wayland(clipboard)))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn backend(&self) -> &'static str {
        match self {
            Self::Wayland(_) => "wayland",
        }
    }

    pub(crate) fn store_text(&self, text: &str) {
        match self {
            Self::Wayland(clipboard) => clipboard.store(text.to_string()),
        }
    }

    pub(crate) fn load_text(&self) -> Result<String, String> {
        match self {
            Self::Wayland(clipboard) => clipboard.load().map_err(|error| error.to_string()),
        }
    }
}
