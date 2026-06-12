use super::{
    DeviceActionError, eject_udisks2_device, mount_udisks2_device, safely_remove_udisks2_device,
    unmount_udisks2_device,
};
use crate::core::pane::PaneId;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevicePlaceOperation {
    Mount,
    Unmount,
    Eject,
    SafelyRemove,
}

impl DevicePlaceOperation {
    pub fn in_progress_message(self, path: &Path) -> String {
        match self {
            Self::Mount => format!("Mounting {}", path.display()),
            Self::Unmount => format!("Unmounting {}", path.display()),
            Self::Eject => format!("Ejecting {}", path.display()),
            Self::SafelyRemove => format!("Safely removing {}", path.display()),
        }
    }

    pub fn success_message(self, path: &Path) -> String {
        match self {
            Self::Mount => format!("Mounted {}", path.display()),
            Self::Unmount => format!("Unmounted {}", path.display()),
            Self::Eject => format!("Ejected {}", path.display()),
            Self::SafelyRemove => format!("Safely removed {}", path.display()),
        }
    }

    pub fn error_message(self, path: &Path, error: &DeviceActionError) -> String {
        match self {
            Self::Mount => format!("Cannot mount {}: {error}", path.display()),
            Self::Unmount => format!("Cannot unmount {}: {error}", path.display()),
            Self::Eject => format!("Cannot eject {}: {error}", path.display()),
            Self::SafelyRemove => format!("Cannot safely remove {}: {error}", path.display()),
        }
    }
}

#[derive(Debug)]
pub struct DevicePlaceOperationResult {
    pub pane_id: PaneId,
    pub path: PathBuf,
    pub operation: DevicePlaceOperation,
    pub result: Result<Option<PathBuf>, DeviceActionError>,
}

pub async fn perform_device_place_operation(
    pane_id: PaneId,
    path: PathBuf,
    operation: DevicePlaceOperation,
) -> DevicePlaceOperationResult {
    let result = match operation {
        DevicePlaceOperation::Mount => mount_udisks2_device(&path)
            .await
            .map(|result| Some(result.mount_point)),
        DevicePlaceOperation::Unmount => unmount_udisks2_device(&path).await.map(|()| None),
        DevicePlaceOperation::Eject => eject_udisks2_device(&path).await.map(|()| None),
        DevicePlaceOperation::SafelyRemove => {
            safely_remove_udisks2_device(&path).await.map(|()| None)
        }
    };
    DevicePlaceOperationResult {
        pane_id,
        path,
        operation,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_place_operation_messages_match_file_manager_actions() {
        let path = Path::new("/run/media/disk");
        assert_eq!(
            DevicePlaceOperation::Mount.in_progress_message(path),
            "Mounting /run/media/disk"
        );
        assert_eq!(
            DevicePlaceOperation::Unmount.success_message(path),
            "Unmounted /run/media/disk"
        );
        assert_eq!(
            DevicePlaceOperation::Eject
                .error_message(path, &DeviceActionError::CannotEject(path.to_path_buf())),
            "Cannot eject /run/media/disk: device cannot be ejected: /run/media/disk"
        );
        assert_eq!(
            DevicePlaceOperation::SafelyRemove.success_message(path),
            "Safely removed /run/media/disk"
        );
    }
}
