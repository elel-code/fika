# Bus Control Reference

This document records the D-Bus source references and the intended Fika
mapping for a shared bus-control layer. The goal is to stop spreading raw
`zbus::Connection::{session,system}` calls across launcher, portal, privileged
helper, devices, Ark DnD, and future FileManager1 integration.

## Dolphin Sources

- `../dolphin/src/main.cpp`
  - Dolphin creates a `KDBusService` for normal windows and daemon mode.
  - If the session bus is unavailable, normal startup keeps working by using
    `KDBusService::NoExitOnFailure`; D-Bus integration must not be a hard
    dependency for local directory browsing.
  - Normal startup first tries `Dolphin::attachToExistingInstance()` through
    session D-Bus unless `--new-window` was requested.
- `../dolphin/src/dbusinterface.{h,cpp}`
  - Registers `/org/freedesktop/FileManager1` on the session bus and requests
    `org.freedesktop.FileManager1` with queued ownership.
  - Implements `ShowFolders`, `ShowItems`, and `ShowItemProperties`.
  - Validates incoming URI strings before dispatch, then either attaches to an
    existing Dolphin instance or opens a new window.
  - Exposes `SortOrderForUrl` as a Dolphin-specific helper on the same object.
- `../dolphin/src/views/draganddrophelper.cpp`
  - Ark drag extraction reads
    `application/x-kde-ark-dndextract-service` and
    `application/x-kde-ark-dndextract-path`, then calls
    `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)` on the session
    bus instead of running an ordinary copy/move/link drop.
- `../dolphin/src/itemactions/hidefileitemaction.cpp` and
  `../dolphin/src/itemactions/setfoldericonitemaction.cpp`
  - After changing per-directory metadata, Dolphin emits KDirNotify
    `FilesChanged` on the session bus so other views refresh.
- `../dolphin/src/panels/information/informationpanel.cpp`
  - Subscribes to KDirNotify `FileRenamed`, `FilesAdded`, `FilesChanged`,
    `FilesRemoved`, `enteredDirectory`, and `leftDirectory` signals.
- `../dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp`
  - Subscribes to desktop setting changes through session D-Bus and updates
    animation behavior without coupling the scroller to connection creation.
- `../dolphin/src/panels/terminal/terminalpanel.cpp`
  - Uses session D-Bus proxies for KIOFuse integration; calls are asynchronous
    and report failures without blocking the UI.

## Cosmic Files Sources

- `../cosmic-files/cosmic-files-applet/src/main.rs`
  - Creates a blocking session bus service with zbus and owns
    `org.freedesktop.FileManager1`.
  - Serves `/org/freedesktop/FileManager1` through a small dedicated process.
- `../cosmic-files/cosmic-files-applet/src/file_manager.rs`
  - Implements the freedesktop FileManager1 `ShowFolders`, `ShowItems`, and
    `ShowItemProperties` methods.
  - Dispatches incoming URI requests into the main `cosmic-files` executable.
- `../cosmic-files/src/mounter/`
  - The mounter architecture is the relevant Rust-side reference for UDisks2
    discovery and action routing. Device discovery should use system bus
    ObjectManager/Properties interfaces, then project backend-neutral devices
    into the UI.

## Current Fika State

- `src/core/bus.rs`
  - Defines `BusKind::{Session,System}`, `BusCallTarget`, `BusConfig`,
    `BusController`, and structured `BusError`.
  - Lazily caches session and system `zbus::Connection` handles behind a shared
    controller, with a 30s default idle timeout.
  - Fika's direct `zbus` and `zbus_polkit` dependencies disable default
    features and do not request `zbus/tokio`, `zbus/async-io`, or
    `zbus/blocking-api` themselves. This is deliberate: zbus executor features
    are crate-global, and forcing `zbus/tokio` would also switch GPUI's
    transitive `ashpd`/accessibility zbus calls onto Tokio even though those
    futures are polled by GPUI/accesskit executors.
  - Cargo feature unification can still show `zbus/async-io` in `cargo tree`
    because GPUI's Linux portal/accessibility stack requests it. Fika does not
    add a direct `async-io` dependency; Fika-owned timeout/retry/sleep logic is
    still polled inside `with_bus_tokio_context()` so Tokio timers are only used
    at Fika's bus-control boundary.
  - Connection creation, generic proxy creation, and timeout/retry method calls
    all poll inside a Tokio runtime context when called from GPUI tasks, so
    shared D-Bus/systemd paths do not panic when the caller thread has no Tokio
    reactor.
  - `BusController::proxy()` returns an owned zbus proxy and keeps connection
    ownership/proxy creation out of launcher, Ark, and UDisks2 feature modules.
- `src/core/launcher.rs`
  - `launch_with_systemd_user()` now uses `BusController::shared()` for the
    session bus, builds an `org.freedesktop.systemd1.Manager` proxy, then calls
    `StartTransientUnit` through the shared timeout/retry helper.
  - This already gives Open With, service menu, Ark fallback, and Open in New
    Window actions the right process boundary while moving connection ownership
    and retry policy out of the launcher module.
