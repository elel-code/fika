use std::{
    env,
    ffi::OsString,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use winit::window::Window;

pub(crate) struct ShellClipboard {
    backend: ShellClipboardBackend,
}

#[derive(Clone, Copy)]
enum ShellClipboardBackend {
    WlClipboard,
    Xclip,
}

impl ShellClipboard {
    pub(crate) fn from_window(_window: &dyn Window) -> Result<Option<Self>, String> {
        if env::var_os("WAYLAND_DISPLAY").is_some()
            && command_exists("wl-copy")
            && command_exists("wl-paste")
        {
            return Ok(Some(Self {
                backend: ShellClipboardBackend::WlClipboard,
            }));
        }
        if env::var_os("DISPLAY").is_some() && command_exists("xclip") {
            return Ok(Some(Self {
                backend: ShellClipboardBackend::Xclip,
            }));
        }
        Ok(None)
    }

    pub(crate) fn backend(&self) -> &'static str {
        match self.backend {
            ShellClipboardBackend::WlClipboard => "wl-clipboard",
            ShellClipboardBackend::Xclip => "xclip",
        }
    }

    pub(crate) fn store_text(&self, text: &str) -> Result<(), String> {
        match self.backend {
            ShellClipboardBackend::WlClipboard => {
                run_clipboard_store("wl-copy", &["--type", "text/plain;charset=utf-8"], text)
            }
            ShellClipboardBackend::Xclip => {
                run_clipboard_store("xclip", &["-selection", "clipboard"], text)
            }
        }
    }

    pub(crate) fn load_text(&self) -> Result<String, String> {
        match self.backend {
            ShellClipboardBackend::WlClipboard => {
                run_clipboard_load("wl-paste", &["--no-newline", "--type", "text/plain"])
            }
            ShellClipboardBackend::Xclip => {
                run_clipboard_load("xclip", &["-selection", "clipboard", "-out"])
            }
        }
    }
}

fn run_clipboard_store(program: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program} spawn: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("{program} stdin unavailable"))?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|error| format!("{program} write: {error}"))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("{program} wait: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(program, &output.stderr))
    }
}

fn run_clipboard_load(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("{program} spawn: {error}"))?;
    if !output.status.success() {
        return Err(command_error(program, &output.stderr));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("{program} utf8: {error}"))
}

fn command_error(program: &str, stderr: &[u8]) -> String {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    if message.is_empty() {
        format!("{program} failed")
    } else {
        format!("{program}: {message}")
    }
}

fn command_exists(program: &str) -> bool {
    let program = OsString::from(program);
    if PathBuf::from(&program).components().count() > 1 {
        return PathBuf::from(program).is_file();
    }
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .any(|path| path.join(&program).is_file())
}
