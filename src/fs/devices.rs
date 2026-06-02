use crate::DeviceEntry;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub(crate) fn mounted_devices() -> Vec<DeviceEntry> {
    let roots = mount_roots();
    let (mounted, mount_source) = mounted_devices_from_mountinfo(&roots, "/proc/self/mountinfo")
        .map(|devices| {
            device_debug_log(&format!(
                "mountinfo discovered {} mounted device row(s)",
                devices.len().saturating_sub(1)
            ));
            (devices, DeviceMountSource::Mountinfo)
        })
        .unwrap_or_else(|| {
            device_debug_log("mountinfo unavailable; falling back to mount-root directory scan");
            (
                mounted_devices_from_roots(roots),
                DeviceMountSource::RootScan,
            )
        });
    let mounted_rows = mounted.len().saturating_sub(1);
    let discovered = match udisks2_removable_devices() {
        Ok(devices) => {
            device_debug_log(&format!(
                "UDisks2 discovered {} external media row(s)",
                devices.len()
            ));
            devices
        }
        Err(err) => {
            device_debug_log(&format!("UDisks2 discovery unavailable: {err}"));
            Vec::new()
        }
    };
    let udisks_rows = discovered.len();
    let merged = merge_device_entries_with_stats(mounted, discovered);
    device_debug_log(&device_merge_summary(&merged.stats));
    device_debug_log(&device_discovery_summary(
        mount_source,
        mounted_rows,
        udisks_rows,
        &merged.devices,
    ));
    device_debug_log_devices("merged", &merged.devices);
    merged.devices
}

pub(crate) fn device_diagnostics_report() -> String {
    format_device_diagnostics_report(&mounted_devices())
}

fn format_device_diagnostics_report(devices: &[DeviceEntry]) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "Fika Devices diagnostics");
    let _ = writeln!(output, "rows: {}", devices.len());
    for (index, device) in devices.iter().enumerate() {
        let _ = writeln!(
            output,
            "[{index}] label=\"{}\" marker=\"{}\" path=\"{}\" device_path=\"{}\" mounted={} can_mount={} can_unmount={} can_eject={} error=\"{}\"",
            diagnostic_value(device.label.as_str()),
            diagnostic_value(device.marker.as_str()),
            diagnostic_value(device.path.as_str()),
            diagnostic_value(device.device_path.as_str()),
            device.mounted,
            device.can_mount,
            device.can_unmount,
            device.can_eject,
            diagnostic_value(device.error.as_str()),
        );
    }
    output
}

fn diagnostic_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn mounted_devices_from_roots(roots: Vec<PathBuf>) -> Vec<DeviceEntry> {
    mounted_devices_from_paths(mounted_children_from_roots(&roots))
}

fn mounted_devices_from_mountinfo(
    roots: &[PathBuf],
    mountinfo_path: &str,
) -> Option<Vec<DeviceEntry>> {
    let contents = fs::read_to_string(mountinfo_path).ok()?;
    let mount_points = parse_mountinfo_records(&contents)
        .into_iter()
        .filter(|record| is_device_mount_record(record, roots))
        .map(|record| record.mount_point)
        .collect::<Vec<_>>();

    Some(mounted_devices_from_paths(mount_points))
}

fn mounted_devices_from_paths(paths: Vec<PathBuf>) -> Vec<DeviceEntry> {
    let mut devices = Vec::from([filesystem_entry()]);
    let mut seen = HashSet::from([String::from("/")]);

    let mut paths = paths;
    paths.sort_by_key(|path| mount_label(path).to_lowercase());

    for path in paths {
        let path_text = path.display().to_string();
        if !seen.insert(path_text.clone()) {
            continue;
        }
        devices.push(device_entry(
            mount_label(&path),
            path_text.clone(),
            path_text,
            mount_marker(&path),
            true,
            DeviceCapabilities::default(),
        ));
    }

    devices
}

fn merge_device_entries_with_stats(
    mounted_devices: Vec<DeviceEntry>,
    discovered_devices: Vec<DeviceEntry>,
) -> DeviceMergeResult {
    let mut devices = Vec::new();
    let mut seen = HashMap::new();
    let mut origins = Vec::new();

    for device in mounted_devices {
        let path = device.path.to_string();
        if let Some(index) = seen.get(&path).copied() {
            merge_device_metadata(&mut devices[index], &device);
        } else {
            seen.insert(path, devices.len());
            devices.push(device);
            origins.push(DeviceEntryOrigin {
                mounted_table: true,
                udisks: false,
            });
        }
    }

    for device in discovered_devices {
        let path = device.path.to_string();
        if let Some(index) = seen.get(&path).copied() {
            merge_device_metadata(&mut devices[index], &device);
            origins[index].udisks = true;
        } else {
            seen.insert(path, devices.len());
            devices.push(device);
            origins.push(DeviceEntryOrigin {
                mounted_table: false,
                udisks: true,
            });
        }
    }

    DeviceMergeResult {
        stats: DeviceMergeStats::from_origins(&devices, &origins),
        devices,
    }
}

fn merge_device_metadata(existing: &mut DeviceEntry, discovered: &DeviceEntry) {
    if existing.device_path == existing.path && discovered.device_path != discovered.path {
        existing.device_path = discovered.device_path.clone();
    }
    existing.can_eject |= discovered.can_eject;
    existing.can_mount |= discovered.can_mount;
    existing.can_unmount |= discovered.can_unmount;
    existing.mounted |= discovered.mounted;
}

