use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::dnd::{
    DndAction, DndActions, DndEvent, DndIcon, DndMimePayload, DndOfferId, DndReadPipe, DndSourceId,
};
use crate::event::{
    Event, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind, PopupConfigureKind,
    SurfaceEvent, ToplevelState,
};
use crate::input::{InputSerial, InputSerialSource};
use crate::surface::{
    DecorationPreference, Gravity, ManagedBlur, PopupAnchor, PopupPositioner, ProtocolSurface,
    SurfaceHandle, SurfaceId, SurfaceKind, SurfaceShared,
};
use crate::{
    BlurRegion, BlurState, CursorIcon, DialogAttributes, LogicalPosition, LogicalSize,
    PopupAttributes, SuggestedSize, ToplevelAttributes,
};
use smithay_client_toolkit::compositor::{
    CompositorHandler, CompositorState, FrameCallbackData, Region,
};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{DataSourceHandler, DragSource};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop as CalloopEventLoop, LoopSignal};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::backend::ObjectId;
use smithay_client_toolkit::reexports::client::globals::{GlobalList, registry_queue_init};
use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction as WlDndAction;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_data_device, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::{
    xdg_positioner, xdg_toplevel,
};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardData, KeyboardHandler, Modifiers as SctkModifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    CursorIcon as SctkCursorIcon, PointerData, PointerEvent as SctkPointerEvent,
    PointerEventKind as SctkPointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::dialog::{Dialog, DialogHandler};
