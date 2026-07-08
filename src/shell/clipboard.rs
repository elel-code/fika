use std::collections::HashMap;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use compio::io::{AsyncRead, AsyncWrite};
use compio::runtime::fd::AsyncFd;
use fika_core::{FileClipboardRole, encode_file_clipboard_text, run_operation_task};
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{
    DataOfferError, DataOfferHandler, DragOffer,
};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, ReadPipe, WritePipe};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{
    PointerData, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::{
    delegate_data_device, delegate_pointer, delegate_registry, delegate_seat, registry_handlers,
};
use smithay_client_toolkit::{
    reexports::calloop::EventLoop as CalloopEventLoop,
    reexports::calloop::channel::{self, Channel, Sender as CalloopSender},
    reexports::calloop_wayland_source::WaylandSource,
    reexports::client::backend::Backend,
    reexports::client::globals::{GlobalList, registry_queue_init},
    reexports::client::protocol::wl_data_device::WlDataDevice,
    reexports::client::protocol::wl_data_device_manager::DndAction,
    reexports::client::protocol::wl_data_source::WlDataSource,
    reexports::client::protocol::wl_keyboard::WlKeyboard,
    reexports::client::protocol::wl_pointer::WlPointer,
    reexports::client::protocol::wl_seat::WlSeat,
    reexports::client::protocol::wl_surface::WlSurface,
    reexports::client::{Connection, Dispatch, Proxy, QueueHandle},
};
use wayland_backend::client::ObjectId;
use winit::window::Window;

const MIME_GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
const MIME_TEXT_URI_LIST: &str = "text/uri-list";
const MIME_TEXT_PLAIN_UTF8: &str = "text/plain;charset=utf-8";
const MIME_UTF8_STRING: &str = "UTF8_STRING";
const MIME_TEXT_PLAIN: &str = "text/plain";
const MIME_STRING: &str = "STRING";
const MIME_TEXT: &str = "TEXT";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileClipboardExportRequest {
    pub(crate) role: FileClipboardRole,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) text: String,
}

pub(crate) struct ShellClipboard {
    command_tx: CalloopSender<ClipboardCommand>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ShellClipboard {
    pub(crate) fn from_window(window: &dyn Window) -> Result<Option<Self>, String> {
        let display = match window
            .display_handle()
            .map_err(|error| format!("raw display handle unavailable: {error}"))?
            .as_raw()
        {
            RawDisplayHandle::Wayland(display) => display.display.as_ptr(),
            _ => return Ok(None),
        };

        // SAFETY: the raw display comes from the live winit window and the
        // clipboard worker is dropped before the window during app teardown.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let (command_tx, command_rx) = channel::channel();
        let worker = thread::Builder::new()
            .name("fika-wayland-clipboard".to_string())
            .spawn(move || clipboard_worker(connection, command_rx))
            .map_err(|error| error.to_string())?;

        Ok(Some(Self {
            command_tx,
            worker: Some(worker),
        }))
    }

    pub(crate) fn backend(&self) -> &'static str {
        "wayland-wl-data-device"
    }

    pub(crate) fn store_text_async(
        &self,
        text: String,
    ) -> Result<mpsc::Receiver<IoResult<()>>, String> {
        self.store_content_async(ClipboardContent::text(&text))
    }

    pub(crate) fn store_file_clipboard_async(
        &self,
        role: FileClipboardRole,
        paths: Vec<PathBuf>,
        text: String,
    ) -> Result<mpsc::Receiver<IoResult<()>>, String> {
        self.store_content_async(ClipboardContent::file_list(role, &paths, &text))
    }

    pub(crate) fn load_text_async(&self) -> Result<mpsc::Receiver<IoResult<String>>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_command(ClipboardCommand::Load { reply_tx })?;
        Ok(reply_rx)
    }

    fn store_content_async(
        &self,
        content: ClipboardContent,
    ) -> Result<mpsc::Receiver<IoResult<()>>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_command(ClipboardCommand::Store { content, reply_tx })?;
        Ok(reply_rx)
    }

    fn send_command(&self, command: ClipboardCommand) -> Result<(), String> {
        self.command_tx
            .send(command)
            .map_err(|_| "clipboard worker stopped".to_string())
    }
}

