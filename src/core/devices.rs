use super::bus::{BusCallTarget, BusController, BusError, BusKind};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const PROC_SELF_MOUNTINFO: &str = "/proc/self/mountinfo";
pub const UDISKS2_SERVICE: &str = "org.freedesktop.UDisks2";
pub const UDISKS2_OBJECT_MANAGER_PATH: &str = "/org/freedesktop/UDisks2";
pub const DBUS_OBJECT_MANAGER_INTERFACE: &str = "org.freedesktop.DBus.ObjectManager";
pub const UDISKS2_BLOCK_INTERFACE: &str = "org.freedesktop.UDisks2.Block";
pub const UDISKS2_DRIVE_INTERFACE: &str = "org.freedesktop.UDisks2.Drive";
pub const UDISKS2_FILESYSTEM_INTERFACE: &str = "org.freedesktop.UDisks2.Filesystem";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountInfoEntry {
    pub mount_id: u32,
    pub parent_id: u32,
    pub major_minor: String,
    pub root: PathBuf,
    pub mount_point: PathBuf,
    pub mount_options: String,
    pub optional_fields: Vec<String>,
    pub filesystem_type: String,
    pub source: String,
    pub super_options: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub device_path: PathBuf,
    pub mount_point: Option<PathBuf>,
    pub filesystem_type: Option<String>,
    pub label: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub removable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    Added(DeviceInfo),
    Removed(PathBuf),
    Changed(DeviceInfo),
}

#[derive(Debug)]
pub enum DeviceDiscoveryError {
    Bus(BusError),
    MountInfo(String),
    Udisks2(String),
}

impl fmt::Display for DeviceDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => write!(f, "{error}"),
            Self::MountInfo(message) => write!(f, "{message}"),
            Self::Udisks2(message) => write!(f, "{message}"),
        }
    }
}

impl Error for DeviceDiscoveryError {}

impl From<BusError> for DeviceDiscoveryError {
    fn from(error: BusError) -> Self {
        Self::Bus(error)
    }
}

pub type Udisks2PropertyMap = BTreeMap<String, OwnedValue>;
pub type Udisks2InterfaceMap = BTreeMap<String, Udisks2PropertyMap>;