use smithay_client_toolkit::shell::xdg::popup::{
    ConfigureKind, Popup, PopupConfigure, PopupHandler,
};
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell};
use smithay_client_toolkit::shm::slot::{Buffer as ShmBuffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;

#[derive(Clone, Debug)]
pub struct RuntimeOptions {
    /// Initial capacity for the owned event queue.
    pub event_capacity: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            event_capacity: 128,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub xdg_dialog_v1: bool,
    pub popup_reposition: bool,
    pub kde_blur: bool,
    pub cursor_shape: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("failed to connect to the Wayland compositor: {0}")]
    Connect(String),
    #[error("failed to initialize the Wayland registry: {0}")]
    Registry(String),
    #[error("required Wayland global is unavailable: {0}")]
    MissingGlobal(String),
    #[error("failed to initialize the event loop: {0}")]
    EventLoop(String),
    #[error("surface {0:?} does not exist")]
    SurfaceNotFound(SurfaceId),
    #[error("surface {0:?} cannot be used as a parent for this role")]
    InvalidParent(SurfaceId),
    #[error("popup positioner is invalid: {0}")]
    InvalidPositioner(&'static str),
    #[error("popup grabs require a pointer-press or touch-down serial")]
    InvalidPopupGrab,
    #[error("popup grab serial belongs to another Wayland connection or is no longer current")]
    ForeignOrStalePopupGrab,
    #[error("drag origin has no focused pointer seat with a current button serial")]
    InvalidDragSerial,
    #[error("DnD content must contain at least one MIME payload")]
    EmptyDndContent,
    #[error("DnD offer {0:?} does not exist")]
    DndOfferNotFound(DndOfferId),
    #[error("the compositor does not support {0}")]
    Unsupported(&'static str),
    #[error("Wayland protocol operation failed: {0}")]
    Protocol(String),
}

impl From<GlobalError> for RuntimeError {
    fn from(error: GlobalError) -> Self {
        Self::Protocol(error.to_string())
    }
}

/// Thread-safe handle for interrupting a blocking [`Runtime::dispatch`].
#[derive(Clone, Debug)]
pub struct WakeHandle(LoopSignal);

impl WakeHandle {
    pub fn wake(&self) {
        self.0.wakeup();
    }
}

/// The Wayland connection, protocol object graph, calloop dispatcher and owned event queue.
pub struct Runtime {
    connection: Connection,
    queue_handle: QueueHandle<RuntimeState>,
    event_loop: CalloopEventLoop<'static, RuntimeState>,
    state: RuntimeState,
    wake: WakeHandle,
    capabilities: RuntimeCapabilities,
}

impl Runtime {
    pub fn connect(options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| RuntimeError::Connect(error.to_string()))?;
        Self::from_connection(connection, options)
    }

    pub fn from_connection(
        connection: Connection,
        options: RuntimeOptions,
    ) -> Result<Self, RuntimeError> {
        let (globals, event_queue) = registry_queue_init(&connection)
            .map_err(|error| RuntimeError::Registry(error.to_string()))?;
        let queue_handle = event_queue.handle();
        let event_loop = CalloopEventLoop::<RuntimeState>::try_new()
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))?;

        let compositor = CompositorState::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let shm = Shm::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let xdg_shell = XdgShell::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let output_state = OutputState::new(&globals, &queue_handle);
        let seat_state = SeatState::new(&globals, &queue_handle);
        let data_device_manager = DataDeviceManagerState::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let blur_manager = globals
            .bind(&queue_handle, 1..=1, BlurManagerData)
            .ok()
            .map(|manager| KdeBlurManager { manager });

        let capabilities = RuntimeCapabilities {
            xdg_dialog_v1: has_global(&globals, "xdg_wm_dialog_v1"),
            popup_reposition: xdg_shell.xdg_wm_base().version() >= 3,
            kde_blur: blur_manager.is_some(),
            cursor_shape: has_global(&globals, "wp_cursor_shape_manager_v1"),
        };

        let state = RuntimeState {
            registry_state: RegistryState::new(&globals),
            output_state,
            seat_state,
            data_device_manager,
            compositor,
            shm,
            xdg_shell,
            blur_manager,
            surfaces: HashMap::new(),
            surface_ids: HashMap::new(),
            children: HashMap::new(),
            seats: HashMap::new(),
            keyboard_focus: HashMap::new(),
            incoming_dnd: HashMap::new(),
            active_dnd_by_device: HashMap::new(),
            outgoing_dnd: HashMap::new(),
            events: VecDeque::with_capacity(options.event_capacity),
            next_surface_id: 1,
            next_dnd_id: 1,
            next_button_order: 1,
        };

        WaylandSource::new(connection.clone(), event_queue)
            .insert(event_loop.handle())
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))?;
        let wake = WakeHandle(event_loop.get_signal());

        Ok(Self {
            connection,
            queue_handle,
            event_loop,
            state,
            wake,
            capabilities,
        })
    }

    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn wake_handle(&self) -> WakeHandle {
        self.wake.clone()
    }

    /// Wait for and dispatch protocol events. `None` waits indefinitely.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        self.event_loop
            .dispatch(timeout, &mut self.state)
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.state.events.drain(..)
    }

    pub fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let window = self.state.xdg_shell.create_window(
            surface,
            window_decorations(attributes.decorations),
            &self.queue_handle,
        );
        apply_toplevel_attributes(window.xdg_toplevel(), &attributes);
        window.commit();
        Ok(self.insert_surface(
            ProtocolSurface::Toplevel(window),
            None,
            SurfaceKind::Toplevel,
        ))
    }

    /// Create a parented toplevel and add xdg-dialog-v1 modality when available.
    ///
    /// If xdg-dialog-v1 is unavailable, the result remains a correctly parented
    /// transient toplevel and `capabilities().xdg_dialog_v1` is false.
    pub fn create_dialog(
        &mut self,
        parent: SurfaceId,
        attributes: DialogAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let parent_shared = self.parent_toplevel(parent)?;
        let parent_toplevel = parent_shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(parent))?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);

        let protocol = if self.capabilities.xdg_dialog_v1 {
            let dialog = self.state.xdg_shell.create_dialog(
                surface,
                window_decorations(attributes.toplevel.decorations),
                &self.queue_handle,
                parent_toplevel,
            )?;
            apply_toplevel_attributes(dialog.xdg_toplevel(), &attributes.toplevel);
            dialog.set_modal(attributes.modal);
            dialog.commit();
            ProtocolSurface::NativeDialog(dialog)
        } else {
            let window = self.state.xdg_shell.create_window(
                surface,
                window_decorations(attributes.toplevel.decorations),
                &self.queue_handle,
            );
            apply_toplevel_attributes(window.xdg_toplevel(), &attributes.toplevel);
            window.xdg_toplevel().set_parent(Some(parent_toplevel));
            window.commit();
            ProtocolSurface::FallbackDialog(window)
        };

        Ok(self.insert_surface(protocol, Some(parent_shared), SurfaceKind::Dialog))
    }

    pub fn create_popup(
        &mut self,
        parent: SurfaceId,
        attributes: PopupAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        validate_positioner(&attributes.positioner)?;
        if attributes
            .grab
            .as_ref()
            .is_some_and(|serial| !serial.is_popup_grab())
        {
            return Err(RuntimeError::InvalidPopupGrab);
        }
        if let Some(serial) = attributes.grab.as_ref() {
            let same_connection =
                serial.seat.backend().upgrade().as_ref() == Some(&self.connection.backend());
            let current = self
                .state
                .seats
                .get(&serial.seat.id().protocol_id())
                .and_then(|objects| objects.latest_button_serial)
                .is_some_and(|value| value.serial == serial.serial);
            if !same_connection || !current {
                return Err(RuntimeError::ForeignOrStalePopupGrab);
            }
        }

        let parent_shared = self
            .state
            .surfaces
            .get(&parent)
            .cloned()
            .ok_or(RuntimeError::SurfaceNotFound(parent))?;
        let positioner = self.make_positioner(&attributes.positioner)?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let popup = Popup::from_surface(
            Some(parent_shared.protocol.xdg_surface()),
            &positioner,
            &self.queue_handle,
            surface,
            &self.state.xdg_shell,
        )?;
        if let Some(serial) = attributes.grab.as_ref() {
            popup.xdg_popup().grab(&serial.seat, serial.serial);
        }
        popup.commit();

        Ok(self.insert_surface(
            ProtocolSurface::Popup(popup),
            Some(parent_shared),
            SurfaceKind::Popup,
        ))
    }

    pub fn reposition_popup(
        &mut self,
        surface: SurfaceId,
        positioner: &PopupPositioner,
        token: u32,
    ) -> Result<(), RuntimeError> {
        validate_positioner(positioner)?;
        if !self.capabilities.popup_reposition {
            return Err(RuntimeError::Unsupported("xdg-popup reposition"));
        }
        let positioner = self.make_positioner(positioner)?;
        let shared = self.surface_shared(surface)?;
        let ProtocolSurface::Popup(popup) = &shared.protocol else {
            return Err(RuntimeError::InvalidParent(surface));
        };
        popup.reposition(&positioner, token);
        Ok(())
    }

    pub fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        self.state
            .surfaces
            .get(&surface)
            .cloned()
            .map(|shared| SurfaceHandle { shared })
    }

    pub fn request_frame(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        wl_surface.frame(&self.queue_handle, FrameCallbackData(wl_surface.clone()));
        Ok(())
    }

    pub fn commit(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.surface_shared(surface)?.wl_surface().commit();
        Ok(())
    }

    pub fn set_window_geometry(
        &self,
        surface: SurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        if size.is_empty() {
            return Err(RuntimeError::Protocol(
                "window geometry must have non-zero dimensions".to_string(),
            ));
        }
        self.surface_shared(surface)?
            .protocol
            .xdg_surface()
            .set_window_geometry(
                origin.x,
                origin.y,
                u32_to_i32(size.width),
                u32_to_i32(size.height),
            );
        Ok(())
    }

    pub fn set_title(
        &self,
        surface: SurfaceId,
        title: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        toplevel.set_title(title.into());
        Ok(())
    }

    pub fn set_app_id(
        &self,
        surface: SurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        toplevel.set_app_id(app_id.into());
        Ok(())
    }

    pub fn set_min_size(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        let size = size.unwrap_or_default();
        toplevel.set_min_size(u32_to_i32(size.width), u32_to_i32(size.height));
        Ok(())
    }

    pub fn set_max_size(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        let size = size.unwrap_or_default();
        toplevel.set_max_size(u32_to_i32(size.width), u32_to_i32(size.height));
        Ok(())
    }

    /// Set the integer buffer scale used to interpret attached renderer buffers.
    pub fn set_buffer_scale(&self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        if factor < 1 {
            return Err(RuntimeError::Protocol(
                "buffer scale must be at least one".to_string(),
            ));
        }
        self.surface_shared(surface)?
            .wl_surface()
            .set_buffer_scale(factor);
        Ok(())
    }

    pub fn set_blur(&self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        let manager = self
            .state
            .blur_manager
            .as_ref()
            .ok_or(RuntimeError::Unsupported("org_kde_kwin_blur_manager"))?;
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        let mut current = shared.blur.lock().expect("blur state mutex poisoned");

        match state {
            BlurState::Disabled => {
                if current.is_some() {
                    manager.manager.unset(wl_surface);
                    current.take();
                }
            }
            BlurState::Enabled(region) => {
                if current.is_none() {
                    let blur = manager
                        .manager
                        .create(wl_surface, &self.queue_handle, BlurData);
                    *current = Some(ManagedBlur(blur));
                }
                let blur = &current.as_ref().expect("blur was initialized").0;
                match region {
                    BlurRegion::EntireSurface => blur.set_region(None),
                    BlurRegion::Rectangles(rectangles) => {
                        let region = Region::new(&self.state.compositor)?;
                        for rectangle in rectangles.into_iter().filter(|rect| !rect.is_empty()) {
                            region.add(
                                rectangle.origin.x,
                                rectangle.origin.y,
                                u32_to_i32(rectangle.size.width),
                                u32_to_i32(rectangle.size.height),
                            );
                        }
                        blur.set_region(Some(region.wl_region()));
                    }
                }
                blur.commit();
            }
        }
        Ok(())
    }

    pub fn set_cursor(&self, icon: CursorIcon) -> Result<(), RuntimeError> {
        for objects in self.state.seats.values() {
            let Some(pointer) = objects.pointer.as_ref() else {
                continue;
            };
            if pointer
                .pointer()
                .data::<PointerData<()>>()
                .and_then(PointerData::latest_enter_serial)
                .is_none()
            {
                continue;
            }
            pointer
                .set_cursor(&self.connection, map_cursor_icon(icon))
                .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
        }
        Ok(())
    }

    /// Start an external drag using the origin's focused pointer seat.
    ///
    /// Call this while handling the pointer gesture which activated the drag.
    /// The runtime owns the compositor serial and selects the newest matching
    /// seat, so applications do not need to retain protocol serials.
    pub fn start_drag(
        &mut self,
        origin: SurfaceId,
        payloads: Vec<DndMimePayload>,
        actions: DndActions,
        icon: Option<DndIcon>,
    ) -> Result<DndSourceId, RuntimeError> {
        if payloads.is_empty() {
            return Err(RuntimeError::EmptyDndContent);
        }
        let origin_surface = self.surface_shared(origin)?;
        let candidates = self.state.seats.iter().map(|(seat_id, objects)| {
            (
                *seat_id,
                objects.pointer_focus,
                objects.data_device.is_some(),
                objects.latest_button_serial,
            )
        });
        let (seat_id, serial) =
            select_drag_seat(origin, candidates).ok_or(RuntimeError::InvalidDragSerial)?;
        let icon = icon
            .map(|icon| prepare_dnd_icon_surface(&mut self.state, &self.queue_handle, icon))
            .transpose()?;
        let data_device = self
            .state
            .seats
            .get(&seat_id)
            .and_then(|objects| objects.data_device.as_ref())
            .ok_or(RuntimeError::Unsupported("wl_data_device"))?;
        let source = self.state.data_device_manager.create_drag_and_drop_source(
            &self.queue_handle,
            payloads.iter().map(DndMimePayload::mime),
            map_dnd_actions(actions),
        );
        let id = DndSourceId(self.state.next_dnd_id);
        self.state.next_dnd_id += 1;
        source.start_drag(
            data_device,
            origin_surface.wl_surface(),
            icon.as_ref().map(|icon| &icon.surface),
            serial,
        );
        // Match winit #4571: on KDE, committing the icon before start_drag can
        // prevent its offset from taking effect.
        if let Some(icon) = icon.as_ref() {
            icon.surface.commit();
        }
        self.state.outgoing_dnd.insert(
            source.inner().id(),
            OutgoingDndSource {
                id,
                _source: source,
                payloads,
                selected_action: None,
                _icon: icon,
            },
        );
        Ok(id)
    }

    pub fn set_dnd_offer_actions(
        &self,
        offer: DndOfferId,
        accepted_mime: Option<&str>,
        actions: DndActions,
        preferred: Option<DndAction>,
    ) -> Result<(), RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .get(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        offer
            .offer
            .accept_mime_type(offer.offer.serial, accepted_mime.map(str::to_string));
        offer.offer.set_actions(
            map_dnd_actions(actions),
            preferred
                .map(map_dnd_action)
                .unwrap_or_else(WlDndAction::empty),
        );
        Ok(())
    }

    pub fn receive_dnd(
        &self,
        offer: DndOfferId,
        mime: impl Into<String>,
    ) -> Result<DndReadPipe, RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .get(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        offer
            .offer
            .receive(mime.into())
            .map(DndReadPipe)
            .map_err(|error| RuntimeError::Protocol(error.to_string()))
    }

    pub fn finish_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .remove(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        self.state
            .active_dnd_by_device
            .retain(|_, active| *active != offer.id);
        offer.offer.finish();
        offer.offer.destroy();
        Ok(())
    }

    /// Remove a surface and every descendant from the runtime in child-first order.
    /// Renderer-held [`SurfaceHandle`] leases may keep those protocol objects alive;
    /// each child lease holds its parent so the protocol destruction order remains valid.
    pub fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        if !self.state.surfaces.contains_key(&surface) {
            return Err(RuntimeError::SurfaceNotFound(surface));
        }
        let mut order = Vec::new();
        collect_post_order(&self.state.children, surface, &mut order);
        for id in &order {
            self.state.remove_surface(*id);
        }
        Ok(order)
    }

    fn insert_surface(
        &mut self,
        protocol: ProtocolSurface,
        parent: Option<Arc<SurfaceShared>>,
        kind: SurfaceKind,
    ) -> SurfaceId {
        let id = SurfaceId(self.state.next_surface_id);
        self.state.next_surface_id += 1;
        let protocol_id = protocol.wl_surface().id();
        let parent_id = parent.as_ref().map(|parent| parent.id);
        let shared = Arc::new(SurfaceShared {
            blur: Default::default(),
            protocol,
            parent,
            connection: self.connection.clone(),
            id,
            kind,
        });
        self.state.surface_ids.insert(protocol_id, id);
        self.state.surfaces.insert(id, shared);
        if let Some(parent_id) = parent_id {
            self.state.children.entry(parent_id).or_default().push(id);
        }
        id
    }

    fn surface_shared(&self, surface: SurfaceId) -> Result<Arc<SurfaceShared>, RuntimeError> {
        self.state
            .surfaces
            .get(&surface)
            .cloned()
            .ok_or(RuntimeError::SurfaceNotFound(surface))
    }

    fn parent_toplevel(&self, parent: SurfaceId) -> Result<Arc<SurfaceShared>, RuntimeError> {
        let shared = self.surface_shared(parent)?;
        if shared.protocol.xdg_toplevel().is_none() {
            return Err(RuntimeError::InvalidParent(parent));
        }
        Ok(shared)
    }

    fn make_positioner(&self, value: &PopupPositioner) -> Result<XdgPositioner, RuntimeError> {
        let positioner = XdgPositioner::new(&self.state.xdg_shell)?;
        positioner.set_size(u32_to_i32(value.size.width), u32_to_i32(value.size.height));
        positioner.set_anchor_rect(
            value.anchor_rect.origin.x,
            value.anchor_rect.origin.y,
            u32_to_i32(value.anchor_rect.size.width),
            u32_to_i32(value.anchor_rect.size.height),
        );
        positioner.set_anchor(map_anchor(value.anchor));
        positioner.set_gravity(map_gravity(value.gravity));
        positioner.set_constraint_adjustment(map_constraints(value.constraints));
        positioner.set_offset(value.offset.x, value.offset.y);
        if positioner.version() >= 3 {
            if value.reactive {
                positioner.set_reactive();
            }
            if let Some(parent_size) = value.parent_size {
                positioner.set_parent_size(
                    u32_to_i32(parent_size.width),
                    u32_to_i32(parent_size.height),
                );
            }
            if let Some(serial) = value.parent_configure {
                positioner.set_parent_configure(serial);
            }
        }
        Ok(positioner)
    }
}

