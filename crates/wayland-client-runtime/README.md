# wayland-client-runtime

`wayland-client-runtime` is a Wayland-only client protocol, surface and event
layer built on Smithay Client Toolkit (SCTK). It intentionally models Wayland
roles instead of imitating a cross-platform `Window` API.

The crate is currently developed in the Fika workspace. Its public API is
general-purpose and contains no Fika-specific model or renderer dependency.

## Implemented boundary

| Area | Public behavior |
| --- | --- |
| Connection/event loop | Owns the Wayland connection, event queue and calloop dispatcher; exposes an owned event queue and cross-thread wake handle |
| Toplevels | Creates xdg-toplevel surfaces and reports configure/close/frame/scale events |
| Dialogs | Creates parented xdg-dialog-v1 surfaces with modality; falls back to a parented xdg-toplevel when unsupported |
| Activation | Exports and consumes `xdg-activation-v1` tokens, carries optional app/seat/serial context, and supports coalesced user-attention requests |
| Toplevel icons | Uses `xdg-toplevel-icon-v1` with XDG theme names, one or more immutable SHM pixel fallbacks, compositor-preferred sizes, and explicit commit semantics |
| Popups | Exposes the complete xdg-positioner anchor, gravity, constraint, offset, reactive and reposition state; accepts only opaque press/down serials for grabs |
| Lifetimes | Owns a surface tree, removes descendants child-first, and makes every renderer lease retain its ancestors |
| Rendering | `SurfaceHandle` implements raw-window-handle 0.6 for both wgpu and direct Vulkan use |
| Scaling | Uses `wp-fractional-scale-v1` together with `wp-viewporter`, reports `f64` preferred scales, and retains integer output-scale fallback |
| Blur | Uses `ext-background-effect-v1` and preserves complete-surface or arbitrary surface-local rectangle regions |
| Data transfer | Clipboard and DnD share one MIME-content model, runtime connection, seat/serial state, data devices and pipe I/O; application-specific formats stay in the application |
| Drag and drop | Handles incoming/outgoing offers, action negotiation and lifecycle events; optional RGBA previews use owned SHM drag-icon surfaces |
| Input | Translates framed multi-touch, keyboard, and pointer events into crate-owned values; uses cursor-shape when available and automatically falls back to the system cursor theme |

## Basic use

```no_run
use std::time::Duration;
use wayland_client_runtime::{Runtime, RuntimeOptions, ToplevelAttributes};

let mut runtime = Runtime::connect(RuntimeOptions::default())?;
let surface = runtime.create_toplevel(ToplevelAttributes {
    title: "Wayland application".into(),
    app_id: "dev.example.Application".into(),
    ..Default::default()
})?;

let renderer_handle = runtime.surface_handle(surface).unwrap();
// Pass renderer_handle (usually in an Arc) to wgpu, or use its raw handles
// to create VK_KHR_wayland_surface objects.

loop {
    runtime.dispatch(Some(Duration::from_millis(16)))?;
    for event in runtime.drain_events() {
        // Update application state. Handler calls never re-enter application code.
        let _ = event;
    }
}
# Ok::<(), wayland_client_runtime::RuntimeError>(())
```

## Region blur

Unlike a whole-window boolean, the blur request preserves the protocol's
region support:

```no_run
use wayland_client_runtime::{BlurRegion, BlurState, LogicalRect};
# use wayland_client_runtime::{Runtime, SurfaceId};
# fn apply(runtime: &Runtime, surface: SurfaceId) -> Result<(), wayland_client_runtime::RuntimeError> {
runtime.set_blur(
    surface,
    BlurState::Enabled(BlurRegion::Rectangles(vec![
        LogicalRect::new(0, 0, 800, 56),
        LogicalRect::new(0, 56, 240, 544),
    ])),
)?;
# Ok(())
# }
```

Applications should inspect
`Runtime::capabilities().ext_background_effect`. The capability becomes true
only when the compositor advertises the protocol's dynamic `blur` bit;
compositors without it return `RuntimeError::Unsupported`.

ext-background-effect-v1 state is double-buffered with `wl_surface`. Call
`Runtime::commit` after changing blur, or let the next renderer buffer commit
apply it. This keeps blur updates inside the same explicit surface commit
boundary as geometry, scale, and renderer state.

## Drag icons and cursors

`Runtime::start_drag` derives the serial and data device from the pointer seat
focused on the origin surface; applications call it during the activating
pointer gesture and do not retain protocol serials. It accepts an optional
`DndIcon`, validates its buffer scale, converts straight RGBA into the
premultiplied native-endian ARGB8888 representation required by `wl_shm`, and
applies the logical hotspot offset. The icon is committed after `start_drag`
for KDE compatibility and remains owned until the source finishes or is
cancelled. `SourceDropped` reports acceptance immediately, while
`SourceFinished` marks the point where source and icon resources are released.