- `src/core/privilege.rs`
  - Client helpers for privileged file operations and external-edit lifecycle
    now acquire system/session connections through `BusController::shared()`.
  - Client-side helper proxy calls and session-helper readiness polling also
    run inside the bus Tokio context, including `tokio::time::sleep` while
    waiting for a pkexec-started session helper to appear.
  - The helper service's external-edit unit watcher now uses async zbus proxies
    on a local Tokio runtime instead of `zbus::blocking`.
  - The installable helper service owns
    `org.fika.FileManager1.Privileged` at
    `/org/fika/FileManager1/Privileged`.
  - Polkit authorization and caller UID lookup intentionally use the service
    method's active zbus connection, because authorization must be tied to the
    caller message/header.
  - Blocking systemd unit state checks also create their own session bus
    connections.
- `src/bin/fika-xdp-filechooser.rs`
  - Owns `org.freedesktop.impl.portal.desktop.fika` on the session bus and
    serves `/org/freedesktop/portal/desktop`.
  - The portal backend has an isolated current-thread tokio runtime and does
    not reuse any core bus object.
- `src/core/archive.rs`
  - Parses Ark DnD service/path MIME payload into a validated service name and
    object path.
  - Builds `ArkDndExtractRequest` and executes
    `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)` through the
    shared session bus helper. GPUI/backend external MIME offer routing is still
    pending.
- `src/core/devices.rs`
  - Reads the initial UDisks2 object tree through the shared system bus helper:
    `org.freedesktop.DBus.ObjectManager.GetManagedObjects()` on
    `org.freedesktop.UDisks2`.
  - Converts UDisks2 Block/Drive/Filesystem properties into a core snapshot and
    merges it with `/proc/self/mountinfo`.
  - `Udisks2MonitorState` can apply ObjectManager `InterfacesAdded` /
    `InterfacesRemoved` and Properties `PropertiesChanged` payloads to the raw
    UDisks2 object map, then emit `DeviceEvent` diffs from the rederived
    snapshot.
  - `watch_udisks2_devices()` uses the shared system bus connection with a zbus
    `MessageStream` match on `/org/freedesktop/UDisks2` path namespace, converts
    ObjectManager/Properties signal messages into `Udisks2Signal`, and publishes
    `DeviceMonitorMessage` snapshots/events over a core channel boundary.
- `src/main.rs`
  - UI actions call launcher/privilege helpers but should not create or own
    D-Bus connections directly.

## Fika Target Mapping

The shared `src/core/bus.rs` layer should own the cross-feature D-Bus boundary:

- Define `BusKind::{Session,System}` and a `BusController` or similar core
  object that lazily creates zbus connections.
- Keep connection creation out of GPUI render/input paths. UI actions should
  call core operations that use the bus controller internally.
- Cache session and system connections while active, with an idle close policy
  aligned to the TODO target of roughly 30 seconds.
- Provide method-call helpers that attach:
  - destination service
  - object path
  - interface
  - method
  - timeout
  - retry policy
  - structured error context
- Convert D-Bus errors into a core `BusError` carrying bus kind, service,
  interface, method, and message.
- Route these existing call sites through the shared layer:
  - systemd user `StartTransientUnit` in `launcher.rs`
  - privileged-helper client calls in `privilege.rs`
  - privileged-helper service registration path where practical
  - portal backend service registration once the backend can share a core
    helper without entangling binaries
  - Ark DnD `extractSelectedFilesTo(destination)`
  - future FileManager1 session service
  - future UDisks2 system bus discovery/mount/eject calls
  - future KDirNotify emit/listen helpers
- Device discovery should subscribe to system bus
  `org.freedesktop.DBus.ObjectManager.InterfacesAdded`,
  `InterfacesRemoved`, and
  `org.freedesktop.DBus.Properties.PropertiesChanged` for UDisks2, then feed a
  core devices model rather than mutating Places UI state directly.
- FileManager1 should be a session bus service boundary separate from the GPUI
  window object. Incoming URIs should validate and normalize before routing to
  pane/window actions.

## Initial Implementation Slices

Completed:

- Added the core error and request types:
  `BusKind`, `BusCallTarget`, `BusError`, `BusConfig`, and `BusController`.
- Moved `launch_with_systemd_user()` to the shared session bus helper while
  preserving existing `SystemdLaunchResult` and tests.
- Added the core Ark DnD executor boundary:
  `ArkDndExtractRequest` validates the destination and maps the Ark service/path
  payload to `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)`, then
  executes it through the shared session bus helper.
- Moved privileged-helper client calls for file operations, external edit
  prepare/commit/discard/associate, and session helper readiness checks to the
  shared bus connection helper while preserving the helper service API.

Remaining:

1. Wire GPUI/backend external drag data offers carrying Ark MIME values into
   the core Ark DnD executor instead of the ordinary file copy/move/link path.
2. Move the remaining privileged-helper blocking user-unit watcher connection
   path where practical, or document it as a blocking service-side exception.
3. Add UDisks2 discovery on top of the system bus helper and keep device
   projection UI-neutral.
4. Add FileManager1 registration after the core router can safely dispatch
   incoming URI requests to an app/window action queue.

## Constraints

- Local file browsing must continue when the session bus is unavailable.
- System bus failures must degrade devices/privileged operations, not pane
  rendering.
- Long-running or blocking D-Bus operations must not run on the GPUI render
  path.
- Bus subscriptions must route results through stable pane/window/action
  identities. They must not retarget based on current focus.
- Each feature should keep its domain-specific public API. The bus layer is a
  transport/lifecycle boundary, not a place for file manager behavior.
