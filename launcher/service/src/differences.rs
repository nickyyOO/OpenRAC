//! A game's differences from the original: what its PC port does otherwise than the PS2 game,
//! with the game fixes the player can switch back.

use openrac_launcher_core::differences::{self, Differences};
use openrac_launcher_core::launcher_log;

use crate::launcher::Launcher;

impl Launcher {
    fn differences_root(&self) -> Option<std::path::PathBuf> {
        self.config().root.or_else(|| self.inner.bundled.clone())
    }

    /// The differences of `game` in disc `version` (`pal`), with the fixes' values. Blocks.
    pub fn differences(&self, game: &str, version: &str) -> Result<Differences, String> {
        let home = openrac_launcher_core::saves::data_home();
        differences::view(self.differences_root().as_deref(), &home, game, version)
    }

    /// Switches a game fix on (the port's) or off (the console's behaviour). Blocks.
    pub fn set_game_fix(&self, game: &str, version: &str, id: &str, on: bool) -> Result<Differences, String> {
        let home = openrac_launcher_core::saves::data_home();
        let done = differences::set_fix(self.differences_root().as_deref(), &home, game, version, id, on)?;
        launcher_log::line(&format!(
            "{game}/{version}: game fix {id} {}",
            if on { "on" } else { "off (the original's)" }
        ));
        Ok(done)
    }
}
