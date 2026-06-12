use super::bus::{BusCallTarget, BusController, BusError, BusKind};
use futures_lite::StreamExt;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use zbus::message::Type as DbusMessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{MatchRule, MessageStream};

mod actions;

pub use actions::{
    DevicePlaceOperation, DevicePlaceOperationResult, perform_device_place_operation,
};

pub const PROC_SELF_MOUNTINFO: &str = "/proc/self/mountinfo";
pub const UDISKS2_SERVICE: &str = "org.freedesktop.UDisks2";
pub const UDISKS2_OBJECT_MANAGER_PATH: &str = "/org/freedesktop/UDisks2";
pub const DBUS_OBJECT_MANAGER_INTERFACE: &str = "org.freedesktop.DBus.ObjectManager";
pub const UDISKS2_BLOCK_INTERFACE: &str = "org.freedesktop.UDisks2.Block";
pub const UDISKS2_DRIVE_INTERFACE: &str = "org.freedesktop.UDisks2.Drive";
pub const UDISKS2_FILESYSTEM_INTERFACE: &str = "org.freedesktop.UDisks2.Filesystem";
pub const UDISKS2_FILESYSTEM_MOUNT_METHOD: &str = "Mount";
pub const UDISKS2_FILESYSTEM_UNMOUNT_METHOD: &str = "Unmount";
pub const UDISKS2_DRIVE_EJECT_METHOD: &str = "Eject";
pub const UDISKS2_DRIVE_POWER_OFF_METHOD: &str = "PowerOff";

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
    pub ejectable: bool,
    pub can_power_off: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    Added(DeviceInfo),
    Removed(PathBuf),
    Changed(DeviceInfo),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceMonitorMessage {
    Snapshot(Vec<DeviceInfo>),
    Events {
        events: Vec<DeviceEvent>,
        devices: Vec<DeviceInfo>,
    },
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

#[derive(Debug)]
pub enum DeviceActionError {
    Discovery(DeviceDiscoveryError),
    Bus(BusError),
    DeviceNotFound(PathBuf),
    NotMounted(PathBuf),
    MissingDrive(PathBuf),
    CannotEject(PathBuf),
    CannotPowerOff(PathBuf),
}

impl fmt::Display for DeviceActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery(error) => write!(f, "{error}"),
            Self::Bus(error) => write!(f, "{error}"),
            Self::DeviceNotFound(path) => {
                write!(f, "no UDisks2 filesystem device matches {}", path.display())
            }
            Self::NotMounted(path) => write!(f, "device is not mounted: {}", path.display()),
            Self::MissingDrive(path) => {
                write!(f, "device has no UDisks2 drive object: {}", path.display())
            }
            Self::CannotEject(path) => write!(f, "device cannot be ejected: {}", path.display()),
            Self::CannotPowerOff(path) => {
                write!(f, "device cannot be safely removed: {}", path.display())
            }
        }
    }
}

impl Error for DeviceActionError {}

impl From<DeviceDiscoveryError> for DeviceActionError {
    fn from(error: DeviceDiscoveryError) -> Self {
        Self::Discovery(error)
    }
}

impl From<BusError> for DeviceActionError {
    fn from(error: BusError) -> Self {
        Self::Bus(error)
    }
}

pub type Udisks2PropertyMap = BTreeMap<String, OwnedValue>;
pub type Udisks2InterfaceMap = BTreeMap<String, Udisks2PropertyMap>;
type Udisks2ObjectMap = BTreeMap<String, Udisks2InterfaceMap>;

#[derive(Debug)]
pub struct Udisks2RawObject {
    pub object_path: String,
    pub interfaces: Udisks2InterfaceMap,
}