include!("runtime_helpers.rs");
struct RuntimeState {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    data_device_manager: DataDeviceManagerState,
    compositor: CompositorState,
    shm: Shm,
    xdg_shell: XdgShell,
    blur_manager: Option<KdeBlurManager>,
    surfaces: HashMap<SurfaceId, Arc<SurfaceShared>>,
    surface_ids: HashMap<ObjectId, SurfaceId>,
    children: HashMap<SurfaceId, Vec<SurfaceId>>,
    seats: HashMap<u32, SeatObjects>,
    keyboard_focus: HashMap<u32, SurfaceId>,
    incoming_dnd: HashMap<DndOfferId, IncomingDndOffer>,
    active_dnd_by_device: HashMap<ObjectId, DndOfferId>,
    outgoing_dnd: HashMap<ObjectId, OutgoingDndSource>,
    events: VecDeque<Event>,
    next_surface_id: u64,
    next_dnd_id: u64,
    next_button_order: u64,
}

struct IncomingDndOffer {
    id: DndOfferId,
    offer: DragOffer,
    surface: SurfaceId,
}

struct OutgoingDndSource {
    id: DndSourceId,
    _source: DragSource,
    payloads: Vec<DndMimePayload>,
    selected_action: Option<DndAction>,
    _icon: Option<DndIconSurface>,
}

