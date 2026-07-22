//! A generic `wl_data_device` clipboard worker.
//!
//! MIME policy is supplied by the caller. This keeps desktop-specific file
//! clipboard formats out of the runtime while sharing all SCTK, seat, serial,
//! pipe and calloop handling.

use std::any::Any;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::os::fd::OwnedFd;
use std::sync::{Arc, mpsc};
use std::thread;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{
    DataOfferError, DataOfferHandler, DragOffer,
};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, ReadPipe, WritePipe};
use smithay_client_toolkit::reexports::calloop::EventLoop as CalloopEventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{self, Channel, Sender as CalloopSender};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::backend::Backend;
use smithay_client_toolkit::reexports::client::globals::{GlobalList, registry_queue_init};
use smithay_client_toolkit::reexports::client::protocol::wl_data_device::WlDataDevice;
use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction;
use smithay_client_toolkit::reexports::client::protocol::wl_data_source::WlDataSource;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_keyboard, wl_pointer, wl_seat, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardData, KeyboardHandler, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    PointerData, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::{delegate_registry, registry_handlers};

pub use crate::data_transfer::{
    MIME_STRING, MIME_TEXT, MIME_TEXT_PLAIN, MIME_TEXT_PLAIN_UTF8, MIME_UTF8_STRING,
    MimePayload as ClipboardMimePayload, TransferContent as ClipboardContent, text_mime_types,
};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("raw display handle is unavailable: {0}")]
    DisplayHandle(String),
    #[error("failed to start the Wayland clipboard worker: {0}")]
    WorkerStart(String),
    #[error("the Wayland clipboard worker has stopped")]
    WorkerStopped,
}

/// A clipboard event loop running on its own thread and Wayland event queue.
pub struct Clipboard {
    command_tx: CalloopSender<ClipboardCommand>,
    worker: Option<thread::JoinHandle<()>>,
    // A foreign wl_display stays valid for as long as the object that exposed it.
    display_owner: Option<Box<dyn Any + Send + Sync>>,
}

impl Clipboard {
    /// Share the wl_display owned by a renderer/window object.
    ///
    /// Returning `Ok(None)` means the owner is not a Wayland object. The owner
    /// is retained until after the clipboard thread is joined.
    pub fn from_display_owner<T>(owner: Arc<T>) -> Result<Option<Self>, ClipboardError>
    where
        T: HasDisplayHandle + Send + Sync + ?Sized + 'static,
    {
        let display = match owner
            .display_handle()
            .map_err(|error| ClipboardError::DisplayHandle(error.to_string()))?
            .as_raw()
        {
            RawDisplayHandle::Wayland(display) => display.display,
            _ => return Ok(None),
        };

        // SAFETY: `display_owner` retains `owner` until the worker is joined,
        // so the foreign wl_display outlives the derived Connection.
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let mut clipboard = Self::from_connection(connection)?;
        clipboard.display_owner = Some(Box::new(owner));
        Ok(Some(clipboard))
    }

    pub fn from_connection(connection: Connection) -> Result<Self, ClipboardError> {
        let (command_tx, command_rx) = channel::channel();
        let worker = thread::Builder::new()
            .name("wayland-client-clipboard".to_string())
            .spawn(move || clipboard_worker(connection, command_rx))
            .map_err(|error| ClipboardError::WorkerStart(error.to_string()))?;
        Ok(Self {
            command_tx,
            worker: Some(worker),
            display_owner: None,
        })
    }

