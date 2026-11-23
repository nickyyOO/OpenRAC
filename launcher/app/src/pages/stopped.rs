//! The notice on a game's page after it stopped unexpectedly, and what its buttons do.

use std::cell::RefCell;
use std::time::Duration;

use openrac_launcher_service::{crash_after_update, crash_mods_line, Launcher, Stopped as StoppedGame, StoppedMods};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::shell::{self, Shell};
use crate::{words, App, AppWindow, Games, Links, RollBack, Stopped};

/// The mods behind the stop on show, read off the window's thread: by when it stopped.
#[derive(Default)]
pub struct StoppedState {
    mods: RefCell<Option<(u64, StoppedMods)>>,
}

pub fn install(window: &AppWindow) {
    let stopped = window.global::<Stopped>();
    stopped.on_copy(|| {
        on_stopped(|l, s, title| {
            l.spawn(move |l| {
                l.copy_stopped(&s, &title)?;
                let _ = slint::invoke_from_event_loop(|| shell::with(|s| copied(s.window())));
                Ok(())
            })
        })
    });
    stopped.on_save(|| {
        on_stopped(|l, s, _| {
            shell::with(|sh| sh.window().global::<Stopped>().set_saving(true));
            l.spawn(move |l| {
                let saved = l.save_stopped(&s);
                let place = saved.as_ref().ok().cloned().flatten().unwrap_or_default();
                let _ = slint::invoke_from_event_loop(move || {
                    shell::with(|sh| {
                        let ui = sh.window().global::<Stopped>();
                        ui.set_saving(false);
                        ui.set_saved_to(place.into());
                    })
                });
                saved.map(|_| ())
            })
        })
    });
    stopped.on_dismiss(|| {
        on_stopped(|l, _, _| {
            l.spawn(|l| {
                l.dismiss_stopped();
                Ok(())
            })
        })
    });
    stopped.on_open_folder(|| on_stopped(|l, s, _| l.spawn(move |l| l.open_stopped_folder(&s))));
    stopped.on_open_discord(|| open_url(|links| links.get_discord()));
    stopped.on_open_issues(|| open_url(|links| links.get_issues()));
    stopped.on_roll_back(|id| {
        on_stopped(move |l, s, _| {
            let updated = shell_mods(&s).into_iter().find(|m| m.id == id.as_str());
            let Some(m) = updated else { return };
            l.spawn(move |l| {
                l.crash_rollback(&s, &m.id)?;
                l.rolled_back(&m.name, &m.from);
                let _ = slint::invoke_from_event_loop(move || {
                    shell::with(|sh| {
                        if let Some((_, mods)) = sh.stopped.mods.borrow_mut().as_mut() {
                            mods.updated.retain(|u| u.id != m.id);
                        }
                        sh.show_stopped();
                    })
                });
                Ok(())
            })
        })
    });
}

fn shell_mods(stopped: &StoppedGame) -> Vec<openrac_launcher_service::core::mod_updates::UpdatedMod> {
    let mut found = Vec::new();
    shell::with(|s| {
        if let Some((at, mods)) = s.stopped.mods.borrow().as_ref() {
            if *at == stopped.at {
                found = mods.updated.clone();
            }
        }
    });
    found
}

fn open_url(link: impl FnOnce(&Links) -> slint::SharedString) {
    shell::with(|s| {
        let url = link(&s.window().global::<Links>());
        s.window().global::<App>().invoke_open_url(url);
    });
}

fn copied(window: &AppWindow) {
    window.global::<Stopped>().set_copied(true);
    let weak = window.as_weak();
    slint::Timer::single_shot(Duration::from_secs(4), move || {
        if let Some(window) = weak.upgrade() {
            window.global::<Stopped>().set_copied(false);
        }
    });
}

/// The stopped game's title as its page names it.
fn title_of(launcher: &Launcher, version: &str) -> String {
    let view = launcher.view();
    view.library
        .as_ref()
        .and_then(|l| l.games.iter().flat_map(|g| &g.versions).find(|v| v.version.key == version))
        .map(|v| v.version.title.clone())
        .unwrap_or_else(|| "The game".into())
}

