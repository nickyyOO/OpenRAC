//! The launcher's window (Slint), on a desktop and on Android. What the launcher does is the
//! service crate's; this crate shows it (`ui/`, `view/`) and is the [`service::Host`] it asks
//! for dialogs and files.

slint::include_modules!();

mod art;
#[cfg(not(target_os = "android"))]
mod dialogs;
mod host;
mod shell;
mod theme;
mod toasts;

#[cfg(not(target_os = "android"))]
pub mod desktop;