struct DeviceMergeResult {
    devices: Vec<DeviceEntry>,
    stats: DeviceMergeStats,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DeviceEntryOrigin {
    mounted_table: bool,
    udisks: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DeviceMergeStats {
    mounted_only: usize,
    udisks_only: usize,
    merged: usize,
}

impl DeviceMergeStats {
    fn from_origins(devices: &[DeviceEntry], origins: &[DeviceEntryOrigin]) -> Self {
        let mut stats = Self::default();
        for (device, origin) in devices.iter().zip(origins) {
            if device.path.as_str() == "/" {
                continue;
            }
            match (origin.mounted_table, origin.udisks) {
                (true, true) => stats.merged += 1,
                (true, false) => stats.mounted_only += 1,
                (false, true) => stats.udisks_only += 1,
                (false, false) => {}
            }
        }
        stats
    }
}

fn device_merge_summary(stats: &DeviceMergeStats) -> String {
    format!(
        "merge mounted_only={} udisks_only={} merged={}",
        stats.mounted_only, stats.udisks_only, stats.merged
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeviceMountSource {
    Mountinfo,
    RootScan,
}

impl DeviceMountSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Mountinfo => "mountinfo",
            Self::RootScan => "root-scan",
        }
    }
}

fn device_discovery_summary(
    mount_source: DeviceMountSource,
    mounted_rows: usize,
    udisks_rows: usize,
    devices: &[DeviceEntry],
) -> String {
    let final_rows = devices.len().saturating_sub(1);
    let mounted_final = devices
        .iter()
        .filter(|device| device.path.as_str() != "/" && device.mounted)
        .count();
    let unmounted_final = devices
        .iter()
        .filter(|device| device.path.as_str() != "/" && !device.mounted)
        .count();
    let mountable = devices.iter().filter(|device| device.can_mount).count();
    let unmountable = devices.iter().filter(|device| device.can_unmount).count();
    let ejectable = devices.iter().filter(|device| device.can_eject).count();
    format!(
        "summary mount_source={} mounted_rows={} udisks_rows={} final_rows={} mounted={} unmounted={} mountable={} unmountable={} ejectable={}",
        mount_source.as_str(),
        mounted_rows,
        udisks_rows,
        final_rows,
        mounted_final,
        unmounted_final,
        mountable,
        unmountable,
        ejectable
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DeviceCapabilities {
    can_mount: bool,
    can_unmount: bool,
    can_eject: bool,
}

fn filesystem_entry() -> DeviceEntry {
    device_entry(
        "Filesystem".into(),
        "/".into(),
        "/".into(),
        "/".into(),
        true,
        DeviceCapabilities::default(),
    )
}

fn device_entry(
    label: String,
    path: String,
    device_path: String,
    marker: String,
    mounted: bool,
    capabilities: DeviceCapabilities,
) -> DeviceEntry {
    DeviceEntry {
        label: label.into(),
        path: path.into(),
        device_path: device_path.into(),
        marker: marker.into(),
        mounted,
        can_mount: capabilities.can_mount,
        can_unmount: capabilities.can_unmount,
        can_eject: capabilities.can_eject,
        pending_action: String::new().into(),
        error: String::new().into(),
    }
}

fn mount_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(user) = env::var_os("USER").filter(|user| !user.is_empty()) {
        roots.push(PathBuf::from("/run/media").join(&user));
        roots.push(PathBuf::from("/media").join(user));
    }
    roots.push(PathBuf::from("/media"));
    roots.push(PathBuf::from("/mnt"));
    roots
}

fn mounted_children_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| mounted_children(root))
        .collect()
}

fn mounted_children(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut children = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let path = entry.path();
            if path == root {
                return None;
            }
            Some(path)
        })
        .collect::<Vec<_>>();
    children.sort_by_key(|path| mount_label(path).to_lowercase());
    children
}

type Properties = HashMap<String, OwnedValue>;
type InterfaceMap = HashMap<String, Properties>;
type ManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;

fn udisks2_removable_devices() -> Result<Vec<DeviceEntry>, String> {
    let connection = system_bus_connection(Duration::from_millis(750))?;
    udisks2_removable_devices_with_connection(&connection)
}

pub(crate) fn mount_device(device_path: &str) -> Result<PathBuf, String> {
    let connection = system_bus_connection(Duration::from_secs(120))?;
    mount_device_with_connection(&connection, device_path)
}

pub(crate) fn unmount_device(device_path: &str) -> Result<(), String> {
    let connection = system_bus_connection(Duration::from_secs(120))?;
    unmount_device_with_connection(&connection, device_path)
}

pub(crate) fn eject_device(device_path: &str) -> Result<(), String> {
    let connection = system_bus_connection(Duration::from_secs(120))?;
    eject_device_with_connection(&connection, device_path)
}

fn system_bus_connection(timeout: Duration) -> Result<Connection, String> {
    zbus::blocking::connection::Builder::system()
        .map_err(|err| format!("cannot create system bus builder: {err}"))?
        .method_timeout(timeout)
        .build()
        .map_err(|err| format!("cannot connect to system bus: {err}"))
}

fn udisks2_removable_devices_with_connection(
    connection: &Connection,
) -> Result<Vec<DeviceEntry>, String> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )
    .map_err(|err| format!("cannot create UDisks2 ObjectManager proxy: {err}"))?;

    let objects: ManagedObjects = proxy
        .call("GetManagedObjects", &())
        .map_err(|err| format!("GetManagedObjects failed: {err}"))?;
    Ok(udisks2_removable_devices_from_objects(&objects))
}

fn mount_device_with_connection(
    connection: &Connection,
    device_path: &str,
) -> Result<PathBuf, String> {
    let objects = udisks2_managed_objects(connection)?;
    let target = udisks2_mount_target_from_objects(&objects, device_path)?;
    if let Some(mount_point) = target.mounted_at {
        return Ok(mount_point);
    }

    let proxy = Proxy::new(
        connection,
        "org.freedesktop.UDisks2",
        target.block_path.as_str(),
        "org.freedesktop.UDisks2.Filesystem",
    )
    .map_err(|err| format!("cannot create UDisks2 Filesystem proxy: {err}"))?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let mount_point: String = proxy
        .call("Mount", &(options))
        .map_err(|err| format_udisks2_call_error("Mount", &err))?;
    Ok(PathBuf::from(mount_point))
}

fn unmount_device_with_connection(
    connection: &Connection,
    device_path: &str,
) -> Result<(), String> {
    let objects = udisks2_managed_objects(connection)?;
    let target = udisks2_device_target_from_objects(&objects, device_path)?;
    if !target.mounted {
        return Ok(());
    }

    let proxy = Proxy::new(
        connection,
        "org.freedesktop.UDisks2",
        target.block_path.as_str(),
        "org.freedesktop.UDisks2.Filesystem",
    )
    .map_err(|err| format!("cannot create UDisks2 Filesystem proxy: {err}"))?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let _: () = proxy
        .call("Unmount", &(options))
        .map_err(|err| format_udisks2_call_error("Unmount", &err))?;
    Ok(())
}

