//! The window's side, on the window's thread: what each part of the window shows, read again from
//! the launcher when it says something changed, and what the window's buttons ask of it.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use openrac_launcher_service::core::library::GameView;
use openrac_launcher_service::{player_state, player_version, Change, Launcher, Notice, Situation, Tone as PlayerTone};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::pages::activity::{self, ActivityState};
use crate::pages::differences::{self, DifferencesState};
use crate::pages::game::{self, GameState};
use crate::pages::help::{self, HelpState};
use crate::pages::mods::{self, ModsState};
use crate::pages::saves::{self, SavesState};
use crate::pages::settings::{self, SettingsState};
use crate::pages::stopped::{self, StoppedState};
use crate::toasts::Toasts;
use crate::{art, theme, App, AppWindow, Games, Page, SideEntry, Tone};

pub struct Shell {
    window: AppWindow,
    launcher: OnceCell<Launcher>,
    toasts: Rc<Toasts>,
    /// The answer a question over the window waits for.
    asked: RefCell<Option<Sender<bool>>>,
    pub game: GameState,
    pub help: HelpState,
    pub saves: SavesState,
    pub stopped: StoppedState,
    pub differences: DifferencesState,
    pub activity: ActivityState,
    pub settings: SettingsState,
    pub mods: ModsState,
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
        game: GameState::default(),
        help: HelpState::default(),
        saves: SavesState::default(),
        stopped: StoppedState::default(),
        differences: DifferencesState::default(),
        activity: ActivityState::default(),
        settings: SettingsState::default(),
        mods: ModsState::default(),
    });
    SHELL.with(|s| *s.borrow_mut() = Some(shell));
    let app = window.global::<App>();
    app.set_version(env!("CARGO_PKG_VERSION").into());
    app.on_dismiss_toast(|id| with(|s| s.toasts.dismiss(&s.window, id)));
    app.on_copy_text(|text| crate::system::copy(&text));
    app.on_answer(|yes| with(|s| s.answer(yes)));
    app.on_open_url(|url| {
        if let Err(e) = crate::system::open_url(&url) {
            with(|s| s.notify(Notice::error(e)));
        }
    });
    app.on_open_page(|page| with(|s| s.open_page(page)));
    window.global::<Games>().on_pick(|key| with(|s| s.pick_version(&key)));
    game::install(window);
    help::install(window);
    saves::install(window);
    stopped::install(window);
    differences::install(window);
    activity::install(window);
    settings::install(window);
    mods::install(window);
    #[cfg(not(target_os = "android"))]
    crate::pages::shortcuts::install(window);
}

impl Shell {
    pub fn launcher(&self) -> Option<&Launcher> {
        self.launcher.get()
    }

    pub fn window(&self) -> &AppWindow {
        &self.window
    }

