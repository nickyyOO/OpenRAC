//! A game's page: what the GamePage global shows, read from the launcher, and what its buttons do.

use std::cell::RefCell;
use std::collections::HashMap;

use openrac_launcher_service::core::actions::{Scope, State};
use openrac_launcher_service::core::library::{GameView, VersionView};
use openrac_launcher_service::core::reports::ReportEntry;
use openrac_launcher_service::labels::title_lines;
use openrac_launcher_service::{
    action, failed_step, is_built, play_warning, player_state, player_version, rebuild_steps, summarize, tool_failure,
    update_step, Icon, Job, Launcher, Pace, PlayerKind, PlayerState, Situation,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::shell::{self, Shell};
use crate::{art, words, App, AppWindow, GameNotice, GamePage, Icons, JobStrip, MenuItem, Page, PlayerButton, Steps};

/// What the page keeps between reads: each job's pace (for "about 3 minutes left"), and what is
/// read from files off the window's thread.
#[derive(Default)]
pub struct GameState {
    paces: RefCell<HashMap<u64, Pace>>,
    /// The game whose details were read, and what was read.
    details: RefCell<Option<Details>>,
}

#[derive(Clone)]
struct Details {
    key: String,
    mods: usize,
    last: Option<ReportEntry>,
}

/// The band's colours and the title's typeface for each game.
struct Look {
    from: u32,
    to: u32,
    font: &'static str,
    weight: i32,
}

fn look(game: &str) -> Look {
    let (from, to, font, weight) = match game {
        "rac1" => (0x3c7a3a, 0x12331f, "Audiowide", 400),
        "rac2" => (0x7a4d1a, 0x2a1a0c, "Orbitron", 600),
        "rac3" => (0x25514f, 0x0d1f24, "Orbitron", 600),
        "rac4" => (0x7f1d1d, 0x0f1115, "Orbitron", 700),
        "rac5" => (0x223342, 0x131d26, "Orbitron", 600),
        "rac6" => (0x0369a1, 0x082f49, "Orbitron", 600),
        _ => (0x2c2a26, 0x191919, "Orbitron", 600),
    };
    Look { from, to, font, weight }
}

/// Wires the page's buttons.
pub fn install(window: &AppWindow) {
    let page = window.global::<GamePage>();
    page.on_act(|| on_game(|l, key, _| l.player_act(key)));
    page.on_menu_chosen(|id| shell::with(|s| chosen(s, &id)));
    page.on_set_mods(|on| shell::with(|s| s.launcher().map(|l| l.set_play_with_mods(on)).unwrap_or_default()));
    page.on_open_mods(|| shell::with(|s| s.open_page(Page::Mods)));
    page.on_see_output(|| shell::with(|s| s.open_page(Page::Activity)));
    page.on_open_develop(|| shell::with(|s| s.open_page(Page::Develop)));
    page.on_follow_progress(|| {
        shell::with(|s| s.window().global::<App>().invoke_open_url("https://openrac.dev".into()))
    });
    page.on_notice_chosen(|id| {
        if id == "update" {
            on_game(|l, key, v| {
                let incremental = l.config().incremental_builds;
                if let Some(update) = update_step(v, incremental) {
                    l.run_soon(Scope::Version(key.into()), update.action, None);
                }
            });
        }
    });
}

/// Runs `work` with the launcher and the shown game's version.
fn on_game(work: impl FnOnce(&Launcher, &str, &VersionView)) {
    shell::with(|s| {
        let Some(launcher) = s.launcher() else { return };
        let key = s.window().global::<crate::Games>().get_selected();
        let view = launcher.view();
        let v = view
            .library
            .as_ref()
            .and_then(|l| l.games.iter().flat_map(|g| &g.versions).find(|v| v.version.key == key.as_str()));
        if let Some(v) = v {
            work(launcher, &key, v);
        }
    });
}

/// What the menu beside the main button does.
fn chosen(shell: &Shell, id: &str) {
    if id == "activity" {
        return shell.open_page(Page::Activity);
    }
    let id = id.to_string();
    on_game(move |l, key, v| {
        let (key, game, title, serial, dir) = (
            key.to_string(),
            v.version.game.clone(),
            v.version.title.clone(),
            v.version.serial.clone(),
            v.version.dir.clone(),
        );
        match id.as_str() {
            "rebuild" => l.spawn(move |l| {
                l.rebuild(&key);
                Ok(())
            }),
            "preview" => {
                if let Some(preview) = action(v, "editor-preview").cloned() {
                    l.run_soon(Scope::Version(key), preview, None);
                }
            }
            "import-pcsx2" => l.spawn(move |l| l.import_pcsx2_saves(&serial)),
            "open-folder" => l.spawn(move |l| l.open_in_checkout(&dir)),
            "remove" => l.spawn(move |l| l.remove_set_up(&game, &title)),
            _ => {}
        }
    });
}

/// The menu beside the main button: what is done less often than playing.
fn menu(icons: &Icons, game: &GameView, v: &VersionView, next: &PlayerState) -> Vec<MenuItem> {
    let item = |id: &str, label: &str, icon: SharedString, danger: bool| MenuItem {
        id: id.into(),
        label: label.into(),
        icon,
        danger,
    };
    let set_up = game.install.as_ref().is_some_and(|i| i.extracted.is_some());
    let mut items = Vec::new();
    // From scratch: everything built before is deleted first.
    if is_built(game, v) && matches!(next.kind, PlayerKind::Play | PlayerKind::Levels) && rebuild_steps(v).is_some() {
        items.push(item("rebuild", "Rebuild the game", icons.get_build(), false));
    }
    if action(v, "editor-preview").is_some_and(|a| a.runnable) {
        items.push(item("preview", "Preview a level in OpenRAC Editor", icons.get_play(), false));
    }
    if set_up && !v.version.serial.is_empty() {
        items.push(item("import-pcsx2", "Import saves from PCSX2", icons.get_save(), false));
    }
    items.push(item("open-folder", "Open the game's folder", icons.get_folder(), false));
    items.push(item("activity", "Activity log", icons.get_terminal(), false));
    if set_up {
        items.push(item("remove", "Remove the set-up", icons.get_trash(), true));
    }
    items
}

fn icon(icons: &Icons, icon: Icon) -> SharedString {
    match icon {
        Icon::Play => icons.get_play(),
        Icon::Stop => icons.get_x(),
        Icon::Disc => icons.get_disc(),
        Icon::Build => icons.get_build(),
    }
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn strings(list: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(list.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

impl GameState {
    /// Fills the page for the version `key` from what the launcher knows now.
    pub fn show(&self, window: &AppWindow, launcher: &Launcher, key: &str) {
        let page = window.global::<GamePage>();
        let icons = window.global::<Icons>();
        let view = launcher.view();
        let jobs = launcher.jobs();
        let android_game = launcher.android_game();
        let now = Situation { view: &view, jobs: &jobs, android_game: android_game.as_deref() };
        let found = view
            .library
            .as_ref()
            .and_then(|l| l.games.iter().find(|g| g.versions.iter().any(|v| v.version.key == key)));
        let Some((game, v)) = found.and_then(|g| Some((g, player_version(g, &view)?))) else {
            page.set_found(false);
            return;
        };
        let config = launcher.config();
        let next = player_state(game, v, &now);
        let set_up = game.install.as_ref().is_some_and(|i| i.extracted.is_some());

        page.set_found(true);
        page.set_key(v.version.key.as_str().into());
        let (series, name) = title_lines(&v.version.title);
        page.set_series(series.into());
        page.set_name(name.into());
        page.set_year(game.year.map(|y| y.to_string()).unwrap_or_default().into());
        page.set_picture(art::game_background(&game.id));
        let look = look(&game.id);
        page.set_from(color(look.from));
        page.set_to(color(look.to));
        page.set_title_font(look.font.into());
        page.set_title_weight(look.weight);

        page.set_button(PlayerButton {
            label: next.label.as_str().into(),
            icon: icon(&icons, next.icon),
            primary: next.primary,
            disabled: next.disabled,
            busy: next.job.is_some(),
            hint: next.hint.as_str().into(),
        });
        page.set_menu(ModelRc::new(VecModel::from(menu(&icons, game, v, &next))));

        // The Mods switch shows while Play would start the game.
        let starts_play =
            next.job.is_none() && matches!(next.kind, PlayerKind::Play | PlayerKind::Build | PlayerKind::Levels);
        page.set_mods_shown(starts_play);
        page.set_mods_on(config.play_with_mods);
        let details = self.details.borrow().clone().filter(|d| d.key == key);
        let count = details.as_ref().map(|d| d.mods).unwrap_or_default();
        page.set_mods_count(if count == 0 { "enabled".into() } else { format!("enabled: {count}").into() });
        let last = details.and_then(|d| d.last).filter(|_| next.kind == PlayerKind::Play && set_up);
        page.set_last_played(last.as_ref().map(words::last_played).unwrap_or_default().into());

        let stopped = failed_step(&jobs, v).filter(|_| next.job.is_none());
        page.set_stopped(
            stopped
                .map(|j| format!("{} stopped: {}.", j.title, tool_failure(&j.lines).error.title.to_lowercase()))
                .unwrap_or_default()
                .into(),
        );
        page.set_job(self.job_strip(next.job.as_ref()));

        let levels = action(v, "port-levels");
        let levels_ready = match action(v, "port-levels-check") {
            Some(check) => check.done,
            None => levels.is_some_and(|a| a.done),
        };
        let shows_build = next.kind != PlayerKind::Unavailable && (next.kind != PlayerKind::SetUp || next.primary);
        page.set_steps(Steps {
            shown: !matches!(next.kind, PlayerKind::Play | PlayerKind::Playing | PlayerKind::Return),
            set_up,
            built_shown: shows_build,
            built: is_built(game, v),
            levels_shown: shows_build && levels.is_some(),
            levels: levels_ready,
        });

        page.set_notices(ModelRc::new(VecModel::from(notices(&view, v, &next, config.incremental_builds))));

        page.set_developer(config.developer);
        let mut chips = vec![v.version.key.clone()];
        if !v.version.serial.is_empty() {
            chips.push(v.version.serial.clone());
        }
        if let Some(p) = &v.version.progress {
            chips.push(format!("{} matched", words::percent(p.percent)));
        }
        page.set_chips(strings(chips));

        let port_ready = action(v, "play").is_some_and(|a| a.state != State::Planned);
        page.set_coming(!port_ready);
        page.set_coming_text(
            format!(
                "OpenRAC is rebuilding {} so it runs natively on your computer. Play appears here when it is ready{}",
                v.version.title,
                if set_up {
                    "; your disc is already set up for it."
                } else {
                    ". You can set your disc up now, so it is ready too."
                }
            )
            .into(),
        );
        page.set_coming_percent(v.version.progress.as_ref().map(|p| p.percent as f32).unwrap_or(-1.0));
    }

    fn job_strip(&self, job: Option<&Job>) -> JobStrip {
        let Some(job) = job else { return JobStrip::default() };
        let mut paces = self.paces.borrow_mut();
        let summary = summarize(job, paces.entry(job.key).or_default());
        JobStrip {
            shown: true,
            phase: summary.phase.into(),
            count: summary.count.into(),
            percent: summary.percent.unwrap_or(-1.0),
            bar: summary.bar,
            left: summary.left.unwrap_or_default().into(),
            tail: summary.tail.join("\n").into(),
        }
    }

    /// Reads, off the window's thread, what the page shows from files: how many mods are on, and
    /// when the game was last played. The page is filled again when they are read.
    pub fn read_details(&self, launcher: &Launcher, key: &str) {
        let Some((game, version)) = key.split_once('/') else { return };
        let (launcher, key, game, version) = (launcher.clone(), key.to_string(), game.to_string(), version.to_string());
        std::thread::spawn(move || {
            let details =
                Details { mods: launcher.mods_on_count(&game, &version), last: launcher.last_played(&game), key };
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    *s.game.details.borrow_mut() = Some(details);
                    s.show_game();
                })
            });
        });
    }
}

