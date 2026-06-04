use rustix::pipe::{PipeFlags, pipe_with};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::mpsc;
use std::thread;
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
};

use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardMimeData {
    pub(crate) mime_type: String,
    pub(crate) data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardOffer {
    pub(crate) mime_type: String,
    pub(crate) data: Vec<u8>,
}

impl ClipboardOffer {
    pub(crate) fn new(mime_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            mime_type: mime_type.into(),
            data,
        }
    }
}

pub(crate) fn list_mime_types() -> Result<Vec<String>, String> {
    connect_and_read(|client| Ok(client.mime_types().to_vec()))
}

pub(crate) fn read_mime(mime_type: &str) -> Result<ClipboardMimeData, String> {
    connect_and_read(|client| {
        let data = client.read_mime(mime_type)?;
        Ok(ClipboardMimeData {
            mime_type: mime_type.to_string(),
            data,
        })
    })
}

pub(crate) fn publish_mime_data(offers: Vec<ClipboardOffer>) -> Result<(), String> {
    if offers.is_empty() {
        return Err("clipboard payload has no MIME offers".to_string());
    }
    if let Some(offer) = offers.iter().find(|offer| offer.mime_type.is_empty()) {
        return Err(format!(
            "clipboard payload contains an empty MIME type with {} byte(s)",
            offer.data.len()
        ));
    }

    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("fika-wayland-clipboard".to_string())
        .spawn(move || {
            let result = run_clipboard_owner(offers, ready_tx.clone());
            if let Err(err) = result {
                let _ = ready_tx.send(Err(err));
            }
        })
        .map_err(|err| format!("Wayland clipboard owner thread: {err}"))?;

    ready_rx
        .recv()
        .map_err(|err| format!("Wayland clipboard owner setup: {err}"))?
}

fn connect_and_read<T>(
    read: impl FnOnce(&dyn SelectionClient) -> Result<T, String>,
) -> Result<T, String> {
    let connection =
        Connection::connect_to_env().map_err(|err| format!("Wayland clipboard connect: {err}"))?;
    let (globals, _registry_queue) =
        registry_queue_init::<RegistryState>(&connection).map_err(|err| err.to_string())?;

    if has_global(&globals, ExtDataControlManagerV1::interface().name) {
        let mut event_queue = connection.new_event_queue::<ExtSelectionState>();
        let mut state =
            ExtSelectionState::bind(&globals, event_queue.handle(), connection.clone())?;
        wait_for_selection(&mut event_queue, &mut state, |state| state.ready)?;
        return read(&state);
    }

    if has_global(&globals, ZwlrDataControlManagerV1::interface().name) {
        let mut event_queue = connection.new_event_queue::<WlrSelectionState>();
        let mut state = WlrSelectionState::bind(&globals, event_queue.handle(), connection)?;
        wait_for_selection(&mut event_queue, &mut state, |state| state.ready)?;
        return read(&state);
    }

    Err("Wayland data-control clipboard protocol is not available".to_string())
}

fn run_clipboard_owner(
    offers: Vec<ClipboardOffer>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let connection =
        Connection::connect_to_env().map_err(|err| format!("Wayland clipboard connect: {err}"))?;
    let (globals, _registry_queue) =
        registry_queue_init::<RegistryState>(&connection).map_err(|err| err.to_string())?;

    if has_global(&globals, ExtDataControlManagerV1::interface().name) {
        let mut event_queue = connection.new_event_queue::<ExtClipboardOwnerState>();
        let mut state = ExtClipboardOwnerState::bind(
            &globals,
            event_queue.handle(),
            connection.clone(),
            offers,
        )?;
        connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        let _ = ready_tx.send(Ok(()));
        while !state.cancelled {
            event_queue
                .blocking_dispatch(&mut state)
                .map_err(|err| format!("Wayland clipboard owner dispatch: {err}"))?;
        }
        return Ok(());
    }

    if has_global(&globals, ZwlrDataControlManagerV1::interface().name) {
        let mut event_queue = connection.new_event_queue::<WlrClipboardOwnerState>();
        let mut state =
            WlrClipboardOwnerState::bind(&globals, event_queue.handle(), connection, offers)?;
        state
            .connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        let _ = ready_tx.send(Ok(()));
        while !state.cancelled {
            event_queue
                .blocking_dispatch(&mut state)
                .map_err(|err| format!("Wayland clipboard owner dispatch: {err}"))?;
        }
        return Ok(());
    }

    Err("Wayland data-control clipboard protocol is not available".to_string())
}