    pub fn backend(&self) -> &'static str {
        "wayland-wl-data-device"
    }

    pub fn store_text_async(
        &self,
        text: impl Into<String>,
    ) -> Result<mpsc::Receiver<io::Result<()>>, ClipboardError> {
        self.store_async(ClipboardContent::text(text.into()))
    }

    pub fn store_async(
        &self,
        content: ClipboardContent,
    ) -> Result<mpsc::Receiver<io::Result<()>>, ClipboardError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_command(ClipboardCommand::Store { content, reply_tx })?;
        Ok(reply_rx)
    }

    pub fn load_text_async(&self) -> Result<mpsc::Receiver<io::Result<String>>, ClipboardError> {
        self.load_async(text_mime_types().iter().copied())
    }

    pub fn load_async(
        &self,
        preferred_mimes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<mpsc::Receiver<io::Result<String>>, ClipboardError> {
        let preferred_mimes = preferred_mimes.into_iter().map(Into::into).collect();
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_command(ClipboardCommand::Load {
            preferred_mimes,
            reply_tx,
        })?;
        Ok(reply_rx)
    }

    fn send_command(&self, command: ClipboardCommand) -> Result<(), ClipboardError> {
        self.command_tx
            .send(command)
            .map_err(|_| ClipboardError::WorkerStopped)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = self.command_tx.send(ClipboardCommand::Exit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // `display_owner` is dropped after the derived connection/thread.
    }
}

#[derive(Debug)]
enum ClipboardCommand {
    Store {
        content: ClipboardContent,
        reply_tx: mpsc::Sender<io::Result<()>>,
    },
    Load {
        preferred_mimes: Vec<String>,
        reply_tx: mpsc::Sender<io::Result<String>>,
    },
    Exit,
}

fn clipboard_worker(connection: Connection, command_rx: Channel<ClipboardCommand>) {
    let Ok((globals, event_queue)) = registry_queue_init(&connection) else {
        return;
    };
    let Ok(mut event_loop) = CalloopEventLoop::<ClipboardWorkerState>::try_new() else {
        return;
    };
    let loop_handle = event_loop.handle();
    let Some(mut state) = ClipboardWorkerState::new(&globals, &event_queue.handle()) else {
        return;
    };

    if loop_handle
        .insert_source(command_rx, |event, _, state| {
            if let channel::Event::Msg(command) = event {
                match command {
                    ClipboardCommand::Store { content, reply_tx } => {
                        let _ = reply_tx.send(state.store_selection(content));
                    }
                    ClipboardCommand::Load {
                        preferred_mimes,
                        reply_tx,
                    } => {
                        if let Err(error) = state.load_selection(preferred_mimes, reply_tx.clone())
                        {
                            let _ = reply_tx.send(Err(error));
                        }
                    }
                    ClipboardCommand::Exit => state.exit = true,
                }
            }
        })
        .is_err()
    {
        return;
    }

    if WaylandSource::new(connection, event_queue)
        .insert(loop_handle)
        .is_err()
    {
        return;
    }

    while !state.exit {
        if event_loop.dispatch(None, &mut state).is_err() {
            break;
        }
    }
}

struct ClipboardWorkerState {
    data_device_manager: DataDeviceManagerState,
    registry_state: RegistryState,
    seat_state: SeatState,
    seats: HashMap<u32, ClipboardSeatState>,
    latest_seat: Option<u32>,
    queue_handle: QueueHandle<Self>,
    sources: Vec<ClipboardSource>,
    exit: bool,
}

impl ClipboardWorkerState {
    fn new(globals: &GlobalList, queue_handle: &QueueHandle<Self>) -> Option<Self> {
        let data_device_manager = DataDeviceManagerState::bind(globals, queue_handle).ok()?;
        let seat_state = SeatState::new(globals, queue_handle);
        let seats = seat_state
            .seats()
            .map(|seat| {
                let data_device = data_device_manager.get_data_device(queue_handle, &seat);
                let mut state = ClipboardSeatState::default();
                state.data_device = Some(data_device);
                (seat.id().protocol_id(), state)
            })
            .collect();

        Some(Self {
            data_device_manager,
            registry_state: RegistryState::new(globals),
            seat_state,
            seats,
            latest_seat: None,
            queue_handle: queue_handle.clone(),
            sources: Vec::new(),
            exit: false,
        })
    }

    fn store_selection(&mut self, content: ClipboardContent) -> io::Result<()> {
        let latest = self
            .latest_seat
            .ok_or_else(|| io::Error::other("no Wayland seat event has been observed"))?;
        let seat = self
            .seats
            .get_mut(&latest)
            .ok_or_else(|| io::Error::other("active Wayland seat was removed"))?;
        if !seat.has_focus() {
            return Err(io::Error::other("Wayland seat does not focus this client"));
        }
        let data_device = seat
            .data_device
            .as_ref()
            .ok_or_else(|| io::Error::other("Wayland data device is unavailable"))?;
        if seat.latest_serial == 0 {
            return Err(io::Error::other("Wayland selection serial is unavailable"));
        }

        let source = self
            .data_device_manager
            .create_copy_paste_source(&self.queue_handle, content.mime_types());
        source.set_selection(data_device, seat.latest_serial);
        self.sources.push(ClipboardSource { source, content });
        Ok(())
    }

    fn load_selection(
        &mut self,
        preferred_mimes: Vec<String>,
        reply_tx: mpsc::Sender<io::Result<String>>,
    ) -> io::Result<()> {
        let latest = self
            .latest_seat
            .ok_or_else(|| io::Error::other("no Wayland seat event has been observed"))?;
        let seat = self
            .seats
            .get_mut(&latest)
            .ok_or_else(|| io::Error::other("active Wayland seat was removed"))?;
        if !seat.has_focus() {
            return Err(io::Error::other("Wayland seat does not focus this client"));
        }
        let selection = seat
            .data_device
            .as_ref()
            .and_then(|device| device.data().selection_offer())
            .ok_or_else(|| io::Error::other("selection is empty"))?;
        let mime = selection
            .with_mime_types(|offered| {
                preferred_mimes
                    .iter()
                    .find(|mime| offered.iter().any(|item| item == *mime))
                    .cloned()
            })
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "supported MIME type not found"))?;
        let read_pipe = selection
            .receive(mime.clone())
            .map_err(|error| match error {
                DataOfferError::InvalidReceive => io::Error::other("selection offer is not ready"),
                DataOfferError::Io(error) => error,
            })?;
        spawn_read_pipe(mime, read_pipe, reply_tx);
        Ok(())
    }

    fn write_selection(&self, source: &WlDataSource, mime: String, write_pipe: WritePipe) {
        let content = self
            .sources
            .iter()
            .find(|candidate| candidate.source.inner() == source)
            .map(|candidate| &candidate.content);
        if let Some(bytes) = content.and_then(|content| content.bytes_for_mime(&mime)) {
            spawn_write_pipe(write_pipe, bytes);
        }
    }
}

