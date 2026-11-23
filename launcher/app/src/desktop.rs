//! The launcher on a desktop: the window, then the service started beside it.

use std::sync::Arc;

use openrac_launcher_service::{bundled_copy, Launcher, Paths, Start};
use slint::ComponentHandle;

use crate::host::WindowHost;
use crate::{shell, AppWindow};

pub fn run() {
    let window = AppWindow::new().expect("the launcher's window opens");
    shell::install(&window);
    std::thread::spawn(|| {
        let paths = Paths::desktop();
        let bundled = bundled_copy(paths.resource_dir.as_deref(), paths.config_file().as_deref());
        let start = Start { paths, version: env!("CARGO_PKG_VERSION").to_string(), bundled };
        let launcher = Launcher::start(start, Arc::new(WindowHost));
        launcher.refresh();
        let _ = slint::invoke_from_event_loop(move || shell::with(|shell| shell.ready(launcher)));
    });
    window.run().expect("the launcher's window runs");
}
