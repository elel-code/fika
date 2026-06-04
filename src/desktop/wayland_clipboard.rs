use rustix::pipe::{PipeFlags, pipe_with};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsFd;
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
};

use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::ExtDataControlSourceV1,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClipboardMimeData {
    pub(crate) mime_type: String,
    pub(crate) data: Vec<u8>,
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
