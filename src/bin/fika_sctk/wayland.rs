use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_xdg_shell, delegate_xdg_window,
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    reexports::protocols::wp::{
        fractional_scale::v1::client::{
            wp_fractional_scale_manager_v1::{self, WpFractionalScaleManagerV1},
            wp_fractional_scale_v1::{self, WpFractionalScaleV1},
        },
        viewporter::client::{
            wp_viewport::{self, WpViewport},
            wp_viewporter::{self, WpViewporter},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::window::{Window, WindowConfigure, WindowHandler},
    },
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};

use super::app::{FikaSctkApp, FractionalScaleData};

impl CompositorHandler for FikaSctkApp {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        self.set_legacy_scale_factor(new_factor);
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
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => eprintln!("[fika-sctk] keyboard-unavailable error={error}"),
            }
        }
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
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
            self.set_keyboard_focus(false);
        }
        if capability == Capability::Pointer && self.pointer.is_some() {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for FikaSctkApp {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[Keysym],
    ) {
        if self.window.wl_surface() == surface {
            self.set_keyboard_focus(true);
            eprintln!("[fika-sctk] keyboard-focus=1 pressed={keysyms:?}");
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.window.wl_surface() == surface {
            self.set_keyboard_focus(false);
            eprintln!("[fika-sctk] keyboard-focus=0");
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.press_key(event);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.repeat_key(event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.update_modifiers(modifiers);
    }
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
                    self.scroll_at(event.position.0, event.position.1, horizontal, vertical);
                }
                _ => {}
            }
        }
    }
}

delegate_compositor!(FikaSctkApp);
delegate_keyboard!(FikaSctkApp);
delegate_output!(FikaSctkApp);
delegate_pointer!(FikaSctkApp);
delegate_seat!(FikaSctkApp);
delegate_xdg_shell!(FikaSctkApp);
delegate_xdg_window!(FikaSctkApp);
delegate_registry!(FikaSctkApp);

impl Dispatch<WpViewporter, GlobalData> for FikaSctkApp {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: wp_viewporter::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, GlobalData> for FikaSctkApp {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: wp_viewport::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData> for FikaSctkApp {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, FractionalScaleData> for FikaSctkApp {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        data: &FractionalScaleData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if data.surface != *state.window.wl_surface() {
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.set_fractional_scale(scale as f32 / 120.0);
        }
    }
}

impl ProvidesRegistryState for FikaSctkApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}
