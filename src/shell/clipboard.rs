use std::{cell::RefCell, env, path::PathBuf};

use arboard::{Clipboard, Error as ClipboardError};
use fika_core::{FileClipboardRole, encode_file_clipboard_text};
use winit::window::Window;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileClipboardExportRequest {
    pub(crate) role: FileClipboardRole,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) text: String,
}

pub(crate) struct ShellClipboard {
    clipboard: RefCell<Clipboard>,
}

impl ShellClipboard {
    pub(crate) fn from_window(_window: &dyn Window) -> Result<Option<Self>, String> {
        if env::var_os("WAYLAND_DISPLAY").is_none() && env::var_os("DISPLAY").is_none() {
            return Ok(None);
        }
        Clipboard::new()
            .map(|clipboard| {
                Some(Self {
                    clipboard: RefCell::new(clipboard),
                })
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn backend(&self) -> &'static str {
        "arboard"
    }

    pub(crate) fn store_text(&self, text: &str) -> Result<(), String> {
        self.clipboard
            .try_borrow_mut()
            .map_err(|error| format!("clipboard already borrowed: {error}"))?
            .set_text(text)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn load_text(&self) -> Result<String, String> {
        let mut clipboard = self
            .clipboard
            .try_borrow_mut()
            .map_err(|error| format!("clipboard already borrowed: {error}"))?;
        match clipboard.get_text() {
            Ok(text) => Ok(text),
            Err(ClipboardError::ContentNotAvailable) => clipboard
                .get()
                .file_list()
                .map(|paths| encode_file_clipboard_text(FileClipboardRole::Copy, &paths))
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }
}
