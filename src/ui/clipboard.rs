mod state;
mod tasks;

pub(crate) use state::{
    ClipboardMode, ClipboardState, primary_paste_clipboard_state, standard_paste_clipboard_state,
};
pub(crate) use tasks::paste_clipboard_result_async;
