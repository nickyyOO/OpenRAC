//! Where the launcher keeps its own files. The folders are the ones earlier launchers used, so an
//! update finds its settings: `<config dir>/dev.openrac.launcher/launcher.json` and the local data
//! folder of the same name.

use std::path::{Path, PathBuf};

/// The launcher's id: the name of its folders, and its Android package.
pub const IDENTIFIER: &str = "dev.openrac.launcher";

/// The name the launcher's packages install it under (its resources on Linux).
pub const PRODUCT_NAME: &str = "OpenRAC Launcher";

#[derive(Debug, Clone, Default)]
pub struct Paths {
    /// The folder of launcher.json, progress.json and launcher.log.
    pub config_dir: Option<PathBuf>,
    /// The per-user data folder (where older launchers set games up).
    pub local_data_dir: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    /// The folder the launcher's package put its resources in (the copy of OpenRAC it carries).
    pub resource_dir: Option<PathBuf>,
}

impl Paths {
    /// A desktop's folders.
    pub fn desktop() -> Paths {
        Paths {
            config_dir: dirs::config_dir().map(|d| d.join(IDENTIFIER)),
            local_data_dir: dirs::data_local_dir().map(|d| d.join(IDENTIFIER)),
            home_dir: dirs::home_dir(),
            resource_dir: std::env::current_exe().ok().and_then(|exe| resource_dir(&exe)),
        }
    }

    pub fn config_file(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|d| d.join("launcher.json"))
    }

    /// The copy of the figures fetched from bytematch.dev, beside the settings (always writable).
    pub fn progress_cache(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|d| d.join("progress.json"))
    }
}

/// Where the package keeps its resources, for the program at `exe`: beside it on Windows, in the
/// bundle's Resources on macOS, and on Linux in `../lib/OpenRAC Launcher` (a package, an AppImage's
/// usr/) when that is there, else beside it.
fn resource_dir(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    if cfg!(target_os = "macos") {
        return Some(dir.parent()?.join("Resources"));
    }
    if cfg!(target_os = "linux") {
        let lib = dir.parent().map(|usr| usr.join("lib").join(PRODUCT_NAME));
        if let Some(lib) = lib.filter(|l| l.is_dir()) {
            return Some(lib);
        }
    }
    Some(dir.to_path_buf())
}
