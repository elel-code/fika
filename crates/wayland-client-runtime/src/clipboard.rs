//! Clipboard MIME vocabulary for [`Runtime`](crate::Runtime) selections.
//!
//! Clipboard and drag-and-drop operations share the runtime connection, seat
//! serials, data devices, [`crate::TransferContent`] and transfer pipes.
//! Applications set and receive selections through
//! [`crate::Runtime::store_selection`] and [`crate::Runtime::receive_selection`].

pub use crate::data_transfer::{
    MIME_STRING, MIME_TEXT, MIME_TEXT_PLAIN, MIME_TEXT_PLAIN_UTF8, MIME_UTF8_STRING,
    MimePayload as ClipboardMimePayload, TransferContent as ClipboardContent,
    TransferReadPipe as ClipboardReadPipe, text_mime_types,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_aliases_use_the_shared_transfer_content() {
        let content = ClipboardContent::text("hello");
        assert_eq!(
            content.payloads()[0].mime(),
            text_mime_types().first().copied().unwrap()
        );
    }
}
