# Upstream references

This crate is an original Wayland-only implementation based on public Wayland
protocol specifications and SCTK APIs. The following upstream work was used to
understand required behavior and failure modes; source was not copied into this
dual-licensed crate.

## winit popup work

- PR: <https://github.com/rust-windowing/winit/pull/4543>
- Reviewed head: `f1bf67b0a80fa325acb3642c662e34200aea1f69`
- Behavior retained here: full xdg-positioner state, initial-map popup grabs,
  compositor dismissal, reposition tokens, nested popup child-first teardown,
  parent lifetime and raw-handle lifetime considerations.

The winit implementation is shaped by a cross-platform window trait. This
crate does not reproduce its `CoreWindow` structure or cross-platform API.

## winit dialog work

- PR: <https://github.com/rust-windowing/winit/pull/4627>
- Reviewed head: `1327e17ce0bed2b9acd11759c114e1171d2f34b2`
- Behavior retained here: a dialog is a parented xdg-toplevel; xdg-dialog-v1
  augments that role with compositor-visible modality.

The PR was draft and contained popup history/TODO paths, so it was treated as
a behavioral reference rather than an implementation source.

## winit drag-and-drop work

- PR: <https://github.com/rust-windowing/winit/pull/4571>
- Reviewed merge commit: `156433eb912a62a1a07da0f9bbaa2d775270c788`
- Behavior retained here: the Wayland layer owns button serial selection,
  resolves the data device from the pointer-focused origin seat, starts the
  protocol drag before committing the icon surface for KDE compatibility, and
  reports drop acceptance without releasing the source or icon before
  `dnd_finished`/cancellation.

The public API remains Wayland-native and payload-oriented; it does not copy
winit's cross-platform `DataTransfer` abstraction.

## SCTK dialog support

- PR: <https://github.com/Smithay/client-toolkit/pull/532>
- Released in: `smithay-client-toolkit 0.21.0`
- API used here: `shell::xdg::dialog::Dialog`, `XdgShell::create_dialog`,
  parent assignment and `Dialog::set_modal`.

## Blur protocol

The region-capable implementation uses the generated MIT-licensed bindings in
`wayland-protocols-plasma` for `org_kde_kwin_blur_manager` and
`org_kde_kwin_blur`. It preserves the protocol's nullable region, arbitrary
`wl_region`, update/commit and unset operations instead of reducing them to a
single whole-surface boolean.

## Core data-device and cursor behavior

DnD source/offer negotiation follows the `wl_data_device` protocol and SCTK
0.21 data-device APIs. Drag previews are independent `wl_surface` objects with
premultiplied ARGB8888 SHM buffers and remain owned until source completion or
cancellation.

Cursor selection uses SCTK's themed-pointer path. It prefers
`wp_cursor_shape_manager_v1` and falls back to the system XCursor theme without
changing the crate-owned cursor vocabulary.

Last reviewed: 2026-07-22.