Clipboard selections use the same `TransferContent` and runtime-owned seat
serials as drag sources. `Runtime::store_selection` and
`Runtime::receive_selection` therefore require no second Wayland connection or
clipboard-only event thread. Incoming DnD offers remain valid until the queued
leave/drop event has been consumed and the application explicitly discards or
finishes them.

`Runtime::set_cursor` uses `wp_cursor_shape_manager_v1` when advertised. On
older compositors, SCTK loads the same semantic cursor from the configured
system XCursor theme; applications do not need a separate fallback path.

Touch dispatch preserves Wayland frame ordering and the Weston-compatible
last-up fallback. Down/up serials remain seat-scoped, active touch-down serials
can request popup grabs, and cancel/capability-removal events clear every
affected surface without leaving stale touch IDs behind.

## Fractional scaling

Fractional scaling is enabled only when both `wp-fractional-scale-v1` and
`wp-viewporter` are available. Each managed surface then owns one fractional
scale object and one viewport for its complete lifetime. Preferred scale
numerators are divided by the protocol denominator of 120 and emitted as
`SurfaceEvent::ScaleFactorChanged { factor: f64, .. }`; legacy integer output
scale events are ignored for those surfaces.

Render buffers at `round(logical_size * factor)`, keep
`wl_surface.buffer_scale` at one, and call
`Runtime::set_viewport_destination(surface, Some(logical_size))` before the
commit that attaches the resized buffer. The viewport destination is
double-buffered with the surface. Without the paired globals, use the integer
scale event and `Runtime::set_buffer_scale` instead.

## Surface activation

When `Runtime::capabilities().xdg_activation_v1` is true,
`Runtime::request_activation_token` starts an asynchronous token request. Its
`ActivationTokenAttributes` can carry the target app ID and the opaque
`InputSerial` from the input or focus event that initiated the request. The
result arrives as `Event::Activation(ActivationEvent::TokenDone { .. })` with
a stable `ActivationRequestId`, so multiple outstanding requests remain
distinguishable.

Forward the resulting `ActivationToken` to another process using
`XDG_ACTIVATION_TOKEN`, D-Bus platform data, or another IPC channel. A receiving
client wraps that string with `ActivationToken::from_raw` and calls
`Runtime::activate_surface`. `Runtime::request_user_attention` implements the
winit behavior directly by requesting a surface-associated token and applying
it back to the same toplevel; duplicate requests are coalesced until the
compositor completes the first one. Token protocol objects are destroyed after
their one `done` event.

## Toplevel icons

When `Runtime::capabilities().xdg_toplevel_icon_v1` is true,
`Runtime::set_toplevel_icon` assigns an icon to a toplevel or dialog. A
`ToplevelIcon` may contain an XDG icon-theme name, multiple square RGBA buffers,
or both. Supplying both lets the compositor resolve the current theme first and
use the pixel representations as fallbacks. `Runtime::preferred_toplevel_icon_sizes`
returns the sorted, deduplicated logical sizes announced by the compositor; an
empty list means it has no preference (or the protocol is unavailable).

Pixel representations carry their own integer scale and are validated before
any protocol request is sent. The runtime copies straight RGBA into immutable,
premultiplied native-endian ARGB8888 SHM storage and keeps that storage alive
for the applied icon. Replacing the icon releases the previous storage. Passing
`None` restores the compositor's default icon.

Icon assignment is double-buffered. Call `Runtime::commit` after
`set_toplevel_icon`, or let the next renderer commit apply it. The temporary
`xdg_toplevel_icon_v1` object becomes immutable after assignment and is
destroyed immediately; the compositor keeps the assigned icon until it is
explicitly replaced or cleared.

## Internal module boundary

Protocol-specific ownership and dispatch are kept in focused modules:
`activation`, `fractional_scale`, `toplevel_icon`, `touch`, and the shared SHM
pixel formatter. `Runtime` binds those modules and translates their callbacks
into crate-owned events, while `SurfaceShared` only retains per-surface protocol
resources. This keeps generated Wayland proxy details out of the public API and
prevents application policy from leaking into reusable protocol code.

## Protocol lifetime rule

`Runtime::destroy_surface` removes a complete subtree in post-order. A
renderer-held `SurfaceHandle` can extend a role's lifetime; each child lease
retains a strong parent lease, so an externally retained nested popup still
cannot outlive its protocol parent. Drop renderer surfaces before requesting
final teardown when immediate destruction is required.

## License

Licensed under either Apache-2.0 or MIT, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.
