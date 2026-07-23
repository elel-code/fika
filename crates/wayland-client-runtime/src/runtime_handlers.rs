impl ProvidesRegistryState for RuntimeState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers!(OutputState, SeatState);
}

impl OutputHandler for RuntimeState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl BackgroundEffectHandler for RuntimeState {
    fn background_effect_state(&mut self) -> &mut BackgroundEffectState {
        &mut self.background_effect_state
    }

    fn update_capabilities(&mut self) {}
}

impl CompositorHandler for RuntimeState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if let Some(surface) = self.surface_id(surface) {
            self.events
                .push_back(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                    surface,
                    factor,
                }));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        if let Some(surface) = self.surface_id(surface) {
            self.events
                .push_back(Event::Surface(SurfaceEvent::Frame { surface, time }));
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

fn toplevel_state(configure: &WindowConfigure) -> ToplevelState {
    let mut state = ToplevelState::empty();
    state.set(ToplevelState::MAXIMIZED, configure.is_maximized());
    state.set(ToplevelState::FULLSCREEN, configure.is_fullscreen());
    state.set(ToplevelState::RESIZING, configure.is_resizing());
    state.set(ToplevelState::ACTIVATED, configure.is_activated());
    state.set(ToplevelState::TILED_LEFT, configure.is_tiled_left());
    state.set(ToplevelState::TILED_RIGHT, configure.is_tiled_right());
    state.set(ToplevelState::TILED_TOP, configure.is_tiled_top());
    state.set(ToplevelState::TILED_BOTTOM, configure.is_tiled_bottom());
    state.set(
        ToplevelState::SUSPENDED,
        configure
            .state
            .contains(smithay_client_toolkit::reexports::csd_frame::WindowState::SUSPENDED),
    );
    state
}

fn push_toplevel_configure(
    state: &mut RuntimeState,
    surface: &wl_surface::WlSurface,
    configure: WindowConfigure,
    serial: u32,
) {
    let Some(surface) = state.surface_id(surface) else {
        return;
    };
    let suggested_size = SuggestedSize::new(
        configure.new_size.0.map(|value| value.get()),
        configure.new_size.1.map(|value| value.get()),
    );
    state
        .events
        .push_back(Event::Surface(SurfaceEvent::Configure {
            surface,
            suggested_size,
            state: toplevel_state(&configure),
            serial,
        }));
}

impl WindowHandler for RuntimeState {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(surface) = self.surface_id(window.wl_surface()) {
            self.events
                .push_back(Event::Surface(SurfaceEvent::CloseRequested { surface }));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        push_toplevel_configure(self, window.wl_surface(), configure, serial);
    }
}

impl DialogHandler for RuntimeState {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, dialog: &Dialog) {
        if let Some(surface) = self.surface_id(dialog.wl_surface()) {
            self.events
                .push_back(Event::Surface(SurfaceEvent::CloseRequested { surface }));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        dialog: &Dialog,
        configure: WindowConfigure,
        serial: u32,
    ) {
        push_toplevel_configure(self, dialog.wl_surface(), configure, serial);
    }
}

impl PopupHandler for RuntimeState {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        configure: PopupConfigure,
    ) {
        let Some(surface) = self.surface_id(popup.wl_surface()) else {
            return;
        };
        let kind = match configure.kind {
            ConfigureKind::Initial => PopupConfigureKind::Initial,
            ConfigureKind::Reactive => PopupConfigureKind::Reactive,
            ConfigureKind::Reposition { token } => PopupConfigureKind::Reposition { token },
            _ => PopupConfigureKind::Reactive,
        };
        self.events
            .push_back(Event::Surface(SurfaceEvent::PopupConfigure {
                surface,
                position: LogicalPosition::new(configure.position.0, configure.position.1),
                size: LogicalSize::new(
                    configure.width.max(0) as u32,
                    configure.height.max(0) as u32,
                ),
                serial: configure.serial,
                kind,
            }));
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if let Some(surface) = self.surface_id(popup.wl_surface()) {
            self.events
                .push_back(Event::Surface(SurfaceEvent::PopupDone { surface }));
        }
    }
}

