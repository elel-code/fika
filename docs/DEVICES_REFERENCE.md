# Devices Reference

This document records the source references for Fika's device discovery,
mount-state model, and future Places sidebar integration.

## Dolphin Sources

- `../dolphin/src/dolphinplacesmodelsingleton.cpp`
  - Dolphin's Places model subclasses `KFilePlacesModel`.
  - Device entries, groups, icons, mount state, and hidden rows are provided by
    KDE Frameworks and Solid rather than by Dolphin's view code.
  - The model also advertises Ark drag-extract MIME types, but Ark drops are
    handled by the Places panel instead of ordinary place reordering.
- `../dolphin/src/panels/places/placespanel.cpp`
  - `PlacesPanel` is a `KFilePlacesView` bound to the singleton
    `DolphinPlacesModel`.
  - It connects `rowsInserted` and `rowsAboutToBeRemoved` to attach and detach
    Solid device signals as devices appear and disappear.
  - `deviceForIndex()` is used to find a Solid device for context menu and
    teardown actions.
  - `StorageAccess::teardownRequested` is forwarded to Dolphin's higher-level
    storage teardown flow so user-visible unmount/eject operations remain
    asynchronous.
  - Drag move rejects non-writable place URLs for external drags while still
    allowing internal place reordering.
- `../dolphin/src/statusbar/mountpointobserver.cpp`
  - Free-space display is queried from the active mount point through KIO and
    is decoupled from Places rendering.

## Cosmic Files Sources

- `../cosmic-files/src/mounter/mod.rs`
  - Defines a backend-neutral `Mounter` trait that exposes items, mount,
    unmount, network scan, and a subscription for mounter events.
  - UI code consumes mounter items instead of owning the discovery backend.
- `../cosmic-files/src/mounter/gvfs.rs`
  - Uses `gio::VolumeMonitor` to enumerate mounts and volumes.
  - Hides shadowed mounts, maps mounted roots to local paths when possible, and
    exposes mount names, icons, URI, remote flag, mounted state, and path.
  - Connects `mount_added`, `mount_removed`, `mount_changed`,
    `volume_added`, `volume_removed`, and `volume_changed` to a single changed
    event so the UI can rescan the model.
  - Mount/unmount operations are asynchronous and report results back through
    mounter events.

## Fika Mapping

- `src/core/devices.rs`
  - Defines `MountInfoEntry`, `DeviceInfo`, and `DeviceEvent`.
  - Parses `/proc/self/mountinfo` through `parse_mountinfo()` and
    `devices_from_mountinfo()`.
  - Decodes Linux mountinfo octal escapes such as `\040` before constructing
    paths.
  - Filters pseudo filesystems and loop/squashfs-style system images from the
    device list.
  - Emits a backend-neutral `DeviceInfo` with device path, mount point,
    filesystem type, best-effort label, optional capacity, and best-effort
    removable flag.
  - Marks mounts under `/media`, `/run/media`, `/mnt`, or `/Volumes` as
    removable until UDisks2 provides authoritative metadata.
  - Reads the initial UDisks2 ObjectManager snapshot from the shared system
    bus through `read_udisks2_snapshot_with_bus()`.
  - Parses `org.freedesktop.UDisks2.Block`, `.Drive`, and `.Filesystem`
    properties into `Udisks2Snapshot`, then merges block/drive metadata with
    mountinfo mount points.
  - Uses UDisks2 labels, filesystem type, size, removable, ejectable, and
    `CanPowerOff` metadata when present, while keeping mountinfo as a fallback
    for mounted devices.
  - Treats `Block.HintIgnore=true` as authoritative, so ignored UDisks2
    devices are not reintroduced by mountinfo fallback.
  - Provides `device_events_between()` to convert old/new `DeviceInfo`
    snapshots into deterministic Added/Removed/Changed events.
  - Adds `Udisks2MonitorState` and `Udisks2Signal` so core can apply
    ObjectManager `InterfacesAdded`/`InterfacesRemoved` and Properties
    `PropertiesChanged` payloads to the raw UDisks2 object map, rederive the
    snapshot, and emit deterministic `DeviceEvent` diffs without exposing
    D-Bus payload structure to Places UI code.
  - `watch_udisks2_devices()` subscribes to the system bus with a
    `path_namespace=/org/freedesktop/UDisks2` zbus `MessageStream`, converts
    ObjectManager and Properties signals with `udisks2_signal_from_message()`,
    re-reads mountinfo for each accepted signal, and publishes
    `DeviceMonitorMessage::{Snapshot,Events}` through a core channel boundary.
  - `Udisks2DeviceActionTarget` is resolved at operation time from the current
    UDisks2 ObjectManager snapshot plus mountinfo. Fika keeps the block object
    path for `Filesystem.Mount()` / `Filesystem.Unmount()` and the drive object
    path for `Drive.Eject()` / `Drive.PowerOff()` out of the UI layer, together
    with `Ejectable` and `CanPowerOff` capability flags.
  - `mount_udisks2_device()`, `unmount_udisks2_device()`, and
    `eject_udisks2_device()` call UDisks2 through the shared system bus with
    empty `a{sv}` options. `safely_remove_udisks2_device()` optionally unmounts
    the filesystem and then calls `Drive.PowerOff(a{sv})`. These APIs return
    structured errors for missing devices, unmounted filesystem targets, missing
    drive objects, and unsupported eject/power-off capabilities.
- `src/main.rs`
  - Places has static built-ins, persisted user bookmarks, a dynamic
    "Removable Devices" section for removable `DeviceInfo` rows, and a static
    Root entry under Devices.
  - Startup reads `/proc/self/mountinfo` and projects mounted removable devices
    into Places without writing them to `user-places.xbel`.
  - A low-frequency background refresh reads a fresh UDisks2 snapshot through
    `read_udisks2_devices()` and falls back to mountinfo if the system bus is
    unavailable. The result is applied through the same dynamic Places
    replacement path and never writes removable devices into `user-places.xbel`.
  - Startup also starts a live UDisks2 device monitor. The GPUI background loop
    drains `DeviceMonitorMessage` snapshots/events and applies them through
    `finish_device_refresh()` / `replace_removable_device_places()`. While the
    live monitor is active, the low-frequency snapshot refresh is paused; if the
    monitor cannot run, the snapshot refresh remains the fallback.
  - `replace_removable_device_places()` replaces only the dynamic removable
    device section, keeps user bookmarks before grouped sections, skips paths
    already covered by built-ins/user bookmarks, and gives device rows a drive
    icon instead of bookmark/folder styling.
  - Unmounted removable devices are preserved as grey device rows using their
    block device path as a non-navigable placeholder. Click runs the UDisks2
    mount action and navigates to the returned mount point; row-middle drop and
    Open/Open in New Pane/Open in New Window context actions remain disabled so
    `/dev/*` is never treated as a directory.
  - Device rows add Dolphin/Solid-style actions to the Places context menu:
    unmounted rows show Mount plus capability-gated Eject/Safely Remove, and
    mounted rows show Unmount plus capability-gated Eject/Safely Remove.
    Successful operations force a device snapshot refresh in addition to
    relying on the live UDisks2 monitor.
- `src/core/bus.rs`
  - UDisks2 ObjectManager and Properties signal streams use the shared
    system-bus controller.

## Remaining Work

- Verify mount/unmount/eject/power-off against real removable devices and
  Polkit prompts, including failures and user cancellation paths.
- Continue refining Solid parity for multi-partition drives and teardown flows
  such as "Safely Remove" when sibling filesystems are mounted.
