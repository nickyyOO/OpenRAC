//! The window's side, on the window's thread: what each part of the window shows, read again from
//! the launcher when it says something changed, and what the window's buttons ask of it.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use openrac_launcher_service::core::library::GameView;
use openrac_launcher_service::{player_state, player_version, Change, Launcher, Notice, Situation, Tone as PlayerTone};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::toasts::Toasts;
use crate::{art, theme, App, AppWindow, Games, Page, SideEntry, Tone};

pub struct Shell {
    window: AppWindow,
    launcher: OnceCell<Launcher>,
    toasts: Rc<Toasts>,
    /// The answer a question over the window waits for.
    asked: RefCell<Option<Sender<bool>>>,
}

thread_local! {
    static SHELL: RefCell<Option<Rc<Shell>>> = const { RefCell::new(None) };
}

/// Runs `work` with the shell, once the window has one.
pub fn with(work: impl FnOnce(&Shell)) {
    let shell = SHELL.with(|s| s.borrow().clone());
    if let Some(shell) = shell {
        work(&shell);
    }
}

/// Gives `window` its shell and wires its buttons.
pub fn install(window: &AppWindow) {
    let shell = Rc::new(Shell {
        window: window.clone_strong(),
        launcher: OnceCell::new(),
        toasts: Rc::default(),
        asked: RefCell::default(),
    });
    SHELL.with(|s| *s.borrow_mut() = Some(shell));
    let app = window.global::<App>();
    app.set_version(env!("CARGO_PKG_VERSION").into());
    app.on_dismiss_toast(|id| with(|s| s.toasts.dismiss(&s.window, id)));
    app.on_copy_text(|text| copy(&text));
    app.on_answer(|yes| with(|s| s.answer(yes)));
    app.on_open_url(|url| open_url(&url));
    app.on_open_page(|page| with(|s| s.open_page(page)));
    window.global::<Games>().on_pick(|key| with(|s| s.pick_version(&key)));
}

impl Shell {
    fn launcher(&self) -> Option<&Launcher> {
        self.launcher.get()
    }

    /// The service has started: the window shows the launcher.
    pub fn ready(&self, launcher: Launcher) {
        let _ = self.launcher.set(launcher);
        let app = self.window.global::<App>();
        self.config_changed();
        self.library_changed();
        app.set_ready(true);
    }

    pub fn changed(&self, change: Change) {
        if self.launcher().is_none() {
            return;
        }
        match change {
            Change::Config => self.config_changed(),
            Change::Library => self.library_changed(),
            Change::Jobs => self.jobs_changed(),
            Change::Stopped | Change::GameData => {}
        }
    }

    pub fn notify(&self, notice: Notice) {
        self.toasts.show(&self.window, notice);
    }

    /// Asks the player in a dialog over the window; `answer` hears it.
    pub fn ask(&self, text: &str, yes: &str, answer: Sender<bool>) {
        // A question already open is answered no: one dialog at a time.
        if let Some(earlier) = self.asked.replace(Some(answer)) {
            let _ = earlier.send(false);
        }
        let app = self.window.global::<App>();
        app.set_ask_text(text.into());
        app.set_ask_yes(yes.into());
        app.set_asking(true);
    }

    fn answer(&self, yes: bool) {
        self.window.global::<App>().set_asking(false);
        if let Some(answer) = self.asked.take() {
            let _ = answer.send(yes);
        }
    }

    fn open_page(&self, page: Page) {
        let app = self.window.global::<App>();
        // With developer tools on, what the launcher ran is Develop's console.
        let page = if page == Page::Activity && app.get_developer() { Page::Develop } else { page };
        app.set_page(page);
        self.follow_view();
    }

    // ---- what each part shows ------------------------------------------------------------------

    fn config_changed(&self) {
        let Some(launcher) = self.launcher() else { return };
        let config = launcher.config();
        let app = self.window.global::<App>();
        app.set_developer(config.developer);
        app.set_setup_complete(config.setup_complete);
        self.apply_theme();
    }

    fn apply_theme(&self) {
        let Some(launcher) = self.launcher() else { return };
        let config = launcher.config();
        let selected = self.window.global::<Games>().get_selected();
        let game = selected.split('/').next().filter(|g| !g.is_empty());
        theme::apply(&self.window, &config.theme, game, config.game_colors);
    }

    fn library_changed(&self) {
        let Some(launcher) = self.launcher() else { return };
        let view = launcher.view();
        let jobs = launcher.jobs();
        let android_game = launcher.android_game();
        let now = Situation { view: &view, jobs: &jobs, android_game: android_game.as_deref() };
        let games: &[GameView] = view.library.as_ref().map(|l| l.games.as_slice()).unwrap_or_default();
        let entries: Vec<SideEntry> = games
            .iter()
            .filter_map(|g| {
                let v = player_version(g, &view)?;
                let state = player_state(g, v, &now);
                let recent = view.crashes.get(&g.id).is_some_and(|c| c.recent > 0);
                Some(SideEntry {
                    key: v.version.key.as_str().into(),
                    name: g.title.as_str().into(),
                    disc: art::disc(&g.id, &v.version.name),
                    icon: "".into(),
                    state: state.status.into(),
                    tone: tone(state.tone),
                    mark: recent,
                    group: "".into(),
                })
            })
            .collect();
        let games_ui = self.window.global::<Games>();
        // The first game is picked until the player picks one.
        if games_ui.get_selected().is_empty() {
            if let Some(first) = entries.first() {
                games_ui.set_selected(first.key.clone());
                self.apply_theme();
            }
        }
        games_ui.set_entries(ModelRc::new(VecModel::from(entries)));
    }

    fn jobs_changed(&self) {
        let Some(launcher) = self.launcher() else { return };
        let app = self.window.global::<App>();
        app.set_running_job(launcher.running_job().map(|j| j.title).unwrap_or_default().into());
        app.set_pending_jobs(launcher.pending_jobs() as i32);
        self.library_changed();
    }

    fn pick_version(&self, key: &str) {
        self.window.global::<Games>().set_selected(key.into());
        self.apply_theme();
        self.follow_view();
    }

    /// The launcher hears which game's page is open (Discord, the stopped-game notice).
    fn follow_view(&self) {
        let Some(launcher) = self.launcher() else { return };
        let app = self.window.global::<App>();
        let on_game = matches!(app.get_page(), Page::Games);
        let key = self.window.global::<Games>().get_selected();
        launcher.set_viewing(on_game.then(|| key.to_string()).filter(|k| !k.is_empty()));
    }
}

fn tone(tone: PlayerTone) -> Tone {
    match tone {
        PlayerTone::Ok => Tone::Ok,
        PlayerTone::Warn => Tone::Warn,
        PlayerTone::Info => Tone::Info,
    }
}

/// Opens a web page in the browser: https only.
fn open_url(url: &str) {
    if url.starts_with("https://") {
        let _ = open::that_detached(url);
    }
}

fn copy(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text.to_string());
    }
}