#[derive(Debug)]
pub enum Udisks2Signal {
    InterfacesAdded {
        object_path: String,
        interfaces: Udisks2InterfaceMap,
    },
    InterfacesRemoved {
        object_path: String,
        interfaces: Vec<String>,
    },
    PropertiesChanged {
        object_path: String,
        interface: String,
        changed: Udisks2PropertyMap,
        invalidated: Vec<String>,
    },
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
    pub has_filesystem: bool,
    pub hint_ignore: bool,
    pub hint_system: bool,
    pub removable: bool,
    pub ejectable: bool,
    pub can_power_off: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Udisks2Snapshot {
    pub block_devices: Vec<Udisks2BlockDevice>,
}

#[derive(Debug)]
pub struct Udisks2MonitorState {
    raw_objects: Udisks2ObjectMap,
    snapshot: Udisks2Snapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Udisks2DeviceActionTarget {
    pub device_path: PathBuf,
    pub mount_point: Option<PathBuf>,
    pub block_object_path: String,
    pub drive_object_path: Option<String>,
    pub ejectable: bool,
    pub can_power_off: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Udisks2MountResult {
    pub target: Udisks2DeviceActionTarget,
    pub mount_point: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Udisks2DriveInfo {
    object_path: String,
    removable: bool,
    media_removable: bool,
    ejectable: bool,
    can_power_off: bool,
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

pub async fn watch_udisks2_devices(
    sender: Sender<DeviceMonitorMessage>,
) -> Result<(), DeviceDiscoveryError> {
    watch_udisks2_devices_with_bus(BusController::shared(), sender).await
}

pub async fn watch_udisks2_devices_with_bus(
    bus: &BusController,
    sender: Sender<DeviceMonitorMessage>,
) -> Result<(), DeviceDiscoveryError> {
    let mut state = read_udisks2_monitor_state_with_bus(bus).await?;
    let mut mount_entries = read_current_mount_entries()?;
    let initial_devices = devices_from_udisks2_snapshot(state.snapshot(), &mount_entries);
    if sender
        .send(DeviceMonitorMessage::Snapshot(initial_devices))
        .is_err()
    {
        return Ok(());
    }

    let connection = bus.connection(BusKind::System).await?;
    let rule = MatchRule::builder()
        .msg_type(DbusMessageType::Signal)
        .path_namespace(UDISKS2_OBJECT_MANAGER_PATH)
        .map_err(|error| DeviceDiscoveryError::Udisks2(error.to_string()))?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &connection, Some(256))
        .await
        .map_err(|error| {
            DeviceDiscoveryError::Udisks2(format!(
                "failed to subscribe to UDisks2 signals: {error}"
            ))
        })?;

    while let Some(message) = stream.next().await {
        let message = message.map_err(|error| {
            DeviceDiscoveryError::Udisks2(format!("failed to receive UDisks2 signal: {error}"))
        })?;
        let Some(signal) = udisks2_signal_from_message(&message)? else {
            continue;
        };
        mount_entries = read_current_mount_entries()?;
        let events = device_events_for_udisks2_signal(&mut state, signal, &mount_entries)?;
        if events.is_empty() {
            continue;
        }
        let devices = devices_from_udisks2_snapshot(state.snapshot(), &mount_entries);
        if sender
            .send(DeviceMonitorMessage::Events { events, devices })
            .is_err()
        {
            return Ok(());
        }
    }
    Ok(())
}

pub async fn read_udisks2_devices_with_bus(
    bus: &BusController,
) -> Result<Vec<DeviceInfo>, DeviceDiscoveryError> {
    let snapshot = read_udisks2_snapshot_with_bus(bus).await?;
    let mount_entries = read_current_mount_entries()?;
    Ok(devices_from_udisks2_snapshot(&snapshot, &mount_entries))
}

pub async fn mount_udisks2_device(path: &Path) -> Result<Udisks2MountResult, DeviceActionError> {
    mount_udisks2_device_with_bus(BusController::shared(), path).await
}

pub async fn mount_udisks2_device_with_bus(
    bus: &BusController,
    path: &Path,
) -> Result<Udisks2MountResult, DeviceActionError> {
    let target = resolve_udisks2_device_action_target_with_bus(bus, path).await?;
    let bus_target = BusCallTarget::new(
        BusKind::System,
        UDISKS2_SERVICE,
        target.block_object_path.clone(),
        UDISKS2_FILESYSTEM_INTERFACE,
        UDISKS2_FILESYSTEM_MOUNT_METHOD,
    )?;
    let proxy = bus.proxy(&bus_target).await?;
    let method = bus_target.method().to_string();
    let mount_point = bus
        .call_with_retry(&bus_target, || {
            let proxy = &proxy;
            let method = method.clone();
            async move {
                let options = HashMap::<String, OwnedValue>::new();
                proxy.call::<_, _, String>(method.as_str(), &options).await
            }
        })
        .await?;
    Ok(Udisks2MountResult {
        target,
        mount_point: PathBuf::from(mount_point),
    })
}

pub async fn unmount_udisks2_device(path: &Path) -> Result<(), DeviceActionError> {
    unmount_udisks2_device_with_bus(BusController::shared(), path).await
}

pub async fn unmount_udisks2_device_with_bus(
    bus: &BusController,
    path: &Path,
) -> Result<(), DeviceActionError> {
    let target = resolve_udisks2_device_action_target_with_bus(bus, path).await?;
    if target.mount_point.is_none() {
        return Err(DeviceActionError::NotMounted(path.to_path_buf()));
    }
    call_udisks2_void_action(
        bus,
        &target.block_object_path,
        UDISKS2_FILESYSTEM_INTERFACE,
        UDISKS2_FILESYSTEM_UNMOUNT_METHOD,
    )
    .await
}

pub async fn eject_udisks2_device(path: &Path) -> Result<(), DeviceActionError> {
    eject_udisks2_device_with_bus(BusController::shared(), path).await
}

pub async fn eject_udisks2_device_with_bus(
    bus: &BusController,
    path: &Path,
) -> Result<(), DeviceActionError> {
    let target = resolve_udisks2_device_action_target_with_bus(bus, path).await?;
    if !target.ejectable {
        return Err(DeviceActionError::CannotEject(path.to_path_buf()));
    }
    let drive_object_path = target
        .drive_object_path
        .ok_or_else(|| DeviceActionError::MissingDrive(path.to_path_buf()))?;
    call_udisks2_void_action(
        bus,
        &drive_object_path,
        UDISKS2_DRIVE_INTERFACE,
        UDISKS2_DRIVE_EJECT_METHOD,
    )
    .await
}

pub async fn safely_remove_udisks2_device(path: &Path) -> Result<(), DeviceActionError> {
    safely_remove_udisks2_device_with_bus(BusController::shared(), path).await
}

pub async fn safely_remove_udisks2_device_with_bus(
    bus: &BusController,
    path: &Path,
) -> Result<(), DeviceActionError> {
    let target = resolve_udisks2_device_action_target_with_bus(bus, path).await?;
    if !target.can_power_off {
        return Err(DeviceActionError::CannotPowerOff(path.to_path_buf()));
    }
    let drive_object_path = target
        .drive_object_path
        .as_deref()
        .ok_or_else(|| DeviceActionError::MissingDrive(path.to_path_buf()))?;
    if target.mount_point.is_some() {
        call_udisks2_void_action(
            bus,
            &target.block_object_path,
            UDISKS2_FILESYSTEM_INTERFACE,
            UDISKS2_FILESYSTEM_UNMOUNT_METHOD,
        )
        .await?;
    }
    call_udisks2_void_action(
        bus,
        drive_object_path,
        UDISKS2_DRIVE_INTERFACE,
        UDISKS2_DRIVE_POWER_OFF_METHOD,
    )
    .await
}

async fn call_udisks2_void_action(
    bus: &BusController,
    object_path: &str,
    interface: &str,
    method_name: &str,
) -> Result<(), DeviceActionError> {
    let target = BusCallTarget::new(
        BusKind::System,
        UDISKS2_SERVICE,
        object_path,
        interface,
        method_name,
    )?;
    let proxy = bus.proxy(&target).await?;
    let method = target.method().to_string();
    bus.call_with_retry(&target, || {
        let proxy = &proxy;
        let method = method.clone();
        async move {
            let options = HashMap::<String, OwnedValue>::new();
            proxy.call::<_, _, ()>(method.as_str(), &options).await
        }
    })
    .await?;
    Ok(())
}

pub async fn resolve_udisks2_device_action_target_with_bus(
    bus: &BusController,
    path: &Path,
) -> Result<Udisks2DeviceActionTarget, DeviceActionError> {
    let snapshot = read_udisks2_snapshot_with_bus(bus).await?;
    let mount_entries = read_current_mount_entries()?;
    resolve_udisks2_device_action_target(&snapshot, &mount_entries, path)
        .ok_or_else(|| DeviceActionError::DeviceNotFound(path.to_path_buf()))
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
    let proxy = bus.proxy(&target).await?;
    let method = target.method().to_string();
    let objects = bus
        .call_with_retry(&target, || {
            let proxy = &proxy;
            let method = method.clone();
            async move {
                proxy
                    .call::<_, _, zbus::fdo::ManagedObjects>(method.as_str(), &())
                    .await
            }
        })
        .await?;
    udisks2_snapshot_from_managed_objects(&objects).map_err(DeviceDiscoveryError::Udisks2)
}

pub async fn read_udisks2_monitor_state_with_bus(
    bus: &BusController,
) -> Result<Udisks2MonitorState, DeviceDiscoveryError> {
    let target = BusCallTarget::new(
        BusKind::System,
        UDISKS2_SERVICE,
        UDISKS2_OBJECT_MANAGER_PATH,
        DBUS_OBJECT_MANAGER_INTERFACE,
        "GetManagedObjects",
    )?;
    let proxy = bus.proxy(&target).await?;
    let method = target.method().to_string();
    let objects = bus
        .call_with_retry(&target, || {
            let proxy = &proxy;
            let method = method.clone();
            async move {
                proxy
                    .call::<_, _, zbus::fdo::ManagedObjects>(method.as_str(), &())
                    .await
            }
        })
        .await?;
    udisks2_monitor_state_from_managed_objects(&objects).map_err(DeviceDiscoveryError::Udisks2)
}

pub fn udisks2_monitor_state_from_managed_objects(
    objects: &zbus::fdo::ManagedObjects,
) -> Result<Udisks2MonitorState, String> {
    Udisks2MonitorState::from_raw_objects(udisks2_raw_objects_from_managed_objects(objects)?)
}

pub fn udisks2_signal_from_message(
    message: &zbus::Message,
) -> Result<Option<Udisks2Signal>, DeviceDiscoveryError> {
    let header = message.header();
    let interface = header.interface().map(ToString::to_string);
    let member = header.member().map(ToString::to_string);
    match (interface.as_deref(), member.as_deref()) {
        (Some(DBUS_OBJECT_MANAGER_INTERFACE), Some("InterfacesAdded")) => {
            let (object_path, interfaces): (
                OwnedObjectPath,
                HashMap<zbus::names::OwnedInterfaceName, HashMap<String, OwnedValue>>,
            ) = message.body().deserialize().map_err(|error| {
                DeviceDiscoveryError::Udisks2(format!("invalid InterfacesAdded payload: {error}"))
            })?;
            Ok(Some(Udisks2Signal::InterfacesAdded {
                object_path: object_path.to_string(),
                interfaces: owned_interfaces_to_udisks2_map(interfaces),
            }))
        }
        (Some(DBUS_OBJECT_MANAGER_INTERFACE), Some("InterfacesRemoved")) => {
            let (object_path, interfaces): (OwnedObjectPath, Vec<zbus::names::OwnedInterfaceName>) =
                message.body().deserialize().map_err(|error| {
                    DeviceDiscoveryError::Udisks2(format!(
                        "invalid InterfacesRemoved payload: {error}"
                    ))
                })?;
            Ok(Some(Udisks2Signal::InterfacesRemoved {
                object_path: object_path.to_string(),
                interfaces: interfaces
                    .into_iter()
                    .map(|interface| interface.to_string())
                    .collect(),
            }))
        }
        (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
            let object_path = header.path().map(ToString::to_string).ok_or_else(|| {
                DeviceDiscoveryError::Udisks2(
                    "PropertiesChanged signal missing object path".to_string(),
                )
            })?;
            let (interface, changed, invalidated): (
                zbus::names::OwnedInterfaceName,
                HashMap<String, OwnedValue>,
                Vec<String>,
            ) = message.body().deserialize().map_err(|error| {
                DeviceDiscoveryError::Udisks2(format!("invalid PropertiesChanged payload: {error}"))
            })?;
            Ok(Some(Udisks2Signal::PropertiesChanged {
                object_path,
                interface: interface.to_string(),
                changed: changed.into_iter().collect(),
                invalidated,
            }))
        }
        _ => Ok(None),
    }
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
            ejectable: false,
            can_power_off: false,
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
    Ok(
        Udisks2MonitorState::from_raw_objects(udisks2_raw_objects_from_managed_objects(objects)?)?
            .into_snapshot(),
    )
}

pub fn udisks2_raw_objects_from_managed_objects(
    objects: &zbus::fdo::ManagedObjects,
) -> Result<Vec<Udisks2RawObject>, String> {
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
    Ok(raw_objects)
}

fn read_current_mount_entries() -> Result<Vec<MountInfoEntry>, DeviceDiscoveryError> {
    let contents = fs::read_to_string(PROC_SELF_MOUNTINFO).map_err(|error| {
        DeviceDiscoveryError::MountInfo(format!("failed to read {PROC_SELF_MOUNTINFO}: {error}"))
    })?;
    parse_mountinfo(&contents).map_err(DeviceDiscoveryError::MountInfo)
}

fn owned_interfaces_to_udisks2_map(
    interfaces: HashMap<zbus::names::OwnedInterfaceName, HashMap<String, OwnedValue>>,
) -> Udisks2InterfaceMap {
    interfaces
        .into_iter()
        .map(|(interface, properties)| (interface.to_string(), properties.into_iter().collect()))
        .collect()
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
        let removable = drive.is_some_and(|info| {
            info.removable || info.media_removable || info.ejectable || info.can_power_off
        });
        let ejectable = drive.is_some_and(|info| info.ejectable);
        let can_power_off = drive.is_some_and(|info| info.can_power_off);
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
            has_filesystem: filesystem.is_some(),
            hint_ignore: property_owned::<bool>(block, "HintIgnore").unwrap_or(false),
            hint_system: property_owned::<bool>(block, "HintSystem").unwrap_or(false),
            removable,
            ejectable,
            can_power_off,
        });
    }
    block_devices.sort_by(|left, right| left.device_path.cmp(&right.device_path));
    Udisks2Snapshot { block_devices }
}

impl Udisks2MonitorState {
    pub fn from_raw_objects(objects: Vec<Udisks2RawObject>) -> Result<Self, String> {
        let mut state = Self {
            raw_objects: objects
                .into_iter()
                .map(|object| (object.object_path, object.interfaces))
                .collect(),
            snapshot: Udisks2Snapshot::default(),
        };
        state.refresh_snapshot()?;
        Ok(state)
    }

