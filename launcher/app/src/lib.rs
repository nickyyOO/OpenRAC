//! The launcher's window (Slint), on a desktop and on Android. What the launcher does is the
//! service crate's; this crate shows it (`ui/`, `pages/`) and is the [`service::Host`] it asks
//! for dialogs and files.

slint::include_modules!();

mod art;
#[cfg(not(target_os = "android"))]
mod dialogs;
mod host;
mod pages;
mod shell;
#[cfg(not(target_os = "android"))]
mod steam;
#[cfg(not(target_os = "android"))]
mod system;
mod theme;
mod toasts;
mod words;

#[cfg(not(target_os = "android"))]
pub mod desktop;