/// What shows above the band: no actions, a warning before Play, an update ready.
fn notices(
    view: &openrac_launcher_service::View,
    v: &VersionView,
    next: &PlayerState,
    incremental: bool,
) -> Vec<GameNotice> {
    let notice =
        |text: String, button: &str, id: &str| GameNotice { text: text.into(), button: button.into(), id: id.into() };
    let mut list = Vec::new();
    if let Some(error) = view.library.as_ref().and_then(|l| l.actions_error.as_ref()) {
        list.push(notice(format!("The games show, but no actions: {error}"), "", "actions"));
    }
    if let Some(warning) = play_warning(v) {
        list.push(notice(warning.to_string(), "", "warning"));
    }
    // Recommended, never forced: Play still starts the build there is.
    let update = update_step(v, incremental).filter(|_| v.port_outdated && next.kind == PlayerKind::Play);
    if let Some(update) = update {
        let title = &v.version.title;
        let text = if update.incremental {
            format!(
                "An update is ready. Build {title} again to play the latest version: only what changed is built (the Incremental builds setting), and your saves and levels are kept."
            )
        } else {
            let because = update
                .clean_because
                .map(|why| format!(" From scratch even with Incremental builds on, because {why}."))
                .unwrap_or_default();
            format!(
                "An update is ready. Rebuild {title} to play the latest version: it takes a few minutes, and your saves and levels are kept.{because}"
            )
        };
        list.push(notice(text, if update.incremental { "Build now" } else { "Rebuild now" }, "update"));
    }
    list
}