struct DndIconSurface {
    surface: wl_surface::WlSurface,
    _buffer: ShmBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ButtonSerial {
    surface: SurfaceId,
    serial: u32,
    order: u64,
}

impl Drop for DndIconSurface {
    fn drop(&mut self) {
        if self.surface.is_alive() {
            self.surface.destroy();
        }
    }
}

impl RuntimeState {
    fn surface_id(&self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surface_ids.get(&surface.id()).copied()
    }

    fn remove_surface(&mut self, id: SurfaceId) {
        let Some(shared) = self.surfaces.remove(&id) else {
            return;
        };
        self.surface_ids.remove(&shared.wl_surface().id());
        self.children.remove(&id);
        if let Some(parent) = shared.parent.as_ref() {
            if let Some(children) = self.children.get_mut(&parent.id) {
                children.retain(|child| *child != id);
            }
        }
        self.keyboard_focus.retain(|_, focused| *focused != id);
        for objects in self.seats.values_mut() {
            if objects.pointer_focus == Some(id) {
                objects.pointer_focus = None;
            }
        }
    }

    fn record_button_serial(&mut self, seat_id: u32, surface: SurfaceId, serial: u32) {
        let order = self.next_button_order;
        self.next_button_order = self.next_button_order.saturating_add(1);
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.latest_button_serial = Some(ButtonSerial {
                surface,
                serial,
                order,
            });
        }
    }

