//! What a game's page does besides its main button: the menu beside it, the Mods switch, the
//! disc switch, the line on when it was last played.

use std::path::PathBuf;

use openrac_launcher_core::{detect, disc_choice, enhanced, install, launcher_log, overrides, pcsx2_card, plugins};

use crate::host::{Change, Notice};
use crate::launcher::{scan, Launcher};

impl Launcher {
    /// Removes what setting `game` up made, after the player confirmed. The disc image and the
    /// saves are kept. Blocks.
    pub fn remove_set_up(&self, game: &str, title: &str) -> Result<(), String> {
        let sure = self.host().confirm(
            &format!(
                "Remove what was set up from your {title} disc? Your disc image and your saves are kept; set it up again any time."
            ),
            "Remove",
        );
        if !sure {
            return Ok(());
        }
        let install = self.config().install_dir.ok_or("no install folder (Settings)")?;
        install::uninstall(&install, game)?;
        launcher_log::line(&format!("{game}: the set-up was removed"));
        self.refresh();
        Ok(())
    }

    /// Developers only: plays `version`'s disc of its game from now on.
    pub fn choose_disc(&self, version: &str) -> Result<(), String> {
        let mut config = self.config();
        let (catalog, found) = scan(&config)?;
        disc_choice::choose(&mut config, &catalog, &found, version)?;
        self.store_config(&config)?;
        self.refresh();
        Ok(())
    }

    /// The game's saves from a PCSX2 memory card onto OpenRAC's card, after a backup of it.
    /// Blocks.
    pub fn import_pcsx2_saves(&self, serial: &str) -> Result<(), String> {
        let start = pcsx2_card::pcsx2_memcards_dir();
        let Some(source) = self.host().pick_file(
            "Choose a PCSX2 memory card",
            "PS2 memory card",
            &["ps2", "mcd", "mc2", "bin"],
            start.as_deref(),
        ) else {
            return Ok(());
        };
        let yes = self.host().confirm(
            "Copy this game's saves from the PCSX2 memory card? The saves on OpenRAC's card are backed up first, then replaced.",
            "Import saves",
        );
        if !yes {
            return Ok(());
        }
        self.card_free(serial)?;
        let done = pcsx2_card::import_saves(serial, &source).map_err(|e| format!("Import failed: {e}"))?;
        self.changed(Change::GameData);
        let kept = if done.backup.is_some() { " The saves that were there are in Backups." } else { "" };
        self.notify(Notice::info(format!("Imported the saves from PCSX2.{kept}")));
        Ok(())
    }

    /// Fails while a game runs with `serial`'s memory card: its saves cannot change under it.
    pub fn card_free(&self, serial: &str) -> Result<(), String> {
        let card = openrac_launcher_core::saves::get_memcard_dir(serial);
        let holder = match self.android_game() {
            Some(library) => cfg!(target_os = "android").then_some(library),
            None => openrac_launcher_core::save_edit::card_in_use(&card, serial)?,
        };
        match holder {
            Some(game) if !game.is_empty() => Err(format!("{game} is running with this memory card: close it first")),
            Some(_) => Err("The game is running with this memory card: close it first".into()),
            None => Ok(()),
        }
    }

    /// Opens a file or folder of the checkout (a README, baserom/, a game's folder) with the
    /// system's program. Paths outside the checkout are refused. An installed launcher's own
    /// folder is read-only: the folders a checkout keeps under assets/ are then the player's, in
    /// the install folder.
    pub fn open_in_checkout(&self, path: &str) -> Result<(), String> {
        let folder = self.checkout_path(path)?;
        self.host().open_path(&folder)
    }

    fn checkout_path(&self, path: &str) -> Result<PathBuf, String> {
        let config = self.config();
        let root = config.root.clone().ok_or("the OpenRAC folder is not set")?;
        let rel = path.split('#').next().unwrap_or_default();
        let on_copy = self.bundled().is_some_and(|copy| detect::on_the_copy(Some(&root), copy));
        if let (true, Some(install)) = (on_copy, config.install_dir.as_deref()) {
            if let Some(folder) = install::player_folder(install, rel) {
                std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
                return Ok(folder);
            }
        }
        let root = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let wanted = openrac_launcher_core::under(&root, rel);
        if rel.starts_with("assets/") && !rel.split('/').any(|part| part == "..") && !wanted.exists() {
            // A folder the tools write before they first have: opened empty rather than refused.
            std::fs::create_dir_all(&wanted).map_err(|e| format!("{path}: {e}"))?;
        }
        let target = std::fs::canonicalize(wanted).map_err(|e| format!("{path}: {e}"))?;
        if !target.starts_with(&root) {
            return Err(format!("{path} is outside the OpenRAC folder"));
        }
        Ok(target)
    }

    /// Turns the Mods switch under Play: off, Play starts the game without mods; on, with what the
    /// Mods tab switched on.
    pub fn set_play_with_mods(&self, on: bool) {
        self.update_config(|c| c.play_with_mods = on);
    }

    /// How many mods the Mods tab has switched on for `game` in disc `version` (`pal`): its
    /// gameplay mods, its packs and Enhanced graphics. Blocks.
    pub fn mods_on_count(&self, game: &str, version: &str) -> usize {
        let Some(install) = self.config().install_dir else { return 0 };
        let mods = plugins::list(&plugins::dir(&install, game), game, version).unwrap_or_default();
        let packs = overrides::list(&overrides::dir(&install, game)).unwrap_or_default();
        let enhanced = enhanced::supported(game, version)
            && enhanced::view(&openrac_launcher_core::saves::data_home(), game, version, Default::default())
                .entry
                .enabled;
        mods.iter().filter(|m| m.enabled).count() + packs.iter().filter(|p| p.enabled).count() + usize::from(enhanced)
    }
}
