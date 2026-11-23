// No console window behind the launcher's on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    openrac_launcher::desktop::run();
}
