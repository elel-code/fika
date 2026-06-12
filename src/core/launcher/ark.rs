use super::{DesktopLaunchCommand, DesktopLaunchPlan};
use std::path::{Path, PathBuf};

const ARK_DESKTOP_FILE: &str = "/usr/share/applications/org.kde.ark.desktop";

pub fn ark_compress_launch_plan(paths: &[PathBuf]) -> Result<DesktopLaunchPlan, String> {
    if paths.is_empty() {
        return Err("No item selected".to_string());
    }
    Ok(DesktopLaunchPlan {
        desktop_id: "fika-ark-compress".to_string(),
        desktop_file: PathBuf::from(ARK_DESKTOP_FILE),
        app_name: "Ark: Compress".to_string(),
        commands: vec![DesktopLaunchCommand {
            program: "ark".to_string(),
            args: std::iter::once("--add".to_string())
                .chain(paths.iter().map(|path| path.display().to_string()))
                .collect(),
        }],
    })
}

pub fn ark_extract_here_launch_plan(archive: &Path) -> Result<DesktopLaunchPlan, String> {
    ark_extract_launch_plan("fika-ark-extract-here", "Ark: Extract Here", archive, false)
}

pub fn ark_extract_to_launch_plan(archive: &Path) -> Result<DesktopLaunchPlan, String> {
    ark_extract_launch_plan("fika-ark-extract-to", "Ark: Extract To", archive, true)
}

fn ark_extract_launch_plan(
    desktop_id: &str,
    app_name: &str,
    archive: &Path,
    dialog: bool,
) -> Result<DesktopLaunchPlan, String> {
    if archive.as_os_str().is_empty() {
        return Err("No archive selected".to_string());
    }
    let destination = archive
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut args = vec!["--batch".to_string()];
    if dialog {
        args.push("--dialog".to_string());
    }
    args.extend([
        "--destination".to_string(),
        destination.display().to_string(),
        archive.display().to_string(),
    ]);
    Ok(DesktopLaunchPlan {
        desktop_id: desktop_id.to_string(),
        desktop_file: PathBuf::from(ARK_DESKTOP_FILE),
        app_name: app_name.to_string(),
        commands: vec![DesktopLaunchCommand {
            program: "ark".to_string(),
            args,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ark_compress_fallback_builds_systemd_launch_plan() {
        let plan = ark_compress_launch_plan(&[
            PathBuf::from("/tmp/fika-ark-compress/note.txt"),
            PathBuf::from("/tmp/fika-ark-compress/todo.txt"),
        ])
        .unwrap();

        assert_eq!(plan.desktop_id, "fika-ark-compress");
        assert_eq!(plan.app_name, "Ark: Compress");
        assert_eq!(plan.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(plan.commands.len(), 1);
        assert_eq!(plan.commands[0].program, "ark");
        assert_eq!(
            plan.commands[0].args,
            vec![
                "--add",
                "/tmp/fika-ark-compress/note.txt",
                "/tmp/fika-ark-compress/todo.txt"
            ]
        );
    }

    #[test]
    fn ark_extract_fallback_builds_systemd_launch_plans() {
        let archive = Path::new("/tmp/fika-ark-extract/archive.zip");

        let here = ark_extract_here_launch_plan(archive).unwrap();
        assert_eq!(here.desktop_id, "fika-ark-extract-here");
        assert_eq!(here.app_name, "Ark: Extract Here");
        assert_eq!(here.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(here.commands.len(), 1);
        assert_eq!(here.commands[0].program, "ark");
        assert_eq!(
            here.commands[0].args,
            vec![
                "--batch",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/archive.zip"
            ]
        );

        let extract_to = ark_extract_to_launch_plan(archive).unwrap();
        assert_eq!(extract_to.desktop_id, "fika-ark-extract-to");
        assert_eq!(extract_to.app_name, "Ark: Extract To");
        assert_eq!(extract_to.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(extract_to.commands.len(), 1);
        assert_eq!(extract_to.commands[0].program, "ark");
        assert_eq!(
            extract_to.commands[0].args,
            vec![
                "--batch",
                "--dialog",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/archive.zip"
            ]
        );
    }
}