impl Drop for ShellClipboard {
    fn drop(&mut self) {
        let _ = self.command_tx.send(ClipboardCommand::Exit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug)]
enum ClipboardCommand {
    Store {
        content: ClipboardContent,
        reply_tx: mpsc::Sender<IoResult<()>>,
    },
    Load {
        reply_tx: mpsc::Sender<IoResult<String>>,
    },
    Exit,
}

#[derive(Clone, Debug)]
struct ClipboardContent {
    payloads: Vec<ClipboardMimePayload>,
}

impl ClipboardContent {
    fn text(text: &str) -> Self {
        let bytes = Arc::<[u8]>::from(text.as_bytes());
        Self {
            payloads: text_mimes()
                .into_iter()
                .map(|mime| ClipboardMimePayload {
                    mime: mime.to_string(),
                    bytes: bytes.clone(),
                })
                .collect(),
        }
    }

    fn file_list(role: FileClipboardRole, paths: &[PathBuf], text: &str) -> Self {
        let uri_list = encode_file_clipboard_text(FileClipboardRole::Copy, paths);
        let gnome_role = match role {
            FileClipboardRole::Copy => "copy",
            FileClipboardRole::Cut => "cut",
        };
        let gnome = if uri_list.is_empty() {
            gnome_role.to_string()
        } else {
            format!("{gnome_role}\n{uri_list}")
        };
        let text_bytes = Arc::<[u8]>::from(text.as_bytes());
        let uri_bytes = Arc::<[u8]>::from(uri_list.as_bytes());
        let gnome_bytes = Arc::<[u8]>::from(gnome.as_bytes());
        let mut payloads = vec![
            ClipboardMimePayload {
                mime: MIME_GNOME_COPIED_FILES.to_string(),
                bytes: gnome_bytes,
            },
            ClipboardMimePayload {
                mime: MIME_TEXT_URI_LIST.to_string(),
                bytes: uri_bytes,
            },
        ];
        payloads.extend(text_mimes().into_iter().map(|mime| ClipboardMimePayload {
            mime: mime.to_string(),
            bytes: text_bytes.clone(),
        }));
        Self { payloads }
    }

    fn mime_types(&self) -> impl Iterator<Item = &str> {
        self.payloads.iter().map(|payload| payload.mime.as_str())
    }

