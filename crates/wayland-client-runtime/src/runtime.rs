use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::activation::{ActivationHandler, ActivationManager, ActivationTokenPurpose};
use crate::data_transfer::{TransferContent, TransferReadPipe};
use crate::dnd::{DndAction, DndActions, DndEvent, DndIcon, DndOfferId, DndReadPipe, DndSourceId};
use crate::event::{
    Event, EventBuffer, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind,
    PopupConfigureKind, SurfaceEvent, ToplevelState, TouchEvent, TouchEventKind,
};
use crate::fractional_scale::{FractionalScaleHandler, FractionalScaleManager};
use crate::input::{InputSerial, InputSerialSource};
use crate::layer_shell::{
    LayerProtocolEvent, LayerShellManager, LayerSurfaceAttributes, LayerSurfaceData,
    LayerSurfaceError, LayerSurfaceEvent, LayerSurfaceState, handle_layer_event,
};
use crate::output::output_info;
use crate::pointer_axis::{map_axis_source, map_axis_value};
use crate::pointer_constraints::{
    PointerCaptureTarget, PointerProtocols, SeatPointerSession, validate_pointer_capture_state,
};
use crate::pointer_gestures::{
    GestureSubscriptionChange, PointerGestureHandler, PointerGestureManager,
    PointerGestureSubscriptions, SeatPointerGestures,
};
use crate::shm_format::copy_rgba_to_premultiplied_argb8888;
use crate::surface::{
    DecorationPreference, Gravity, ManagedBlur, PopupAnchor, PopupPositioner, ProtocolSurface,
    SurfaceHandle, SurfaceId, SurfaceKind, SurfaceShared,
};
use crate::toplevel_icon::ToplevelIconManager;
use crate::text_input::{PendingBatch, SeatTextInput, TextInputHandler, TextInputManager};
use crate::toplevel_interaction::{
    PointerPressTracker, ToplevelInteraction, select_active_pointer_press,
};
use crate::touch::{TouchData, TouchHandler, TouchPoints};
use crate::{
    ActivationEvent, ActivationRequestId, ActivationToken, ActivationTokenAttributes, BlurRegion,
    BlurState, CursorIcon, DialogAttributes, LogicalPosition, LogicalSize, OutputEvent, OutputId,
    OutputInfo, PointerCaptureState, PointerConstraint, PointerConstraintError,
    PointerConstraintRegion, PointerGestureEvent, PopupAttributes, RelativePointerEvent,
    ResizeEdge, SuggestedSize, TextInputEvent, TextInputState, ToplevelAttributes, ToplevelIcon,
};
use smithay_client_toolkit::background_effect::{
    BackgroundEffectHandler, BackgroundEffectState,
};
use smithay_client_toolkit::compositor::{
    CompositorHandler, CompositorState, FrameCallbackData, Region,
};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler, DragSource,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop as CalloopEventLoop, LoopSignal};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::backend::ObjectId;
use smithay_client_toolkit::reexports::client::globals::{GlobalList, registry_queue_init};
use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction as WlDndAction;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_data_device, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface,
    wl_touch,
};
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::{
    xdg_positioner, xdg_toplevel,
};
use smithay_client_toolkit::reexports::protocols::wp::pointer_constraints::zv1::client::zwp_confined_pointer_v1::ZwpConfinedPointerV1;
use smithay_client_toolkit::reexports::protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::ZwpLockedPointerV1;
use smithay_client_toolkit::reexports::protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3;
use smithay_client_toolkit::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::Capability as BackgroundEffectCapability;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardData, KeyboardHandler, Modifiers as SctkModifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    CursorIcon as SctkCursorIcon, PointerData, PointerEvent as SctkPointerEvent,
    PointerEventKind as SctkPointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::pointer_constraints::PointerConstraintsHandler;