    pub fn snapshot(&self) -> &Udisks2Snapshot {
        &self.snapshot
    }

    pub fn into_snapshot(self) -> Udisks2Snapshot {
        self.snapshot
    }

    pub fn apply_signal(&mut self, signal: Udisks2Signal) -> Result<(), String> {
        match signal {
            Udisks2Signal::InterfacesAdded {
                object_path,
                interfaces,
            } => {
                let object = self.raw_objects.entry(object_path).or_default();
                for (interface, properties) in interfaces {
                    object.insert(interface, properties);
                }
            }
            Udisks2Signal::InterfacesRemoved {
                object_path,
                interfaces,
            } => {
                let remove_object = if let Some(object) = self.raw_objects.get_mut(&object_path) {
                    for interface in interfaces {
                        object.remove(&interface);
                    }
                    object.is_empty()
                } else {
                    false
                };
                if remove_object {
                    self.raw_objects.remove(&object_path);
                }
            }
            Udisks2Signal::PropertiesChanged {
                object_path,
                interface,
                changed,
                invalidated,
            } => {
                let properties = self
                    .raw_objects
                    .entry(object_path)
                    .or_default()
                    .entry(interface)
                    .or_default();
                for name in invalidated {
                    properties.remove(&name);
                }
                for (name, value) in changed {
                    properties.insert(name, value);
                }
            }
        }
        self.refresh_snapshot()
    }

