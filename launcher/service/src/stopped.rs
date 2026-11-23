//! The notice on a game's page after it stopped unexpectedly: the report to send us, the mods it
//! ran with, and the mod updates that came just before (each can be rolled back).

use openrac_launcher_core::mod_updates::{self, UpdatedMod};
use openrac_launcher_core::plugins;

use crate::host::Notice;
use crate::launcher::{Launcher, Stopped};

/// What the notice reads from files, off the window's thread.
#[derive(Debug, Clone, Default)]
pub struct StoppedMods {
    /// The gameplay mods the game loaded at its last start: a mod may be what crashed.
    pub loaded: Vec<String>,
    /// The mod updates that came before the crash.
    pub updated: Vec<UpdatedMod>,
}

/// The text to copy when the game stopped without writing a report: what happened, and the
/// launcher's version. Not a crash: there is nothing to report.
pub fn stop_text(stopped: &Stopped, title: &str, launcher_version: &str) -> String {
    let how = match stopped.code {
        None => "it was closed (by you, or by the system)".to_string(),
        Some(code) => format!("it ended with exit code {code}"),
    };
    let tail = stopped.lines[stopped.lines.len().saturating_sub(25)..].join("\n");
    let tail = if tail.is_empty() { "(none)".to_string() } else { tail };
    format!(
        "**OpenRAC: {title} stopped**\n\n- What happened: {how}, without a crash report\n- Launcher: {launcher_version}\n\nThe game's last lines:\n```\n{tail}\n```"
    )
}

/// "Gameplay mods were on (a, b). If it crashes again, …".
pub fn crash_mods_line(ids: &[String]) -> String {
    format!(
        "Gameplay mods were on ({}). If it crashes again, turn off Mods under Play, and tell us which mods you had.",
        ids.join(", ")
    )
}

/// "This came after A and B updated. …".
pub fn crash_after_update(names: &[String]) -> String {
    let list = match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("This came after {list} updated. Rolling back keeps the earlier version until a fixed one is out.")
}

impl Launcher {
    fn plugins_dir(&self, game: &str) -> Option<std::path::PathBuf> {
        self.config().install_dir.map(|install| plugins::dir(&install, game))
    }

    /// The mods behind a crash. Blocks.
    pub fn stopped_mods(&self, stopped: &Stopped) -> StoppedMods {
        let Some(dir) = self.plugins_dir(&stopped.game).filter(|_| stopped.crash.is_some()) else {
            return StoppedMods::default();
        };
        let loaded = plugins::read_status(&dir)
            .filter(|s| !s.purist)
            .map(|s| s.mods.into_iter().filter(|m| m.state == "loaded").map(|m| m.id).collect())
            .unwrap_or_default();
        let updated = mod_updates::last_update(&dir)
            .filter(|last| last.at * 1000 <= stopped.at)
            .map(|last| last.mods)
            .unwrap_or_default();
        StoppedMods { loaded, updated }
    }

    /// After a crash: the mod `id` back at the version its update replaced, kept there. Blocks.
    pub fn crash_rollback(&self, stopped: &Stopped, id: &str) -> Result<(), String> {
        let dir = self.plugins_dir(&stopped.game).ok_or("no install folder (Settings)")?;
        let disc = stopped.version.split('/').nth(1).unwrap_or_default();
        mod_updates::crash_rollback(&dir, &stopped.game, disc, id)?;
        Ok(())
    }

    /// Copies the crash report, or what happened when the game wrote none. Blocks.
    pub fn copy_stopped(&self, stopped: &Stopped, title: &str) -> Result<(), String> {
        match &stopped.crash {
            Some(crash) => self.copy_report(&stopped.game, &crash.file),
            None => {
                self.host().copy(&stop_text(stopped, title, self.version()));
                Ok(())
            }
        }
    }

    /// Saves the crash report (or the session's log) as a file. Blocks.
    pub fn save_stopped(&self, stopped: &Stopped) -> Result<Option<String>, String> {
        let file = match &stopped.crash {
            Some(crash) => crash.file.clone(),
            None => format!("openrac-{}.log", stopped.version.replace('/', "-")),
        };
        self.save_report(&stopped.game, &file)
    }

    /// Shows the folder of the game's crash reports (its logs' when it wrote none).
    pub fn open_stopped_folder(&self, stopped: &Stopped) -> Result<(), String> {
        let data = crate::launcher::game_data(&self.config(), &stopped.game).ok_or("no install folder (Settings)")?;
        let crashes = data.join("crashes");
        if stopped.crash.is_some() && crashes.is_dir() {
            return self.host().open_path(&crashes);
        }
        self.open_reports_folder(&stopped.game)
    }

    /// Says what a rollback did.
    pub fn rolled_back(&self, name: &str, version: &str) {
        self.notify(Notice::info(format!("{name} is back at {version}.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_as_a_list() {
        assert!(crash_after_update(&["A".into(), "B".into(), "C".into()]).starts_with("This came after A, B and C updated."));
        assert!(crash_after_update(&["A".into()]).starts_with("This came after A updated."));
    }
}