fn eject_device_with_connection(connection: &Connection, device_path: &str) -> Result<(), String> {
    let objects = udisks2_managed_objects(connection)?;
    let target = udisks2_device_target_from_objects(&objects, device_path)?;

    let proxy = Proxy::new(
        connection,
        "org.freedesktop.UDisks2",
        target.drive_path.as_str(),
        "org.freedesktop.UDisks2.Drive",
    )
    .map_err(|err| format!("cannot create UDisks2 Drive proxy: {err}"))?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let _: () = proxy
        .call("Eject", &(options))
        .map_err(|err| format_udisks2_call_error("Eject", &err))?;
    Ok(())
}

fn format_udisks2_call_error(action: &str, err: &zbus::Error) -> String {
    format!(
        "UDisks2 {action} failed: {}",
        udisks2_error_guidance(err).unwrap_or_else(|| err.to_string())
    )
}

fn udisks2_error_guidance(err: &zbus::Error) -> Option<String> {
    let zbus::Error::MethodError(name, detail, _) = err else {
        return None;
    };
    let name = name.to_string();
    let detail = detail.as_deref().unwrap_or("no details");
    udisks2_error_guidance_from_parts(&name, detail)
}

fn udisks2_error_guidance_from_parts(name: &str, detail: &str) -> Option<String> {
    let lower_detail = detail.to_ascii_lowercase();
    let guidance = if name.ends_with(".DeviceBusy") || lower_detail.contains("busy") {
        "device is busy; close files, terminals, or applications using it, then retry"
    } else if name.ends_with(".NotAuthorized")
        || name.ends_with(".AccessDenied")
        || lower_detail.contains("not authorized")
        || lower_detail.contains("permission denied")
    {
        "authorization was denied or no polkit agent handled the request"
    } else if name.ends_with(".AlreadyMounted") {
        "device is already mounted; refresh Devices and open the mount point"
    } else if name.ends_with(".NotMounted") {
        "device is not mounted; refresh Devices before retrying"
    } else if name.ends_with(".Cancelled") || lower_detail.contains("cancelled") {
        "operation was cancelled"
    } else if name.ends_with(".TimedOut") || lower_detail.contains("timed out") {
        "operation timed out; check whether the device is responding"
    } else {
        return None;
    };
    Some(format!("{guidance} ({name}: {detail})"))
}

fn udisks2_managed_objects(connection: &Connection) -> Result<ManagedObjects, String> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )
    .map_err(|err| format!("cannot create UDisks2 ObjectManager proxy: {err}"))?;

    proxy
        .call("GetManagedObjects", &())
        .map_err(|err| format!("GetManagedObjects failed: {err}"))
}

#[derive(Debug, Eq, PartialEq)]
struct UDisks2MountTarget {
    block_path: OwnedObjectPath,
    mounted_at: Option<PathBuf>,
}

#[derive(Debug, Eq, PartialEq)]
struct UDisks2DeviceTarget {
    block_path: OwnedObjectPath,
    drive_path: OwnedObjectPath,
    mounted: bool,
    can_eject: bool,
}

fn udisks2_mount_target_from_objects(
    objects: &ManagedObjects,
    device_path: &str,
) -> Result<UDisks2MountTarget, String> {
    let target = udisks2_device_target_from_objects(objects, device_path)?;
    let interfaces = objects
        .get(&target.block_path)
        .ok_or_else(|| "device is no longer available".to_string())?;
    let filesystem = interfaces
        .get("org.freedesktop.UDisks2.Filesystem")
        .ok_or_else(|| "device has no mountable filesystem".to_string())?;
    let mounted_at = mount_points_property(filesystem, "MountPoints")
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(PathBuf::from);
    Ok(UDisks2MountTarget {
        block_path: target.block_path,
        mounted_at,
    })
}

fn udisks2_device_target_from_objects(
    objects: &ManagedObjects,
    device_path: &str,
) -> Result<UDisks2DeviceTarget, String> {
    for (object_path, interfaces) in objects {
        let Some(block) = interfaces.get("org.freedesktop.UDisks2.Block") else {
            continue;
        };
        let Some(device) = byte_string_property(block, "Device") else {
            continue;
        };
        if device != device_path {
            continue;
        }
        if block_is_hidden(block) {
            return Err("device is hidden by UDisks2".to_string());
        }
        if !is_removable_media_object(objects, block) {
            return Err("device is not removable media".to_string());
        }
        let Some(filesystem) = interfaces.get("org.freedesktop.UDisks2.Filesystem") else {
            return Err("device has no mountable filesystem".to_string());
        };
        let Some(drive_path) = owned_object_path_property(block, "Drive") else {
            return Err("device has no drive object".to_string());
        };
        let Some(drive) = objects
            .get(&drive_path)
            .and_then(|interfaces| interfaces.get("org.freedesktop.UDisks2.Drive"))
        else {
            return Err("device drive is no longer available".to_string());
        };
        let mounted = mount_points_property(filesystem, "MountPoints")
            .unwrap_or_default()
            .into_iter()
            .next()
            .is_some();
        return Ok(UDisks2DeviceTarget {
            block_path: object_path.clone(),
            drive_path,
            mounted,
            can_eject: bool_property(drive, "Ejectable"),
        });
    }

    Err("device is no longer available".to_string())
}

fn udisks2_removable_devices_from_objects(objects: &ManagedObjects) -> Vec<DeviceEntry> {
    let mut devices = objects
        .values()
        .filter_map(|interfaces| udisks2_removable_device_from_interfaces(objects, interfaces))
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.label.to_string().to_lowercase());
    devices
}

fn udisks2_removable_device_from_interfaces(
    objects: &ManagedObjects,
    interfaces: &InterfaceMap,
) -> Option<DeviceEntry> {
    let block = interfaces.get("org.freedesktop.UDisks2.Block")?;
    let device = byte_string_property(block, "Device").unwrap_or_else(|| "<unknown>".to_string());
    if block_is_hidden(block) {
        device_debug_log(&format!("UDisks2 skip {device}: hidden/system block hint"));
        return None;
    }

    if let Some(reason) = removable_media_rejection(objects, block) {
        device_debug_log(&format!("UDisks2 skip {device}: {reason}"));
        return None;
    }
    let drive = drive_for_block(objects, block)?;

    let Some(filesystem) = interfaces.get("org.freedesktop.UDisks2.Filesystem") else {
        device_debug_log(&format!("UDisks2 skip {device}: no filesystem interface"));
        return None;
    };
    let mount_points = mount_points_property(filesystem, "MountPoints").unwrap_or_default();
    let mounted = !mount_points.is_empty();
    let mount_point = mount_points.into_iter().next();
    let label = udisks2_display_label(block, drive, mount_point.as_deref(), &device);
    let marker = udisks2_marker(drive, &label);
    let path = mount_point.unwrap_or(device.clone());
    let capabilities = DeviceCapabilities {
        can_mount: !mounted,
        can_unmount: mounted,
        can_eject: bool_property(drive, "Ejectable"),
    };
    let entry = device_entry(
        label.clone(),
        path.clone(),
        device,
        marker,
        mounted,
        capabilities,
    );
    device_debug_log(&format!(
        "UDisks2 accept label={} marker={} path={} device_path={} mounted={} mountable={} unmountable={} ejectable={}",
        entry.label,
        entry.marker,
        entry.path,
        entry.device_path,
        entry.mounted,
        entry.can_mount,
        entry.can_unmount,
        entry.can_eject
    ));
    Some(entry)
}

