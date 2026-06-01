use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, Value};

static UNIT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchResult {
    pub(crate) unit: Option<String>,
    pub(crate) diagnostic: Option<String>,
}

pub(crate) fn spawn_in_user_scope(
    program: &str,
    args: &[String],
    label: Option<&str>,
) -> Result<LaunchResult, String> {
    spawn_in_user_scope_with_dir(program, args, label, None)
}

pub(crate) fn spawn_in_user_scope_with_dir(
    program: &str,
    args: &[String],
    label: Option<&str>,
    cwd: Option<&Path>,
) -> Result<LaunchResult, String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let child = command.spawn().map_err(|err| format!("{program}: {err}"))?;

    let pid = child.id();
    let (unit, diagnostic) = match start_scope(pid, label.unwrap_or(program)) {
        Ok(unit) => (Some(unit), None),
        Err(err) => (None, Some(format!("systemd user scope unavailable: {err}"))),
    };

    drop(child);
    Ok(LaunchResult { unit, diagnostic })
}

fn start_scope(pid: u32, label: &str) -> Result<String, String> {
    let connection =
        Connection::session().map_err(|err| format!("cannot connect to session bus: {err}"))?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;

    let unit = unit_name(pid);
    let description = format!("Fika Open - {label}");
    let pids = vec![pid];
    let properties = vec![
        ("PIDs", Value::new(pids)),
        ("Description", Value::new(description.as_str())),
        ("CollectMode", Value::new("inactive-or-failed")),
    ];
    let aux: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let _: OwnedObjectPath = proxy
        .call(
            "StartTransientUnit",
            &(unit.as_str(), "replace", properties, aux),
        )
        .map_err(|err| format!("StartTransientUnit failed: {err}"))?;
    Ok(unit)
}

fn unit_name(pid: u32) -> String {
    let counter = UNIT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("fika-open-{pid}-{counter}.scope")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_scope_name_is_valid_and_unique() {
        let first = unit_name(42);
        let second = unit_name(42);

        assert!(first.starts_with("fika-open-42-"));
        assert!(first.ends_with(".scope"));
        assert_ne!(first, second);
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
        );
    }
}