    fn refresh_snapshot(&mut self) -> Result<(), String> {
        self.snapshot = udisks2_snapshot_from_raw_objects(&self.raw_objects_vec()?);
        Ok(())
    }

    fn raw_objects_vec(&self) -> Result<Vec<Udisks2RawObject>, String> {
        raw_objects_from_map(&self.raw_objects)
    }
}

pub fn device_events_for_udisks2_signal(
    state: &mut Udisks2MonitorState,
    signal: Udisks2Signal,
    mount_entries: &[MountInfoEntry],
) -> Result<Vec<DeviceEvent>, DeviceDiscoveryError> {
    let previous = devices_from_udisks2_snapshot(state.snapshot(), mount_entries);
    state
        .apply_signal(signal)
        .map_err(DeviceDiscoveryError::Udisks2)?;
    let current = devices_from_udisks2_snapshot(state.snapshot(), mount_entries);
    Ok(device_events_between(&previous, &current))
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
            || block.can_power_off
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
                ejectable: block.ejectable,
                can_power_off: block.can_power_off,
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

pub fn udisks2_device_action_targets(
    snapshot: &Udisks2Snapshot,
    mount_entries: &[MountInfoEntry],
) -> Vec<Udisks2DeviceActionTarget> {
    let mount_by_source = mount_entries
        .iter()
        .filter(|entry| is_device_mount(entry))
        .map(|entry| (PathBuf::from(&entry.source), entry))
        .collect::<BTreeMap<_, _>>();

    snapshot
        .block_devices
        .iter()
        .filter(|block| block.has_filesystem && !block.hint_ignore)
        .map(|block| {
            let mount_point = block.mount_points.first().cloned().or_else(|| {
                mount_by_source
                    .get(&block.device_path)
                    .map(|entry| entry.mount_point.clone())
            });
            Udisks2DeviceActionTarget {
                device_path: block.device_path.clone(),
                mount_point,
                block_object_path: block.object_path.clone(),
                drive_object_path: block.drive_path.clone(),
                ejectable: block.ejectable,
                can_power_off: block.can_power_off,
            }
        })
        .collect()
}

pub fn resolve_udisks2_device_action_target(
    snapshot: &Udisks2Snapshot,
    mount_entries: &[MountInfoEntry],
    path: &Path,
) -> Option<Udisks2DeviceActionTarget> {
    udisks2_device_action_targets(snapshot, mount_entries)
        .into_iter()
        .find(|target| {
            target.device_path == path
                || target
                    .mount_point
                    .as_deref()
                    .is_some_and(|mount_point| mount_point == path)
        })
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
                can_power_off: property_owned::<bool>(properties, "CanPowerOff").unwrap_or(false),
                size_bytes: property_owned::<u64>(properties, "Size").filter(|size| *size > 0),
            },
        );
    }
    drives
}