    fn push_key(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
        state: KeyState,
        serial: u32,
        event: KeyEvent,
    ) {
        let keyboard_id = keyboard.id().protocol_id();
        let Some(surface) = self.keyboard_focus.get(&keyboard_id).copied() else {
            return;
        };
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        let serial = InputSerial::new(data.seat().clone(), serial, InputSerialSource::KeyboardKey);
        self.events.push_back(Event::Keyboard(KeyboardEvent::Key {
            surface,
            state,
            time: event.time,
            raw_code: event.raw_code,
            keysym: event.keysym.raw(),
            text: event.utf8,
            serial,
        }));
    }
}

fn collect_post_order(
    children: &HashMap<SurfaceId, Vec<SurfaceId>>,
    id: SurfaceId,
    order: &mut Vec<SurfaceId>,
) {
    if let Some(direct_children) = children.get(&id) {
        for child in direct_children.iter().copied() {
            collect_post_order(children, child, order);
        }
    }
    order.push(id);
}

fn select_drag_seat(
    origin: SurfaceId,
    candidates: impl IntoIterator<Item = (u32, Option<SurfaceId>, bool, Option<ButtonSerial>)>,
) -> Option<(u32, u32)> {
    candidates
        .into_iter()
        .filter_map(|(seat_id, pointer_focus, has_data_device, button)| {
            let button = button?;
            (has_data_device && pointer_focus == Some(origin) && button.surface == origin)
                .then_some((seat_id, button))
        })
        .max_by_key(|(_, button)| button.order)
        .map(|(seat_id, button)| (seat_id, button.serial))
}

