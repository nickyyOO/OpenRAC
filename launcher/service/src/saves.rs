//! A game's memory card: what the Saves block shows, and its backups.

use openrac_launcher_core::save_transfer::{self, SaveCopySource};
use openrac_launcher_core::saves::{self, GameSaveStatus, SaveBackupInfo};

use crate::host::{Change, Notice};
use crate::launcher::Launcher;

/// A game's card as the Saves block shows it.
#[derive(Debug, Clone)]
pub struct Card {
    pub status: GameSaveStatus,
    /// The game's other discs whose cards hold saves this one can copy (PAL and US).
    pub copy_sources: Vec<SaveCopySource>,
}

/// What a backup is called: its note, starting with a capital, else "Backup".
pub fn backup_label(backup: &SaveBackupInfo) -> String {
    let note = backup.note.as_deref().unwrap_or_default().trim();
    let mut chars = note.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Backup".into(),
    }
}

/// The empty slots in one line, as people count them: "Slots 3 to 5 are empty".
pub fn empty_slots(numbers: &[usize]) -> String {
    let mut n = numbers.to_vec();
    n.sort_unstable();
    match n.as_slice() {
        [] => String::new(),
        [one] => format!("Slot {one} is empty"),
        [first, .., last] if n.windows(2).all(|w| w[1] == w[0] + 1) => format!("Slots {first} to {last} are empty"),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(|x| x.to_string()).collect();
            format!("Slots {} and {last} are empty", rest.join(", "))
        }
    }
}

impl Launcher {
    /// The card of the disc `serial`, and the other discs to copy from. Blocks.
    pub fn card(&self, serial: &str) -> Card {
        Card { status: saves::inspect_saves(serial), copy_sources: save_transfer::copy_sources(serial) }
    }

    /// Backs the card up, with the player's note. Blocks.
    pub fn back_up_saves(&self, serial: &str, note: Option<&str>) -> Result<(), String> {
        let done = saves::backup_saves(serial, note.filter(|n| !n.trim().is_empty()))
            .map_err(|e| format!("Backup failed: {e}"))?;
        self.notify(Notice::info(format!("Backed up: {}.", backup_label(&done))));
        self.changed(Change::GameData);
        Ok(())
    }

    /// Puts a backup back, after the player confirmed; the saves there now are backed up first.
    /// `when`: the backup's time in the player's words. Blocks.
    pub fn restore_saves(&self, serial: &str, backup: &SaveBackupInfo, when: &str) -> Result<(), String> {
        let label = format!("{} ({when})", backup_label(backup));
        let sure = self.host().confirm(
            &format!("Restore the backup \"{label}\"? Your current saves are backed up first, then replaced."),
            "Restore",
        );
        if !sure {
            return Ok(());
        }
        self.card_free(serial)?;
        saves::restore_backup(serial, &backup.name).map_err(|e| format!("Restore failed: {e}"))?;
        self.notify(Notice::info(format!("Restored {label}.")));
        self.changed(Change::GameData);
        Ok(())
    }

    /// Shows the card's folder, made first when there is none yet.
    pub fn open_saves_folder(&self, serial: &str) -> Result<(), String> {
        let dir = saves::get_memcard_dir(serial);
        let _ = std::fs::create_dir_all(&dir);
        self.host().open_path(&dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_slots_read_as_a_sentence() {
        assert_eq!(empty_slots(&[]), "");
        assert_eq!(empty_slots(&[4]), "Slot 4 is empty");
        assert_eq!(empty_slots(&[5, 3, 4]), "Slots 3 to 5 are empty");
        assert_eq!(empty_slots(&[1, 3, 5]), "Slots 1, 3 and 5 are empty");
    }
}
