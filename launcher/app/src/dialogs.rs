//! The system's file and folder dialogs. They are called off the window's thread, which waits
//! meanwhile; macOS wants its dialogs on the main thread, so there they are run there.

use std::path::PathBuf;

/// A file of one of `extensions`, or None when the player picked nothing.
pub fn pick_file(title: &str, kind: &str, extensions: &[&str]) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new().set_title(title).add_filter(kind, extensions);
    on_main(move || dialog.pick_file())
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new().set_title(title);
    on_main(move || dialog.pick_folder())
}

/// Where to save a file named `name`.
pub fn save_file(title: &str, name: &str) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new().set_title(title).set_file_name(name);
    on_main(move || dialog.save_file())
}

#[cfg(target_os = "macos")]
fn on_main<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = std::sync::mpsc::channel();
    let _ = slint::invoke_from_event_loop(move || {
        let _ = done.send(work());
    });
    result.recv().expect("the window's thread answers")
}

#[cfg(not(target_os = "macos"))]
fn on_main<T>(work: impl FnOnce() -> T) -> T {
    work()
}