impl SeatHandler for RuntimeState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.ensure_seat_data_device(qh, &seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let seat_id = seat.id().protocol_id();
        // SeatState binds seats already present in the initial registry without
        // invoking new_seat. Capability callbacks are therefore also an
        // initialization path for per-seat data devices.
        self.ensure_seat_data_device(qh, &seat);
        let objects = self.seats.entry(seat_id).or_default();
        match capability {
            Capability::Keyboard if objects.keyboard.is_none() => {
                objects.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            }
            Capability::Pointer if objects.pointer.is_none() => {
                let cursor_surface = self.compositor.create_surface(qh);
                objects.pointer = self
                    .seat_state
                    .get_pointer_with_theme::<RuntimeState, ()>(
                        qh,
                        &seat,
                        self.shm.wl_shm(),
                        cursor_surface,
                        ThemeSpec::System,
                    )
                    .ok();
            }
            Capability::Touch => {}
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
        let Some(objects) = self.seats.get_mut(&seat.id().protocol_id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                if let Some(keyboard) = objects.keyboard.take() {
                    self.keyboard_focus.remove(&keyboard.id().protocol_id());
                    objects.keyboard_focus = None;
                    if keyboard.version() >= 3 {
                        keyboard.release();
                    }
                }
            }
            Capability::Pointer => {
                objects.pointer.take();
                objects.pointer_focus = None;
                objects.latest_button_serial = None;
            }
            Capability::Touch => {}
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if let Some(objects) = self.seats.remove(&seat.id().protocol_id())
            && let Some(device) = objects.data_device.as_ref()
            && let Some(offer) = self.active_dnd_by_device.remove(&device.inner().id())
            && let Some(record) = self.incoming_dnd.remove(&offer)
        {
            record.offer.destroy();
        }
    }
}

impl RuntimeState {
    fn ensure_seat_data_device(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
    ) {
        let seat_id = seat.id().protocol_id();
        if self
            .seats
            .get(&seat_id)
            .is_none_or(|objects| objects.data_device.is_none())
        {
            let device = self.data_device_manager.get_data_device(qh, seat);
            self.seats.entry(seat_id).or_default().data_device = Some(device);
        }
    }
}

impl ShmHandler for RuntimeState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl PointerHandler for RuntimeState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[SctkPointerEvent],
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat = data.seat().clone();
        let seat_id = seat.id().protocol_id();
        for event in events {
            let Some(surface) = self.surface_id(&event.surface) else {
                continue;
            };
            let kind = match &event.kind {
                SctkPointerEventKind::Enter { serial } => {
                    self.record_selection_serial(seat_id, *serial);
                    if let Some(objects) = self.seats.get_mut(&seat_id) {
                        objects.pointer_focus = Some(surface);
                    }
                    PointerEventKind::Enter {
                        serial: InputSerial::new(
                            seat.clone(),
                            *serial,
                            InputSerialSource::PointerEnter,
                        ),
                    }
                }
                SctkPointerEventKind::Leave { .. } => {
                    if let Some(objects) = self.seats.get_mut(&seat_id)
                        && objects.pointer_focus == Some(surface)
                    {
                        objects.pointer_focus = None;
                    }
                    PointerEventKind::Leave
                }
                SctkPointerEventKind::Motion { time } => PointerEventKind::Motion { time: *time },
                SctkPointerEventKind::Press {
                    time,
                    button,
                    serial,
                } => {
                    self.record_button_serial(seat_id, surface, *serial);
                    PointerEventKind::Press {
                        time: *time,
                        button: *button,
                        serial: InputSerial::new(
                            seat.clone(),
                            *serial,
                            InputSerialSource::PointerPress,
                        ),
                    }
                }
                SctkPointerEventKind::Release {
                    time,
                    button,
                    serial,
                } => {
                    self.record_button_serial(seat_id, surface, *serial);
                    PointerEventKind::Release {
                        time: *time,
                        button: *button,
                        serial: InputSerial::new(
                            seat.clone(),
                            *serial,
                            InputSerialSource::PointerRelease,
                        ),
                    }
                }
                SctkPointerEventKind::Axis {
                    time,
                    horizontal,
                    vertical,
                    ..
                } => PointerEventKind::Axis {
                    time: *time,
                    horizontal: horizontal.absolute,
                    vertical: vertical.absolute,
                },
            };
            self.events.push_back(Event::Pointer(PointerEvent {
                surface,
                position: event.position,
                kind,
            }));
        }
    }
}

impl KeyboardHandler for RuntimeState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
        raw: &[u32],
        _: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        self.keyboard_focus
            .insert(keyboard.id().protocol_id(), surface);
        let seat_id = data.seat().id().protocol_id();
        self.record_selection_serial(seat_id, serial);
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.keyboard_focus = Some(surface);
        }
        self.events.push_back(Event::Keyboard(KeyboardEvent::Enter {
            surface,
            serial: InputSerial::new(
                data.seat().clone(),
                serial,
                InputSerialSource::KeyboardEnter,
            ),
            pressed_raw_codes: raw.to_vec(),
        }));
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let surface = self.surface_id(surface);
        self.keyboard_focus.remove(&keyboard.id().protocol_id());
        if let Some(data) = keyboard.data::<KeyboardData<Self, ()>>()
            && let Some(objects) = self.seats.get_mut(&data.seat().id().protocol_id())
            && objects.keyboard_focus == surface
        {
            objects.keyboard_focus = None;
        }
        if let Some(surface) = surface {
            self.events
                .push_back(Event::Keyboard(KeyboardEvent::Leave { surface }));
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Pressed, serial, event);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Repeated, serial, event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Released, serial, event);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        modifiers: SctkModifiers,
        _: RawModifiers,
        _: u32,
    ) {
        if let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() {
            self.record_selection_serial(data.seat().id().protocol_id(), serial);
        }
        let Some(surface) = self
            .keyboard_focus
            .get(&keyboard.id().protocol_id())
            .copied()
        else {
            return;
        };
        self.events
            .push_back(Event::Keyboard(KeyboardEvent::Modifiers {
                surface,
                modifiers: Modifiers {
                    ctrl: modifiers.ctrl,
                    alt: modifiers.alt,
                    shift: modifiers.shift,
                    caps_lock: modifiers.caps_lock,
                    logo: modifiers.logo,
                    num_lock: modifiers.num_lock,
                },
            }));
    }
}