    fn bytes_for_mime(&self, mime: &str) -> Option<Arc<[u8]>> {
        self.payloads
            .iter()
            .find(|payload| payload.mime == mime)
            .map(|payload| payload.bytes.clone())
    }
}

#[derive(Clone, Debug)]
struct ClipboardMimePayload {
    mime: String,
    bytes: Arc<[u8]>,
}

fn text_mimes() -> [&'static str; 5] {
    [
        MIME_TEXT_PLAIN_UTF8,
        MIME_TEXT_PLAIN,
        MIME_UTF8_STRING,
        MIME_STRING,
        MIME_TEXT,
    ]
}

fn preferred_mime(offered: &[String]) -> Option<String> {
    [
        MIME_GNOME_COPIED_FILES,
        MIME_TEXT_URI_LIST,
        MIME_TEXT_PLAIN_UTF8,
        MIME_UTF8_STRING,
        MIME_TEXT_PLAIN,
        MIME_STRING,
        MIME_TEXT,
    ]
    .into_iter()
    .find(|mime| offered.iter().any(|offered| offered == mime))
    .map(str::to_string)
}

fn normalize_loaded_text(mime: &str, text: String) -> String {
    match mime {
        MIME_TEXT_PLAIN_UTF8 | MIME_TEXT_PLAIN => text.replace("\r\n", "\n").replace('\r', "\n"),
        _ => text,
    }
}

const CLIPBOARD_PIPE_CHUNK: usize = 4096;

fn spawn_compio_read_pipe(
    mime: String,
    read_pipe: ReadPipe,
    reply_tx: mpsc::Sender<IoResult<String>>,
) {
    let fail_tx = reply_tx.clone();
    let spawn_result = thread::Builder::new()
        .name("fika-clipboard-read".to_string())
        .spawn(move || {
            let result = pollster::block_on(run_operation_task(move || async move {
                read_pipe_text_compio(mime, read_pipe).await
            }))
            .map_err(|error| IoError::other(error.to_string()))
            .and_then(|result| result);
            let _ = reply_tx.send(result);
        });
    if let Err(error) = spawn_result {
        let _ = fail_tx.send(Err(IoError::other(error.to_string())));
    }
}

fn spawn_compio_write_pipe(mime: String, write_pipe: WritePipe, bytes: Arc<[u8]>) {
    let log_mime = mime.clone();
    if let Err(error) = thread::Builder::new()
        .name("fika-clipboard-write".to_string())
        .spawn(move || {
            let result = pollster::block_on(run_operation_task(move || async move {
                write_pipe_bytes_compio(write_pipe, bytes).await
            }))
            .map_err(|error| IoError::other(error.to_string()))
            .and_then(|result| result);
            if let Err(error) = result {
                fika_log!("[fika-wgpu] clipboard-send-error mime={mime} error={error}");
            }
        })
    {
        fika_log!("[fika-wgpu] clipboard-send-thread-error mime={log_mime} error={error}");
    }
}

async fn read_pipe_text_compio(mime: String, read_pipe: ReadPipe) -> IoResult<String> {
    let owned_fd: OwnedFd = read_pipe.into();
    let mut pipe = AsyncFd::new(File::from(owned_fd))?;
    let mut content = Vec::new();

    loop {
        let buffer = Vec::with_capacity(CLIPBOARD_PIPE_CHUNK);
        let compio::buf::BufResult(result, buffer) = pipe.read(buffer).await;
        let read = result?;
        if read == 0 {
            break;
        }
        content.extend_from_slice(&buffer[..read]);
    }

    let text = String::from_utf8(content)
        .unwrap_or_else(|error| String::from_utf8_lossy(&error.into_bytes()).into_owned());
    Ok(normalize_loaded_text(&mime, text))
}

async fn write_pipe_bytes_compio(write_pipe: WritePipe, bytes: Arc<[u8]>) -> IoResult<()> {
    let owned_fd: OwnedFd = write_pipe.into();
    let mut pipe = AsyncFd::new(File::from(owned_fd))?;
    let mut written = 0;

    while written < bytes.len() {
        let end = (written + CLIPBOARD_PIPE_CHUNK).min(bytes.len());
        let buffer = Vec::from(&bytes[written..end]);
        let compio::buf::BufResult(result, _) = pipe.write(buffer).await;
        let count = result?;
        if count == 0 {
            return Err(IoError::new(
                ErrorKind::WriteZero,
                "clipboard pipe accepted zero bytes",
            ));
        }
        written += count;
    }

    Ok(())
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
                    ClipboardCommand::Load { reply_tx } => {
                        if let Err(error) = state.load_selection(reply_tx.clone()) {
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
    seats: HashMap<ObjectId, ClipboardSeatState>,
    latest_seat: Option<ObjectId>,
    queue_handle: QueueHandle<Self>,
    sources: Vec<CopyPasteSource>,
    content: ClipboardContent,
    exit: bool,
}

impl ClipboardWorkerState {
    fn new(globals: &GlobalList, queue_handle: &QueueHandle<Self>) -> Option<Self> {
        let data_device_manager = DataDeviceManagerState::bind(globals, queue_handle).ok()?;
        let seat_state = SeatState::new(globals, queue_handle);
        let mut seats = HashMap::new();
        for seat in seat_state.seats() {
            seats.insert(seat.id(), ClipboardSeatState::default());
        }

        Some(Self {
            data_device_manager,
            registry_state: RegistryState::new(globals),
            seat_state,
            seats,
            latest_seat: None,
            queue_handle: queue_handle.clone(),
            sources: Vec::new(),
            content: ClipboardContent::text(""),
            exit: false,
        })
    }

    fn store_selection(&mut self, content: ClipboardContent) -> IoResult<()> {
        let latest = self
            .latest_seat
            .as_ref()
            .ok_or_else(|| IoError::other("no Wayland seat event has been observed"))?;
        let seat = self
            .seats
            .get_mut(latest)
            .ok_or_else(|| IoError::other("active Wayland seat was removed"))?;
        if !seat.has_focus {
            return Err(IoError::other("Wayland seat is not focused on Fika"));
        }
        let data_device = seat
            .data_device
            .as_ref()
            .ok_or_else(|| IoError::other("Wayland data device is unavailable"))?;
        if seat.latest_serial == 0 {
            return Err(IoError::other("Wayland selection serial is unavailable"));
        }

        self.content = content;
        let source = self
            .data_device_manager
            .create_copy_paste_source(&self.queue_handle, self.content.mime_types());
        source.set_selection(data_device, seat.latest_serial);
        self.sources.push(source);
        Ok(())
    }

    fn load_selection(&mut self, reply_tx: mpsc::Sender<IoResult<String>>) -> IoResult<()> {
        let latest = self
            .latest_seat
            .as_ref()
            .ok_or_else(|| IoError::other("no Wayland seat event has been observed"))?;
        let seat = self
            .seats
            .get_mut(latest)
            .ok_or_else(|| IoError::other("active Wayland seat was removed"))?;
        if !seat.has_focus {
            return Err(IoError::other("Wayland seat is not focused on Fika"));
        }
        let selection = seat
            .data_device
            .as_ref()
            .and_then(|device| device.data().selection_offer())
            .ok_or_else(|| IoError::other("selection is empty"))?;
        let mime = selection
            .with_mime_types(preferred_mime)
            .ok_or_else(|| IoError::new(ErrorKind::NotFound, "supported MIME type not found"))?;
        let read_pipe = selection
            .receive(mime.clone())
            .map_err(|error| match error {
                DataOfferError::InvalidReceive => IoError::other("selection offer is not ready"),
                DataOfferError::Io(error) => error,
            })?;

        self.read_selection_pipe(mime, read_pipe, reply_tx)
    }

    fn read_selection_pipe(
        &mut self,
        mime: String,
        read_pipe: ReadPipe,
        reply_tx: mpsc::Sender<IoResult<String>>,
    ) -> IoResult<()> {
        spawn_compio_read_pipe(mime, read_pipe, reply_tx);
        Ok(())
    }

    fn send_request(&mut self, mime: String, write_pipe: WritePipe) {
        let Some(bytes) = self.content.bytes_for_mime(&mime) else {
            return;
        };
        spawn_compio_write_pipe(mime, write_pipe, bytes);
    }
}

impl SeatHandler for ClipboardWorkerState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.insert(seat.id(), ClipboardSeatState::default());
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let Some(seat_state) = self.seats.get_mut(&seat.id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                seat_state.keyboard = Some(seat.get_keyboard(qh, seat.id()));
                if seat_state.data_device.is_none() {
                    seat_state.data_device =
                        Some(self.data_device_manager.get_data_device(qh, &seat));
                }
            }
            Capability::Pointer => {
                seat_state.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let Some(seat_state) = self.seats.get_mut(&seat.id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                seat_state.data_device = None;
                if let Some(keyboard) = seat_state.keyboard.take() {
                    if keyboard.version() >= 3 {
                        keyboard.release();
                    }
                }
            }
            Capability::Pointer => {
                if let Some(pointer) = seat_state.pointer.take() {
                    if pointer.version() >= 3 {
                        pointer.release();
                    }
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.remove(&seat.id());
    }
}

impl PointerHandler for ClipboardWorkerState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(pointer_data) = pointer.data::<PointerData>() else {
            return;
        };
        let seat_id = pointer_data.seat().id();
        let Some(seat_state) = self.seats.get_mut(&seat_id) else {
            return;
        };
        let mut updated_serial = false;
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. }
                | PointerEventKind::Release { serial, .. } => {
                    updated_serial = true;
                    seat_state.latest_serial = serial;
                }
                _ => {}
            }
        }
        if updated_serial {
            self.latest_seat = Some(seat_id);
        }
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
        _: &WlSurface,
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
        _: &WlDataSource,
        mime: String,
        write_pipe: WritePipe,
    ) {
        self.send_request(mime, write_pipe);
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.sources.retain(|candidate| candidate.inner() != source);
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
    registry_handlers![SeatState];

    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
}