#[derive(Debug)]
pub struct Udisks2RawObject {
    pub object_path: String,
    pub interfaces: Udisks2InterfaceMap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Udisks2BlockDevice {
    pub object_path: String,
    pub device_path: PathBuf,
    pub drive_path: Option<String>,
    pub id_label: Option<String>,
    pub id_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub mount_points: Vec<PathBuf>,
    pub hint_ignore: bool,
    pub hint_system: bool,
    pub removable: bool,
    pub ejectable: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Udisks2Snapshot {
    pub block_devices: Vec<Udisks2BlockDevice>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Udisks2DriveInfo {
    object_path: String,
    removable: bool,
    media_removable: bool,
    ejectable: bool,
    size_bytes: Option<u64>,
}

pub fn read_mountinfo_devices() -> Result<Vec<DeviceInfo>, String> {
    let contents = fs::read_to_string(PROC_SELF_MOUNTINFO)
        .map_err(|error| format!("failed to read {PROC_SELF_MOUNTINFO}: {error}"))?;
    devices_from_mountinfo(&contents)
}

pub async fn read_udisks2_devices() -> Result<Vec<DeviceInfo>, DeviceDiscoveryError> {
    read_udisks2_devices_with_bus(BusController::shared()).await
}

pub async fn read_udisks2_devices_with_bus(
    bus: &BusController,
) -> Result<Vec<DeviceInfo>, DeviceDiscoveryError> {
    let snapshot = read_udisks2_snapshot_with_bus(bus).await?;
    let contents = fs::read_to_string(PROC_SELF_MOUNTINFO).map_err(|error| {
        DeviceDiscoveryError::MountInfo(format!("failed to read {PROC_SELF_MOUNTINFO}: {error}"))
    })?;
    let mount_entries = parse_mountinfo(&contents).map_err(DeviceDiscoveryError::MountInfo)?;
    Ok(devices_from_udisks2_snapshot(&snapshot, &mount_entries))
}

pub async fn read_udisks2_snapshot_with_bus(
    bus: &BusController,
) -> Result<Udisks2Snapshot, DeviceDiscoveryError> {
    let target = BusCallTarget::new(
        BusKind::System,
        UDISKS2_SERVICE,
        UDISKS2_OBJECT_MANAGER_PATH,
        DBUS_OBJECT_MANAGER_INTERFACE,
        "GetManagedObjects",
    )?;
    let connection = bus.connection(BusKind::System).await?;
    let proxy = zbus::fdo::ObjectManagerProxy::new(
        &connection,
        UDISKS2_SERVICE,
        UDISKS2_OBJECT_MANAGER_PATH,
    )
    .await
    .map_err(|error| {
        DeviceDiscoveryError::Bus(BusError::Proxy {
            target: target.clone(),
            message: error.to_string(),
        })
    })?;
    let objects = bus
        .call_with_retry(&target, || async {
            proxy.get_managed_objects().await.map_err(zbus::Error::from)
        })
        .await?;
    udisks2_snapshot_from_managed_objects(&objects).map_err(DeviceDiscoveryError::Udisks2)
}

pub fn devices_from_mountinfo(contents: &str) -> Result<Vec<DeviceInfo>, String> {
    let entries = parse_mountinfo(contents)?;
    Ok(devices_from_mount_entries(&entries))
}

pub fn devices_from_mount_entries(entries: &[MountInfoEntry]) -> Vec<DeviceInfo> {
    let mut devices = BTreeMap::<PathBuf, DeviceInfo>::new();
    for entry in entries {
        if !is_device_mount(entry) {
            continue;
        }
        let device_path = PathBuf::from(&entry.source);
        let info = DeviceInfo {
            device_path: device_path.clone(),
            mount_point: Some(entry.mount_point.clone()),
            filesystem_type: Some(entry.filesystem_type.clone()),
            label: device_label(entry),
            capacity_bytes: None,
            removable: likely_removable(entry),
        };
        devices
            .entry(device_path)
            .and_modify(|existing| {
                if !existing.removable && info.removable {
                    *existing = info.clone();
                }
            })
            .or_insert(info);
    }
    devices.into_values().collect()
}

pub fn udisks2_snapshot_from_managed_objects(
    objects: &zbus::fdo::ManagedObjects,
) -> Result<Udisks2Snapshot, String> {
    let mut raw_objects = Vec::with_capacity(objects.len());
    for (object_path, interfaces) in objects {
        let mut raw_interfaces = Udisks2InterfaceMap::new();
        for (interface, properties) in interfaces {
            let mut raw_properties = Udisks2PropertyMap::new();
            for (name, value) in properties {
                raw_properties.insert(
                    name.clone(),
                    value.try_clone().map_err(|error| {
                        format!(
                            "failed to clone UDisks2 property {} on {}: {error}",
                            name, object_path
                        )
                    })?,
                );
            }
            raw_interfaces.insert(interface.as_str().to_string(), raw_properties);
        }
        raw_objects.push(Udisks2RawObject {
            object_path: object_path.to_string(),
            interfaces: raw_interfaces,
        });
    }
    Ok(udisks2_snapshot_from_raw_objects(&raw_objects))
}

pub fn udisks2_snapshot_from_raw_objects(objects: &[Udisks2RawObject]) -> Udisks2Snapshot {
    let drives = udisks2_drives_from_raw_objects(objects);
    let mut block_devices = Vec::new();
    for object in objects {
        let Some(block) = object.interfaces.get(UDISKS2_BLOCK_INTERFACE) else {
            continue;
        };
        let Some(device_path) = property_bytes_path(block, "PreferredDevice")
            .or_else(|| property_bytes_path(block, "Device"))
        else {
            continue;
        };
        let drive_path = property_object_path(block, "Drive");
        let drive = drive_path.as_ref().and_then(|path| drives.get(path));
        let filesystem = object.interfaces.get(UDISKS2_FILESYSTEM_INTERFACE);
        let size_bytes = property_owned::<u64>(block, "Size")
            .filter(|size| *size > 0)
            .or_else(|| drive.and_then(|info| info.size_bytes));
        let removable =
            drive.is_some_and(|info| info.removable || info.media_removable || info.ejectable);
        let ejectable = drive.is_some_and(|info| info.ejectable);
        block_devices.push(Udisks2BlockDevice {
            object_path: object.object_path.clone(),
            device_path,
            drive_path,
            id_label: property_string(block, "IdLabel"),
            id_type: property_string(block, "IdType"),
            size_bytes,
            mount_points: filesystem
                .map(|properties| property_byte_path_array(properties, "MountPoints"))
                .unwrap_or_default(),
            hint_ignore: property_owned::<bool>(block, "HintIgnore").unwrap_or(false),
            hint_system: property_owned::<bool>(block, "HintSystem").unwrap_or(false),
            removable,
            ejectable,
        });
    }
    block_devices.sort_by(|left, right| left.device_path.cmp(&right.device_path));
    Udisks2Snapshot { block_devices }
}

pub fn devices_from_udisks2_snapshot(
    snapshot: &Udisks2Snapshot,
    mount_entries: &[MountInfoEntry],
) -> Vec<DeviceInfo> {
    let mut mount_by_source = BTreeMap::<PathBuf, &MountInfoEntry>::new();
    for entry in mount_entries {
        if is_device_mount(entry) {
            mount_by_source.insert(PathBuf::from(&entry.source), entry);
        }
    }

    let mut devices = BTreeMap::<PathBuf, DeviceInfo>::new();
    let mut ignored_device_paths = BTreeSet::<PathBuf>::new();
    for block in &snapshot.block_devices {
        if block.hint_ignore {
            ignored_device_paths.insert(block.device_path.clone());
            continue;
        }
        let mount_entry = mount_by_source.get(&block.device_path).copied();
        let mount_point = block
            .mount_points
            .first()
            .cloned()
            .or_else(|| mount_entry.map(|entry| entry.mount_point.clone()));
        let filesystem_type = mount_entry
            .map(|entry| entry.filesystem_type.clone())
            .or_else(|| block.id_type.clone());
        let label = block
            .id_label
            .clone()
            .filter(|label| !label.trim().is_empty())
            .or_else(|| label_from_mount_point(mount_point.as_deref()))
            .or_else(|| label_from_device_path(&block.device_path));
        let removable = block.removable
            || block.ejectable
            || mount_point
                .as_deref()
                .is_some_and(mount_point_likely_removable);
        devices.insert(
            block.device_path.clone(),
            DeviceInfo {
                device_path: block.device_path.clone(),
                mount_point,
                filesystem_type,
                label,
                capacity_bytes: block.size_bytes,
                removable,
            },
        );
    }

    for info in devices_from_mount_entries(mount_entries) {
        if ignored_device_paths.contains(&info.device_path) {
            continue;
        }
        devices.entry(info.device_path.clone()).or_insert(info);
    }

    devices.into_values().collect()
}

pub fn device_events_between(previous: &[DeviceInfo], current: &[DeviceInfo]) -> Vec<DeviceEvent> {
    let previous = previous
        .iter()
        .map(|device| (device.device_path.clone(), device))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .map(|device| (device.device_path.clone(), device))
        .collect::<BTreeMap<_, _>>();
    let mut events = Vec::new();
    for device_path in previous.keys() {
        if !current.contains_key(device_path) {
            events.push(DeviceEvent::Removed(device_path.clone()));
        }
    }
    for (device_path, device) in current {
        match previous.get(&device_path) {
            None => events.push(DeviceEvent::Added(device.clone())),
            Some(previous) if **previous != *device => {
                events.push(DeviceEvent::Changed(device.clone()));
            }
            Some(_) => {}
        }
    }
    events
}

pub fn parse_mountinfo(contents: &str) -> Result<Vec<MountInfoEntry>, String> {
    let mut entries = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(parse_mountinfo_line(line, index + 1)?);
    }
    Ok(entries)
}

fn parse_mountinfo_line(line: &str, line_number: usize) -> Result<MountInfoEntry, String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let Some(separator) = fields.iter().position(|field| *field == "-") else {
        return Err(format!(
            "mountinfo line {line_number} is missing '-' separator"
        ));
    };
    if separator < 6 {
        return Err(format!(
            "mountinfo line {line_number} has too few pre-separator fields"
        ));
    }
    if fields.len() < separator + 4 {
        return Err(format!(
            "mountinfo line {line_number} has too few post-separator fields"
        ));
    }

    let mount_id = fields[0]
        .parse::<u32>()
        .map_err(|error| format!("mountinfo line {line_number} has invalid mount id: {error}"))?;
    let parent_id = fields[1]
        .parse::<u32>()
        .map_err(|error| format!("mountinfo line {line_number} has invalid parent id: {error}"))?;
    let major_minor = fields[2].to_string();
    if !major_minor.contains(':') {
        return Err(format!(
            "mountinfo line {line_number} has invalid major:minor field"
        ));
    }

    Ok(MountInfoEntry {
        mount_id,
        parent_id,
        major_minor,
        root: PathBuf::from(decode_mountinfo_field(fields[3], line_number)?),
        mount_point: PathBuf::from(decode_mountinfo_field(fields[4], line_number)?),
        mount_options: fields[5].to_string(),
        optional_fields: fields[6..separator]
            .iter()
            .map(|field| (*field).to_string())
            .collect(),
        filesystem_type: decode_mountinfo_field(fields[separator + 1], line_number)?,
        source: decode_mountinfo_field(fields[separator + 2], line_number)?,
        super_options: fields[separator + 3..].join(" "),
    })
}

fn decode_mountinfo_field(value: &str, line_number: usize) -> Result<String, String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let Some(escape) = bytes.get(index + 1..index + 4) else {
            return Err(format!("mountinfo line {line_number} has truncated escape"));
        };
        if !escape.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
            return Err(format!("mountinfo line {line_number} has invalid escape"));
        }
        let value = (escape[0] - b'0') * 64 + (escape[1] - b'0') * 8 + (escape[2] - b'0');
        decoded.push(value);
        index += 4;
    }
    String::from_utf8(decoded)
        .map_err(|error| format!("mountinfo line {line_number} is not valid UTF-8: {error}"))
}

