# COSMIC Files Reference

This file records local `./cosmic-files` reference points for follow-up Fika polish.

## Reference Policy

Fika should prefer `./cosmic-files` as the Rust-side implementation and visual design reference when the behavior is not tied to the current Dolphin-like main-pane layout. Dolphin remains the reference for the column-first icon arrangement, mature context-menu edge cases, and selection semantics that are already documented in `docs/DOLPHIN_REFERENCE.md`.

Alignment does not mean copying every implementation detail. If COSMIC's model produces a calmer UI or cleaner Rust boundary, Fika should move toward it; when Fika already has a stronger user-facing behavior, such as bounded directory LRU for instant revisit redraws or the current column-first virtual pane, keep the Fika behavior and document the reason.

Keep the current Fika main-pane item arrangement:

- Column-first icon layout.
- Horizontal main-pane scrolling.
- No vertical main-pane scrolling.
- Current virtualized Slint tile model.

Prefer COSMIC Files for the surrounding product feel and Rust implementation shape:

- Quieter toolbar/sidebar/status surfaces.
- Colors, spacing, layout rhythm, address bar placement, previous/next controls, and search placement/display outside the main file arrangement.
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
- Outside the main-pane item arrangement, Fika should treat COSMIC Files as the default visual reference for all UI chrome: color, spacing, toolbar layout, address entry position, previous/next controls, search entry position/display, status area, menu surfaces, and dialog/transient surface styling.
- The main-pane toolbar and main content should read as one shared layer: use the same calm base surface, keep only necessary divider lines, and avoid a separate toolbar color block.
- Search, split, and theme controls belong in the window-wide shell/header row. Back/Forward and the address entry belong in a separate `PathBar` at the top of the right main-pane content.
- The sidebar content panel and right main pane live in the same below-header content row and should be equal-height. The sidebar may keep Fika's rounded treatment on top of COSMIC's sidebar proportions, while the main pane and its internal toolbar stay visually flatter beside it. The sidebar panel border remains the visible divider, and hover highlighting is unnecessary when the cursor shape already communicates resize affordance.
- Treat COSMIC Files as copyable for all chrome outside the main file arrangement: palette, shell spacing, address-bar position, Back/Forward button treatment, search-field position/display, menu surfaces, dialogs, and sidebar rhythm can all follow COSMIC directly. The explicit Fika deviations are the main pane's column-first item arrangement and the rounded raised sidebar content panel.
- The desired layer model is stable: the window-wide shell/header row owns global tools and search. Below it, the sidebar content panel and right main pane share one equal-height content row; the right main pane starts with `PathBar`, then the search filter strip, grid, and status bar.
- After the current structural work is stable, every non-main-pane UI detail should be audited against COSMIC Files directly: palette, control radii, toolbar rhythm, address bar placement, Back/Forward affordances, search field position/display, menu layout, and sidebar prominence. The deliberate exception is the main pane's current column-first file arrangement and horizontal scroll model.
- Future visual passes should treat COSMIC Files as the source of truth for all chrome outside the main file arrangement: color tokens, main-toolbar/content layer relationship, address-bar alignment, Back/Forward/search placement, menu/dialog styling, and status/toolbar rhythm. Fika may keep a rounded raised sidebar content panel, but should not preserve Dolphin-like chrome by inertia.
- Context menus should keep practical action grouping, but visual weight can move closer to COSMIC Files than Dolphin when the interaction rules are already covered by tests.
- Fika's first visual pass keeps Dolphin-like pane layout, but shifts the surface colors toward COSMIC's quieter feel: off-white light backgrounds, lower-contrast separators, softer hover fills, 7-8px radii for controls/menus, and consistent selected/drop feedback between the sidebar and main pane.
- Current shell direction: the window-wide shell/header row owns search and global tools, while navigation/address controls stay in the right main pane's `PathBar`. The shell/header and main content intentionally do not draw a horizontal divider between them. Below the shell/header, the path bar and file area share one calm base surface while the sidebar is treated as a raised rounded panel in the same content row; its right border is the visible divider, and the resize hit area is transparent without adding layout width.
- Search follows COSMIC's header pattern more closely: the search button becomes a responsive top-bar search field when active, while the main-pane strip only carries recursive/filter actions.
- Current chrome pass: `AppWindow` owns shared base/sidebar/separator color tokens; `TopBar`, `PathBar`, `SearchPanel`, and `StatusBar` stay transparent. `TopBar` deliberately has no bottom separator, while PathBar/SearchPanel/StatusBar keep only their necessary internal separator lines and path/search fields use quiet white/dark input surfaces so the light theme remains readable.
- Current chrome pass: the default sidebar width is 280px, matching COSMIC's narrower navigation rhythm while still allowing user-resized persisted widths.
- Current chrome pass: the active top-bar search field uses bounded min/preferred/max layout constraints instead of a hard fixed width binding, so opening search no longer changes main-pane geometry or creates Slint layout recursion.
- Current chrome pass: Back/Forward and the path entry live in `PathBar` as the first row inside the right main pane below the shell/header. Search, split, and theme controls live in the global `TopBar`; the visible Up/Home button was removed from chrome.
- Current chrome pass: the light shell base is slightly calmer than the raised white sidebar, the sidebar border is a little stronger than flat shell separators, and sidebar rows are inset inside the rounded panel instead of touching the panel edge.
- Current chrome pass: sidebar content geometry uses a same-row panel with a 16px radius below the shell/header, while future COSMIC-style sidebar polish continues.
- Current chrome pass: shared controls now use quieter hover/selected fills, softer 8px radii, and consistent separator colors across the top bar, search strip, status bar, and sidebar rows. Places drag geometry stays unchanged while the row visuals move closer to COSMIC's calmer navigation style.
- Current chrome pass: the header follows COSMIC's compact icon-button rhythm more closely: 32px buttons use a lighter 13px label weight, path/search fields keep the same quiet input surface, and the active search field remains bounded so it cannot squeeze the main pane.

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
- `cosmic-files/src/operation/mod.rs`
- `cosmic-files/src/trash.rs`
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
- COSMIC Files does not appear to use an explicit LRU cache for recently visited directory item lists: `Tab::items_opt` represents the current tab's loaded items, and `change_location()` clears it until `TabRescan` applies fresh items. Fika keeps its own bounded directory-entry LRU for instant back/forward/revisit redraws, and may prefetch Places into that cache as a Fika-specific adaptation.
- Use COSMIC's mounter abstraction as a reference for device rows and network mounts, while keeping the current UDisks2 system-bus path for local removable devices.
- Move thumbnail support closer to the freedesktop thumbnail cache model used by COSMIC, including failure markers and thumbnailer desktop entries.
- Keep thumbnail scheduling as a separate concern from Slint model slicing and directory loading. Viewport sync should hand the thumbnail pipeline a prioritized virtual slice and let the pipeline own visibility-first ordering, duplicate suppression, and bounded job dispatch.
- Keep mouse side-button and pointer-scope logic close to COSMIC's mouse-area model, while preserving Fika's main-pane-only navigation rule.