impl Dispatch<WlKeyboard, ObjectId, ClipboardWorkerState> for ClipboardWorkerState {
    fn event(
        state: &mut ClipboardWorkerState,
        _: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        data: &ObjectId,
        _: &Connection,
        _: &QueueHandle<ClipboardWorkerState>,
    ) {
        use smithay_client_toolkit::reexports::client::protocol::wl_keyboard::Event as WlKeyboardEvent;

        let Some(seat_state) = state.seats.get_mut(data) else {
            return;
        };
        match event {
            WlKeyboardEvent::Key { serial, .. } | WlKeyboardEvent::Modifiers { serial, .. } => {
                seat_state.latest_serial = serial;
                state.latest_seat = Some(data.clone());
            }
            WlKeyboardEvent::Enter { serial, .. } => {
                seat_state.latest_serial = serial;
                seat_state.has_focus = true;
                state.latest_seat = Some(data.clone());
            }
            WlKeyboardEvent::Leave { .. } => {
                seat_state.latest_serial = 0;
                seat_state.has_focus = false;
            }
            _ => {}
        }
    }
}

delegate_seat!(ClipboardWorkerState);
delegate_pointer!(ClipboardWorkerState);
delegate_data_device!(ClipboardWorkerState);
delegate_registry!(ClipboardWorkerState);

