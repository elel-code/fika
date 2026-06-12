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
  - Uses UDisks2 labels, filesystem type, size, removable, and ejectable
    metadata when present, while keeping mountinfo as a fallback for mounted
    devices.
  - Treats `Block.HintIgnore=true` as authoritative, so ignored UDisks2
    devices are not reintroduced by mountinfo fallback.
  - Provides `device_events_between()` to convert old/new `DeviceInfo`
    snapshots into deterministic Added/Removed/Changed events.
- `src/main.rs`
  - Places has static built-ins, persisted user bookmarks, a dynamic
    "Removable Devices" section for mounted removable `DeviceInfo` rows, and a
    static Root entry under Devices.
  - Startup reads `/proc/self/mountinfo` and projects mounted removable devices
    into Places without writing them to `user-places.xbel`.
  - `replace_removable_device_places()` replaces only the dynamic removable
    device section, keeps user bookmarks before grouped sections, skips paths
    already covered by built-ins/user bookmarks, and gives device rows a drive
    icon instead of bookmark/folder styling.
  - Future UDisks2 signal handling should feed fresh `DeviceInfo` snapshots
    into the same replacement path.
- `src/core/bus.rs`
  - Future UDisks2 ObjectManager and Properties signal subscriptions should use
    the shared system-bus controller.

## Remaining Work

- Subscribe to `InterfacesAdded`, `InterfacesRemoved`, and
  `PropertiesChanged`, then update the snapshot incrementally instead of
  only reading the initial ObjectManager state.
- Route device add/remove/change events through the core event channel.
- Route UDisks2 add/remove/change results into
  `replace_removable_device_places()` so the "Removable Devices" section
  updates live while Fika is running.
- Add mount/unmount/eject actions for unmounted or mounted device rows.
