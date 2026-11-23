//! A game's logs and crash reports: the list Help shows, and copying, saving and deleting them.

use openrac_launcher_core::launcher_log;
use openrac_launcher_core::reports::{self, ReportEntry};

use crate::launcher::{game_data, Launcher};

impl Launcher {
    fn data_of(&self, game: &str) -> Result<std::path::PathBuf, String> {
        game_data(&self.config(), game).ok_or_else(|| "no install folder (Settings)".to_string())
    }

    /// `game`'s logs and crash reports, the newest first. Blocks.
    pub fn reports(&self, game: &str) -> Result<Vec<ReportEntry>, String> {
        Ok(reports::list(&self.data_of(game)?))
    }

    /// Puts a report on the clipboard, with the launcher's version: the text to paste in an
    /// issue. Blocks.
    pub fn copy_report(&self, game: &str, file: &str) -> Result<(), String> {
        let text = reports::copy_text(&self.data_of(game)?, file, self.version())?;
        self.host().copy(&text);
        Ok(())
    }

    /// Saves a report as a .zip with the launcher's log, where the host keeps such files.
    /// Returns where it went, or None when the player backed out. Blocks.
    pub fn save_report(&self, game: &str, file: &str) -> Result<Option<String>, String> {
        let data = self.data_of(game)?;
        let folder = self.host().hand_over_folder().ok_or("no folder to save in")?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let out = folder.join(reports::bundle_name(file));
        reports::bundle(&data, file, launcher_log::path().as_deref(), self.version(), &out)?;
        let kept = self.host().hand_over(&out)?;
        if let Some(place) = &kept {
            launcher_log::line(&format!("saved a report: {place}"));
        }
        Ok(kept)
    }

    /// Deletes `files` of `game`'s logs and reports, or all of them when None. Returns how many
    /// went. Blocks.
    pub fn delete_reports(&self, game: &str, files: Option<&[String]>) -> Result<usize, String> {
        let data = self.data_of(game)?;
        let count = match files {
            Some(files) => {
                for file in files {
                    reports::delete(&data, game, file)?;
                }
                files.len()
            }
            None => reports::delete_all(&data, game)?,
        };
        launcher_log::line(&format!("deleted {count} reports of {game}"));
        // The crash counts and the page's last-played line follow.
        self.refresh();
        Ok(count)
    }

    /// Shows the folder `game`'s logs are in.
    pub fn open_reports_folder(&self, game: &str) -> Result<(), String> {
        let data = self.data_of(game)?;
        if !data.is_dir() {
            return Err("nothing has been logged for this game yet".into());
        }
        self.host().open_path(&data)
    }

    /// Detailed logs: the game writes more about what it does.
    pub fn set_detailed_log(&self, on: bool) {
        self.update_config(|c| c.detailed_game_log = on);
    }
}
