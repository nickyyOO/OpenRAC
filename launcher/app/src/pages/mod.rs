//! Each page's Rust side: what its global shows, and what its buttons do.

pub mod activity;
pub mod differences;
pub mod game;
pub mod help;
pub mod saves;
pub mod settings;
#[cfg(not(target_os = "android"))]
pub mod shortcuts;
pub mod stopped;