#[derive(Default)]
struct ClipboardSeatState {
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    data_device: Option<DataDevice>,
    has_focus: bool,
    latest_serial: u32,
}

impl Drop for ClipboardSeatState {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take() {
            if keyboard.version() >= 3 {
                keyboard.release();
            }
        }
        if let Some(pointer) = self.pointer.take() {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_clipboard_content_offers_native_file_and_text_mimes() {
        let paths = [PathBuf::from("/tmp/a file.txt")];
        let text = encode_file_clipboard_text(FileClipboardRole::Cut, &paths);
        let content = ClipboardContent::file_list(FileClipboardRole::Cut, &paths, &text);

        let gnome = String::from_utf8(
            content
                .bytes_for_mime(MIME_GNOME_COPIED_FILES)
                .expect("gnome copied files mime")
                .to_vec(),
        )
        .unwrap();
        let uri_list = String::from_utf8(
            content
                .bytes_for_mime(MIME_TEXT_URI_LIST)
                .expect("uri list mime")
                .to_vec(),
        )
        .unwrap();
        let plain = String::from_utf8(
            content
                .bytes_for_mime(MIME_TEXT_PLAIN_UTF8)
                .expect("plain text mime")
                .to_vec(),
        )
        .unwrap();

        assert_eq!(gnome, "cut\nfile:///tmp/a%20file.txt");
        assert_eq!(uri_list, "file:///tmp/a%20file.txt");
        assert_eq!(plain, "# fika-cut\nfile:///tmp/a%20file.txt");
    }

    #[test]
    fn preferred_mime_chooses_file_clipboard_before_plain_text() {
        let offered = vec![
            MIME_TEXT_PLAIN_UTF8.to_string(),
            MIME_TEXT_URI_LIST.to_string(),
            MIME_GNOME_COPIED_FILES.to_string(),
        ];

        assert_eq!(
            preferred_mime(&offered),
            Some(MIME_GNOME_COPIED_FILES.to_string())
        );
    }
}
