use crate::DeviceEntry;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub(crate) fn mounted_devices() -> Vec<DeviceEntry> {
    let mounted = mounted_devices_from_mountinfo(&mount_roots(), "/proc/self/mountinfo")
        .unwrap_or_else(|| mounted_devices_from_roots(mount_roots()));
    merge_device_entries(mounted, udisks2_removable_devices().unwrap_or_default())
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
            false,
        ));
    }

    devices
}

fn merge_device_entries(
    mounted_devices: Vec<DeviceEntry>,
    discovered_devices: Vec<DeviceEntry>,
) -> Vec<DeviceEntry> {
    let mut devices = Vec::new();
    let mut seen = HashSet::new();

    for device in mounted_devices.into_iter().chain(discovered_devices) {
        if seen.insert(device.path.to_string()) {
            devices.push(device);
        }
    }

    devices
}

fn filesystem_entry() -> DeviceEntry {
    device_entry(
        "Filesystem".into(),
        "/".into(),
        "/".into(),
        "/".into(),
        true,
        false,
    )
}

fn device_entry(
    label: String,
    path: String,
    device_path: String,
    marker: String,
    mounted: bool,
    can_eject: bool,
) -> DeviceEntry {
    DeviceEntry {
        label: label.into(),
        path: path.into(),
        device_path: device_path.into(),
        marker: marker.into(),
        mounted,
        can_eject,
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
        .map_err(|err| format!("UDisks2 Mount failed: {err}"))?;
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
        .map_err(|err| format!("UDisks2 Unmount failed: {err}"))?;
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
        .map_err(|err| format!("UDisks2 Eject failed: {err}"))?;
    Ok(())
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
        if bool_property(block, "HintIgnore") {
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
    if bool_property(block, "HintIgnore") {
        return None;
    }

    if !is_removable_media_object(objects, block) {
        return None;
    }
    let drive = drive_for_block(objects, block)?;

    let device = byte_string_property(block, "Device")?;
    let mount_points = interfaces
        .get("org.freedesktop.UDisks2.Filesystem")
        .and_then(|filesystem| mount_points_property(filesystem, "MountPoints"))
        .unwrap_or_default();
    let mounted = !mount_points.is_empty();
    let label = string_property(block, "IdLabel")
        .filter(|label| !label.is_empty())
        .or_else(|| drive_label(drive))
        .unwrap_or_else(|| mount_label(Path::new(&device)));
    let path = mount_points.into_iter().next().unwrap_or(device.clone());
    let can_eject = bool_property(drive, "Ejectable");
    Some(device_entry(
        label.clone(),
        path.clone(),
        device,
        marker_from_label(&label),
        mounted,
        can_eject,
    ))
}

fn is_removable_media_object(objects: &ManagedObjects, block: &Properties) -> bool {
    drive_for_block(objects, block).is_some_and(|drive| {
        bool_property(drive, "Removable") && bool_property(drive, "MediaAvailable")
    })
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
            None,
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
    fn udisks2_prefers_mount_point_for_mounted_media() {
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
        assert_eq!(devices[0].label, "Framework USB-C Storage");
        assert_eq!(devices[0].path, "/run/media/yk/FIKA USB");
        assert_eq!(devices[0].device_path, "/dev/sdb1");
        assert!(devices[0].mounted);
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
    fn device_merge_keeps_mountinfo_entry_before_udisks_duplicate() {
        let mounted = vec![
            filesystem_entry(),
            device_entry(
                "Mounted USB".into(),
                "/run/media/yk/USB".into(),
                "/run/media/yk/USB".into(),
                "M".into(),
                true,
                false,
            ),
        ];
        let discovered = vec![
            device_entry(
                "Duplicate".into(),
                "/run/media/yk/USB".into(),
                "/dev/sdb1".into(),
                "D".into(),
                true,
                true,
            ),
            device_entry(
                "Unmounted".into(),
                "/dev/sdc1".into(),
                "/dev/sdc1".into(),
                "U".into(),
                false,
                true,
            ),
        ];

        let devices = merge_device_entries(mounted, discovered);

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[1].label, "Mounted USB");
        assert_eq!(devices[2].label, "Unmounted");
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
        HashMap::from([
            ("Removable".to_string(), value(removable)),
            ("MediaAvailable".to_string(), value(media_available)),
            ("Vendor".to_string(), value(vendor.to_string())),
            ("Model".to_string(), value(model.to_string())),
        ])
    }

    fn block_object(device: &str, drive: &str, label: &str, hint_ignore: bool) -> Properties {
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
            ("HintIgnore".to_string(), value(hint_ignore)),
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
