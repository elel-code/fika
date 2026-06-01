use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::AsyncEvent;
use crate::{AppWindow, DeviceEntry};
use futures_lite::StreamExt;
use std::env;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc;
use std::time::Duration;
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

pub(crate) fn start_device_monitor(bridge: &AsyncBridge) {
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let debounce = Arc::clone(&bridge.device_watch_debounce);
    bridge.handle.spawn(async move {
        device_monitor_loop(async_tx, notify_ui, debounce).await;
    });
}

async fn device_monitor_loop(
    async_tx: mpsc::Sender<AsyncEvent>,
    notify_ui: slint::Weak<AppWindow>,
    debounce: Arc<AtomicU64>,
) {
    let mut last_snapshot = device_snapshot_async().await;
    let mut poll = tokio::time::interval(Duration::from_secs(8));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    poll.tick().await;

    let signal_stream = async {
        let connection = zbus::Connection::system()
            .await
            .map_err(|err| format!("cannot connect to system bus: {err}"))?;
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.UDisks2")
            .map_err(|err| format!("cannot build UDisks2 sender match: {err}"))?
            .path_namespace("/org/freedesktop/UDisks2")
            .map_err(|err| format!("cannot build UDisks2 path match: {err}"))?
            .build();
        MessageStream::for_match_rule(rule, &connection, Some(32))
            .await
            .map_err(|err| format!("cannot subscribe to UDisks2 signals: {err}"))
    }
    .await;

    let mut signal_stream = match signal_stream {
        Ok(stream) => {
            device_debug_log("watching UDisks2 signals");
            Some(stream)
        }
        Err(err) => {
            device_debug_log(&format!("{err}; using snapshot polling only"));
            None
        }
    };

    loop {
        if let Some(stream) = signal_stream.as_mut() {
            tokio::select! {
                signal = stream.next() => {
                    if signal.is_some() {
                        let snapshot = device_snapshot_async().await;
                        if device_snapshot_changed(&mut last_snapshot, snapshot) {
                            schedule_devices_changed(&async_tx, &notify_ui, &debounce, "udisks2-signal");
                        }
                    } else {
                        device_debug_log("UDisks2 signal stream ended; using snapshot polling only");
                        signal_stream = None;
                    }
                }
                _ = poll.tick() => {
                    refresh_devices_if_snapshot_changed(&async_tx, &notify_ui, &debounce, &mut last_snapshot).await;
                }
            }
        } else {
            poll.tick().await;
            refresh_devices_if_snapshot_changed(
                &async_tx,
                &notify_ui,
                &debounce,
                &mut last_snapshot,
            )
            .await;
        }
    }
}

async fn refresh_devices_if_snapshot_changed(
    async_tx: &mpsc::Sender<AsyncEvent>,
    notify_ui: &slint::Weak<AppWindow>,
    debounce: &Arc<AtomicU64>,
    last_snapshot: &mut Vec<String>,
) {
    let snapshot = device_snapshot_async().await;
    if !device_snapshot_changed(last_snapshot, snapshot) {
        return;
    }
    schedule_devices_changed(async_tx, notify_ui, debounce, "snapshot-changed");
}

async fn device_snapshot_async() -> Vec<String> {
    tokio::task::spawn_blocking(|| {
        crate::fs::devices::mounted_devices()
            .into_iter()
            .map(device_snapshot_key)
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default()
}

fn device_snapshot_key(device: DeviceEntry) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        device.label,
        device.path,
        device.device_path,
        device.marker,
        device.mounted,
        device.can_mount,
        device.can_unmount,
        device.can_eject
    )
}

fn device_snapshot_changed(last_snapshot: &mut Vec<String>, snapshot: Vec<String>) -> bool {
    if *last_snapshot == snapshot {
        return false;
    }
    *last_snapshot = snapshot;
    true
}

fn schedule_devices_changed(
    async_tx: &mpsc::Sender<AsyncEvent>,
    notify_ui: &slint::Weak<AppWindow>,
    debounce: &Arc<AtomicU64>,
    reason: &str,
) {
    let serial = debounce.fetch_add(1, AtomicOrdering::SeqCst) + 1;
    let async_tx = async_tx.clone();
    let notify_ui = notify_ui.clone();
    let debounce = Arc::clone(debounce);
    let reason = reason.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if debounce.load(AtomicOrdering::SeqCst) != serial {
            return;
        }
        device_debug_log(&format!("device monitor refresh reason={reason}"));
        send_async_event(async_tx, notify_ui, AsyncEvent::DevicesChanged);
    });
}

fn device_debug_log(message: &str) {
    static DEBUG_DEVICES: OnceLock<bool> = OnceLock::new();
    if *DEBUG_DEVICES.get_or_init(|| {
        env::var("FIKA_DEBUG_DEVICES").is_ok_and(|value| env_flag_is_truthy(&value))
    }) {
        eprintln!("[fika devices] {message}");
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

    #[test]
    fn device_snapshot_changed_updates_only_on_real_change() {
        let mut last = vec![
            "USB\u{1f}/run/media/yk/USB\u{1f}/dev/sdb1\u{1f}USB\u{1f}true\u{1f}false\u{1f}true\u{1f}true"
                .into(),
        ];

        let same = last.clone();
        assert!(!device_snapshot_changed(&mut last, same));
        assert_eq!(last.len(), 1);

        let changed = vec![
            "USB\u{1f}/run/media/yk/USB\u{1f}/dev/sdb1\u{1f}USB\u{1f}false\u{1f}true\u{1f}false\u{1f}true"
                .into(),
        ];
        assert!(device_snapshot_changed(&mut last, changed.clone()));
        assert_eq!(last, changed);
    }

    #[test]
    fn device_snapshot_key_tracks_menu_capabilities() {
        let device = DeviceEntry {
            label: "USB".into(),
            path: "/run/media/yk/USB".into(),
            device_path: "/dev/sdb1".into(),
            marker: "U".into(),
            mounted: true,
            can_mount: false,
            can_unmount: true,
            can_eject: true,
            pending_action: String::new().into(),
            error: String::new().into(),
        };

        assert_eq!(
            device_snapshot_key(device),
            "USB\u{1f}/run/media/yk/USB\u{1f}/dev/sdb1\u{1f}U\u{1f}true\u{1f}false\u{1f}true\u{1f}true"
        );
    }
}