fn is_device_mount(entry: &MountInfoEntry) -> bool {
    entry.source.starts_with("/dev/")
        && !is_pseudo_filesystem(&entry.filesystem_type)
        && !entry.source.starts_with("/dev/loop")
}

fn is_pseudo_filesystem(filesystem_type: &str) -> bool {
    matches!(
        filesystem_type,
        "autofs"
            | "binfmt_misc"
            | "bpf"
            | "cgroup"
            | "cgroup2"
            | "configfs"
            | "debugfs"
            | "devpts"
            | "devtmpfs"
            | "efivarfs"
            | "fusectl"
            | "hugetlbfs"
            | "mqueue"
            | "nsfs"
            | "overlay"
            | "proc"
            | "pstore"
            | "ramfs"
            | "rpc_pipefs"
            | "securityfs"
            | "squashfs"
            | "sysfs"
            | "tmpfs"
            | "tracefs"
    )
}

fn device_label(entry: &MountInfoEntry) -> Option<String> {
    entry
        .mount_point
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            Path::new(&entry.source)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
        })
        .map(ToOwned::to_owned)
}

fn likely_removable(entry: &MountInfoEntry) -> bool {
    mount_point_likely_removable(&entry.mount_point)
}

fn mount_point_likely_removable(mount_point: &Path) -> bool {
    mount_point.starts_with("/media")
        || mount_point.starts_with("/run/media")
        || mount_point.starts_with("/Volumes")
        || mount_point.starts_with("/mnt")
}

