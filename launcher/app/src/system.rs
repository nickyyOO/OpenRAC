//! The system's own programs: a folder or a file opened as the desktop opens it, a web page in the
//! browser, the clipboard.

use std::path::Path;

/// Opens `target` (a path or a link) with the system's program for it.
pub fn open(target: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        // The desktop's own opener, in the environment it expects: an AppImage's libraries are
        // ours, not the system's.
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(target);
        openrac_launcher_service::core::appimage::sanitize(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("cannot start xdg-open: {e}"))?;
        // Reaped when it ends, so it never lingers as a zombie.
        std::thread::spawn(move || child.wait());
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        ::open::that_detached(target).map_err(|e| e.to_string())
    }
}

pub fn open_path(path: &Path) -> Result<(), String> {
    open(&path.to_string_lossy())
}

/// Opens a web page: https only.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("{url} is not a web page"));
    }
    open(url)
}

pub fn copy(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text.to_string());
    }
}
