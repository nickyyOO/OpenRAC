//! The player's mods: kept current by themselves (core/src/mod_updates.rs), installed, switched.

use crate::launcher::Launcher;

impl Launcher {
    /// Before Play: the game's mod updates, waited for a short while at most; Play goes on
    /// whatever happens.
    pub(crate) fn update_mods_before_play(&self, _key: &str) {}
}
