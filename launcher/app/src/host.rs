//! The window as the service's [`Host`]: what changed goes to the window's thread, and the
//! questions only a person can answer are asked there.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use openrac_launcher_service::{Change, Host, Notice};

use crate::shell;

pub struct WindowHost;

/// Runs `work` on the window's thread, with the shell.
fn on_window(work: impl FnOnce(&shell::Shell) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || shell::with(work));
}

impl Host for WindowHost {
    fn changed(&self, change: Change) {
        on_window(move |shell| shell.changed(change));
    }

    fn notify(&self, notice: Notice) {
        on_window(move |shell| shell.notify(notice));
    }

    fn confirm(&self, text: &str, yes: &str) -> bool {
        let (answer, answered) = mpsc::channel();
        let (text, yes) = (text.to_string(), yes.to_string());
        on_window(move |shell| shell.ask(&text, &yes, answer));
        answered.recv().unwrap_or(false)
    }

    fn pick_disc_image(&self, title: &str) -> Option<PathBuf> {
        crate::dialogs::pick_file(title, "Disc image", &["iso", "chd"])
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        crate::dialogs::pick_folder(title)
    }

    fn pick_file(&self, title: &str, kind: &str, extensions: &[&str], start: Option<&Path>) -> Option<PathBuf> {
        crate::dialogs::pick_file_in(title, kind, extensions, start)
    }

    fn save_file(&self, title: &str, name: &str) -> Option<PathBuf> {
        crate::dialogs::save_file(title, name)
    }

    fn open_path(&self, path: &Path) -> Result<(), String> {
        crate::system::open_path(path)
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        crate::system::open_url(url)
    }

    fn copy(&self, text: &str) {
        crate::system::copy(text);
    }
}