fn is_removable_media_object(objects: &ManagedObjects, block: &Properties) -> bool {
    removable_media_rejection(objects, block).is_none()
}

fn removable_media_rejection(objects: &ManagedObjects, block: &Properties) -> Option<&'static str> {
    let Some(drive) = drive_for_block(objects, block) else {
        return Some("no drive object");
    };
    DriveMediaProfile::from_properties(drive).rejection()
}

fn udisks2_marker(drive: &Properties, label: &str) -> String {
    DriveMediaProfile::from_properties(drive)
        .marker()
        .unwrap_or_else(|| marker_from_label(label))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DriveMediaProfile {
    removable: bool,
    media_available: bool,
    media_removable: bool,
    ejectable: bool,
    optical: bool,
    connection_bus: Option<String>,
    media_compatibility: Vec<String>,
}

impl DriveMediaProfile {
    fn from_properties(drive: &Properties) -> Self {
        Self {
            removable: bool_property(drive, "Removable"),
            media_available: bool_property(drive, "MediaAvailable"),
            media_removable: bool_property(drive, "MediaRemovable"),
            ejectable: bool_property(drive, "Ejectable"),
            optical: bool_property(drive, "Optical"),
            connection_bus: string_property(drive, "ConnectionBus"),
            media_compatibility: string_list_property(drive, "MediaCompatibility")
                .unwrap_or_default(),
        }
    }

    fn rejection(&self) -> Option<&'static str> {
        if !self.media_available {
            return Some("media unavailable");
        }
        if self.is_user_visible_external_media() {
            None
        } else {
            Some("not removable, ejectable, optical, or USB-attached")
        }
    }

    fn is_user_visible_external_media(&self) -> bool {
        self.removable
            || self.media_removable
            || self.ejectable
            || self.optical
            || self.connection_bus.as_deref() == Some("usb")
            || self
                .media_compatibility
                .iter()
                .any(|media| media.starts_with("optical_") || media.starts_with("flash_"))
    }

    fn marker(&self) -> Option<String> {
        if self.optical
            || self
                .media_compatibility
                .iter()
                .any(|media| media.starts_with("optical_"))
        {
            Some("CD".to_string())
        } else if self
            .media_compatibility
            .iter()
            .any(|media| media.starts_with("flash_sd") || media.starts_with("flash_mmc"))
        {
            Some("SD".to_string())
        } else if self.connection_bus.as_deref() == Some("usb") {
            Some("USB".to_string())
        } else {
            None
        }
    }
}

fn block_is_hidden(block: &Properties) -> bool {
    bool_property(block, "HintIgnore") || bool_property(block, "HintSystem")
}

fn drive_for_block<'a>(objects: &'a ManagedObjects, block: &Properties) -> Option<&'a Properties> {
    let drive_path = owned_object_path_property(block, "Drive")?;
    objects
        .get(&drive_path)?
        .get("org.freedesktop.UDisks2.Drive")
}

fn bool_property(properties: &Properties, name: &str) -> bool {
    properties
        .get(name)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn string_property(properties: &Properties, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_string)
}

fn string_list_property(properties: &Properties, name: &str) -> Option<Vec<String>> {
    properties
        .get(name)
        .and_then(|value| Vec::<String>::try_from(value.clone()).ok())
}

fn owned_object_path_property(properties: &Properties, name: &str) -> Option<OwnedObjectPath> {
    properties
        .get(name)
        .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())
}

fn byte_string_property(properties: &Properties, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .and_then(byte_string_from_udisks)
}

fn mount_points_property(properties: &Properties, name: &str) -> Option<Vec<String>> {
    properties.get(name).and_then(|value| {
        Vec::<Vec<u8>>::try_from(value.clone())
            .ok()
            .map(|mount_points| {
                mount_points
                    .into_iter()
                    .filter_map(byte_string_from_udisks)
                    .collect()
            })
    })
}

fn byte_string_from_udisks(bytes: Vec<u8>) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    String::from_utf8(bytes[..end].to_vec()).ok()
}

fn drive_label(drive: &Properties) -> Option<String> {
    let vendor = string_property(drive, "Vendor").unwrap_or_default();
    let model = string_property(drive, "Model").unwrap_or_default();
    let label = format!("{vendor} {model}").trim().to_string();
    (!label.is_empty()).then_some(label)
}