    /// The service has started: the window shows the launcher.
    pub fn ready(&self, launcher: Launcher) {
        let _ = self.launcher.set(launcher);
        let app = self.window.global::<App>();
        self.config_changed();
        self.library_changed();
        self.game_picked();
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
            Change::Stopped => {
                self.show_stopped();
                self.read_stopped();
                self.read_game_details();
                self.read_card();
            }
            Change::GameData => {
                self.read_game_details();
                self.read_card();
            }
        }
    }

    pub fn notify(&self, notice: Notice) {
        self.toasts.show(&self.window, notice);
    }

    /// Asks the player in a dialog over the window; `answer` hears it.
    pub fn ask(&self, text: &str, yes: &str, answer: Sender<bool>) {
        self.ask_titled("", text, yes, "Cancel", answer);
    }

    /// [`Shell::ask`] with a title over the words, and the words of the answer that goes back.
    pub fn ask_titled(&self, title: &str, text: &str, yes: &str, no: &str, answer: Sender<bool>) {
        // A question already open is answered no: one dialog at a time.
        if let Some(earlier) = self.asked.replace(Some(answer)) {
            let _ = earlier.send(false);
        }
        let app = self.window.global::<App>();
        app.set_ask_title(title.into());
        app.set_ask_text(text.into());
        app.set_ask_no(no.into());
        app.set_ask_yes(yes.into());
        app.set_asking(true);
    }

    fn answer(&self, yes: bool) {
        self.window.global::<App>().set_asking(false);
        if let Some(answer) = self.asked.take() {
            let _ = answer.send(yes);
        }
    }

    pub fn open_page(&self, page: Page) {
        let app = self.window.global::<App>();
        // With developer tools on, what the launcher ran is Develop's console.
        let page = if page == Page::Activity && app.get_developer() { Page::Develop } else { page };
        let was = app.get_page();
        app.set_page(page);
        if was == Page::Settings && page != Page::Settings {
            self.settings.close(self);
        }
        if page == Page::Settings && was != Page::Settings {
            self.settings.open(self);
        }
        self.follow_view();
        self.show_activity();
    }

    // ---- what each part shows ------------------------------------------------------------------

    fn config_changed(&self) {
        let Some(launcher) = self.launcher() else { return };
        let config = launcher.config();
        let app = self.window.global::<App>();
        app.set_developer(config.developer);
        app.set_setup_complete(config.setup_complete);
        // The first run is the settings' page.
        if !config.setup_complete && !self.window.global::<crate::Settings>().get_setup() {
            self.settings.open(self);
        }
        if config.setup_complete {
            self.window.global::<crate::Settings>().set_setup(false);
        }
        self.apply_theme();
        self.show_game();
        self.show_help();
        self.mods.show(self);
    }

    pub fn apply_theme(&self) {
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
        self.mods.show_list(self);
        self.show_game();
        self.show_help();
        self.show_stopped();
    }

    /// The picked game's page, as the launcher knows it now.
    pub fn show_game(&self) {
        let Some(launcher) = self.launcher() else { return };
        let key = self.window.global::<Games>().get_selected();
        self.game.show(&self.window, launcher, &key);
    }

    /// The shown disc's memory card, read again off the window's thread.
    pub fn read_card(&self) {
        let (Some(launcher), Some(serial)) = (self.launcher(), saves::shown_serial(self)) else { return };
        self.saves.read(launcher, serial);
    }

    pub fn show_activity(&self) {
        self.activity.show(self);
    }

    pub fn show_differences(&self) {
        self.differences.show(self);
    }

    pub fn show_stopped(&self) {
        self.stopped.show(self);
    }

    fn read_stopped(&self) {
        if let Some(launcher) = self.launcher() {
            self.stopped.read(launcher);
        }
    }

    pub fn show_help(&self) {
        self.help.show(self);
    }

    /// What the game's page reads from files (mods on, last played), off the window's thread.
    pub fn read_game_details(&self) {
        let Some(launcher) = self.launcher() else { return };
        let key = self.window.global::<Games>().get_selected();
        self.game.read_details(launcher, &key);
    }

    fn jobs_changed(&self) {
        let Some(launcher) = self.launcher() else { return };
        let app = self.window.global::<App>();
        app.set_running_job(launcher.running_job().map(|j| j.title).unwrap_or_default().into());
        app.set_pending_jobs(launcher.pending_jobs() as i32);
        self.library_changed();
        self.show_differences();
        self.show_activity();
    }

    pub fn pick_version(&self, key: &str) {
        self.window.global::<Games>().set_selected(key.into());
        self.apply_theme();
        self.follow_view();
        self.game_picked();
    }

    /// Another game's page is shown: everything on it is read for that game.
    fn game_picked(&self) {
        self.help.reset(&self.window);
        self.saves.reset(&self.window);
        self.mods.reset(&self.window);
        self.mods.read(self);
        self.show_game();
        self.show_stopped();
        self.read_game_details();
        self.read_card();
        if let Some(launcher) = self.launcher() {
            let key = self.window.global::<Games>().get_selected().to_string();
            self.differences.read(launcher, key);
        }
        #[cfg(not(target_os = "android"))]
        crate::pages::shortcuts::read(self);
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