fn raw_objects_from_map(objects: &Udisks2ObjectMap) -> Result<Vec<Udisks2RawObject>, String> {
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
                            "failed to clone UDisks2 property {name} on {object_path} {interface}: {error}"
                        )
                    })?,
                );
            }
            raw_interfaces.insert(interface.clone(), raw_properties);
        }
        raw_objects.push(Udisks2RawObject {
            object_path: object_path.clone(),
            interfaces: raw_interfaces,
        });
    }
    Ok(raw_objects)
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

    fn interface_map(interfaces: &[(&str, Udisks2PropertyMap)]) -> Udisks2InterfaceMap {
        interfaces
            .iter()
            .map(|(name, properties)| {
                let properties = properties
                    .iter()
                    .map(|(key, value)| (key.clone(), value.try_clone().unwrap()))
                    .collect();
                ((*name).to_string(), properties)
            })
            .collect()
    }

    fn interface_name(name: &'static str) -> zbus::names::InterfaceName<'static> {
        zbus::names::InterfaceName::try_from(name).unwrap()
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
                ejectable: false,
                can_power_off: false,
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
                ejectable: false,
                can_power_off: false,
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
    fn udisks2_signal_parser_reads_interfaces_added_message() {
        let mut block_properties = HashMap::new();
        block_properties.insert(
            "PreferredDevice",
            zbus::zvariant::Value::from(path_bytes("/dev/sdb1")),
        );
        let mut interfaces = HashMap::new();
        interfaces.insert(interface_name(UDISKS2_BLOCK_INTERFACE), block_properties);
        let object_path =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdb1")
                .unwrap();
        let message = zbus::Message::signal(
            UDISKS2_OBJECT_MANAGER_PATH,
            DBUS_OBJECT_MANAGER_INTERFACE,
            "InterfacesAdded",
        )
        .unwrap()
        .build(&(object_path, interfaces))
        .unwrap();

        let signal = udisks2_signal_from_message(&message).unwrap().unwrap();

        match signal {
            Udisks2Signal::InterfacesAdded {
                object_path,
                interfaces,
            } => {
                assert_eq!(object_path, "/org/freedesktop/UDisks2/block_devices/sdb1");
                assert_eq!(
                    property_bytes_path(
                        interfaces.get(UDISKS2_BLOCK_INTERFACE).unwrap(),
                        "PreferredDevice"
                    ),
                    Some(PathBuf::from("/dev/sdb1"))
                );
            }
            other => panic!("unexpected signal: {other:?}"),
        }
    }

    #[test]
    fn udisks2_signal_parser_reads_properties_changed_message_path() {
        let mut changed = HashMap::new();
        changed.insert(
            "MountPoints",
            zbus::zvariant::Value::from(Vec::<Vec<u8>>::new()),
        );
        let message = zbus::Message::signal(
            "/org/freedesktop/UDisks2/block_devices/sdb1",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .unwrap()
        .build(&(
            interface_name(UDISKS2_FILESYSTEM_INTERFACE),
            changed,
            vec!["Size"],
        ))
        .unwrap();

        let signal = udisks2_signal_from_message(&message).unwrap().unwrap();

        match signal {
            Udisks2Signal::PropertiesChanged {
                object_path,
                interface,
                changed,
                invalidated,
            } => {
                assert_eq!(object_path, "/org/freedesktop/UDisks2/block_devices/sdb1");
                assert_eq!(interface, UDISKS2_FILESYSTEM_INTERFACE);
                assert_eq!(
                    changed,
                    properties(&[("MountPoints", owned_value(Vec::<Vec<u8>>::new()))])
                );
                assert_eq!(invalidated, vec!["Size".to_string()]);
            }
            other => panic!("unexpected signal: {other:?}"),
        }
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
                        ("CanPowerOff", owned_value(true)),
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
                has_filesystem: true,
                hint_ignore: false,
                hint_system: false,
                removable: true,
                ejectable: true,
                can_power_off: true,
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
                ejectable: true,
                can_power_off: true,
            }]
        );
    }

    #[test]
    fn udisks2_device_action_targets_keep_block_and_drive_paths() {
        let objects = vec![
            raw_object(
                "/org/freedesktop/UDisks2/drives/USB",
                &[(
                    UDISKS2_DRIVE_INTERFACE,
                    properties(&[
                        ("Removable", owned_value(true)),
                        ("Ejectable", owned_value(true)),
                        ("CanPowerOff", owned_value(true)),
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
                            (
                                "Drive",
                                object_path_value("/org/freedesktop/UDisks2/drives/USB"),
                            ),
                            ("HintIgnore", owned_value(false)),
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

        assert_eq!(
            udisks2_device_action_targets(&snapshot, &[]),
            vec![Udisks2DeviceActionTarget {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Backup")),
                block_object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                drive_object_path: Some("/org/freedesktop/UDisks2/drives/USB".to_string()),
                ejectable: true,
                can_power_off: true,
            }]
        );
        assert_eq!(
            resolve_udisks2_device_action_target(&snapshot, &[], Path::new("/run/media/yk/Backup"))
                .map(|target| target.block_object_path),
            Some("/org/freedesktop/UDisks2/block_devices/sdb1".to_string())
        );
    }

    #[test]
    fn udisks2_device_action_targets_skip_non_filesystems_and_hint_ignore() {
        let objects = vec![
            raw_object(
                "/org/freedesktop/UDisks2/block_devices/sdb",
                &[(
                    UDISKS2_BLOCK_INTERFACE,
                    properties(&[("PreferredDevice", owned_value(path_bytes("/dev/sdb")))]),
                )],
            ),
            raw_object(
                "/org/freedesktop/UDisks2/block_devices/sdc1",
                &[
                    (
                        UDISKS2_BLOCK_INTERFACE,
                        properties(&[
                            ("PreferredDevice", owned_value(path_bytes("/dev/sdc1"))),
                            ("HintIgnore", owned_value(true)),
                        ]),
                    ),
                    (UDISKS2_FILESYSTEM_INTERFACE, properties(&[])),
                ],
            ),
        ];
        let snapshot = udisks2_snapshot_from_raw_objects(&objects);

        assert!(udisks2_device_action_targets(&snapshot, &[]).is_empty());
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
                ejectable: false,
                can_power_off: false,
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
                ejectable: false,
                can_power_off: false,
            },
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdc1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Removed")),
                filesystem_type: Some("vfat".to_string()),
                label: Some("Removed".to_string()),
                capacity_bytes: None,
                removable: true,
                ejectable: false,
                can_power_off: false,
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
                ejectable: false,
                can_power_off: false,
            },
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdd1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Added")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Added".to_string()),
                capacity_bytes: Some(128),
                removable: true,
                ejectable: false,
                can_power_off: false,
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

    #[test]
    fn udisks2_monitor_interfaces_added_and_removed_emit_device_events() {
        let mut state = Udisks2MonitorState::from_raw_objects(Vec::new()).unwrap();
        let mount_entries = Vec::new();

        let drive_events = device_events_for_udisks2_signal(
            &mut state,
            Udisks2Signal::InterfacesAdded {
                object_path: "/org/freedesktop/UDisks2/drives/USB".to_string(),
                interfaces: interface_map(&[(
                    UDISKS2_DRIVE_INTERFACE,
                    properties(&[
                        ("Removable", owned_value(false)),
                        ("MediaRemovable", owned_value(true)),
                        ("Ejectable", owned_value(true)),
                    ]),
                )]),
            },
            &mount_entries,
        )
        .unwrap();
        assert!(drive_events.is_empty());

        let added_events = device_events_for_udisks2_signal(
            &mut state,
            Udisks2Signal::InterfacesAdded {
                object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                interfaces: interface_map(&[
                    (
                        UDISKS2_BLOCK_INTERFACE,
                        properties(&[
                            ("PreferredDevice", owned_value(path_bytes("/dev/sdb1"))),
                            (
                                "Drive",
                                object_path_value("/org/freedesktop/UDisks2/drives/USB"),
                            ),
                            ("IdLabel", owned_value("Backup")),
                            ("IdType", owned_value("exfat")),
                            ("Size", owned_value(64_u64)),
                            ("HintIgnore", owned_value(false)),
                        ]),
                    ),
                    (
                        UDISKS2_FILESYSTEM_INTERFACE,
                        properties(&[(
                            "MountPoints",
                            owned_value(vec![path_bytes("/run/media/yk/Backup")]),
                        )]),
                    ),
                ]),
            },
            &mount_entries,
        )
        .unwrap();

        let added = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: Some(PathBuf::from("/run/media/yk/Backup")),
            filesystem_type: Some("exfat".to_string()),
            label: Some("Backup".to_string()),
            capacity_bytes: Some(64),
            removable: true,
            ejectable: true,
            can_power_off: false,
        };
        assert_eq!(added_events, vec![DeviceEvent::Added(added.clone())]);

        let removed_events = device_events_for_udisks2_signal(
            &mut state,
            Udisks2Signal::InterfacesRemoved {
                object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                interfaces: vec![UDISKS2_BLOCK_INTERFACE.to_string()],
            },
            &mount_entries,
        )
        .unwrap();

        assert_eq!(
            removed_events,
            vec![DeviceEvent::Removed(PathBuf::from("/dev/sdb1"))]
        );
    }

    #[test]
    fn udisks2_monitor_properties_changed_updates_derived_device() {
        let mut state = Udisks2MonitorState::from_raw_objects(vec![
            raw_object(
                "/org/freedesktop/UDisks2/drives/USB",
                &[(
                    UDISKS2_DRIVE_INTERFACE,
                    properties(&[
                        ("MediaRemovable", owned_value(true)),
                        ("Ejectable", owned_value(true)),
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
                            (
                                "Drive",
                                object_path_value("/org/freedesktop/UDisks2/drives/USB"),
                            ),
                            ("IdLabel", owned_value("Old Label")),
                            ("IdType", owned_value("exfat")),
                            ("Size", owned_value(64_u64)),
                        ]),
                    ),
                    (
                        UDISKS2_FILESYSTEM_INTERFACE,
                        properties(&[("MountPoints", owned_value(Vec::<Vec<u8>>::new()))]),
                    ),
                ],
            ),
        ])
        .unwrap();
        let mount_entries = Vec::new();

        let mount_events = device_events_for_udisks2_signal(
            &mut state,
            Udisks2Signal::PropertiesChanged {
                object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                interface: UDISKS2_FILESYSTEM_INTERFACE.to_string(),
                changed: properties(&[(
                    "MountPoints",
                    owned_value(vec![path_bytes("/run/media/yk/Backup")]),
                )]),
                invalidated: Vec::new(),
            },
            &mount_entries,
        )
        .unwrap();

        assert_eq!(
            mount_events,
            vec![DeviceEvent::Changed(DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Backup")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Old Label".to_string()),
                capacity_bytes: Some(64),
                removable: true,
                ejectable: true,
                can_power_off: false,
            })]
        );

        let label_events = device_events_for_udisks2_signal(
            &mut state,
            Udisks2Signal::PropertiesChanged {
                object_path: "/org/freedesktop/UDisks2/block_devices/sdb1".to_string(),
                interface: UDISKS2_BLOCK_INTERFACE.to_string(),
                changed: Udisks2PropertyMap::new(),
                invalidated: vec!["IdLabel".to_string()],
            },
            &mount_entries,
        )
        .unwrap();

        assert_eq!(
            label_events,
            vec![DeviceEvent::Changed(DeviceInfo {
                device_path: PathBuf::from("/dev/sdb1"),
                mount_point: Some(PathBuf::from("/run/media/yk/Backup")),
                filesystem_type: Some("exfat".to_string()),
                label: Some("Backup".to_string()),
                capacity_bytes: Some(64),
                removable: true,
                ejectable: true,
                can_power_off: false,
            })]
        );
    }
}