fn spawn_read_pipe(mime: String, read_pipe: ReadPipe, reply_tx: mpsc::Sender<io::Result<String>>) {
    let fail_tx = reply_tx.clone();
    let result = thread::Builder::new()
        .name("wayland-clipboard-read".to_string())
        .spawn(move || {
            let result = read_pipe_text(&mime, read_pipe);
            let _ = reply_tx.send(result);
        });
    if let Err(error) = result {
        let _ = fail_tx.send(Err(io::Error::other(error.to_string())));
    }
}

fn spawn_write_pipe(write_pipe: WritePipe, bytes: Arc<[u8]>) {
    crate::data_transfer::spawn_write_pipe("wayland-clipboard-write", write_pipe, bytes);
}

fn read_pipe_text(mime: &str, read_pipe: ReadPipe) -> io::Result<String> {
    let owned_fd: OwnedFd = read_pipe.into();
    let mut pipe = File::from(owned_fd);
    let mut content = Vec::new();
    pipe.read_to_end(&mut content)?;
    let text = String::from_utf8(content)
        .unwrap_or_else(|error| String::from_utf8_lossy(&error.into_bytes()).into_owned());
    Ok(match mime {
        MIME_TEXT_PLAIN_UTF8 | MIME_TEXT_PLAIN => text.replace("\r\n", "\n").replace('\r', "\n"),
        _ => text,
    })
}