impl RuntimeState {
    fn drag_offer_for_device(&self, device: &wl_data_device::WlDataDevice) -> Option<DragOffer> {
        self.seats
            .values()
            .filter_map(|objects| objects.data_device.as_ref())
            .find(|candidate| candidate.inner() == device)
            .and_then(|device| device.data().drag_offer())
    }

    fn write_data_source(
        &self,
        source: &wl_data_source::WlDataSource,
        mime: &str,
        pipe: WritePipe,
    ) {
        let Some(bytes) = self
            .outgoing_dnd
            .get(&source.id())
            .map(|record| &record.content)
            .or_else(|| {
                self.selection_sources
                    .get(&source.id())
                    .map(|record| &record.content)
            })
            .and_then(|content| content.bytes_for_mime(mime))
        else {
            return;
        };
        crate::data_transfer::spawn_write_pipe("wayland-transfer-write", pipe, bytes);
    }
}

impl DataDeviceHandler for RuntimeState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &wl_surface::WlSurface,
    ) {
        let Some(surface) = self.surface_id(wl_surface) else {
            return;
        };
        let Some(offer) = self.drag_offer_for_device(data_device) else {
            return;
        };
        let id = DndOfferId(self.next_dnd_id);
        self.next_dnd_id += 1;
        let mime_types = offer.with_mime_types(ToOwned::to_owned);
        let source_actions = dnd_actions(offer.source_actions);
        self.active_dnd_by_device.insert(data_device.id(), id);
        self.incoming_dnd.insert(
            id,
            IncomingDndOffer {
                id,
                offer,
                surface,
            },
        );
        self.events.push_back(Event::Dnd(DndEvent::Enter {
            offer: id,
            surface,
            position: LogicalPosition::new(x.round() as i32, y.round() as i32),
            mime_types,
            source_actions,
        }));
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(id) = self.active_dnd_by_device.remove(&data_device.id()) else {
            return;
        };
        let Some(record) = self.incoming_dnd.get(&id) else {
            return;
        };
        let surface = record.surface;
        self.events.push_back(Event::Dnd(DndEvent::Leave {
            offer: id,
            surface,
        }));
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
    ) {
        let Some(id) = self.active_dnd_by_device.get(&data_device.id()).copied() else {
            return;
        };
        let Some(record) = self.incoming_dnd.get(&id) else {
            return;
        };
        self.events.push_back(Event::Dnd(DndEvent::Motion {
            offer: id,
            surface: record.surface,
            position: LogicalPosition::new(x.round() as i32, y.round() as i32),
        }));
    }

    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
    ) {
    }

    fn drop_performed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(id) = self.active_dnd_by_device.get(&data_device.id()).copied() else {
            return;
        };
        let Some(current) = self.drag_offer_for_device(data_device) else {
            return;
        };
        let Some(record) = self.incoming_dnd.get_mut(&id) else {
            return;
        };
        record.offer = current;
        self.events.push_back(Event::Dnd(DndEvent::Drop {
            offer: id,
            surface: record.surface,
            action: dnd_action(record.offer.selected_action),
        }));
    }
}

impl DataOfferHandler for RuntimeState {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: WlDndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: WlDndAction,
    ) {
    }
}

impl DataSourceHandler for RuntimeState {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.write_data_source(source, &mime, pipe);
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if self.selection_sources.remove(&source.id()).is_some() {
            return;
        }
        if let Some(record) = self.outgoing_dnd.remove(&source.id()) {
            self.events
                .push_back(Event::Dnd(DndEvent::SourceCancelled { source: record.id }));
        }
    }

    fn dnd_dropped(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if let Some(record) = self.outgoing_dnd.get(&source.id()) {
            self.events.push_back(Event::Dnd(DndEvent::SourceDropped {
                source: record.id,
                action: record.selected_action,
            }));
        }
    }

    fn dnd_finished(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if let Some(record) = self.outgoing_dnd.remove(&source.id()) {
            self.events
                .push_back(Event::Dnd(DndEvent::SourceFinished {
                    source: record.id,
                    action: record.selected_action,
                }));
        }
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
        action: WlDndAction,
    ) {
        if let Some(record) = self.outgoing_dnd.get_mut(&source.id()) {
            record.selected_action = dnd_action(action);
        }
    }
}
