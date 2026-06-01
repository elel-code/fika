use crate::DeviceEntry;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn mounted_devices() -> Vec<DeviceEntry> {
    mounted_devices_from_mountinfo(&mount_roots(), "/proc/self/mountinfo")
        .unwrap_or_else(|| mounted_devices_from_roots(mount_roots()))
}

fn mounted_devices_from_roots(roots: Vec<PathBuf>) -> Vec<DeviceEntry> {
    mounted_devices_from_paths(mounted_children_from_roots(&roots))
}

fn mounted_devices_from_mountinfo(
    roots: &[PathBuf],
    mountinfo_path: &str,
) -> Option<Vec<DeviceEntry>> {
    let contents = fs::read_to_string(mountinfo_path).ok()?;
    let mount_points = parse_mountinfo_mount_points(&contents)
        .into_iter()
        .filter(|path| is_device_mount_point(path, roots))
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
            path_text,
            mount_marker(&path),
            true,
        ));
    }

    devices
}

fn filesystem_entry() -> DeviceEntry {
    device_entry("Filesystem".into(), "/".into(), "/".into(), true)
}

fn device_entry(label: String, path: String, marker: String, mounted: bool) -> DeviceEntry {
    DeviceEntry {
        label: label.into(),
        path: path.into(),
        marker: marker.into(),
        mounted,
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

fn parse_mountinfo_mount_points(contents: &str) -> Vec<PathBuf> {
    contents
        .lines()
        .filter_map(parse_mountinfo_mount_point)
        .collect()
}

fn parse_mountinfo_mount_point(line: &str) -> Option<PathBuf> {
    let mut fields = line.split_whitespace();
    fields.next()?;
    fields.next()?;
    fields.next()?;
    fields.next()?;
    fields
        .next()
        .map(|field| PathBuf::from(unescape_mountinfo_field(field)))
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

fn is_device_mount_point(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        if path == root {
            return true;
        }
        path.strip_prefix(root)
            .ok()
            .is_some_and(|relative| relative.components().count() > 0)
    })
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
    mount_label(path)
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().collect())
        .unwrap_or_else(|| "D".into())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