/// Runs `work` with the launcher, the stop on show and the game's title.
fn on_stopped(work: impl FnOnce(&Launcher, StoppedGame, String)) {
    shell::with(|s| {
        let Some(launcher) = s.launcher() else { return };
        if let Some(stopped) = launcher.stopped() {
            let title = title_of(launcher, &stopped.version);
            work(launcher, stopped, title);
        }
    });
}

/// "today at 12:19": inside a sentence.
fn lower_when(at_ms: u64) -> String {
    let day = words::when(at_ms / 1000);
    let mut chars = day.chars();
    chars.next().map(|c| c.to_lowercase().chain(chars).collect()).unwrap_or_default()
}

impl StoppedState {
    /// Reads the mods behind the stop, off the window's thread.
    pub fn read(&self, launcher: &Launcher) {
        let Some(stopped) = launcher.stopped() else { return };
        let launcher = launcher.clone();
        std::thread::spawn(move || {
            let mods = launcher.stopped_mods(&stopped);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    *s.stopped.mods.borrow_mut() = Some((stopped.at, mods));
                    s.show_stopped();
                })
            });
        });
    }

    pub fn show(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let window = shell.window();
        let ui = window.global::<Stopped>();
        let key = window.global::<Games>().get_selected();
        let game = key.split('/').next().unwrap_or_default();
        let Some(stopped) = launcher.stopped().filter(|s| s.game == game) else {
            ui.set_shown(false);
            return;
        };
        let title = title_of(launcher, &stopped.version);
        let crash = stopped.crash.as_ref();
        let when = lower_when(stopped.at);
        ui.set_shown(true);
        ui.set_short(stopped.played_since);
        ui.set_crash(crash.is_some());
        ui.set_heading(
            match (crash, stopped.code) {
                (Some(_), _) => "The game crashed",
                (None, None) => "The game was closed",
                (None, Some(_)) => "The game stopped",
            }
            .into(),
        );
        let say = match (stopped.played_since, crash) {
            (true, Some(_)) => format!("{title} crashed {when}. It has run fine since; the report can still help us."),
            (true, None) => format!("{title} was closed {when}. It has run fine since."),
            (false, Some(c)) => format!(
                "{title} crashed while you were {}. This is a problem in OpenRAC, not something you did, and your saves are kept. Copy the report and paste it in a message to us on Discord: it helps us fix it.",
                words::place(c)
            ),
            (false, None) => format!(
                "{title} was closed without a crash report, so nothing is wrong: your saves are kept, and there is nothing to send us. Its log is under Logs and crash reports, if you want to look at it."
            ),
        };
        ui.set_say(say.into());

        let mods = self.mods.borrow().as_ref().filter(|(at, _)| *at == stopped.at).map(|(_, m)| m.clone()).unwrap_or_default();
        let mods_line = if crash.is_some() && !mods.loaded.is_empty() { crash_mods_line(&mods.loaded) } else { String::new() };
        ui.set_mods_line(mods_line.into());
        let names: Vec<String> = mods.updated.iter().map(|m| m.name.clone()).collect();
        let update_line = if crash.is_some() && !names.is_empty() { crash_after_update(&names) } else { String::new() };
        ui.set_update_line(update_line.into());
        let roll_backs: Vec<RollBack> = mods
            .updated
            .iter()
            .filter(|_| crash.is_some())
            .map(|m| RollBack { id: m.id.as_str().into(), label: format!("Roll back {} to {}", m.name, m.from).into() })
            .collect();
        ui.set_roll_backs(ModelRc::new(VecModel::from(roll_backs)));

        match crash {
            Some(c) => {
                ui.set_fact_name("What the game reported".into());
                ui.set_fact(c.reason.clone().unwrap_or_else(|| "unknown".into()).into());
                ui.set_report(c.file.as_str().into());
                ui.set_tail("".into());
            }
            None => {
                ui.set_fact_name("What happened".into());
                let what = match stopped.code {
                    None => "The game was closed (by you, or by the system).".to_string(),
                    Some(code) => format!("The game ended with exit code {code}, without a crash report."),
                };
                ui.set_fact(what.into());
                ui.set_report("".into());
                ui.set_tail(stopped.lines[stopped.lines.len().saturating_sub(15)..].join("\n").into());
            }
        }
        ui.set_when(words::when(stopped.at / 1000).into());
    }
}