fn udisks2_display_label(
    block: &Properties,
    drive: &Properties,
    mount_point: Option<&str>,
    device: &str,
) -> String {
    string_property(block, "HintName")
        .filter(|label| !label.is_empty())
        .or_else(|| string_property(block, "IdLabel"))
        .filter(|label| !label.is_empty())
        .or_else(|| {
            mount_point
                .map(Path::new)
                .map(mount_label)
                .filter(|label| !label.is_empty())
        })
        .or_else(|| drive_label(drive))
        .unwrap_or_else(|| mount_label(Path::new(device)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MountRecord {
    mount_point: PathBuf,
    source: String,
    fs_type: String,
}

fn parse_mountinfo_records(contents: &str) -> Vec<MountRecord> {
    contents
        .lines()
        .filter_map(parse_mountinfo_record)
        .collect()
}

fn parse_mountinfo_record(line: &str) -> Option<MountRecord> {
    let (mount_fields, fs_fields) = line.split_once(" - ")?;
    let mut fields = mount_fields.split_whitespace();
    fields.next()?;
    fields.next()?;
    fields.next()?;
    fields.next()?;
    let mount_point = fields.next()?;

    let mut fields = fs_fields.split_whitespace();
    let fs_type = fields.next()?;
    let source = fields.next()?;

    Some(MountRecord {
        mount_point: PathBuf::from(unescape_mountinfo_field(mount_point)),
        source: unescape_mountinfo_field(source),
        fs_type: fs_type.to_string(),
    })
}

fn unescape_mountinfo_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && matches!(bytes[index + 1], b'0'..=b'7')
            && matches!(bytes[index + 2], b'0'..=b'7')
            && matches!(bytes[index + 3], b'0'..=b'7')
        {
            let octal = (bytes[index + 1] - b'0') * 64
                + (bytes[index + 2] - b'0') * 8
                + (bytes[index + 3] - b'0');
            output.push(octal);
            index += 4;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn is_device_mount_record(record: &MountRecord, roots: &[PathBuf]) -> bool {
    is_device_mount_point(&record.mount_point, roots)
        && is_real_device_filesystem(&record.source, &record.fs_type)
}

fn is_device_mount_point(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        path == root
            || path
                .strip_prefix(root)
                .ok()
                .is_some_and(|relative| relative.components().count() > 0)
    })
}

fn is_real_device_filesystem(source: &str, fs_type: &str) -> bool {
    if is_pseudo_filesystem(fs_type) {
        return false;
    }

    source.starts_with("/dev/")
        || source.starts_with("UUID=")
        || source.starts_with("LABEL=")
        || fs_type == "fuseblk"
}

fn is_pseudo_filesystem(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "autofs"
            | "bpf"
            | "binfmt_misc"
            | "cgroup"
            | "cgroup2"
            | "configfs"
            | "debugfs"
            | "devpts"
            | "devtmpfs"
            | "fusectl"
            | "hugetlbfs"
            | "mqueue"
            | "nsfs"
            | "proc"
            | "pstore"
            | "rpc_pipefs"
            | "securityfs"
            | "sysfs"
            | "tmpfs"
            | "tracefs"
    )
}

fn mount_label(path: &Path) -> String {
    if let Some(name) = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
    {
        name.to_string()
    } else {
        path.to_string_lossy().into_owned()
    }
}

fn mount_marker(path: &Path) -> String {
    marker_from_label(&mount_label(path))
}

fn marker_from_label(label: &str) -> String {
    label
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().collect())
        .unwrap_or_else(|| "D".into())
}

fn device_debug_log(message: &str) {
    static DEBUG_DEVICES: OnceLock<bool> = OnceLock::new();
    if *DEBUG_DEVICES.get_or_init(|| {
        env::var("FIKA_DEBUG_DEVICES").is_ok_and(|value| env_flag_is_truthy(value.as_str()))
    }) {
        eprintln!("[fika devices] {message}");
    }
}

