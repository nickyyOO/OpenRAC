//! The Saves block on a game's page: the memory card of the version shown, and its backups.

use std::cell::RefCell;

use openrac_launcher_service::core::saves::SaveSlotInfo;
use openrac_launcher_service::{backup_label, empty_slots, Card, Launcher};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::shell::{self, Shell};
use crate::{words, AppWindow, BackupRow, Games, Saves, SlotRow};

/// The card last read, by its disc's serial.
#[derive(Default)]
pub struct SavesState {
    card: RefCell<Option<(String, Card)>>,
}

pub fn install(window: &AppWindow) {
    let saves = window.global::<Saves>();
    saves.on_refresh(|| shell::with(|s| s.read_card()));
    saves.on_open_folder(|| on_card(|l, serial, _| l.spawn(move |l| l.open_saves_folder(&serial))));
    saves.on_back_up(|note| {
        on_card(move |l, serial, _| {
            shell::with(|s| s.window().global::<Saves>().set_creating(true));
            l.spawn(move |l| {
                let done = l.back_up_saves(&serial, Some(note.as_str()));
                let _ =
                    slint::invoke_from_event_loop(|| shell::with(|s| s.window().global::<Saves>().set_creating(false)));
                done
            })
        })
    });
    saves.on_restore(|name| {
        on_card(move |l, serial, card| {
            let Some(backup) = card.status.backups.iter().find(|b| b.name == name.as_str()).cloned() else { return };
            let when = words::when(backup.created_millis / 1000);
            shell::with(|s| s.window().global::<Saves>().set_restoring(name.clone()));
            l.spawn(move |l| {
                let done = l.restore_saves(&serial, &backup, &when);
                let _ = slint::invoke_from_event_loop(|| {
                    shell::with(|s| s.window().global::<Saves>().set_restoring("".into()))
                });
                done
            })
        })
    });
}

/// Runs `work` with the launcher, the shown disc's serial and its card as last read.
fn on_card(work: impl FnOnce(&Launcher, String, &Card)) {
    shell::with(|s| {
        let Some(launcher) = s.launcher() else { return };
        let card = s.saves.card.borrow();
        if let Some((serial, card)) = card.as_ref() {
            work(launcher, serial.clone(), card);
        }
    });
}

/// The serial of the disc whose page is shown.
pub fn shown_serial(shell: &Shell) -> Option<String> {
    let launcher = shell.launcher()?;
    let key = shell.window().global::<Games>().get_selected();
    let view = launcher.view();
    let library = view.library.as_ref()?;
    let game = library.games.iter().find(|g| g.versions.iter().any(|v| v.version.key == key.as_str()))?;
    let v = openrac_launcher_service::player_version(game, &view)?;
    Some(v.version.serial.clone()).filter(|s| !s.is_empty())
}

fn slot_row(slot: &SaveSlotInfo) -> SlotRow {
    let mods = slot.mods.as_ref().is_some_and(|m| !m.is_empty());
    SlotRow {
        index: slot.slot_index as i32,
        label: format!("Slot {}", slot.slot_index + 1).into(),
        planet: slot.planet_name.clone().unwrap_or_else(|| "Unknown planet".into()).into(),
        bolts: words::grouped(u64::from(slot.bolts.unwrap_or_default())).into(),
        when: slot.timestamp.as_deref().map(words::stamp).unwrap_or_default().into(),
        badge: if mods { "Mods".into() } else { "".into() },
        editable: slot.editable,
        detail: format!("{} · {}", slot.filename, words::size(slot.size)).into(),
    }
}

impl SavesState {
    /// A new disc is shown: nothing of the last one's card stays.
    pub fn reset(&self, window: &AppWindow) {
        self.card.borrow_mut().take();
        let saves = window.global::<Saves>();
        saves.set_read(false);
        saves.set_show_backups(false);
    }

    /// Reads the card of `serial` off the window's thread, then shows it.
    pub fn read(&self, launcher: &Launcher, serial: String) {
        let launcher = launcher.clone();
        std::thread::spawn(move || {
            let card = launcher.card(&serial);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    // Another disc was picked meanwhile: its own read shows it.
                    if shown_serial(s).as_deref() == Some(serial.as_str()) {
                        *s.saves.card.borrow_mut() = Some((serial, card));
                        s.saves.show(s);
                    }
                })
            });
        });
    }

    fn show(&self, shell: &Shell) {
        let window = shell.window();
        let saves = window.global::<Saves>();
        let card = self.card.borrow();
        let Some((_, card)) = card.as_ref() else { return };
        let status = &card.status;
        saves.set_developer(shell.launcher().is_some_and(|l| l.config().developer));
        saves.set_exists(status.exists);
        saves.set_folder(status.game_folder_name.clone().unwrap_or_else(|| status.serial.clone()).into());
        let used: Vec<SlotRow> = status.slots.iter().filter(|s| !s.is_empty).map(slot_row).collect();
        let empty: Vec<usize> = status.slots.iter().filter(|s| s.is_empty).map(|s| s.slot_index + 1).collect();
        saves.set_slots(ModelRc::new(VecModel::from(used)));
        saves.set_empty(empty_slots(&empty).into());
        let coop: Vec<SlotRow> = status
            .coop_saves
            .iter()
            .map(|c| SlotRow {
                index: c.slot_index as i32,
                label: format!("Co-op {}", c.slot_index + 1).into(),
                planet: c.planet_name.clone().unwrap_or_else(|| "Unknown planet".into()).into(),
                bolts: words::grouped(u64::from(c.bolts.unwrap_or_default())).into(),
                when: c.timestamp.as_deref().map(words::stamp).unwrap_or_default().into(),
                badge: c.host.clone().unwrap_or_default().into(),
                editable: false,
                detail: "".into(),
            })
            .collect();
        saves.set_coop(ModelRc::new(VecModel::from(coop)));
        let backups: Vec<BackupRow> = status
            .backups
            .iter()
            .map(|b| BackupRow {
                name: b.name.as_str().into(),
                label: backup_label(b).into(),
                when: words::when(b.created_millis / 1000).into(),
                size: words::size(b.total_size).into(),
            })
            .collect();
        saves.set_backups(ModelRc::new(VecModel::from(backups)));
        let copy = match card.copy_sources.as_slice() {
            [] => String::new(),
            [only] => format!("Copy from {}", only.region),
            _ => "Copy from…".into(),
        };
        saves.set_copy_text(copy.into());
        saves.set_read(true);
    }
}
