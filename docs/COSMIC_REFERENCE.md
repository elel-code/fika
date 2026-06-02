# COSMIC Files Reference

This file records local `./cosmic-files` reference points for follow-up Fika polish.

## Reference Policy

Fika should prefer `./cosmic-files` as the Rust-side implementation and visual design reference when the behavior is not tied to the current Dolphin-like main-pane layout. Dolphin remains the reference for the column-first icon arrangement, mature context-menu edge cases, and selection semantics that are already documented in `docs/DOLPHIN_REFERENCE.md`.

Keep the current Fika main-pane item arrangement:

- Column-first icon layout.
- Horizontal main-pane scrolling.
- No vertical main-pane scrolling.
- Current virtualized Slint tile model.

Prefer COSMIC Files for the surrounding product feel and Rust implementation shape:

- Quieter toolbar/sidebar/status surfaces.
- Practical menu grouping and action enablement.
- Desktop app and terminal discovery.
- Clipboard import/export details.
- Operation controller/progress model.
- Device/mounter abstraction.
- Directory loading that keeps item refresh, view state, and thumbnail jobs as separate concerns.
- Thumbnail cache and external thumbnailer integration.
- Mouse-area event scoping.

## Visual Direction

Reference files:

- `cosmic-files/src/menu.rs`
- `cosmic-files/src/app.rs`
- `cosmic-files/src/dialog.rs`

Useful UI direction:

- Prefer quieter surfaces with clearer row spacing, softer separators, and less dense menu groups where it helps scanning.
- Keep primary panes visually simple: sidebar, toolbar, content, and footer should feel integrated instead of heavily framed.
- Context menus should keep practical action grouping, but visual weight can move closer to COSMIC Files than Dolphin when the interaction rules are already covered by tests.
- Fika's first visual pass keeps Dolphin-like pane layout, but shifts the surface colors toward COSMIC's quieter feel: off-white light backgrounds, lower-contrast separators, softer hover fills, 7-8px radii for controls/menus, and consistent selected/drop feedback between the sidebar and main pane.

Follow-up candidates:

- Reduce any remaining heavy pane framing while preserving clear resize and focus affordances.
- Make Places, Devices, dialogs, and context menus feel like one component family rather than separate KDE-like surfaces.
- Review hover/pressed/disabled states against COSMIC's practical density before adding more custom colors.

## Terminal Launch

Reference files:

- `cosmic-files/src/mime_app.rs`
- `cosmic-files/src/app.rs`

Useful terminal rules:

- Build a terminal candidate list from desktop application metadata, specifically `TerminalEmulator` categories.
- Prefer the default `x-scheme-handler/terminal` desktop handler when the desktop reports one.
- Keep a known terminal fallback path for environments without desktop metadata.

Current Fika mapping:

- `src/desktop/terminal.rs` keeps explicit `FIKA_TERMINAL` / `TERMINAL` overrides first.
- After explicit overrides, Fika queries `xdg-mime query default x-scheme-handler/terminal`, resolves the desktop file, and accepts it when it is a visible `TerminalEmulator`.
- Fika then prefers `com.system76.CosmicTerm.desktop`, scans visible `TerminalEmulator` desktop entries, and finally falls back to known terminal executable names.

## Rust Implementation References

Reference files:

- `cosmic-files/src/mime_app.rs`
- `cosmic-files/src/clipboard.rs`
- `cosmic-files/src/operation/controller.rs`
- `cosmic-files/src/operation/recursive.rs`
- `cosmic-files/src/operation/notifiers.rs`
- `cosmic-files/src/mounter/mod.rs`
- `cosmic-files/src/mounter/gvfs.rs`
- `cosmic-files/src/thumbnail_cacher.rs`
- `cosmic-files/src/thumbnailer.rs`
- `cosmic-files/src/mouse_area.rs`
- `cosmic-files/src/menu.rs`
- `cosmic-files/src/app.rs`

Useful implementation direction:

- Keep desktop app discovery and launch behavior close to COSMIC's `MimeAppCache`, while preserving Fika's no-large-XDG-library preference.
- Revisit clipboard handling against COSMIC's cached Wayland clipboard model, especially popup-time paste availability and pasted image/text/video-to-file workflows.
- Evolve Fika's operation queue toward COSMIC's controller/progress split: cancellable operations, clearer progress reporting, and less UI coupling.
- Keep directory reloads close to COSMIC's `Location::scan` / `Tab::set_items` split: refresh item lists without tearing down unrelated view and thumbnail state.
- Use COSMIC's mounter abstraction as a reference for device rows and network mounts, while keeping the current UDisks2 system-bus path for local removable devices.
- Move thumbnail support closer to the freedesktop thumbnail cache model used by COSMIC, including failure markers and thumbnailer desktop entries.
- Keep mouse side-button and pointer-scope logic close to COSMIC's mouse-area model, while preserving Fika's main-pane-only navigation rule.