use smithay_client_toolkit::seat::relative_pointer::{
    RelativeMotionEvent, RelativePointerHandler,
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
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

#[derive(Clone, Debug)]
pub struct RuntimeOptions {
    /// Initial capacity for the owned event batch.
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
    /// The compositor supports exporting and consuming xdg activation tokens.
    pub xdg_activation_v1: bool,
    /// The compositor supports assigning per-toplevel names or pixel icons.
    pub xdg_toplevel_icon_v1: bool,
    /// A deployed layer-shell backend is available.
    pub layer_shell_v1: bool,
    /// The layer-shell backend supports changing an existing surface's layer.
    pub layer_shell_dynamic_layer: bool,
    /// The layer-shell backend supports normal on-demand keyboard focus.
    pub layer_shell_on_demand_keyboard: bool,
    /// The layer-shell backend supports explicit exclusive-edge disambiguation.
    pub layer_shell_exclusive_edge: bool,
    /// The compositor supports seat-scoped text input and input methods.
    pub text_input_v3: bool,
    /// The compositor can confine or lock a pointer to a surface.
    pub pointer_constraints_v1: bool,
    /// The compositor can report accelerated and unaccelerated relative motion.
    pub relative_pointer_v1: bool,
    /// The compositor supports swipe and pinch pointer gestures.
    pub pointer_gestures_v1: bool,
    /// The pointer-gestures-v1 global is new enough to support hold gestures.
    pub pointer_gesture_hold_v1: bool,
    pub popup_reposition: bool,
    /// The compositor currently advertises the ext-background-effect-v1 blur capability.
    pub ext_background_effect: bool,
    /// Fractional scale is usable only when both protocol globals are present.
    pub fractional_scale: bool,
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
    #[error("activation serial belongs to another Wayland connection")]
    ForeignActivationSerial,
    #[error("surface {0:?} is not an activatable toplevel")]
    InvalidActivationTarget(SurfaceId),
    #[error("surface {0:?} cannot have an xdg toplevel icon")]
    InvalidToplevelIconTarget(SurfaceId),
    #[error("surface {0:?} is not an xdg toplevel")]
    InvalidToplevelInteractionTarget(SurfaceId),
    #[error("toplevel interaction requires a focused pointer seat with a pressed button")]
    InvalidToplevelInteractionSerial,
    #[error("surface {0:?} is not a layer surface")]
    InvalidLayerSurfaceTarget(SurfaceId),
    #[error("output {0:?} is no longer available")]
    OutputNotFound(OutputId),
    #[error("invalid layer surface: {0}")]
    InvalidLayerSurface(#[from] LayerSurfaceError),
    #[error("surface {0:?} does not support xdg window geometry")]
    InvalidWindowGeometryTarget(SurfaceId),
    #[error("drag origin has no focused pointer seat with a current button serial")]
    InvalidDragSerial,
    #[error("clipboard selection has no focused seat with a current input serial")]
    InvalidSelectionSerial,
    #[error("clipboard selection is unavailable")]
    SelectionUnavailable,
    #[error("clipboard selection has none of the requested MIME types")]
    SelectionMimeNotFound,
    #[error("DnD offer {0:?} does not exist")]
    DndOfferNotFound(DndOfferId),
    #[error("the compositor does not support {0}")]
    Unsupported(&'static str),
    #[error("surface {0:?} has not requested a locked pointer")]
    PointerNotLocked(SurfaceId),
    #[error("invalid pointer constraint: {0}")]
    InvalidPointerConstraint(#[from] PointerConstraintError),
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
        let (globals, mut event_queue) = registry_queue_init(&connection)
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
        let background_effect_state = BackgroundEffectState::new(&globals, &queue_handle);
        let xdg_activation = ActivationManager::bind(&globals, &queue_handle).ok();
        let toplevel_icon_manager = ToplevelIconManager::bind(&globals, &queue_handle).ok();
        let layer_shell_manager = LayerShellManager::bind(&globals, &queue_handle).ok();
        let text_input_manager = TextInputManager::bind(&globals, &queue_handle).ok();
        let fractional_scale_manager = FractionalScaleManager::bind(&globals, &queue_handle).ok();
        let pointer_gesture_manager = PointerGestureManager::bind(&globals, &queue_handle).ok();
        let pointer_protocols = PointerProtocols::bind(
            &globals,
            &queue_handle,
            has_global(&globals, "zwp_pointer_constraints_v1"),
            has_global(&globals, "zwp_relative_pointer_manager_v1"),
        );
        let data_device_manager = DataDeviceManagerState::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let capabilities = RuntimeCapabilities {
            xdg_dialog_v1: has_global(&globals, "xdg_wm_dialog_v1"),
            xdg_activation_v1: xdg_activation.is_some(),
            xdg_toplevel_icon_v1: toplevel_icon_manager.is_some(),
            layer_shell_v1: layer_shell_manager.is_some(),
            layer_shell_dynamic_layer: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 2),
            layer_shell_on_demand_keyboard: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 4),
            layer_shell_exclusive_edge: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 5),
            text_input_v3: text_input_manager.is_some(),
            pointer_constraints_v1: pointer_protocols.has_constraints(),
            relative_pointer_v1: pointer_protocols.has_relative_pointer(),
            pointer_gestures_v1: pointer_gesture_manager.is_some(),
            pointer_gesture_hold_v1: pointer_gesture_manager
                .as_ref()
                .is_some_and(PointerGestureManager::supports_hold),
            popup_reposition: xdg_shell.xdg_wm_base().version() >= 3,
            ext_background_effect: false,
            fractional_scale: fractional_scale_manager.is_some(),
            cursor_shape: has_global(&globals, "wp_cursor_shape_manager_v1"),
        };

        let mut state = RuntimeState {
            registry_state: RegistryState::new(&globals),
            output_state,
            seat_state,
            background_effect_state,
            data_device_manager,
            compositor,
            shm,
            xdg_shell,
            xdg_activation,
            toplevel_icon_manager,
            layer_shell_manager,
            text_input_manager,
            fractional_scale_manager,
            pointer_gesture_manager,
            pointer_protocols,
            pointer_gesture_subscriptions: PointerGestureSubscriptions::default(),
            surfaces: HashMap::new(),
            surface_ids: HashMap::new(),
            children: HashMap::new(),
            seats: HashMap::new(),
            keyboard_focus: HashMap::new(),
            incoming_dnd: HashMap::new(),
            active_dnd_by_device: HashMap::new(),
            outgoing_dnd: HashMap::new(),
            selection_sources: HashMap::new(),
            pending_attention: HashSet::new(),
            events: EventBuffer::with_capacity(options.event_capacity),
            next_surface_id: 1,
            next_dnd_id: 1,
            next_input_order: 1,
            next_activation_request_id: 1,
        };

        // ext-background-effect-v1 advertises effect support in an event after
        // binding. Complete one roundtrip so capabilities are accurate when
        // `from_connection` returns.
        if has_global(&globals, "ext_background_effect_manager_v1")
            || state.toplevel_icon_manager.is_some()
        {
            event_queue
                .roundtrip(&mut state)
                .map_err(|error| RuntimeError::Registry(error.to_string()))?;
        }

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
        let mut capabilities = self.capabilities;
        capabilities.ext_background_effect =
            supports_ext_background_blur(self.state.background_effect_state.capabilities());
        capabilities
    }

    /// Metadata snapshots for outputs whose initial compositor description is complete.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.state
            .output_state
            .outputs()
            .filter_map(|output| output_info(&self.state.output_state, &output))
            .collect()
    }

    /// Preferred square icon sizes advertised by the compositor, in logical
    /// pixels. An empty list means the compositor has no size preference or
    /// does not support xdg-toplevel-icon-v1.
    pub fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        self.state
            .toplevel_icon_manager
            .as_ref()
            .map(ToplevelIconManager::preferred_sizes)
            .unwrap_or_default()
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
        self.state.events.drain()
    }

    /// Append all pending events to a reusable caller-owned batch.
    ///
    /// Unlike collecting [`Runtime::drain_events`], this lets event loops keep
    /// one allocation across dispatch iterations. Existing items in `target`
    /// are preserved before the newly drained events.
    pub fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        self.state.events.drain_into(target);
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

    /// Create a layer surface and perform its required initial bufferless commit.
    ///
    /// Wait for [`Event::LayerSurface`] with
    /// [`LayerSurfaceEvent::Configure`] before attaching the first renderer
    /// buffer. All layer state is double-buffered with `wl_surface`.
    pub fn create_layer_surface(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let manager = self
            .state
            .layer_shell_manager
            .as_ref()
            .ok_or(RuntimeError::Unsupported("layer-shell-v1"))?;
        let output = attributes
            .output
            .map(|output| self.resolve_output(output))
            .transpose()?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let layer =
            manager.create_surface(&self.queue_handle, surface, output.as_ref(), &attributes)?;
        let id = self.insert_surface(ProtocolSurface::Layer(layer), None, SurfaceKind::Layer);
        self.surface_shared(id)?.wl_surface().commit();
        Ok(id)
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
                .is_some_and(|objects| {
                    is_current_popup_grab(objects, serial.source(), serial.serial)
                });
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
            parent_shared.protocol.xdg_surface(),
            &positioner,
            &self.queue_handle,
            surface,
            &self.state.xdg_shell,
        )?;
        if let Some(layer) = parent_shared.protocol.layer_surface() {
            layer.role().get_popup(popup.xdg_popup());
        }
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

    /// Request a compositor activation token associated with `surface`.
    ///
    /// Completion is asynchronous and reported as
    /// [`Event::Activation`] carrying [`ActivationEvent::TokenDone`].
    /// Supplying a recent input serial generally gives the compositor enough
    /// context to issue an effective token, but all request attributes are
    /// optional in the protocol.
    pub fn request_activation_token(
        &mut self,
        surface: SurfaceId,
        attributes: ActivationTokenAttributes,
    ) -> Result<ActivationRequestId, RuntimeError> {
        self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        if let Some(serial) = attributes.serial.as_ref() {
            self.validate_activation_serial(serial)?;
        }

        let request = take_activation_request_id(&mut self.state.next_activation_request_id);
        self.state
            .xdg_activation
            .as_ref()
            .expect("activation support checked above")
            .request_token(
                &self.queue_handle,
                ActivationTokenPurpose::Export { request, surface },
                shared.wl_surface(),
                attributes,
            );
        Ok(request)
    }

    /// Activate `surface` with a token received from this runtime or through
    /// an external channel such as `XDG_ACTIVATION_TOKEN`.
    pub fn activate_surface(
        &self,
        surface: SurfaceId,
        token: ActivationToken,
    ) -> Result<(), RuntimeError> {
        let activation = self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        validate_activation_target(surface, shared.kind)?;
        activation.activate(shared.wl_surface(), token);
        Ok(())
    }

    /// Ask the compositor to draw attention to `surface`.
    ///
    /// This mirrors winit's Wayland path: request a surface-associated token
    /// and activate the same surface when the token arrives. Repeated requests
    /// are coalesced while one is pending.
    pub fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        validate_activation_target(surface, shared.kind)?;
        if !begin_attention_request(&mut self.state.pending_attention, surface) {
            return Ok(());
        }
        self.state
            .xdg_activation
            .as_ref()
            .expect("activation support checked above")
            .request_token(
                &self.queue_handle,
                ActivationTokenPurpose::Attention { surface },
                shared.wl_surface(),
                ActivationTokenAttributes::default(),
            );
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
            .ok_or(RuntimeError::InvalidWindowGeometryTarget(surface))?
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

    /// Replace the complete double-buffered state of a layer surface.
    ///
    /// Equal state is ignored and only changed protocol fields are sent. Call
    /// [`Runtime::commit`] to apply the update atomically.
    pub fn set_layer_surface_state(
        &self,
        surface: SurfaceId,
        state: LayerSurfaceState,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let layer = shared
            .protocol
            .layer_surface()
            .ok_or(RuntimeError::InvalidLayerSurfaceTarget(surface))?;
        layer.apply_state(state)?;
        Ok(())
    }

    pub fn layer_surface_state(
        &self,
        surface: SurfaceId,
    ) -> Result<LayerSurfaceState, RuntimeError> {
        let shared = self.surface_shared(surface)?;
        shared
            .protocol
            .layer_surface()
            .map(|layer| layer.state())
            .ok_or(RuntimeError::InvalidLayerSurfaceTarget(surface))
    }

    /// Begin a compositor-driven move using the newest pointer press that is
    /// still held over this toplevel.
    ///
    /// Call this while handling a pointer press. Wayland rejects requests made
    /// without the press serial for the active implicit pointer grab.
    pub fn begin_interactive_move(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::Move)
    }

    /// Begin a compositor-driven resize using the newest pointer press that is
    /// still held over this toplevel.
    pub fn begin_interactive_resize(
        &self,
        surface: SurfaceId,
        edge: ResizeEdge,
    ) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::Resize(edge))
    }

    /// Show the compositor's window menu at a surface-local logical position.
    ///
    /// Call this while handling a pointer press so the runtime can supply the
    /// active implicit-grab serial required by xdg-shell.
    pub fn show_window_menu(
        &self,
        surface: SurfaceId,
        position: LogicalPosition,
    ) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::WindowMenu(position))
    }

    /// Set or clear the icon for an individual toplevel.
    ///
    /// Named icons follow the active XDG icon theme. Pixel icons are copied
    /// into immutable premultiplied ARGB8888 SHM buffers. The assignment is
    /// double-buffered and becomes visible on the next surface commit.
    pub fn set_toplevel_icon(
        &self,
        surface: SurfaceId,
        icon: Option<ToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        let manager = self
            .state
            .toplevel_icon_manager
            .as_ref()
            .ok_or(RuntimeError::Unsupported("xdg-toplevel-icon-v1"))?;
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidToplevelIconTarget(surface))?;
        let applied = manager
            .set_icon(&self.queue_handle, &self.state.shm, toplevel, icon)
            .map_err(RuntimeError::Protocol)?;
        *shared
            .toplevel_icon
            .lock()
            .expect("toplevel icon mutex poisoned") = applied;
        Ok(())
    }

    /// Enable, update, or disable text input for a managed surface.
    ///
    /// The desired state is retained even while no seat focuses the surface.
    /// On a text-input-v3 `enter`, it is atomically resent with `enable`; later
    /// updates are committed without resetting the active input method.
    pub fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<TextInputState>,
    ) -> Result<(), RuntimeError> {
        if self.state.text_input_manager.is_none() {
            return Err(RuntimeError::Unsupported("zwp-text-input-v3"));
        }
        let shared = self.surface_shared(surface)?;
        {
            let mut desired = shared
                .text_input
                .lock()
                .expect("surface text input mutex poisoned");
            if *desired == state {
                return Ok(());
            }
            *desired = state.clone();
        }

        for text_input in self
            .state
            .seats
            .values_mut()
            .filter_map(|objects| objects.text_input.as_mut())
        {
            text_input.update(surface, state.as_ref());
        }
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
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        if validate_buffer_scale(
            factor,
            shared.fractional_scale.is_some(),
            wl_surface.version(),
        )? {
            wl_surface.set_buffer_scale(factor);
        }
        Ok(())
    }

    /// Set the surface-local destination size used by wp-viewporter.
    ///
    /// Fractional-scale clients should keep `wl_surface.buffer_scale` at one,
    /// render a buffer sized from the preferred scale, and set this destination
    /// to the unscaled logical surface size. The change takes effect on the
    /// next surface commit. `None` unsets the destination.
    pub fn set_viewport_destination(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        validate_viewport_destination(size)?;
        let shared = self.surface_shared(surface)?;
        let fractional_scale = shared
            .fractional_scale
            .as_ref()
            .ok_or(RuntimeError::Unsupported("wp-viewporter"))?;
        fractional_scale.set_destination(size);
        Ok(())
    }

    /// Set the surface-local compositor background blur request.
    ///
    /// ext-background-effect-v1 must advertise its dynamic blur capability.
    /// Effect state is double-buffered with `wl_surface`; call
    /// [`Runtime::commit`] (or commit the renderer's next buffer) to make the
    /// change visible.
    pub fn set_blur(&self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        let mut current = shared.blur.lock().expect("blur state mutex poisoned");

        match state {
            BlurState::Disabled => {
                current.take();
            }
            BlurState::Enabled(region) => {
                if !self.capabilities().ext_background_effect {
                    return Err(RuntimeError::Unsupported("ext-background-effect-v1 blur"));
                }
                if current.is_none() {
                    *current = Some(ManagedBlur(
                        self.state
                            .background_effect_state
                            .get_background_effect(wl_surface, &self.queue_handle)?,
                    ));
                }

                let blur_region = Region::new(&self.state.compositor)?;
                match region {
                    BlurRegion::EntireSurface => {
                        // NULL explicitly disables blur in this protocol, so
                        // use an oversized region clipped by the compositor.
                        blur_region.add(0, 0, i32::MAX, i32::MAX);
                    }
                    BlurRegion::Rectangles(rectangles) => {
                        for rectangle in rectangles.into_iter().filter(|rect| !rect.is_empty()) {
                            blur_region.add(
                                rectangle.origin.x,
                                rectangle.origin.y,
                                u32_to_i32(rectangle.size.width),
                                u32_to_i32(rectangle.size.height),
                            );
                        }
                    }
                }
                current
                    .as_ref()
                    .expect("blur was initialized")
                    .0
                    .set_blur_region(Some(blur_region.wl_region()));
            }
        }
        Ok(())
    }

    /// Retain the pointer constraint and relative-motion policy for a surface.
    ///
    /// A constraint is created only for a seat whose pointer currently focuses
    /// the surface. It is destroyed on leave and recreated from this retained
    /// state on a later enter, preventing conflicting constraint objects from
    /// accumulating on the same pointer. Equal states are ignored.
    pub fn set_pointer_capture_state(
        &mut self,
        surface: SurfaceId,
        state: PointerCaptureState,
    ) -> Result<(), RuntimeError> {
        validate_pointer_capture_state(&state)?;
        if state.constraint != PointerConstraint::None
            && !self.state.pointer_protocols.has_constraints()
        {
            return Err(RuntimeError::Unsupported("zwp-pointer-constraints-v1"));
        }
        if state.relative_motion && !self.state.pointer_protocols.has_relative_pointer() {
            return Err(RuntimeError::Unsupported("zwp-relative-pointer-v1"));
        }
        let shared = self.surface_shared(surface)?;
        if *shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            == state
        {
            return Ok(());
        }
        let region = if state.constraint == PointerConstraint::None {
            None
        } else {
            make_pointer_constraint_region(&self.state.compositor, &state.region)?
        };

        for objects in self.state.seats.values_mut() {
            let Some(pointer) = objects.pointer.as_ref() else {
                continue;
            };
            objects.pointer_session.sync_capture(
                PointerCaptureTarget::new(
                    surface,
                    shared.wl_surface(),
                    pointer.pointer(),
                    region.as_ref().map(Region::wl_region),
                ),
                &state,
                &self.state.pointer_protocols,
                &self.queue_handle,
            )?;
        }
        *shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned") = state;
        Ok(())
    }

    /// Change only the constraint part of a surface's retained pointer state.
    pub fn set_pointer_constraint(
        &mut self,
        surface: SurfaceId,
        constraint: PointerConstraint,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.constraint = constraint;
        self.set_pointer_capture_state(surface, state)
    }

    /// Subscribe or unsubscribe one focused surface from relative motion.
    ///
    /// Relative events are otherwise suppressed to avoid doubling the normal
    /// pointer event stream. A locked pointer always receives them.
    pub fn set_relative_pointer_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.relative_motion = enabled;
        self.set_pointer_capture_state(surface, state)
    }

    /// Subscribe or unsubscribe a surface from semantic touchpad gestures.
    ///
    /// Gesture protocol objects are created lazily for live pointer seats when
    /// the first surface subscribes and destroyed when the final subscription
    /// disappears. This keeps applications that do not consume gestures at
    /// zero per-seat protocol and event overhead.
    ///
    /// Disabling a surface immediately drops any in-progress route for that
    /// surface and does not synthesize an `End` event; the caller initiating
    /// the unsubscribe should clear its corresponding UI state.
    pub fn set_pointer_gestures_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        self.surface_shared(surface)?;
        if enabled && self.state.pointer_gesture_manager.is_none() {
            return Err(RuntimeError::Unsupported("zwp-pointer-gestures-v1"));
        }
        let change = self
            .state
            .pointer_gesture_subscriptions
            .set(surface, enabled);
        if !enabled && change != GestureSubscriptionChange::Unchanged {
            self.state.clear_pointer_gesture_surface(surface);
        }
        self.state
            .apply_pointer_gesture_subscription_change(change, &self.queue_handle);
        Ok(())
    }

    /// Whether a surface currently subscribes to pointer gesture events.
    pub fn pointer_gestures_enabled(&self, surface: SurfaceId) -> Result<bool, RuntimeError> {
        self.surface_shared(surface)?;
        Ok(self.state.pointer_gesture_subscriptions.contains(surface))
    }

    /// Change only the activation region of a surface's retained constraint.
    ///
    /// Region updates on an existing constraint are double-buffered with the
    /// target `wl_surface`; call [`Runtime::commit`] to apply the change.
    pub fn set_pointer_constraint_region(
        &mut self,
        surface: SurfaceId,
        region: PointerConstraintRegion,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.region = region;
        self.set_pointer_capture_state(surface, state)
    }

    /// Set the restoration hint used when a compositor releases a locked pointer.
    ///
    /// This does not warp the pointer. The request is double-buffered with the
    /// target `wl_surface`; call [`Runtime::commit`] to apply it.
    pub fn set_locked_pointer_position_hint(
        &self,
        surface: SurfaceId,
        position: (f64, f64),
    ) -> Result<(), RuntimeError> {
        if !position.0.is_finite() || !position.1.is_finite() {
            return Err(RuntimeError::Protocol(
                "locked pointer position hint must be finite".to_string(),
            ));
        }
        let shared = self.surface_shared(surface)?;
        if shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .constraint
            != PointerConstraint::Locked
        {
            return Err(RuntimeError::PointerNotLocked(surface));
        }
        for objects in self.state.seats.values() {
            objects
                .pointer_session
                .set_locked_position_hint(surface, position);
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
        let fractional_scale = self
            .state
            .fractional_scale_manager
            .as_ref()
            .map(|manager| manager.create_surface(protocol.wl_surface(), &self.queue_handle));
        let shared = Arc::new(SurfaceShared {
            blur: Default::default(),
            fractional_scale,
            pointer_capture: Default::default(),
            text_input: Default::default(),
            toplevel_icon: Default::default(),
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

    fn activation_manager(&self) -> Result<&ActivationManager, RuntimeError> {
        self.state
            .xdg_activation
            .as_ref()
            .ok_or(RuntimeError::Unsupported("xdg-activation-v1"))
    }

    fn validate_activation_serial(&self, serial: &InputSerial) -> Result<(), RuntimeError> {
        let same_connection =
            serial.seat.backend().upgrade().as_ref() == Some(&self.connection.backend());
        if same_connection {
            Ok(())
        } else {
            Err(RuntimeError::ForeignActivationSerial)
        }
    }

    fn parent_toplevel(&self, parent: SurfaceId) -> Result<Arc<SurfaceShared>, RuntimeError> {
        let shared = self.surface_shared(parent)?;
        if shared.protocol.xdg_toplevel().is_none() {
            return Err(RuntimeError::InvalidParent(parent));
        }
        Ok(shared)
    }

    fn request_toplevel_interaction(
        &self,
        surface: SurfaceId,
        interaction: ToplevelInteraction,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidToplevelInteractionTarget(surface))?;
        let candidates = self.state.seats.values().filter_map(|objects| {
            let pointer = objects.pointer.as_ref()?.pointer();
            let seat = pointer.data::<PointerData<()>>()?.seat().clone();
            Some((
                seat,
                objects.pointer_session.focus(),
                true,
                objects.pointer_presses.latest_for_surface(surface),
            ))
        });
        let (seat, press) = select_active_pointer_press(surface, candidates)
            .ok_or(RuntimeError::InvalidToplevelInteractionSerial)?;
        interaction.send(toplevel, &seat, press.serial);
        Ok(())
    }

    fn resolve_output(&self, id: OutputId) -> Result<wl_output::WlOutput, RuntimeError> {
        self.state
            .output_state
            .outputs()
            .find(|output| {
                self.state
                    .output_state
                    .info(output)
                    .is_some_and(|info| info.id == id.get())
            })
            .ok_or(RuntimeError::OutputNotFound(id))
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

include!("runtime_data_transfer.rs");
include!("runtime_helpers.rs");

fn supports_ext_background_blur(capabilities: Option<BackgroundEffectCapability>) -> bool {
    capabilities.is_some_and(|value| value.contains(BackgroundEffectCapability::Blur))
}

/// Validate a buffer-scale update and report whether a wire request is needed.
///
/// wl_surface v1/v2 have an implicit, immutable scale of one. Treating one as
/// a no-op keeps those compositors usable without sending the v3 request that
/// would otherwise terminate the connection.
fn validate_buffer_scale(
    factor: i32,
    fractional_scale: bool,
    surface_version: u32,
) -> Result<bool, RuntimeError> {
    if factor < 1 {
        return Err(RuntimeError::Protocol(
            "buffer scale must be at least one".to_string(),
        ));
    }
    if fractional_scale && factor != 1 {
        return Err(RuntimeError::Protocol(
            "buffer scale must remain one while fractional scaling is active".to_string(),
        ));
    }
    if surface_version < 3 {
        if factor == 1 {
            return Ok(false);
        }
        return Err(RuntimeError::Unsupported(
            "integer buffer scaling on wl_surface versions below 3",
        ));
    }
    Ok(true)
}

fn validate_viewport_destination(size: Option<LogicalSize>) -> Result<(), RuntimeError> {
    if size.is_some_and(LogicalSize::is_empty) {
        return Err(RuntimeError::Protocol(
            "viewport destination must have non-zero dimensions".to_string(),
        ));
    }
    Ok(())
}

fn make_pointer_constraint_region(
    compositor: &CompositorState,
    region: &PointerConstraintRegion,
) -> Result<Option<Region>, RuntimeError> {
    let PointerConstraintRegion::Rectangles(rectangles) = region else {
        return Ok(None);
    };
    let wire_region = Region::new(compositor)?;
    for rectangle in rectangles {
        wire_region.add(
            rectangle.origin.x,
            rectangle.origin.y,
            rectangle.size.width as i32,
            rectangle.size.height as i32,
        );
    }
    Ok(Some(wire_region))
}

fn validate_activation_target(surface: SurfaceId, kind: SurfaceKind) -> Result<(), RuntimeError> {
    match kind {
        SurfaceKind::Toplevel | SurfaceKind::Dialog => Ok(()),
        SurfaceKind::Popup | SurfaceKind::Layer => {
            Err(RuntimeError::InvalidActivationTarget(surface))
        }
    }
}

fn take_activation_request_id(next: &mut u64) -> ActivationRequestId {
    let request = ActivationRequestId(*next);
    *next = next.wrapping_add(1).max(1);
    request
}

fn begin_attention_request(pending: &mut HashSet<SurfaceId>, surface: SurfaceId) -> bool {
    pending.insert(surface)
}

struct RuntimeState {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    background_effect_state: BackgroundEffectState,
    data_device_manager: DataDeviceManagerState,
    compositor: CompositorState,
    shm: Shm,
    xdg_shell: XdgShell,
    xdg_activation: Option<ActivationManager>,
    toplevel_icon_manager: Option<ToplevelIconManager>,
    layer_shell_manager: Option<LayerShellManager>,
    text_input_manager: Option<TextInputManager>,
    fractional_scale_manager: Option<FractionalScaleManager>,
    pointer_gesture_manager: Option<PointerGestureManager>,
    pointer_protocols: PointerProtocols,
    pointer_gesture_subscriptions: PointerGestureSubscriptions,
    surfaces: HashMap<SurfaceId, Arc<SurfaceShared>>,
    surface_ids: HashMap<ObjectId, SurfaceId>,
    children: HashMap<SurfaceId, Vec<SurfaceId>>,
    seats: HashMap<u32, SeatObjects>,
    keyboard_focus: HashMap<u32, SurfaceId>,
    incoming_dnd: HashMap<DndOfferId, IncomingDndOffer>,
    active_dnd_by_device: HashMap<ObjectId, DndOfferId>,
    outgoing_dnd: HashMap<ObjectId, OutgoingDndSource>,
    selection_sources: HashMap<ObjectId, SelectionSource>,
    pending_attention: HashSet<SurfaceId>,
    events: EventBuffer,
    next_surface_id: u64,
    next_dnd_id: u64,
    next_input_order: u64,
    next_activation_request_id: u64,
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        // Protocol leaves must disappear before the resources they reference:
        // data sources/offers before data devices, pointer constraints and text
        // inputs before their seats, and seat-scoped objects before surfaces.
        self.outgoing_dnd.clear();
        self.incoming_dnd.clear();
        self.selection_sources.clear();
        self.seats.clear();
        self.surfaces.clear();
    }
}

struct IncomingDndOffer {
    id: DndOfferId,
    offer: DragOffer,
    surface: SurfaceId,
}

struct OutgoingDndSource {
    id: DndSourceId,
    _source: DragSource,
    content: TransferContent,
    selected_action: Option<DndAction>,
    _icon: Option<DndIconSurface>,
}

struct SelectionSource {
    _source: CopyPasteSource,
    content: TransferContent,
}

struct DndIconSurface {
    surface: wl_surface::WlSurface,
    _buffer: ShmBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionSerial {
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
        let gesture_change = self.pointer_gesture_subscriptions.remove_surface(id);
        self.surface_ids.remove(&shared.wl_surface().id());
        self.pending_attention.remove(&id);
        self.children.remove(&id);
        if let Some(parent) = shared.parent.as_ref()
            && let Some(children) = self.children.get_mut(&parent.id)
        {
            children.retain(|child| *child != id);
        }
        self.keyboard_focus.retain(|_, focused| *focused != id);
        for objects in self.seats.values_mut() {
            objects.pointer_session.remove_surface(id);
            match gesture_change {
                GestureSubscriptionChange::DetachSeats => {
                    objects.pointer_gestures.take();
                }
                GestureSubscriptionChange::Unchanged | GestureSubscriptionChange::KeepSeats => {
                    if let Some(gestures) = objects.pointer_gestures.as_ref() {
                        gestures.remove_surface(id);
                    }
                }
                GestureSubscriptionChange::AttachSeats => {
                    unreachable!("removing a gesture subscription cannot activate it")
                }
            }
            objects.pointer_presses.remove_surface(id);
            if objects.keyboard_focus == Some(id) {
                objects.keyboard_focus = None;
            }
            if let Some(text_input) = objects.text_input.as_mut() {
                text_input.remove_surface(id);
            }
            objects.touch_points.remove_surface(id);
        }
    }

    fn clear_pointer_gesture_surface(&self, surface: SurfaceId) {
        for objects in self.seats.values() {
            if let Some(gestures) = objects.pointer_gestures.as_ref() {
                gestures.remove_surface(surface);
            }
        }
    }

    fn record_pointer_press(&mut self, seat_id: u32, surface: SurfaceId, button: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects
                .pointer_presses
                .press(button, surface, serial, order);
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn record_pointer_release(&mut self, seat_id: u32, button: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.pointer_presses.release(button);
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn record_selection_serial(&mut self, seat_id: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn take_input_order(&mut self) -> u64 {
        let order = self.next_input_order;
        self.next_input_order = self.next_input_order.saturating_add(1);
        order
    }

    fn apply_pointer_gesture_subscription_change(
        &mut self,
        change: GestureSubscriptionChange,
        queue_handle: &QueueHandle<Self>,
    ) {
        match change {
            GestureSubscriptionChange::Unchanged | GestureSubscriptionChange::KeepSeats => {}
            GestureSubscriptionChange::AttachSeats => {
                let Some(manager) = self.pointer_gesture_manager.as_ref() else {
                    return;
                };
                for objects in self.seats.values_mut() {
                    objects.ensure_pointer_gestures(manager, queue_handle);
                }
            }
            GestureSubscriptionChange::DetachSeats => {
                for objects in self.seats.values_mut() {
                    objects.pointer_gestures.take();
                }
            }
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
        self.record_selection_serial(data.seat().id().protocol_id(), serial);
        let serial = InputSerial::new(data.seat().clone(), serial, InputSerialSource::KeyboardKey);
        self.events.push(Event::Keyboard(KeyboardEvent::Key {
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

fn is_current_popup_grab(objects: &SeatObjects, source: InputSerialSource, serial: u32) -> bool {
    match source {
        InputSerialSource::PointerPress => objects.pointer_presses.contains_serial(serial),
        InputSerialSource::TouchDown => objects.touch_points.contains_serial(serial),
        _ => false,
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

#[derive(Default)]
struct SeatObjects {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    pointer_gestures: Option<SeatPointerGestures>,
    touch: Option<wl_touch::WlTouch>,
    pointer_session: SeatPointerSession,
    text_input: Option<SeatTextInput>,
    data_device: Option<DataDevice>,
    pointer_presses: PointerPressTracker,
    latest_selection_serial: Option<SelectionSerial>,
    keyboard_focus: Option<SurfaceId>,
    touch_points: TouchPoints,
}

impl SeatObjects {
    fn has_focus(&self) -> bool {
        self.pointer_session.focus().is_some() || self.keyboard_focus.is_some()
    }

    fn ensure_pointer_gestures(
        &mut self,
        manager: &PointerGestureManager,
        queue_handle: &QueueHandle<RuntimeState>,
    ) {
        if self.pointer_gestures.is_some() {
            return;
        }
        let Some(pointer) = self.pointer.as_ref().map(ThemedPointer::pointer) else {
            return;
        };
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        self.pointer_gestures =
            Some(manager.create_seat_gestures(pointer, data.seat(), queue_handle));
    }
}

impl Drop for SeatObjects {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take()
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        self.pointer_gestures.take();
        self.pointer_session.detach();
        self.pointer.take();
        if let Some(touch) = self.touch.take()
            && touch.version() >= 3
        {
            touch.release();
        }
        self.text_input.take();
    }
}

include!("runtime_handlers.rs");

impl ActivationHandler for RuntimeState {
    fn activation_token_done(&mut self, purpose: ActivationTokenPurpose, token: String) {
        match purpose {
            ActivationTokenPurpose::Export { request, surface } => {
                self.events
                    .push(Event::Activation(ActivationEvent::TokenDone {
                        request,
                        requesting_surface: surface,
                        token: ActivationToken::from_raw(token),
                    }));
            }
            ActivationTokenPurpose::Attention { surface } => {
                self.pending_attention.remove(&surface);
                if let Some(shared) = self.surfaces.get(&surface)
                    && let Some(activation) = self.xdg_activation.as_ref()
                {
                    activation.activate(shared.wl_surface(), ActivationToken::from_raw(token));
                }
            }
        }
    }
}

impl FractionalScaleHandler for RuntimeState {
    fn preferred_scale(&mut self, surface: &wl_surface::WlSurface, factor: f64) {
        if let Some(surface) = self.surface_id(surface) {
            self.events
                .push(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                    surface,
                    factor,
                }));
        }
    }
}

impl TouchHandler for RuntimeState {
    fn touch_frame_event(&mut self, seat: &wl_seat::WlSeat, event: wl_touch::Event) {
        self.dispatch_touch_event(seat, event);
    }

    fn touch_cancelled(&mut self, seat: &wl_seat::WlSeat) {
        self.touch_cancel(seat);
    }
}

impl PointerGestureHandler for RuntimeState {
    fn pointer_gesture_surface(&mut self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surface_id(surface)
            .filter(|surface| self.pointer_gesture_subscriptions.contains(*surface))
    }

    fn pointer_gesture_event(&mut self, event: PointerGestureEvent) {
        let input = event
            .serial()
            .map(|serial| (serial.seat.id().protocol_id(), serial.serial));
        if let Some((seat_id, serial)) = input {
            self.record_selection_serial(seat_id, serial);
        }
        self.events.push(Event::PointerGesture(event));
    }
}

impl PointerConstraintsHandler for RuntimeState {
    fn confined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        confined_pointer: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.confined_changed(confined_pointer, true)
        });
    }

    fn unconfined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        confined_pointer: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.confined_changed(confined_pointer, false)
        });
    }

    fn locked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        locked_pointer: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.locked_changed(locked_pointer, true)
        });
    }

    fn unlocked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        locked_pointer: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.locked_changed(locked_pointer, false)
        });
    }
}

