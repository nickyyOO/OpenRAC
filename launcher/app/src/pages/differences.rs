//! The Differences block on a game's page: read off the window's thread, the fixes switched there.

use std::cell::RefCell;

use openrac_launcher_service::core::differences::{DifferenceView, Differences as GameDifferences, Kind};
use openrac_launcher_service::{is_playing, Launcher};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::shell::{self, Shell};
use crate::{AppWindow, DiffGroup, DiffRow, Differences, Games};

/// The differences on show, by the version they are of.
#[derive(Default)]
pub struct DifferencesState {
    shown: RefCell<Option<(String, GameDifferences)>>,
}

pub fn install(window: &AppWindow) {
    window.global::<Differences>().on_set_fix(|id, on| {
        shell::with(|s| {
            let Some(launcher) = s.launcher() else { return };
            let Some((key, _)) = s.differences.shown.borrow().clone() else { return };
            let Some((game, disc)) = key.split_once('/') else { return };
            let (game, disc) = (game.to_string(), disc.to_string());
            s.window().global::<Differences>().set_saving(id.clone());
            launcher.spawn(move |l| {
                let done = l.set_game_fix(&game, &disc, &id, on);
                let failed = done.as_ref().err().cloned();
                let _ = slint::invoke_from_event_loop(move || {
                    shell::with(|s| {
                        s.window().global::<Differences>().set_saving("".into());
                        if let Ok(d) = done {
                            *s.differences.shown.borrow_mut() = Some((key, d));
                        }
                        s.show_differences();
                    })
                });
                failed.map_or(Ok(()), |e| Err(format!("The fix could not be changed: {e}")))
            });
        })
    });
}

fn row(entry: &DifferenceView) -> DiffRow {
    let refs =
        entry.setting.as_ref().map(|s| s.key.clone()).into_iter().chain(entry.refs.iter().cloned()).collect::<Vec<_>>();
    DiffRow {
        id: entry.id.as_str().into(),
        name: entry.name.as_str().into(),
        switchable: entry.kind == Kind::Fix,
        on: entry.on.unwrap_or(true),
        state: match entry.kind {
            Kind::Guard | Kind::Always => "Always on",
            _ => "",
        }
        .into(),
        original: entry.original.as_str().into(),
        port: entry.port.as_str().into(),
        refs: refs.join(" · ").into(),
    }
}

/// The entries in their groups: the fixes (with the ones always on), the guards, the PC's own.
fn groups(entries: &[DifferenceView], playing: bool) -> Vec<DiffGroup> {
    let all = [
        (
            "Game fixes",
            "On, the port's fix; off, the game plays as on a PlayStation 2. The game's Options menu has the same switches. Some are always on.",
            &[Kind::Fix, Kind::Always][..],
        ),
        (
            "Kept from crashing",
            "Where a PlayStation 2 carried on unnoticed but a PC would crash. Always on: off would only bring the crash back.",
            &[Kind::Guard][..],
        ),
        ("Playing on a PC", "What comes with the game being a PC program.", &[Kind::Port][..]),
    ];
    all.iter()
        .map(|(title, about, kinds)| {
            let rows: Vec<DiffRow> = entries.iter().filter(|e| kinds.contains(&e.kind)).map(row).collect();
            DiffGroup {
                title: (*title).into(),
                about: (*about).into(),
                locked: playing && kinds.contains(&Kind::Fix),
                rows: ModelRc::new(VecModel::from(rows)),
            }
        })
        .filter(|g| slint::Model::row_count(&g.rows) > 0)
        .collect()
}

impl DifferencesState {
    /// Reads the differences of the version `key` off the window's thread, then shows them.
    pub fn read(&self, launcher: &Launcher, key: String) {
        self.shown.borrow_mut().take();
        let Some((game, disc)) = key.split_once('/').map(|(g, d)| (g.to_string(), d.to_string())) else { return };
        let launcher = launcher.clone();
        std::thread::spawn(move || {
            let read = launcher.differences(&game, &disc).ok();
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    if s.window().global::<Games>().get_selected() == key.as_str() {
                        *s.differences.shown.borrow_mut() = read.map(|d| (key, d));
                        s.show_differences();
                    }
                })
            });
        });
    }

    pub fn show(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let ui = shell.window().global::<Differences>();
        let shown = self.shown.borrow();
        let Some((key, d)) = shown.as_ref() else {
            ui.set_shown(false);
            return;
        };
        let jobs = launcher.jobs();
        let view = launcher.view();
        let playing = view
            .library
            .as_ref()
            .and_then(|l| l.games.iter().flat_map(|g| &g.versions).find(|v| &v.version.key == key))
            .is_some_and(|v| is_playing(&jobs, v));
        let groups = groups(&d.entries, playing);
        ui.set_shown(!groups.is_empty());
        ui.set_developer(launcher.config().developer);
        ui.set_settings_file(d.settings_file.as_str().into());
        ui.set_groups(ModelRc::new(VecModel::from(groups)));
    }
}
