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
| Popups | Exposes the complete xdg-positioner anchor, gravity, constraint, offset, reactive and reposition state; accepts only opaque press/down serials for grabs |
| Lifetimes | Owns a surface tree, removes descendants child-first, and makes every renderer lease retain its ancestors |
| Rendering | `SurfaceHandle` implements raw-window-handle 0.6 for both wgpu and direct Vulkan use |
| Blur | Uses `ext-background-effect-v1` and preserves complete-surface or arbitrary surface-local rectangle regions |
| Data transfer | Clipboard and DnD share one MIME-content model, runtime connection, seat/serial state, data devices and pipe I/O; application-specific formats stay in the application |
| Drag and drop | Handles incoming/outgoing offers, action negotiation and lifecycle events; optional RGBA previews use owned SHM drag-icon surfaces |
| Input | Translates keyboard and pointer events into crate-owned values; uses cursor-shape when available and automatically falls back to the system cursor theme |

Touch event types are reserved in the API, while touch dispatch will be enabled
once its SCTK 0.21 dispatch integration is finalized.

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

## Protocol lifetime rule

`Runtime::destroy_surface` removes a complete subtree in post-order. A
renderer-held `SurfaceHandle` can extend a role's lifetime; each child lease
retains a strong parent lease, so an externally retained nested popup still
cannot outlive its protocol parent. Drop renderer surfaces before requesting
final teardown when immediate destruction is required.

## License

Licensed under either Apache-2.0 or MIT, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.