fn device_debug_log_devices(phase: &str, devices: &[DeviceEntry]) {
    if devices.is_empty() {
        device_debug_log(&format!("{phase}: no device rows"));
        return;
    }
    for (index, device) in devices.iter().enumerate() {
        device_debug_log(&format!(
            "{phase}[{index}] label={} marker={} path={} device_path={} mounted={} mountable={} unmountable={} ejectable={} error={}",
            device.label,
            device.marker,
            device.path,
            device.device_path,
            device.mounted,
            device.can_mount,
            device.can_unmount,
            device.can_eject,
            device.error
        ));
    }
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{DynamicType, Value};

    fn test_dir(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("fika-devices-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn mounted_devices_includes_filesystem_first() {
        let devices = mounted_devices_from_roots(Vec::new());

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "Filesystem");
        assert_eq!(devices[0].path, "/");
        assert_eq!(devices[0].marker, "/");
        assert!(devices[0].mounted);
    }

    #[test]
    fn device_diagnostics_report_lists_rows_and_escapes_fields() {
        let devices = vec![device_entry(
            "USB \"Disk\"".into(),
            "/run/media/yk/USB\nDisk".into(),
            "/dev/sdb1".into(),
            "USB".into(),
            true,
            DeviceCapabilities {
                can_unmount: true,
                can_eject: true,
                ..DeviceCapabilities::default()
            },
        )];

        let report = format_device_diagnostics_report(&devices);

        assert!(report.starts_with("Fika Devices diagnostics\nrows: 1\n"));
        assert!(report.contains("label=\"USB \\\"Disk\\\"\""));
        assert!(report.contains("path=\"/run/media/yk/USB\\nDisk\""));
        assert!(report.contains("device_path=\"/dev/sdb1\""));
        assert!(report.contains("mounted=true can_mount=false can_unmount=true can_eject=true"));
    }

    #[test]
    fn mounted_devices_lists_mounted_children() {
        let root = test_dir("children");
        fs::create_dir_all(root.join("USB Disk")).unwrap();
        fs::write(root.join("not-a-device"), "ignored").unwrap();

        let devices = mounted_devices_from_roots(vec![root.clone()]);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].label, "USB Disk");
        assert_eq!(devices[1].path, root.join("USB Disk").display().to_string());
        assert_eq!(devices[1].marker, "U");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mounted_devices_uses_mountinfo_mount_points() {
        let root = test_dir("mountinfo");
        let mount_point = root.join("USB Disk");
        fs::create_dir_all(&mount_point).unwrap();
        let mountinfo = format!(
            "42 24 8:1 / {} rw,nosuid,nodev - ext4 /dev/sdb1 rw\n",
            mount_point.display().to_string().replace(' ', "\\040")
        );
        let mountinfo_path = root.join("mountinfo");
        fs::write(&mountinfo_path, mountinfo).unwrap();

        let devices = mounted_devices_from_mountinfo(
            std::slice::from_ref(&root),
            mountinfo_path.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].label, "USB Disk");
        assert_eq!(devices[1].path, mount_point.display().to_string());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mounted_devices_ignores_unmounted_directories_when_mountinfo_is_available() {
        let root = test_dir("mountinfo-authoritative");
        fs::create_dir_all(root.join("plain-dir")).unwrap();
        let mountinfo_path = root.join("mountinfo");
        fs::write(&mountinfo_path, "").unwrap();

        let devices = mounted_devices_from_mountinfo(
            std::slice::from_ref(&root),
            mountinfo_path.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, "/");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mounted_devices_ignores_pseudo_filesystems_in_mountinfo() {
        let root = test_dir("mountinfo-pseudo");
        let real_mount = root.join("USB Disk");
        let pseudo_mount = root.join("runtime");
        fs::create_dir_all(&real_mount).unwrap();
        fs::create_dir_all(&pseudo_mount).unwrap();
        let mountinfo = format!(
            "42 24 8:1 / {} rw,nosuid,nodev - ext4 /dev/sdb1 rw\n\
             43 24 0:42 / {} rw,nosuid,nodev - tmpfs tmpfs rw\n",
            real_mount.display().to_string().replace(' ', "\\040"),
            pseudo_mount.display()
        );
        let mountinfo_path = root.join("mountinfo");
        fs::write(&mountinfo_path, mountinfo).unwrap();

        let devices = mounted_devices_from_mountinfo(
            std::slice::from_ref(&root),
            mountinfo_path.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].label, "USB Disk");
        assert!(
            !devices
                .iter()
                .any(|device| device.label.as_str() == "runtime")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mountinfo_records_decode_mount_point_and_source() {
        let records = parse_mountinfo_records(
            "42 24 8:1 / /run/media/yk/USB\\040Disk rw,nosuid,nodev - ext4 /dev/disk/by-label/USB\\040Disk rw\n",
        );

        assert_eq!(
            records,
            vec![MountRecord {
                mount_point: PathBuf::from("/run/media/yk/USB Disk"),
                source: "/dev/disk/by-label/USB Disk".to_string(),
                fs_type: "ext4".to_string(),
            }]
        );
    }

    #[test]
    fn mounted_devices_deduplicates_roots() {
        let root = test_dir("dedupe");
        fs::create_dir_all(root.join("stick")).unwrap();

        let devices = mounted_devices_from_roots(vec![root.clone(), root.clone()]);

        assert_eq!(
            devices
                .iter()
                .filter(|device| device.label.as_str() == "stick")
                .count(),
            1
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn udisks2_lists_unmounted_removable_media() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "FIKA USB",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "FIKA USB");
        assert_eq!(devices[0].path, "/dev/sdb1");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
        assert_eq!(devices[0].marker, "F");
        assert!(!devices[0].mounted);
    }

    #[test]
    fn udisks2_prefers_mount_point_label_for_mounted_media_without_volume_label() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "",
                false,
            ),
            Some(filesystem_object(vec!["/run/media/yk/FIKA USB"])),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "FIKA USB");
        assert_eq!(devices[0].path, "/run/media/yk/FIKA USB");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
        assert!(devices[0].mounted);
    }

    #[test]
    fn udisks2_uses_drive_label_for_unmounted_media_without_volume_label() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "Framework USB-C Storage");
        assert_eq!(devices[0].path, "/dev/sdb1");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
        assert!(!devices[0].mounted);
    }

    #[test]
    fn udisks2_filters_ignored_nonremovable_and_empty_media() {
        let ignored = udisks_objects(
            drive_object(true, true, "Ignored", "Disk"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Ignored",
                true,
            ),
            None,
        );
        let fixed = udisks_objects(
            drive_object(false, true, "Fixed", "Disk"),
            block_object(
                "/dev/nvme0n1p1",
                "/org/freedesktop/UDisks2/drives/test",
                "Fixed",
                false,
            ),
            None,
        );
        let empty = udisks_objects(
            drive_object(true, false, "Empty", "Reader"),
            block_object(
                "/dev/sdc1",
                "/org/freedesktop/UDisks2/drives/test",
                "Empty",
                false,
            ),
            None,
        );

        assert!(udisks2_removable_devices_from_objects(&ignored).is_empty());
        assert!(udisks2_removable_devices_from_objects(&fixed).is_empty());
        assert!(udisks2_removable_devices_from_objects(&empty).is_empty());
    }

    #[test]
    fn udisks2_filters_removable_blocks_without_filesystem_interface() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "FIKA USB",
                false,
            ),
            None,
        );

        assert!(udisks2_removable_devices_from_objects(&objects).is_empty());
    }

    #[test]
    fn udisks2_lists_usb_attached_media_even_when_not_marked_removable() {
        let objects = udisks_objects(
            drive_object_with_options(false, true, false, "usb", "External", "SSD"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Backup",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "Backup");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
        assert_eq!(devices[0].marker, "USB");
        assert!(!devices[0].mounted);
    }

    #[test]
    fn udisks2_lists_media_removable_drives() {
        let objects = udisks_objects(
            drive_object_with_media_options(false, true, true, false, "", "Card", "Reader"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Camera Card",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "Camera Card");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
    }

    #[test]
    fn udisks2_lists_media_compatibility_external_media() {
        let flash = udisks_objects(
            drive_object_with_media_compatibility(
                false,
                true,
                false,
                "",
                ["flash_sd"],
                "USB",
                "Reader",
            ),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Camera Card",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );
        let optical = udisks_objects(
            drive_object_with_media_compatibility(
                false,
                true,
                false,
                "",
                ["optical_dvd"],
                "HL-DT-ST",
                "DVD-RW",
            ),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Install Disc",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        let flash_devices = udisks2_removable_devices_from_objects(&flash);
        let optical_devices = udisks2_removable_devices_from_objects(&optical);

        assert_eq!(flash_devices.len(), 1);
        assert_eq!(flash_devices[0].marker, "SD");
        assert_eq!(optical_devices.len(), 1);
        assert_eq!(optical_devices[0].marker, "CD");
    }

    #[test]
    fn udisks2_filters_empty_optical_drives() {
        let objects = udisks_objects(
            drive_object_with_media_compatibility(
                false,
                false,
                true,
                "",
                ["optical_dvd"],
                "HL-DT-ST",
                "DVD-RW",
            ),
            block_object(
                "/dev/sr0",
                "/org/freedesktop/UDisks2/drives/test",
                "",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        assert!(udisks2_removable_devices_from_objects(&objects).is_empty());
    }

    #[test]
    fn udisks2_prefers_hint_name_for_display_label() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object_with_hint_name(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "Volume Label",
                "Desktop Name",
                false,
            ),
            Some(filesystem_object(vec!["/run/media/yk/Volume Label"])),
        );

        let devices = udisks2_removable_devices_from_objects(&objects);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].label, "Desktop Name");
        assert_eq!(devices[0].marker, "D");
    }

    #[test]
    fn udisks2_filters_system_hint_blocks_even_on_usb_media() {
        let objects = udisks_objects(
            drive_object_with_options(false, true, false, "usb", "System", "Disk"),
            block_object_with_system_hint(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "System",
                false,
                true,
            ),
            Some(filesystem_object(Vec::new())),
        );

        assert!(udisks2_removable_devices_from_objects(&objects).is_empty());
    }

    #[test]
    fn udisks2_filters_blocks_without_drive_objects() {
        let mut objects = ManagedObjects::new();
        let block_interfaces = HashMap::from([(
            "org.freedesktop.UDisks2.Block".to_string(),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/missing",
                "Missing",
                false,
            ),
        )]);
        objects.insert(
            OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdb1").unwrap(),
            block_interfaces,
        );

        assert!(udisks2_removable_devices_from_objects(&objects).is_empty());
    }

    #[test]
    fn udisks2_error_guidance_explains_common_device_failures() {
        assert_eq!(
            udisks2_error_guidance_from_parts(
                "org.freedesktop.UDisks2.Error.DeviceBusy",
                "target is busy",
            ),
            Some(
                "device is busy; close files, terminals, or applications using it, then retry \
                 (org.freedesktop.UDisks2.Error.DeviceBusy: target is busy)"
                    .to_string()
            )
        );
        assert_eq!(
            udisks2_error_guidance_from_parts(
                "org.freedesktop.UDisks2.Error.NotAuthorized",
                "not authorized",
            ),
            Some(
                "authorization was denied or no polkit agent handled the request \
                 (org.freedesktop.UDisks2.Error.NotAuthorized: not authorized)"
                    .to_string()
            )
        );
        assert_eq!(
            udisks2_error_guidance_from_parts(
                "org.freedesktop.UDisks2.Error.NotMounted",
                "not mounted",
            ),
            Some(
                "device is not mounted; refresh Devices before retrying \
                 (org.freedesktop.UDisks2.Error.NotMounted: not mounted)"
                    .to_string()
            )
        );
        assert_eq!(
            udisks2_error_guidance_from_parts(
                "org.freedesktop.UDisks2.Error.Unknown",
                "unexpected failure",
            ),
            None
        );
    }

    #[test]
    fn device_discovery_summary_counts_final_rows_and_capabilities() {
        let devices = vec![
            filesystem_entry(),
            device_entry(
                "Mounted USB".into(),
                "/run/media/yk/USB".into(),
                "/dev/sdb1".into(),
                "M".into(),
                true,
                DeviceCapabilities {
                    can_unmount: true,
                    ..DeviceCapabilities::default()
                },
            ),
            device_entry(
                "Unmounted Card".into(),
                "/dev/sdc1".into(),
                "/dev/sdc1".into(),
                "U".into(),
                false,
                DeviceCapabilities {
                    can_mount: true,
                    can_eject: true,
                    ..DeviceCapabilities::default()
                },
            ),
        ];

        assert_eq!(
            device_discovery_summary(DeviceMountSource::Mountinfo, 1, 2, &devices),
            "summary mount_source=mountinfo mounted_rows=1 udisks_rows=2 final_rows=2 mounted=1 unmounted=1 mountable=1 unmountable=1 ejectable=1"
        );
        assert_eq!(
            device_discovery_summary(DeviceMountSource::RootScan, 0, 0, &[filesystem_entry()]),
            "summary mount_source=root-scan mounted_rows=0 udisks_rows=0 final_rows=0 mounted=0 unmounted=0 mountable=0 unmountable=0 ejectable=0"
        );
    }

    #[test]
    fn device_merge_keeps_mountinfo_entry_before_udisks_duplicate() {
        let mounted = vec![
            filesystem_entry(),
            device_entry(
                "Mounted USB".into(),
                "/run/media/yk/USB".into(),
                "/run/media/yk/USB".into(),
                "M".into(),
                true,
                DeviceCapabilities::default(),
            ),
        ];
        let discovered = vec![
            device_entry(
                "Duplicate".into(),
                "/run/media/yk/USB".into(),
                "/dev/sdb1".into(),
                "D".into(),
                true,
                DeviceCapabilities {
                    can_mount: false,
                    can_unmount: true,
                    can_eject: true,
                },
            ),
            device_entry(
                "Unmounted".into(),
                "/dev/sdc1".into(),
                "/dev/sdc1".into(),
                "U".into(),
                false,
                DeviceCapabilities {
                    can_mount: true,
                    can_unmount: false,
                    can_eject: true,
                },
            ),
        ];

        let merged = merge_device_entries_with_stats(mounted, discovered);
        let devices = merged.devices;

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[1].label, "Mounted USB");
        assert_eq!(devices[1].path, "/run/media/yk/USB");
        assert_eq!(devices[1].device_path, "/dev/sdb1");
        assert_eq!(devices[1].marker, "M");
        assert!(devices[1].mounted);
        assert!(devices[1].can_eject);
        assert_eq!(devices[2].label, "Unmounted");
        assert_eq!(
            merged.stats,
            DeviceMergeStats {
                mounted_only: 0,
                udisks_only: 1,
                merged: 1,
            }
        );
        assert_eq!(
            device_merge_summary(&merged.stats),
            "merge mounted_only=0 udisks_only=1 merged=1"
        );
    }

    #[test]
    fn udisks2_mount_target_finds_unmounted_filesystem_device() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "FIKA USB",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );

        assert_eq!(
            udisks2_mount_target_from_objects(&objects, "/dev/sdb1").unwrap(),
            UDisks2MountTarget {
                block_path: OwnedObjectPath::try_from(
                    "/org/freedesktop/UDisks2/block_devices/sdb1"
                )
                .unwrap(),
                mounted_at: None,
            }
        );
    }

    #[test]
    fn udisks2_mount_target_returns_existing_mount_point() {
        let objects = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "FIKA USB",
                false,
            ),
            Some(filesystem_object(vec!["/run/media/yk/FIKA USB"])),
        );

        assert_eq!(
            udisks2_mount_target_from_objects(&objects, "/dev/sdb1").unwrap(),
            UDisks2MountTarget {
                block_path: OwnedObjectPath::try_from(
                    "/org/freedesktop/UDisks2/block_devices/sdb1"
                )
                .unwrap(),
                mounted_at: Some(PathBuf::from("/run/media/yk/FIKA USB")),
            }
        );
    }

    #[test]
    fn udisks2_mount_target_rejects_unmountable_devices() {
        let fixed = udisks_objects(
            drive_object(false, true, "Fixed", "Disk"),
            block_object(
                "/dev/nvme0n1p1",
                "/org/freedesktop/UDisks2/drives/test",
                "Fixed",
                false,
            ),
            Some(filesystem_object(Vec::new())),
        );
        let no_filesystem = udisks_objects(
            drive_object(true, true, "Framework", "USB-C Storage"),
            block_object(
                "/dev/sdb1",
                "/org/freedesktop/UDisks2/drives/test",
                "FIKA USB",
                false,
            ),
            None,
        );

        assert_eq!(
            udisks2_mount_target_from_objects(&fixed, "/dev/nvme0n1p1").unwrap_err(),
            "device is not removable media"
        );
        assert_eq!(
            udisks2_mount_target_from_objects(&no_filesystem, "/dev/sdb1").unwrap_err(),
            "device has no mountable filesystem"
        );
        assert_eq!(
            udisks2_mount_target_from_objects(&no_filesystem, "/dev/missing").unwrap_err(),
            "device is no longer available"
        );
    }

    fn udisks_objects(
        drive: Properties,
        block: Properties,
        filesystem: Option<Properties>,
    ) -> ManagedObjects {
        let mut objects = ManagedObjects::new();
        objects.insert(
            OwnedObjectPath::try_from("/org/freedesktop/UDisks2/drives/test").unwrap(),
            HashMap::from([("org.freedesktop.UDisks2.Drive".to_string(), drive)]),
        );
        let mut block_interfaces =
            HashMap::from([("org.freedesktop.UDisks2.Block".to_string(), block)]);
        if let Some(filesystem) = filesystem {
            block_interfaces.insert("org.freedesktop.UDisks2.Filesystem".to_string(), filesystem);
        }
        objects.insert(
            OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdb1").unwrap(),
            block_interfaces,
        );
        objects
    }

    fn drive_object(
        removable: bool,
        media_available: bool,
        vendor: &str,
        model: &str,
    ) -> Properties {
        drive_object_with_options(removable, media_available, false, "", vendor, model)
    }

    fn drive_object_with_options(
        removable: bool,
        media_available: bool,
        ejectable: bool,
        connection_bus: &str,
        vendor: &str,
        model: &str,
    ) -> Properties {
        drive_object_with_media_options(
            removable,
            media_available,
            false,
            ejectable,
            connection_bus,
            vendor,
            model,
        )
    }

    fn drive_object_with_media_options(
        removable: bool,
        media_available: bool,
        media_removable: bool,
        ejectable: bool,
        connection_bus: &str,
        vendor: &str,
        model: &str,
    ) -> Properties {
        drive_object_from_fixture(DriveFixture {
            removable,
            media_available,
            media_removable,
            ejectable,
            connection_bus,
            media_compatibility: Vec::new(),
            vendor,
            model,
        })
    }

    fn drive_object_with_media_compatibility<const N: usize>(
        removable: bool,
        media_available: bool,
        ejectable: bool,
        connection_bus: &str,
        media_compatibility: [&str; N],
        vendor: &str,
        model: &str,
    ) -> Properties {
        drive_object_from_fixture(DriveFixture {
            removable,
            media_available,
            media_removable: false,
            ejectable,
            connection_bus,
            media_compatibility: media_compatibility
                .into_iter()
                .map(str::to_string)
                .collect(),
            vendor,
            model,
        })
    }

    struct DriveFixture<'a> {
        removable: bool,
        media_available: bool,
        media_removable: bool,
        ejectable: bool,
        connection_bus: &'a str,
        media_compatibility: Vec<String>,
        vendor: &'a str,
        model: &'a str,
    }

    fn drive_object_from_fixture(fixture: DriveFixture<'_>) -> Properties {
        let optical = fixture
            .media_compatibility
            .iter()
            .any(|media| media.starts_with("optical_"));
        HashMap::from([
            ("Removable".to_string(), value(fixture.removable)),
            ("MediaAvailable".to_string(), value(fixture.media_available)),
            ("MediaRemovable".to_string(), value(fixture.media_removable)),
            ("Ejectable".to_string(), value(fixture.ejectable)),
            ("Optical".to_string(), value(optical)),
            (
                "ConnectionBus".to_string(),
                value(fixture.connection_bus.to_string()),
            ),
            (
                "MediaCompatibility".to_string(),
                value(fixture.media_compatibility),
            ),
            ("Vendor".to_string(), value(fixture.vendor.to_string())),
            ("Model".to_string(), value(fixture.model.to_string())),
        ])
    }

    fn block_object(device: &str, drive: &str, label: &str, hint_ignore: bool) -> Properties {
        block_object_with_hint_name_and_system_hint(device, drive, label, "", hint_ignore, false)
    }

    fn block_object_with_system_hint(
        device: &str,
        drive: &str,
        label: &str,
        hint_ignore: bool,
        hint_system: bool,
    ) -> Properties {
        block_object_with_hint_name_and_system_hint(
            device,
            drive,
            label,
            "",
            hint_ignore,
            hint_system,
        )
    }

    fn block_object_with_hint_name(
        device: &str,
        drive: &str,
        label: &str,
        hint_name: &str,
        hint_ignore: bool,
    ) -> Properties {
        block_object_with_hint_name_and_system_hint(
            device,
            drive,
            label,
            hint_name,
            hint_ignore,
            false,
        )
    }

    fn block_object_with_hint_name_and_system_hint(
        device: &str,
        drive: &str,
        label: &str,
        hint_name: &str,
        hint_ignore: bool,
        hint_system: bool,
    ) -> Properties {
        HashMap::from([
            (
                "Device".to_string(),
                value(
                    device
                        .as_bytes()
                        .iter()
                        .copied()
                        .chain([0])
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "Drive".to_string(),
                value(OwnedObjectPath::try_from(drive).unwrap()),
            ),
            ("IdLabel".to_string(), value(label.to_string())),
            ("HintName".to_string(), value(hint_name.to_string())),
            ("HintIgnore".to_string(), value(hint_ignore)),
            ("HintSystem".to_string(), value(hint_system)),
        ])
    }

    fn filesystem_object(mount_points: Vec<&str>) -> Properties {
        let mount_points = mount_points
            .into_iter()
            .map(|mount_point| {
                mount_point
                    .as_bytes()
                    .iter()
                    .copied()
                    .chain([0])
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        HashMap::from([("MountPoints".to_string(), value(mount_points))])
    }

    fn value<T>(value: T) -> OwnedValue
    where
        T: Into<Value<'static>> + DynamicType,
    {
        OwnedValue::try_from(Value::new(value)).unwrap()
    }
}