impl SeatHandler for ClipboardWorkerState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let data_device = self.data_device_manager.get_data_device(qh, &seat);
        let mut state = ClipboardSeatState::default();
        state.data_device = Some(data_device);
        self.seats.insert(seat.id().protocol_id(), state);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let Some(state) = self.seats.get_mut(&seat.id().protocol_id()) else {
            return;
        };
        match capability {
            Capability::Keyboard if state.keyboard.is_none() => {
                state.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            }
            Capability::Pointer if state.pointer.is_none() => {
                state.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let Some(state) = self.seats.get_mut(&seat.id().protocol_id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                state.keyboard_focus = false;
                if let Some(keyboard) = state.keyboard.take()
                    && keyboard.version() >= 3
                {
                    keyboard.release();
                }
            }
            Capability::Pointer => {
                state.pointer_focus = false;
                if let Some(pointer) = state.pointer.take()
                    && pointer.version() >= 3
                {
                    pointer.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let id = seat.id().protocol_id();
        self.seats.remove(&id);
        if self.latest_seat == Some(id) {
            self.latest_seat = None;
        }
    }
}

impl PointerHandler for ClipboardWorkerState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(state) = self.seats.get_mut(&seat_id) else {
            return;
        };
        for event in events {
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    state.pointer_focus = true;
                    state.latest_serial = serial;
                    self.latest_seat = Some(seat_id);
                }
                PointerEventKind::Leave { .. } => state.pointer_focus = false,
                PointerEventKind::Press { serial, .. }
                | PointerEventKind::Release { serial, .. } => {
                    state.latest_serial = serial;
                    self.latest_seat = Some(seat_id);
                }
                _ => {}
            }
        }
    }
}

impl KeyboardHandler for ClipboardWorkerState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        serial: u32,
        _: &[u32],
        _: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
        self.update_keyboard_serial(keyboard, serial, Some(true));
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        if let Some(state) = self.seats.get_mut(&data.seat().id().protocol_id()) {
            state.keyboard_focus = false;
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        _: KeyEvent,
    ) {
        self.update_keyboard_serial(keyboard, serial, None);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        _: KeyEvent,
    ) {
        self.update_keyboard_serial(keyboard, serial, None);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        _: KeyEvent,
    ) {
        self.update_keyboard_serial(keyboard, serial, None);
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.update_keyboard_serial(keyboard, serial, None);
    }
}

impl ClipboardWorkerState {
    fn update_keyboard_serial(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        focus: Option<bool>,
    ) {
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(state) = self.seats.get_mut(&seat_id) else {
            return;
        };
        state.latest_serial = serial;
        if let Some(focus) = focus {
            state.keyboard_focus = focus;
        }
        self.latest_seat = Some(seat_id);
    }
}

impl DataDeviceHandler for ClipboardWorkerState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &wl_surface::WlSurface,
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataSourceHandler for ClipboardWorkerState {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.write_selection(source, mime, pipe);
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.sources
            .retain(|candidate| candidate.source.inner() != source);
    }
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
}

impl DataOfferHandler for ClipboardWorkerState {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl ProvidesRegistryState for ClipboardWorkerState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers!(SeatState);
}

delegate_registry!(ClipboardWorkerState);
smithay_client_toolkit::delegate_dispatch2!(ClipboardWorkerState);

struct ClipboardSource {
    source: CopyPasteSource,
    content: ClipboardContent,
}

#[derive(Default)]
struct ClipboardSeatState {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    data_device: Option<DataDevice>,
    keyboard_focus: bool,
    pointer_focus: bool,
    latest_serial: u32,
}

impl ClipboardSeatState {
    fn has_focus(&self) -> bool {
        self.keyboard_focus || self.pointer_focus
    }
}

impl Drop for ClipboardSeatState {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take()
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        if let Some(pointer) = self.pointer.take()
            && pointer.version() >= 3
        {
            pointer.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_replaces_duplicate_mime_with_last_payload() {
        let content = ClipboardContent::new([
            ClipboardMimePayload::new("text/plain", Arc::<[u8]>::from(&b"old"[..])).unwrap(),
            ClipboardMimePayload::new("text/plain", Arc::<[u8]>::from(&b"new"[..])).unwrap(),
        ])
        .unwrap();
        assert_eq!(&*content.bytes_for_mime("text/plain").unwrap(), b"new");
    }

    #[test]
    fn text_content_offers_common_wayland_text_mimes() {
        let content = ClipboardContent::text("hello");
        let mimes = content.mime_types().collect::<Vec<_>>();
        assert_eq!(mimes, text_mime_types());
    }
}