fn has_global(globals: &GlobalList, interface: &str) -> bool {
    globals
        .contents()
        .with_list(|list| list.iter().any(|global| global.interface == interface))
}

fn bind_first_seat<State>(
    globals: &GlobalList,
    qh: &QueueHandle<State>,
) -> Result<wl_seat::WlSeat, String>
where
    State: Dispatch<wl_seat::WlSeat, ()> + 'static,
{
    globals
        .bind(qh, 1..=9, ())
        .map_err(|err| format!("Wayland clipboard seat: {err}"))
}

fn wait_for_selection<State>(
    event_queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
    ready: impl Fn(&State) -> bool,
) -> Result<(), String> {
    for _ in 0..8 {
        event_queue
            .roundtrip(state)
            .map_err(|err| format!("Wayland clipboard dispatch: {err}"))?;
        if ready(state) {
            return Ok(());
        }
    }
    Err("Wayland clipboard selection is not ready".to_string())
}

trait SelectionClient {
    fn mime_types(&self) -> &[String];
    fn read_mime(&self, mime_type: &str) -> Result<Vec<u8>, String>;
}

fn ensure_mime_offered(mime_types: &[String], mime_type: &str) -> Result<(), String> {
    if mime_types.iter().any(|candidate| candidate == mime_type) {
        Ok(())
    } else {
        Err(format!(
            "Wayland clipboard selection does not advertise {mime_type}"
        ))
    }
}

#[derive(Default)]
struct RegistryState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RegistryState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

fn write_offer_to_fd(offers: &[ClipboardOffer], mime_type: &str, fd: OwnedFd) {
    if let Some(offer) = offers.iter().find(|offer| offer.mime_type == mime_type) {
        let mut file = File::from(fd);
        let _ = file.write_all(&offer.data);
    }
}

struct ExtClipboardOwnerState {
    _seat: wl_seat::WlSeat,
    _manager: ExtDataControlManagerV1,
    _device: ExtDataControlDeviceV1,
    _source: ExtDataControlSourceV1,
    offers: Vec<ClipboardOffer>,
    cancelled: bool,
}

