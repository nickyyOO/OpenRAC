//! Help on a game's page: its logs and crash reports, and what their buttons do.

use std::cell::RefCell;
use std::time::Duration;

use openrac_launcher_service::core::reports::{ReportEntry, ReportKind};
use openrac_launcher_service::Launcher;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::shell::{self, Shell};
use crate::{words, App, AppWindow, Games, Help, Links, ReportRow};

/// What Help says between reads: something was deleted, a report was just copied or saved.
#[derive(Default)]
pub struct HelpState {
    cleared: RefCell<bool>,
}

pub fn install(window: &AppWindow) {
    let help = window.global::<Help>();
    help.on_copy(|file| on_game(move |l, game| copy(l, game, file.to_string())));
    help.on_save(|file| on_game(move |l, game| save(l, game, file.to_string())));
    help.on_delete(|file| on_game(move |l, game| delete(l, game, Some(vec![file.to_string()]))));
    help.on_delete_all(|| on_game(|l, game| delete(l, game, None)));
    help.on_dismiss(|| {
        on_game(|l, game| {
            l.spawn(move |l| {
                l.dismiss_crashes(&game);
                Ok(())
            })
        })
    });
    help.on_open_folder(|| on_game(|l, game| l.spawn(move |l| l.open_reports_folder(&game))));
    help.on_set_detailed(|on| shell::with(|s| s.launcher().map(|l| l.set_detailed_log(on)).unwrap_or_default()));
    help.on_open_issues(|| open_url(|links| links.get_issues()));
    help.on_open_discord(|| open_url(|links| links.get_discord()));
}

fn open_url(link: impl FnOnce(&Links) -> slint::SharedString) {
    shell::with(|s| {
        let url = link(&s.window().global::<Links>());
        s.window().global::<App>().invoke_open_url(url);
    });
}

/// Runs `work` with the launcher and the shown game's id.
fn on_game(work: impl FnOnce(&Launcher, String)) {
    shell::with(|s| {
        let Some(launcher) = s.launcher() else { return };
        let key = s.window().global::<Games>().get_selected();
        if let Some((game, _)) = key.split_once('/') {
            work(launcher, game.to_string());
        }
    });
}

/// Says `text` under Help for a few seconds.
fn say(window: &AppWindow, text: &str) {
    let help = window.global::<Help>();
    help.set_said(text.into());
    let weak = window.as_weak();
    let text = text.to_string();
    slint::Timer::single_shot(Duration::from_secs(6), move || {
        if let Some(window) = weak.upgrade() {
            let help = window.global::<Help>();
            if help.get_said() == text.as_str() {
                help.set_said("".into());
            }
        }
    });
}

fn copy(launcher: &Launcher, game: String, file: String) {
    launcher.spawn(move |l| {
        l.copy_report(&game, &file)?;
        let _ = slint::invoke_from_event_loop(move || {
            shell::with(|s| {
                let help = s.window().global::<Help>();
                help.set_copied(file.as_str().into());
                say(s.window(), "Copied: paste it in your message.");
                let weak = s.window().as_weak();
                slint::Timer::single_shot(Duration::from_secs(4), move || {
                    if let Some(window) = weak.upgrade() {
                        let help = window.global::<Help>();
                        if help.get_copied() == file.as_str() {
                            help.set_copied("".into());
                        }
                    }
                });
            })
        });
        Ok(())
    });
}

fn save(launcher: &Launcher, game: String, file: String) {
    shell::with(|s| s.window().global::<Help>().set_saving(file.as_str().into()));
    launcher.spawn(move |l| {
        let saved = l.save_report(&game, &file);
        let place = saved.as_ref().ok().cloned().flatten();
        let _ = slint::invoke_from_event_loop(move || {
            shell::with(|s| {
                s.window().global::<Help>().set_saving("".into());
                if let Some(place) = &place {
                    say(s.window(), &format!("Saved as {place}: attach this file."));
                }
            })
        });
        saved.map(|_| ())
    });
}

fn delete(launcher: &Launcher, game: String, files: Option<Vec<String>>) {
    launcher.spawn(move |l| {
        l.delete_reports(&game, files.as_deref())?;
        let _ = slint::invoke_from_event_loop(|| {
            shell::with(|s| {
                *s.help.cleared.borrow_mut() = true;
                s.window().global::<Help>().set_said("".into());
                s.read_game_details();
            })
        });
        Ok(())
    });
}

fn row(entry: &ReportEntry) -> ReportRow {
    ReportRow {
        file: entry.file.as_str().into(),
        headline: words::headline(entry).into(),
        when: words::when(entry.time).into(),
        crash: entry.kind == ReportKind::Crash,
    }
}

/// "3 logs, 1 crash": a count of logs alone would read as crashes.
fn count(entries: &[ReportEntry]) -> String {
    let crashes = entries.iter().filter(|e| e.kind == ReportKind::Crash).count();
    let logs = entries.len() - crashes;
    let mut text = format!("{logs} {}", if logs == 1 { "log" } else { "logs" });
    if crashes > 0 {
        text += &format!(", {crashes} {}", if crashes == 1 { "crash" } else { "crashes" });
    }
    text
}

impl HelpState {
    /// A new game is shown: what was said of the last one goes.
    pub fn reset(&self, window: &AppWindow) {
        *self.cleared.borrow_mut() = false;
        let help = window.global::<Help>();
        help.set_read(false);
        help.set_open(false);
        help.set_said("".into());
    }

    pub fn show(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let window = shell.window();
        let help = window.global::<Help>();
        let key = window.global::<Games>().get_selected();
        let config = launcher.config();
        help.set_developer(config.developer);
        help.set_detailed(config.detailed_game_log);
        let Some(details) = shell.game.details(&key) else { return };
        let (entries, failed) = match details.reports {
            Ok(entries) => (entries, String::new()),
            Err(e) => (Vec::new(), e),
        };
        let game = key.split('/').next().unwrap_or_default();
        let recent = launcher.view().crashes.get(game).is_some_and(|c| c.recent > 0);
        let latest = entries.iter().find(|e| e.kind == ReportKind::Crash).filter(|_| recent);
        help.set_has_latest(latest.is_some());
        help.set_latest(latest.map(row).unwrap_or_default());
        help.set_count(count(&entries).into());
        help.set_failed(failed.into());
        help.set_cleared(*self.cleared.borrow());
        help.set_rows(ModelRc::new(VecModel::from(entries.iter().map(row).collect::<Vec<_>>())));
        help.set_read(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ReportKind) -> ReportEntry {
        ReportEntry {
            kind,
            file: String::new(),
            program: None,
            time: 0,
            duration: None,
            reason: None,
            level: None,
            planet: None,
            crashed: false,
            session: None,
            size: 0,
        }
    }

    #[test]
    fn counts_logs_and_crashes_apart() {
        let list = [entry(ReportKind::Session), entry(ReportKind::Session), entry(ReportKind::Crash)];
        assert_eq!(count(&list), "2 logs, 1 crash");
        assert_eq!(count(&list[..1]), "1 log");
    }
}
