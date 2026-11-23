//! A game's page on a desktop: its own Steam entry and a desktop shortcut.

use openrac_launcher_service::core::steam::State;
use openrac_launcher_service::Launcher;
use slint::ComponentHandle;

use crate::shell::{self, Shell};
use crate::{steam, AppWindow, Games, Shortcuts};

pub fn install(window: &AppWindow) {
    let ui = window.global::<Shortcuts>();
    ui.on_add_to_steam(|| {
        run("steam", |launcher, key| {
            steam::add(&launcher.config(), Some(key))
                .map(|state| (state, "Added. If Steam is open and does not show it, restart Steam.".to_string()))
        })
    });
    ui.on_create_shortcut(|| {
        run("desktop", |launcher, key| {
            let path = steam::create_shortcut(&launcher.config(), key)?;
            let state = steam::state(&launcher.config(), Some(key)).unwrap_or(State::NoSteam);
            Ok((state, format!("Shortcut written to {path}.")))
        })
    });
}

/// Does `work` for the shown game off the window's thread, then says how it went.
fn run(busy: &str, work: impl FnOnce(&Launcher, &str) -> Result<(State, String), String> + Send + 'static) {
    shell::with(|s| {
        let Some(launcher) = s.launcher().cloned() else { return };
        let key = s.window().global::<Games>().get_selected().to_string();
        let ui = s.window().global::<Shortcuts>();
        ui.set_busy(busy.into());
        ui.set_said("".into());
        std::thread::spawn(move || {
            let done = work(&launcher, &key);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    let ui = s.window().global::<Shortcuts>();
                    ui.set_busy("".into());
                    match done {
                        Ok((state, said)) => {
                            show_steam(s.window(), state);
                            ui.set_failed(false);
                            ui.set_said(said.into());
                        }
                        Err(e) => {
                            ui.set_failed(true);
                            ui.set_said(e.into());
                        }
                    }
                })
            });
        });
    });
}

fn show_steam(window: &AppWindow, state: State) {
    let ui = window.global::<Shortcuts>();
    let (offers_steam, _) = steam::offers();
    ui.set_steam_shown(offers_steam && state != State::NoSteam);
    let line = match state {
        State::FlatpakSteam => "Flatpak Steam can't start OpenRAC. Use Steam's normal package to add it.",
        State::NoUser => "Sign in to Steam once to add OpenRAC to it.",
        State::Added => "In your Steam library.",
        _ => "Add this game to your Steam library as its own entry. It starts without the launcher's window.",
    };
    ui.set_steam_line(line.into());
    let button = match state {
        State::Added => "In Steam",
        State::Outdated => "Update in Steam",
        _ => "Add to Steam",
    };
    ui.set_steam_button(button.into());
    ui.set_steam_enabled(matches!(state, State::NotAdded | State::Outdated));
}

/// Reads where the shown game stands in Steam, off the window's thread.
pub fn read(shell: &Shell) {
    let Some(launcher) = shell.launcher().cloned() else { return };
    let window = shell.window();
    let key = window.global::<Games>().get_selected().to_string();
    let ui = window.global::<Shortcuts>();
    let (offers_steam, offers_desktop) = steam::offers();
    ui.set_desktop_shown(offers_desktop);
    ui.set_steam_shown(false);
    ui.set_said("".into());
    if !offers_steam || key.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let state = steam::state(&launcher.config(), Some(&key)).unwrap_or(State::NoSteam);
        let _ = slint::invoke_from_event_loop(move || {
            shell::with(|s| {
                if s.window().global::<Games>().get_selected() == key.as_str() {
                    show_steam(s.window(), state);
                }
            })
        });
    });
}