impl ExtClipboardOwnerState {
    fn bind(
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        connection: Connection,
        offers: Vec<ClipboardOffer>,
    ) -> Result<Self, String> {
        let seat = bind_first_seat(globals, &qh)?;
        let manager: ExtDataControlManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|err| format!("Wayland ext-data-control: {err}"))?;
        let device = manager.get_data_device(&seat, &qh, ());
        let source = manager.create_data_source(&qh, ());
        for offer in &offers {
            source.offer(offer.mime_type.clone());
        }
        device.set_selection(Some(&source));
        connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        Ok(Self {
            _seat: seat,
            _manager: manager,
            _device: device,
            _source: source,
            offers,
            cancelled: false,
        })
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ExtClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for ExtClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlManagerV1,
        _event: <ExtDataControlManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for ExtClipboardOwnerState {
    event_created_child!(ExtClipboardOwnerState, ExtDataControlDeviceV1, [
        0 => (ExtDataControlOfferV1, ())
    ]);

    fn event(
        state: &mut Self,
        _proxy: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if matches!(event, ext_data_control_device_v1::Event::Finished) {
            state.cancelled = true;
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for ExtClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlOfferV1,
        _event: ext_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlSourceV1, ()> for ExtClipboardOwnerState {
    fn event(
        state: &mut Self,
        _proxy: &ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => {
                write_offer_to_fd(&state.offers, &mime_type, fd);
            }
            ext_data_control_source_v1::Event::Cancelled => {
                state.cancelled = true;
            }
            _ => {}
        }
    }
}

delegate_noop!(ExtClipboardOwnerState: ignore wl_seat::WlSeat);

struct ExtSelectionState {
    connection: Connection,
    _seat: wl_seat::WlSeat,
    _manager: ExtDataControlManagerV1,
    _device: ExtDataControlDeviceV1,
    pending_offers: Vec<ExtOfferState>,
    selection: Option<ExtDataControlOfferV1>,
    mimes: Vec<String>,
    ready: bool,
}

struct ExtOfferState {
    offer: ExtDataControlOfferV1,
    mimes: Vec<String>,
}

impl ExtSelectionState {
    fn bind(
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        connection: Connection,
    ) -> Result<Self, String> {
        let seat = bind_first_seat(globals, &qh)?;
        let manager: ExtDataControlManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|err| format!("Wayland ext-data-control: {err}"))?;
        let device = manager.get_data_device(&seat, &qh, ());
        Ok(Self {
            connection,
            _seat: seat,
            _manager: manager,
            _device: device,
            pending_offers: Vec::new(),
            selection: None,
            mimes: Vec::new(),
            ready: false,
        })
    }

    fn offer_state_mut(&mut self, offer: &ExtDataControlOfferV1) -> Option<&mut ExtOfferState> {
        self.pending_offers
            .iter_mut()
            .find(|state| state.offer == *offer)
    }
}

impl SelectionClient for ExtSelectionState {
    fn mime_types(&self) -> &[String] {
        &self.mimes
    }

    fn read_mime(&self, mime_type: &str) -> Result<Vec<u8>, String> {
        ensure_mime_offered(&self.mimes, mime_type)?;
        let offer = self
            .selection
            .as_ref()
            .ok_or_else(|| "Wayland clipboard selection is empty".to_string())?;
        let (read_fd, write_fd) = pipe_with(PipeFlags::CLOEXEC)
            .map_err(|err| format!("Wayland clipboard pipe: {err}"))?;
        offer.receive(mime_type.to_string(), write_fd.as_fd());
        drop(write_fd);
        self.connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        let mut data = Vec::new();
        let mut file = File::from(read_fd);
        file.read_to_end(&mut data)
            .map_err(|err| format!("Wayland clipboard read {mime_type}: {err}"))?;
        Ok(data)
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ExtSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for ExtSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlManagerV1,
        _event: <ExtDataControlManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for ExtSelectionState {
    event_created_child!(ExtSelectionState, ExtDataControlDeviceV1, [
        0 => (ExtDataControlOfferV1, ())
    ]);

    fn event(
        state: &mut Self,
        _proxy: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                state.pending_offers.push(ExtOfferState {
                    offer: id,
                    mimes: Vec::new(),
                });
            }
            ext_data_control_device_v1::Event::Selection { id } => {
                state.selection = id.clone();
                state.mimes = id
                    .as_ref()
                    .and_then(|offer| {
                        state
                            .pending_offers
                            .iter()
                            .find(|pending| pending.offer == *offer)
                            .map(|pending| pending.mimes.clone())
                    })
                    .unwrap_or_default();
                state.ready = true;
            }
            ext_data_control_device_v1::Event::Finished => {
                state.ready = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for ExtSelectionState {
    fn event(
        state: &mut Self,
        proxy: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event
            && let Some(offer) = state.offer_state_mut(proxy)
        {
            offer.mimes.push(mime_type);
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, ()> for ExtSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlSourceV1,
        _event: <ExtDataControlSourceV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(ExtSelectionState: ignore wl_seat::WlSeat);

struct WlrClipboardOwnerState {
    connection: Connection,
    _seat: wl_seat::WlSeat,
    _manager: ZwlrDataControlManagerV1,
    _device: ZwlrDataControlDeviceV1,
    _source: ZwlrDataControlSourceV1,
    offers: Vec<ClipboardOffer>,
    cancelled: bool,
}

impl WlrClipboardOwnerState {
    fn bind(
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        connection: Connection,
        offers: Vec<ClipboardOffer>,
    ) -> Result<Self, String> {
        let seat = bind_first_seat(globals, &qh)?;
        let manager: ZwlrDataControlManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .map_err(|err| format!("Wayland wlr-data-control: {err}"))?;
        let device = manager.get_data_device(&seat, &qh, ());
        let source = manager.create_data_source(&qh, ());
        for offer in &offers {
            source.offer(offer.mime_type.clone());
        }
        device.set_selection(Some(&source));
        connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        Ok(Self {
            connection,
            _seat: seat,
            _manager: manager,
            _device: device,
            _source: source,
            offers,
            cancelled: false,
        })
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlrClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for WlrClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for WlrClipboardOwnerState {
    event_created_child!(WlrClipboardOwnerState, ZwlrDataControlDeviceV1, [
        0 => (ZwlrDataControlOfferV1, ())
    ]);

    fn event(
        state: &mut Self,
        _proxy: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if matches!(event, zwlr_data_control_device_v1::Event::Finished) {
            state.cancelled = true;
        }
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for WlrClipboardOwnerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlOfferV1,
        _event: zwlr_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for WlrClipboardOwnerState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrDataControlSourceV1,
        event: zwlr_data_control_source_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
                write_offer_to_fd(&state.offers, &mime_type, fd);
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                state.cancelled = true;
            }
            _ => {}
        }
    }
}

delegate_noop!(WlrClipboardOwnerState: ignore wl_seat::WlSeat);

struct WlrSelectionState {
    connection: Connection,
    _seat: wl_seat::WlSeat,
    _manager: ZwlrDataControlManagerV1,
    _device: ZwlrDataControlDeviceV1,
    pending_offers: Vec<WlrOfferState>,
    selection: Option<ZwlrDataControlOfferV1>,
    mimes: Vec<String>,
    ready: bool,
}

struct WlrOfferState {
    offer: ZwlrDataControlOfferV1,
    mimes: Vec<String>,
}

impl WlrSelectionState {
    fn bind(
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        connection: Connection,
    ) -> Result<Self, String> {
        let seat = bind_first_seat(globals, &qh)?;
        let manager: ZwlrDataControlManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .map_err(|err| format!("Wayland wlr-data-control: {err}"))?;
        let device = manager.get_data_device(&seat, &qh, ());
        Ok(Self {
            connection,
            _seat: seat,
            _manager: manager,
            _device: device,
            pending_offers: Vec::new(),
            selection: None,
            mimes: Vec::new(),
            ready: false,
        })
    }

    fn offer_state_mut(&mut self, offer: &ZwlrDataControlOfferV1) -> Option<&mut WlrOfferState> {
        self.pending_offers
            .iter_mut()
            .find(|state| state.offer == *offer)
    }
}

impl SelectionClient for WlrSelectionState {
    fn mime_types(&self) -> &[String] {
        &self.mimes
    }

    fn read_mime(&self, mime_type: &str) -> Result<Vec<u8>, String> {
        ensure_mime_offered(&self.mimes, mime_type)?;
        let offer = self
            .selection
            .as_ref()
            .ok_or_else(|| "Wayland clipboard selection is empty".to_string())?;
        let (read_fd, write_fd) = pipe_with(PipeFlags::CLOEXEC)
            .map_err(|err| format!("Wayland clipboard pipe: {err}"))?;
        offer.receive(mime_type.to_string(), write_fd.as_fd());
        drop(write_fd);
        self.connection
            .flush()
            .map_err(|err| format!("Wayland clipboard flush: {err}"))?;
        let mut data = Vec::new();
        let mut file = File::from(read_fd);
        file.read_to_end(&mut data)
            .map_err(|err| format!("Wayland clipboard read {mime_type}: {err}"))?;
        Ok(data)
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlrSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for WlrSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for WlrSelectionState {
    event_created_child!(WlrSelectionState, ZwlrDataControlDeviceV1, [
        0 => (ZwlrDataControlOfferV1, ())
    ]);

    fn event(
        state: &mut Self,
        _proxy: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                state.pending_offers.push(WlrOfferState {
                    offer: id,
                    mimes: Vec::new(),
                });
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                state.selection = id.clone();
                state.mimes = id
                    .as_ref()
                    .and_then(|offer| {
                        state
                            .pending_offers
                            .iter()
                            .find(|pending| pending.offer == *offer)
                            .map(|pending| pending.mimes.clone())
                    })
                    .unwrap_or_default();
                state.ready = true;
            }
            zwlr_data_control_device_v1::Event::Finished => {
                state.ready = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for WlrSelectionState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event
            && let Some(offer) = state.offer_state_mut(proxy)
        {
            offer.mimes.push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for WlrSelectionState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlSourceV1,
        _event: <ZwlrDataControlSourceV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(WlrSelectionState: ignore wl_seat::WlSeat);