impl RelativePointerHandler for RuntimeState {
    fn relative_pointer_motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        relative_pointer: &ZwpRelativePointerV1,
        pointer: &wl_pointer::WlPointer,
        event: RelativeMotionEvent,
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(session) = self
            .seats
            .get(&seat_id)
            .map(|objects| &objects.pointer_session)
            .filter(|session| session.relative_matches(relative_pointer))
        else {
            return;
        };
        let Some(surface) = session.focus() else {
            return;
        };
        if !session.should_emit_relative() {
            return;
        }
        self.events
            .push(Event::RelativePointer(RelativePointerEvent {
                surface,
                time_micros: event.utime,
                delta: event.delta,
                delta_unaccelerated: event.delta_unaccel,
            }));
    }
}

impl RuntimeState {
    fn pointer_constraint_changed(
        &mut self,
        pointer: &wl_pointer::WlPointer,
        update: impl FnOnce(&mut SeatPointerSession) -> Option<crate::PointerConstraintEvent>,
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(event) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| update(&mut objects.pointer_session))
        else {
            return;
        };
        self.events.push(Event::PointerConstraint(event));
    }
}

impl TextInputHandler for RuntimeState {
    fn text_input_entered(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let desired = self.surfaces.get(&surface).and_then(|shared| {
            shared
                .text_input
                .lock()
                .expect("surface text input mutex poisoned")
                .clone()
        });
        let Some(session) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| objects.text_input.as_mut())
            .filter(|session| session.matches(text_input))
        else {
            return;
        };
        session.enter(surface, desired.as_ref());
        self.events
            .push(Event::TextInput(TextInputEvent::Entered { surface }));
    }

    fn text_input_left(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    ) {
        let surface = self.surface_id(surface);
        let Some(session) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| objects.text_input.as_mut())
            .filter(|session| session.matches(text_input))
        else {
            return;
        };
        session.leave();
        if let Some(surface) = surface {
            self.events
                .push(Event::TextInput(TextInputEvent::Left { surface }));
        }
    }

    fn text_input_done(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
        serial: u32,
        batch: PendingBatch,
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let enabled = self
            .seats
            .get(&seat_id)
            .and_then(|objects| objects.text_input.as_ref())
            .is_some_and(|session| session.accepts_done(text_input, surface));
        if enabled {
            self.events.push(Event::TextInput(TextInputEvent::Done(
                batch.into_done(surface, serial),
            )));
        }
    }
}

delegate_registry!(RuntimeState);
smithay_client_toolkit::delegate_dispatch2!(RuntimeState);

include!("runtime_tests.rs");