fn udisks2_drives_from_raw_objects(
    objects: &[Udisks2RawObject],
) -> BTreeMap<String, Udisks2DriveInfo> {
    let mut drives = BTreeMap::new();
    for object in objects {
        let Some(properties) = object.interfaces.get(UDISKS2_DRIVE_INTERFACE) else {
            continue;
        };
        drives.insert(
            object.object_path.clone(),
            Udisks2DriveInfo {
                object_path: object.object_path.clone(),
                removable: property_owned::<bool>(properties, "Removable").unwrap_or(false),
                media_removable: property_owned::<bool>(properties, "MediaRemovable")
                    .unwrap_or(false),
                ejectable: property_owned::<bool>(properties, "Ejectable").unwrap_or(false),
                size_bytes: property_owned::<u64>(properties, "Size").filter(|size| *size > 0),
            },
        );
    }
    drives
}

fn property_owned<T>(properties: &Udisks2PropertyMap, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    properties.get(name)?.try_clone().ok()?.try_into().ok()
}

fn property_string(properties: &Udisks2PropertyMap, name: &str) -> Option<String> {
    property_owned::<String>(properties, name)
        .map(|value| value.trim_matches('\0').trim().to_string())
        .filter(|value| !value.is_empty())
}

fn property_object_path(properties: &Udisks2PropertyMap, name: &str) -> Option<String> {
    let path = property_owned::<OwnedObjectPath>(properties, name)?;
    let path = path.to_string();
    (path != "/").then_some(path)
}

fn property_bytes_path(properties: &Udisks2PropertyMap, name: &str) -> Option<PathBuf> {
    bytes_to_path(&property_owned::<Vec<u8>>(properties, name)?)
}