#[derive(Default)]
struct SeatObjects {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    data_device: Option<DataDevice>,
    pointer_focus: Option<SurfaceId>,
    latest_button_serial: Option<ButtonSerial>,
}

impl Drop for SeatObjects {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take()
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        self.pointer.take();
    }
}

include!("runtime_handlers.rs");
#[derive(Clone, Debug)]
struct KdeBlurManager {
    manager: OrgKdeKwinBlurManager,
}

#[derive(Debug)]
struct BlurManagerData;

impl Dispatch2<OrgKdeKwinBlurManager, RuntimeState> for BlurManagerData {
    fn event(
        &self,
        _: &mut RuntimeState,
        _: &OrgKdeKwinBlurManager,
        _: <OrgKdeKwinBlurManager as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<RuntimeState>,
    ) {
        unreachable!("org_kde_kwin_blur_manager has no events");
    }
}

#[derive(Debug)]
struct BlurData;

impl Dispatch2<OrgKdeKwinBlur, RuntimeState> for BlurData {
    fn event(
        &self,
        _: &mut RuntimeState,
        _: &OrgKdeKwinBlur,
        _: <OrgKdeKwinBlur as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<RuntimeState>,
    ) {
        unreachable!("org_kde_kwin_blur has no events");
    }
}

delegate_registry!(RuntimeState);
smithay_client_toolkit::delegate_dispatch2!(RuntimeState);

include!("runtime_tests.rs");
