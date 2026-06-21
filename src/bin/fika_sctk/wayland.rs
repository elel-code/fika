use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::window::{Window, WindowConfigure, WindowHandler},
    },
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_output, wl_pointer, wl_seat, wl_surface},
};

use super::app::FikaSctkApp;

impl CompositorHandler for FikaSctkApp {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        eprintln!("[fika-sctk] scale-factor={new_factor}");
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for FikaSctkApp {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for FikaSctkApp {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.request_exit();
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        self.handle_configure(configure);
    }
}

impl SeatHandler for FikaSctkApp {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        eprintln!("[fika-sctk] seat-capability={capability:?}");
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => eprintln!("[fika-sctk] pointer-unavailable error={error}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        eprintln!("[fika-sctk] seat-capability-removed={capability:?}");
        if capability == Capability::Pointer && self.pointer.is_some() {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for FikaSctkApp {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            match event.kind {
                Enter { .. } | Motion { .. } => {
                    self.set_pointer(event.position.0, event.position.1);
                }
                Leave { .. } => self.clear_pointer(),
                Press { button, .. } if button == BTN_LEFT => {
                    self.press_primary(event.position.0, event.position.1);
                }
                Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let horizontal = if horizontal.absolute != 0.0 {
                        horizontal.absolute as f32
                    } else {
                        -(horizontal.value120 as f32 / 120.0) * super::metrics::SCROLL_LINE_PX
                    };
                    let vertical = if vertical.absolute != 0.0 {
                        vertical.absolute as f32
                    } else {
                        -(vertical.value120 as f32 / 120.0) * super::metrics::SCROLL_LINE_PX
                    };
                    self.scroll(horizontal, vertical);
                }
                _ => {}
            }
        }
    }
}

delegate_compositor!(FikaSctkApp);
delegate_output!(FikaSctkApp);
delegate_pointer!(FikaSctkApp);
delegate_seat!(FikaSctkApp);
delegate_xdg_shell!(FikaSctkApp);
delegate_xdg_window!(FikaSctkApp);
delegate_registry!(FikaSctkApp);

impl ProvidesRegistryState for FikaSctkApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}