fn property_byte_path_array(properties: &Udisks2PropertyMap, name: &str) -> Vec<PathBuf> {
    property_owned::<Vec<Vec<u8>>>(properties, name)
        .unwrap_or_default()
        .iter()
        .filter_map(|bytes| bytes_to_path(bytes))
        .collect()
}

fn bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    if bytes.is_empty() {
        return None;
    }
    String::from_utf8(bytes.to_vec())
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn label_from_mount_point(mount_point: Option<&Path>) -> Option<String> {
    mount_point
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn label_from_device_path(device_path: &Path) -> Option<String> {
    device_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{ObjectPath, Value};

    fn owned_value<T>(value: T) -> OwnedValue
    where
        Value<'static>: From<T>,
    {
        OwnedValue::try_from(Value::from(value)).unwrap()
    }

    fn object_path_value(path: &'static str) -> OwnedValue {
        OwnedValue::from(ObjectPath::try_from(path).unwrap())
    }

    fn path_bytes(path: &str) -> Vec<u8> {
        let mut bytes = path.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    fn properties(values: &[(&str, OwnedValue)]) -> Udisks2PropertyMap {
        values
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.try_clone().unwrap()))
            .collect()
    }

    fn raw_object(path: &str, interfaces: &[(&str, Udisks2PropertyMap)]) -> Udisks2RawObject {
        Udisks2RawObject {
            object_path: path.to_string(),
            interfaces: interfaces
                .iter()
                .map(|(name, properties)| {
                    let properties = properties
                        .iter()
                        .map(|(key, value)| (key.clone(), value.try_clone().unwrap()))
                        .collect();
                    ((*name).to_string(), properties)
                })
                .collect(),
        }
    }

    #[test]
    fn parse_mountinfo_decodes_escaped_mount_point() {
        let entries = parse_mountinfo(
            "38 24 8:17 / /run/media/yk/My\\040USB rw,nosuid,nodev,relatime shared:12 - vfat /dev/sdb1 rw,uid=1000\n",
        )
        .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].mount_id, 38);
        assert_eq!(entries[0].parent_id, 24);
        assert_eq!(entries[0].major_minor, "8:17");
        assert_eq!(
            entries[0].mount_point,
            PathBuf::from("/run/media/yk/My USB")
        );
        assert_eq!(entries[0].filesystem_type, "vfat");
        assert_eq!(entries[0].source, "/dev/sdb1");
        assert_eq!(entries[0].optional_fields, vec!["shared:12"]);
    }

    #[test]
    fn devices_from_mountinfo_filters_pseudo_filesystems() {
        let devices = devices_from_mountinfo(
            "1 0 0:22 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw\n\
             2 0 0:45 / /tmp rw,nosuid,nodev - tmpfs tmpfs rw,size=1024k\n\
             3 0 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n",
        )
        .unwrap();

        assert_eq!(
            devices,
            vec![DeviceInfo {
                device_path: PathBuf::from("/dev/sda1"),
                mount_point: Some(PathBuf::from("/")),
                filesystem_type: Some("ext4".to_string()),
                label: Some("sda1".to_string()),
                capacity_bytes: None,
                removable: false,
            }]
        );
    }

    #[test]
    fn devices_from_mountinfo_marks_media_mounts_removable() {
        let devices = devices_from_mountinfo(
            "38 24 8:17 / /run/media/yk/My\\040USB rw,nosuid,nodev,relatime - exfat /dev/sdb1 rw\n",
        )
        .unwrap();

        assert_eq!(
            devices,
            vec![DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/My USB")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("My USB".to_string()),
                capacity_bytes: None,
                removable: true,
            }]
        );
    }

    #[test]
    fn parse_mountinfo_rejects_invalid_escape() {
        let error =
            parse_mountinfo("38 24 8:17 / /run/media/yk/Bad\\x20Name rw - vfat /dev/sdb1 rw\n")
                .unwrap_err();

        assert!(error.contains("invalid escape"));
    }

    #[test]
    fn udisks2_snapshot_merges_block_drive_filesystem_and_mountinfo() {
        let objects = vec![
            raw_object(
                "/org/freedesktop/UDisks2/drives/USB",
                &[(
                    UDISKS2_DRIVE_INTERFACE,
                    properties(&[
                        ("Removable", owned_value(false)),
                        ("MediaRemovable", owned_value(true)),
                        ("Ejectable", owned_value(true)),
                        ("Size", owned_value(128_u64)),
                    ]),
                )],
            ),
            raw_object(
                "/org/freedesktop/UDisks2/block_devices/sdb1",
                &[
                    (
                        UDISKS2_BLOCK_INTERFACE,
                        properties(&[
                            ("PreferredDevice", owned_value(path_bytes("/dev/sdb1"))),
                            ("Device", owned_value(path_bytes("/dev/sdb1"))),
                            (
                                "Drive",
                                object_path_value("/org/freedesktop/UDisks2/drives/USB"),
                            ),
                            ("IdLabel", owned_value("Backup")),
                            ("IdType", owned_value("exfat")),
                            ("Size", owned_value(64_u64)),
                            ("HintIgnore", owned_value(false)),
                            ("HintSystem", owned_value(false)),
                        ]),
                    ),
                    (
                        UDISKS2_FILESYSTEM_INTERFACE,
                        properties(&[(
                            "MountPoints",
                            owned_value(vec![path_bytes("/run/media/yk/Backup")]),
                        )]),
                    ),
                ],
            ),
        ];
        let snapshot = udisks2_snapshot_from_raw_objects(&objects);
        let mount_entries = parse_mountinfo(
            "38 24 8:17 / /run/media/yk/Backup rw,nosuid,nodev,relatime - exfat /dev/sdb1 rw\n",
        )
        .unwrap();

        assert_eq!(
            snapshot.block_devices,
            vec![Udisks2BlockDevice {
                object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                device_path: PathBuf::from("/dev/sdb1"),
                drive_path: Some("/org/freedesktop/UDisks2/drives/USB".to_string()),
                id_label: Some("Backup".to_string()),
                id_type: Some("exfat".to_string()),
                size_bytes: Some(64),
                mount_points: vec![PathBuf::from("/run/media/yk/Backup")],
                hint_ignore: false,
                hint_system: false,
                removable: true,
                ejectable: true,
            }]
        );
        assert_eq!(
            devices_from_udisks2_snapshot(&snapshot, &mount_entries),
            vec![DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Backup")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Backup".to_string()),
                capacity_bytes: Some(64),
                removable: true,
            }]
        );
    }

    #[test]
    fn udisks2_snapshot_skips_hint_ignore_and_keeps_mountinfo_fallback() {
        let objects = vec![raw_object(
            "/org/freedesktop/UDisks2/block_devices/sdc1",
            &[(
                UDISKS2_BLOCK_INTERFACE,
                properties(&[
                    ("PreferredDevice", owned_value(path_bytes("/dev/sdc1"))),
                    ("IdType", owned_value("vfat")),
                    ("HintIgnore", owned_value(true)),
                ]),
            )],
        )];
        let snapshot = udisks2_snapshot_from_raw_objects(&objects);
        let mount_entries = parse_mountinfo(
            "38 24 8:33 / /run/media/yk/CAMERA rw,nosuid,nodev,relatime - vfat /dev/sdc1 rw\n\
             39 24 8:49 / /run/media/yk/USB rw,nosuid,nodev,relatime - exfat /dev/sdd1 rw\n",
        )
        .unwrap();

        assert_eq!(
            devices_from_udisks2_snapshot(&snapshot, &mount_entries),
            vec![DeviceInfo {
                device_path: PathBuf::from("/dev/sdd1"),
                mount_point: Some(PathBuf::from("/run/media/yk/USB")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("USB".to_string()),
                capacity_bytes: None,
                removable: true,
            }]
        );
    }

    #[test]
    fn device_events_report_added_removed_and_changed_devices() {
        let previous = vec![
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Old")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Old".to_string()),
                capacity_bytes: Some(64),
                removable: true,
            },
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdc1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Removed")),
                filesystem_type: Some("vfat".to_string()),
                label: Some("Removed".to_string()),
                capacity_bytes: None,
                removable: true,
            },
        ];
        let current = vec![
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/New")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("New".to_string()),
                capacity_bytes: Some(64),
                removable: true,
            },
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdd1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Added")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Added".to_string()),
                capacity_bytes: Some(128),
                removable: true,
            },
        ];

        assert_eq!(
            device_events_between(&previous, &current),
            vec![
                DeviceEvent::Removed(PathBuf::from("/dev/sdc1")),
                DeviceEvent::Changed(current[0].clone()),
                DeviceEvent::Added(current[1].clone()),
            ]
        );
    }
}
