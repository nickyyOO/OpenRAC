//! Settings, and the first run's screen: a draft of the settings the page edits, saved with Save
//! (on Android a moment after each change), and what the launcher found on this machine.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use openrac_launcher_service::core::config::Config;
use openrac_launcher_service::core::detect::{Check, Detected};
use openrac_launcher_service::core::theme as palettes;
use openrac_launcher_service::{is_official, name_problem, server_problem, Launcher, PathKind};
use slint::{Color, ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use crate::shell::{self, Shell};
use crate::{theme, App, AppWindow, Games, Links, PathRow, Settings, ThemeChoice};

#[derive(Default)]
pub struct SettingsState {
    /// What the page edits; None while it is not shown.
    draft: RefCell<Option<Config>>,
    /// The port as typed, which may not be a number yet.
    port: RefCell<String>,
    found: RefCell<Option<Detected>>,
    /// Each path checked, by its setting and value; None while it is checked.
    checks: RefCell<HashMap<(&'static str, String), Option<Check>>>,
    /// The OpenRAC folder is with the games (it was missing when the page opened), not in Developer.
    root_here: Cell<bool>,
    /// The first run asks for these: they were not found.
    ask_root: Cell<bool>,
    ask_python: Cell<bool>,
    name_used: Cell<bool>,
    testing: Cell<bool>,
    /// The last test: the address it tested, whether it answered, the words.
    tested: RefCell<Option<(String, bool, String)>>,
    steam: RefCell<Option<openrac_launcher_service::core::steam::State>>,
    steam_busy: Cell<bool>,
    saving: Cell<bool>,
    /// Android: saves a moment after the last change.
    later: Timer,
}

const PATHS: [(&str, PathKind); 6] = [
    ("root", PathKind::Root),
    ("python", PathKind::Python),
    ("docker", PathKind::Docker),
    ("discs", PathKind::Discs),
    ("install", PathKind::Install),
    ("saves", PathKind::Saves),
];

/// The words of the Up Your Arsenal switch's three questions: title, lines, yes, no.
const RAC3_PROMPTS: [(&str, &str, &str, &str); 3] = [
    (
        "This is a development build",
        "Up Your Arsenal is not finished. It crashes, it looks wrong, and whole parts of it are not there yet.\n\nYes, even the part that looked fine a minute ago.",
        "Yes, it is a development build",
        "Back",
    ),
    (
        "No bug reports on Discord",
        "We are not looking for bug reports. Not on Discord, not in a DM, not as a tiny little question.\n\nWe know it is broken. That is what development means.",
        "I will not report bugs on Discord",
        "Back",
    ),
    (
        "Do not bother the developers",
        "No, really. Read this again.\n\nUntil we announce Up Your Arsenal in a stable release, you will not ping, message or nudge the developers about it. They are busy. We will tell you when it works.",
        "I will wait for the announcement",
        "Take me back",
    ),
];

pub fn install(window: &AppWindow) {
    let ui = window.global::<Settings>();
    ui.set_auto_save(cfg!(target_os = "android"));
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_system(std::env::consts::OS.into());
    ui.on_set_flag(|id, on| {
        if id == "rac3" && on {
            ask_rac3();
        } else {
            edit(|c, _| set_flag(c, &id, on));
        }
    });
    ui.on_set_text(|id, text| edit(|c, port| set_text(c, port, &id, &text)));
    ui.on_set_path(|id, text| {
        edit(|c, _| *path_mut(c, &id) = Some(PathBuf::from(text.trim())).filter(|p| !p.as_os_str().is_empty()))
    });
    ui.on_browse(|id| browse(id.to_string()));
    ui.on_pick_theme(|id| edit(|c, _| c.theme = id.to_string()));
    ui.on_reset_server(|| {
        edit(|c, port| {
            c.multiplayer_server_host = openrac_launcher_service::core::multiplayer::OFFICIAL_HOST.into();
            c.multiplayer_server_port = openrac_launcher_service::core::multiplayer::OFFICIAL_PORT;
            *port = c.multiplayer_server_port.to_string();
        });
        shell::with(|s| bump(s.window()));
    });
    ui.on_test_server(test_server);
    ui.on_add_to_steam(add_to_steam);
    ui.on_save(|| shell::with(|s| s.settings.save(s, false)));
    ui.on_discard(|| shell::with(|s| s.settings.open(s)));
    ui.on_start(|| shell::with(|s| s.settings.save(s, true)));
    ui.on_open_url(|url| shell::with(|s| s.window().global::<App>().invoke_open_url(url)));
}

/// Changes the draft with `work` (it gets the port as typed too) and shows it.
fn edit(work: impl FnOnce(&mut Config, &mut String)) {
    shell::with(|s| {
        let state = &s.settings;
        {
            let mut draft = state.draft.borrow_mut();
            let Some(draft) = draft.as_mut() else { return };
            work(draft, &mut state.port.borrow_mut());
        }
        state.show(s);
        state.preview(s);
        state.save_later(s);
    });
}

fn set_flag(c: &mut Config, id: &str, on: bool) {
    match id {
        "game-colors" => c.game_colors = on,
        "discord-rpc" => c.discord_rpc = on,
        "check-updates" => c.check_updates = on,
        "check-mod-updates" => c.check_mod_updates = on,
        "auto-update-mods" => c.auto_update_mods = on,
        "cleanup-deps" => c.cleanup_unused_deps = on,
        "detailed-log" => c.detailed_game_log = on,
        "editor-advanced" => c.save_editor_advanced = on,
        "developer" => c.developer = on,
        "incremental" => c.incremental_builds = on,
        "rac3" => c.rac3_dev = on,
        _ => {}
    }
}

fn set_text(c: &mut Config, port: &mut String, id: &str, text: &str) {
    match id {
        "name" => c.multiplayer_name = text.to_string(),
        "host" => c.multiplayer_server_host = text.to_string(),
        "port" => {
            *port = text.to_string();
            if let Ok(p) = text.trim().parse::<u16>() {
                c.multiplayer_server_port = p;
            }
        }
        "renderer" => c.renderer = text.to_string(),
        "discord-id" => c.discord_client_id = Some(text.trim().to_string()).filter(|t| !t.is_empty()),
        _ => {}
    }
}

fn path_mut<'a>(c: &'a mut Config, id: &str) -> &'a mut Option<PathBuf> {
    match id {
        "root" => &mut c.root,
        "python" => &mut c.python,
        "docker" => &mut c.docker,
        "discs" => &mut c.discs_dir,
        "install" => &mut c.install_dir,
        _ => &mut c.save_dir,
    }
}

fn path_of<'a>(c: &'a Config, id: &str) -> Option<&'a PathBuf> {
    match id {
        "root" => c.root.as_ref(),
        "python" => c.python.as_ref(),
        "docker" => c.docker.as_ref(),
        "discs" => c.discs_dir.as_ref(),
        "install" => c.install_dir.as_ref(),
        _ => c.save_dir.as_ref(),
    }
}

/// What this page edits, taken from `from` into `into`. The rest (the last game opened, the
/// crashes seen) changes elsewhere while the page is open, so it is neither compared nor saved.
fn take_edits(from: &Config, into: &mut Config) {
    into.root = from.root.clone();
    into.python = from.python.clone();
    into.docker = from.docker.clone();
    into.discs_dir = from.discs_dir.clone();
    into.install_dir = from.install_dir.clone();
    into.save_dir = from.save_dir.clone();
    into.theme = from.theme.clone();
    into.game_colors = from.game_colors;
    into.discord_rpc = from.discord_rpc;
    into.check_updates = from.check_updates;
    into.check_mod_updates = from.check_mod_updates;
    into.auto_update_mods = from.auto_update_mods;
    into.cleanup_unused_deps = from.cleanup_unused_deps;
    into.update_channel = from.update_channel;
    into.update_branch = from.update_branch.clone();
    into.detailed_game_log = from.detailed_game_log;
    into.renderer = from.renderer.clone();
    into.save_editor_advanced = from.save_editor_advanced;
    into.developer = from.developer;
    into.incremental_builds = from.incremental_builds;
    into.rac3_dev = from.rac3_dev;
    into.discord_client_id = from.discord_client_id.clone();
    into.multiplayer_name = from.multiplayer_name.clone();
    into.multiplayer_server_host = from.multiplayer_server_host.clone();
    into.multiplayer_server_port = from.multiplayer_server_port;
}

/// Fields typed into take their values again.
fn bump(window: &AppWindow) {
    let ui = window.global::<Settings>();
    ui.set_revision(ui.get_revision() + 1);
}

fn browse(id: String) {
    let folder = !matches!(id.as_str(), "python" | "docker");
    let title = label(&id).to_string();
    std::thread::spawn(move || {
        let Some(picked) = pick(&title, folder) else { return };
        let _ = slint::invoke_from_event_loop(move || {
            edit(|c, _| *path_mut(c, &id) = Some(picked));
            shell::with(|s| bump(s.window()));
        });
    });
}

#[cfg(not(target_os = "android"))]
fn pick(title: &str, folder: bool) -> Option<PathBuf> {
    if folder {
        crate::dialogs::pick_folder(title)
    } else {
        crate::dialogs::pick_program(title)
    }
}

#[cfg(target_os = "android")]
fn pick(_title: &str, _folder: bool) -> Option<PathBuf> {
    None
}

/// Asks the three questions in turn; only three yeses turn the switch on.
fn ask_rac3() {
    std::thread::spawn(|| {
        for (title, text, yes, no) in RAC3_PROMPTS {
            let (answer, answered) = mpsc::channel();
            let _ = slint::invoke_from_event_loop(move || shell::with(|s| s.ask_titled(title, text, yes, no, answer)));
            if !answered.recv().unwrap_or(false) {
                // The switch goes back off.
                let _ = slint::invoke_from_event_loop(|| shell::with(|s| s.settings.show(s)));
                return;
            }
        }
        let _ = slint::invoke_from_event_loop(|| edit(|c, _| c.rac3_dev = true));
    });
}

fn test_server() {
    shell::with(|s| {
        let state = &s.settings;
        let Some(launcher) = s.launcher().cloned() else { return };
        let Some(draft) = state.draft.borrow().clone() else { return };
        let host = draft.multiplayer_server_host.trim().to_string();
        let port = u32::from(draft.multiplayer_server_port);
        let address = openrac_launcher_service::core::multiplayer::address(&host, draft.multiplayer_server_port);
        state.testing.set(true);
        state.show(s);
        std::thread::spawn(move || {
            let result = launcher.test_server(&host, port);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    let said = match result {
                        Ok(()) => (address.clone(), true, format!("Reachable: {address} answered.")),
                        Err(e) => (address.clone(), false, format!("Unreachable: {e}")),
                    };
                    s.settings.testing.set(false);
                    *s.settings.tested.borrow_mut() = Some(said);
                    s.settings.show(s);
                })
            });
        });
    });
}

#[cfg(not(target_os = "android"))]
fn add_to_steam() {
    shell::with(|s| {
        let Some(launcher) = s.launcher().cloned() else { return };
        s.settings.steam_busy.set(true);
        s.settings.show(s);
        std::thread::spawn(move || {
            let done = crate::steam::add(&launcher.config(), None);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    s.settings.steam_busy.set(false);
                    match done {
                        Ok(state) => {
                            *s.settings.steam.borrow_mut() = Some(state);
                            s.notify(openrac_launcher_service::Notice::info(
                                "Added. If Steam is open and does not show it, restart Steam.",
                            ));
                        }
                        Err(e) => s.notify(openrac_launcher_service::Notice::error(e)),
                    }
                    s.settings.show(s);
                })
            });
        });
    });
}

#[cfg(target_os = "android")]
fn add_to_steam() {}

fn label(id: &str) -> &'static str {
    match id {
        "root" => "OpenRAC folder",
        "python" => "Python",
        "docker" => "Docker",
        "discs" => "Games folder",
        "install" => "Install folder",
        _ => "Save folder",
    }
}

fn hint(id: &str, setup: bool) -> &'static str {
    match id {
        "root" => "Your copy of OpenRAC: the folder with games/ and tools/ in it.",
        "python" => "Python 3.10 or newer: OpenRAC's tools run with it.",
        "docker" => "The build container of Ratchet & Clank (PAL) and Deadlocked runs in it, on Linux and macOS. Optional.",
        "discs" if setup => "The folder with your disc images (.iso). You can also choose it later, on the Games page.",
        "discs" => "The folder with your PlayStation 2 disc images (.iso). OpenRAC finds its games in it.",
        "install" => "Where your games are set up from your discs. A few GB per game.",
        _ => "Where your games keep their saves, for example a folder you sync between computers. Your saves are copied there when you save. Empty: the default.",
    }
}

fn color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()).unwrap_or(0);
    Color::from_rgb_u8(byte(0), byte(2), byte(4))
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

impl SettingsState {
    /// The page opens (or its edits are discarded): the draft is the saved settings again.
    pub fn open(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let config = launcher.config();
        let setup = !config.setup_complete;
        self.root_here.set(!launcher.standalone() && config.root.is_none());
        *self.port.borrow_mut() = config.multiplayer_server_port.to_string();
        *self.draft.borrow_mut() = Some(config.clone());
        *self.tested.borrow_mut() = None;
        let ui = shell.window().global::<Settings>();
        ui.set_setup(setup);
        ui.set_advanced_open(config.detailed_game_log || config.developer || config.save_editor_advanced);
        ui.set_themes(model(
            palettes::palettes()
                .iter()
                .map(|p| ThemeChoice {
                    id: p.id.as_str().into(),
                    name: p.name.as_str().into(),
                    ink: color(&p.swatch[0]),
                    panel: color(&p.swatch[1]),
                    text: color(&p.swatch[2]),
                })
                .collect(),
        ));
        bump(shell.window());
        self.preview(shell);
        self.show(shell);
        self.read(launcher.clone(), setup);
    }

    /// The page is left: a theme tried and not saved goes away.
    pub fn close(&self, shell: &Shell) {
        if cfg!(target_os = "android") && self.changed(shell) {
            self.save(shell, false);
        }
        *self.draft.borrow_mut() = None;
        shell.apply_theme();
    }

    /// What is found on this machine, whether a multiplayer mod is in, and Steam, off the window's thread.
    fn read(&self, launcher: Launcher, setup: bool) {
        std::thread::spawn(move || {
            let found = launcher.detect();
            let name_used = !setup && launcher.player_name_used();
            #[cfg(not(target_os = "android"))]
            let steam = crate::steam::offers().0.then(|| crate::steam::state(&launcher.config(), None).ok()).flatten();
            #[cfg(target_os = "android")]
            let steam = None;
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    let state = &s.settings;
                    state.name_used.set(name_used);
                    *state.steam.borrow_mut() = steam;
                    if setup {
                        let standalone = s.launcher().is_some_and(|l| l.standalone());
                        if let Some(draft) = state.draft.borrow_mut().as_mut() {
                            state.ask_root.set(draft.root.is_none() && found.roots.is_empty());
                            state.ask_python.set(!standalone && draft.python.is_none() && found.pythons.is_empty());
                            let first = |list: &[openrac_launcher_service::core::detect::Candidate]| {
                                list.first().map(|c| c.path.clone())
                            };
                            draft.root = draft.root.take().or_else(|| first(&found.roots));
                            draft.python = draft.python.take().or_else(|| first(&found.pythons));
                            draft.docker = draft.docker.take().or_else(|| first(&found.dockers));
                        }
                        bump(s.window());
                    }
                    *state.found.borrow_mut() = Some(found);
                    state.show(s);
                })
            });
        });
    }

    fn changed(&self, shell: &Shell) -> bool {
        let (Some(launcher), Some(draft)) = (shell.launcher(), self.draft.borrow().clone()) else { return false };
        let saved = launcher.config();
        let mut edited = saved.clone();
        take_edits(&draft, &mut edited);
        edited != saved || *self.port.borrow() != saved.multiplayer_server_port.to_string()
    }

    /// Why the draft cannot be saved yet, if it cannot.
    fn blocked(&self, draft: &Config, name_shown: bool) -> Option<String> {
        let name =
            name_problem(&draft.multiplayer_name)
                .map(|p| if name_shown { p } else { format!("Multiplayer name: {p}") });
        name.or_else(|| server_problem(&draft.multiplayer_server_host, &self.port.borrow()))
    }

    /// The draft's theme on the window while it is tried.
    fn preview(&self, shell: &Shell) {
        let Some(draft) = self.draft.borrow().clone() else { return };
        let selected = shell.window().global::<Games>().get_selected();
        let game = selected.split('/').next().filter(|g| !g.is_empty());
        theme::apply(shell.window(), &draft.theme, game, draft.game_colors);
    }

    /// Android: saved a moment after the last change, once it can be.
    fn save_later(&self, shell: &Shell) {
        if !cfg!(target_os = "android") || self.draft.borrow().as_ref().is_some_and(|d| !d.setup_complete) {
            return;
        }
        let _ = shell;
        self.later.start(slint::TimerMode::SingleShot, Duration::from_millis(600), || {
            shell::with(|s| {
                if s.settings.changed(s) {
                    s.settings.save(s, false);
                }
            })
        });
    }

    /// Saves the draft; `finish` ends the first run.
    pub fn save(&self, shell: &Shell, finish: bool) {
        let Some(launcher) = shell.launcher().cloned() else { return };
        let Some(draft) = self.draft.borrow().clone() else { return };
        if self.saving.get() || self.blocked(&draft, self.name_used.get()).is_some() {
            return;
        }
        let mut config = launcher.config();
        take_edits(&draft, &mut config);
        config.setup_complete |= finish;
        self.saving.set(true);
        self.show(shell);
        std::thread::spawn(move || {
            let done = launcher.save_config(config);
            if done.is_ok() {
                launcher.refresh();
            }
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    s.settings.saving.set(false);
                    match done {
                        // Taken again from what was written; a page left meanwhile stays closed.
                        Ok(_) if s.settings.draft.borrow().is_some() => s.settings.open(s),
                        Ok(_) => {}
                        Err(e) => {
                            s.notify(openrac_launcher_service::Notice::error(e));
                            s.settings.show(s);
                        }
                    }
                })
            });
        });
    }

    /// A path setting's row; its check starts here when it is not known yet.
    fn row(&self, shell: &Shell, draft: &Config, id: &'static str, kind: PathKind, setup: bool) -> PathRow {
        let value = path_of(draft, id).map(|p| p.display().to_string()).unwrap_or_default();
        let (state, message) = if value.is_empty() {
            ("", String::new())
        } else {
            let key = (id, value.clone());
            let known = self.checks.borrow().get(&key).cloned();
            match known {
                Some(Some(check)) => (if check.ok { "ok" } else { "bad" }, check.message),
                Some(None) => ("checking", String::new()),
                None => {
                    self.checks.borrow_mut().insert(key.clone(), None);
                    if let Some(launcher) = shell.launcher().cloned() {
                        std::thread::spawn(move || {
                            let check = launcher.check_path(kind, &PathBuf::from(&key.1));
                            let _ = slint::invoke_from_event_loop(move || {
                                shell::with(|s| {
                                    s.settings.checks.borrow_mut().insert(key, Some(check));
                                    s.settings.show(s);
                                })
                            });
                        });
                    }
                    ("checking", String::new())
                }
            }
        };
        let found = self.found.borrow();
        let candidates = found.as_ref().map(|f| match id {
            "root" => f.roots.as_slice(),
            "python" => f.pythons.as_slice(),
            "docker" => f.dockers.as_slice(),
            _ => &[],
        });
        let found: Vec<SharedString> = candidates
            .unwrap_or_default()
            .iter()
            .map(|c| c.path.display().to_string())
            .filter(|p| *p != value)
            .map(SharedString::from)
            .collect();
        let (get_label, get_url) = match id {
            "root" => ("Get OpenRAC", shell.window().global::<Links>().get_repo()),
            "python" => ("Get Python", shell.window().global::<Links>().get_python()),
            _ => ("", SharedString::new()),
        };
        PathRow {
            id: id.into(),
            label: label(id).into(),
            hint: hint(id, setup).into(),
            value: value.into(),
            state: state.into(),
            message: message.into(),
            detail: draft.developer || id == "saves",
            folder: !matches!(id, "python" | "docker"),
            found: model(found),
            get_label: get_label.into(),
            get_url,
        }
    }

    /// The page, as the draft has it.
    pub fn show(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let Some(draft) = self.draft.borrow().clone() else { return };
        let saved = launcher.config();
        let ui = shell.window().global::<Settings>();
        let android = cfg!(target_os = "android");
        let standalone = launcher.standalone();
        let setup = !saved.setup_complete;
        let kind = |id: &str| PATHS.iter().find(|(i, _)| *i == id).map(|(_, k)| *k).unwrap_or(PathKind::Saves);
        let rows = |ids: &[&'static str]| -> ModelRc<PathRow> {
            model(ids.iter().map(|id| self.row(shell, &draft, id, kind(id), setup)).collect())
        };

        if setup {
            let (ask_root, ask_python) = (self.ask_root.get(), self.ask_python.get());
            ui.set_games_paths(rows(if android { &[] } else { &["discs"] }));
            let mut ask = Vec::new();
            ask.extend(ask_root.then_some("root"));
            ask.extend(ask_python.then_some("python"));
            ui.set_setup_ask(rows(&ask));
            let mut found = Vec::new();
            found.extend((!ask_root).then_some("root"));
            found.extend((!android).then_some("install"));
            found.extend((!standalone && !ask_python).then_some("python"));
            ui.set_setup_found(rows(&found));
            let names: Vec<&str> = [
                (!ask_root && draft.root.is_some()).then_some("OpenRAC"),
                (!standalone && !ask_python && draft.python.is_some()).then_some("Python"),
            ]
            .into_iter()
            .flatten()
            .collect();
            ui.set_found_line(
                match (self.found.borrow().is_some(), names.is_empty()) {
                    (false, _) => "Looking…".to_string(),
                    (true, true) => "Nothing yet.".to_string(),
                    (true, false) => format!("{}. Change them here if they are wrong.", names.join(", ")),
                }
                .into(),
            );
            ui.set_can_start(draft.root.is_some());
        } else {
            let mut games = Vec::new();
            if !android {
                games.extend(self.root_here.get().then_some("root"));
                games.extend(["discs", "install", "saves"]);
            }
            ui.set_games_paths(rows(&games));
            let python_asked = !standalone && !android && !draft.developer && saved.python.is_none();
            ui.set_program_paths(rows(if python_asked { &["python"] } else { &[] }));
            let mut dev = Vec::new();
            if draft.developer && !android {
                dev.extend((!self.root_here.get()).then_some("root"));
                dev.extend(["python", "docker"]);
            }
            ui.set_dev_paths(rows(&dev));
        }

        ui.set_theme(draft.theme.as_str().into());
        ui.set_game_colors(draft.game_colors);
        ui.set_discord_shown(!android);
        ui.set_discord_rpc(draft.discord_rpc);
        self.show_steam(shell);

        let name_shown = self.name_used.get();
        let name_error = name_problem(&draft.multiplayer_name);
        ui.set_name_shown(name_shown);
        ui.set_name(draft.multiplayer_name.as_str().into());
        ui.set_name_bad(name_error.is_some());
        ui.set_name_note(
            name_error
                .clone()
                .unwrap_or_else(|| {
                    "What other players see in online mods. 3 to 16 letters, digits and underscores.".into()
                })
                .into(),
        );
        let port = self.port.borrow().clone();
        let server_error = server_problem(&draft.multiplayer_server_host, &port);
        let official = is_official(&draft.multiplayer_server_host, &port);
        ui.set_server_shown(true);
        ui.set_host(draft.multiplayer_server_host.as_str().into());
        ui.set_port(port.as_str().into());
        ui.set_official(official);
        ui.set_server_bad(server_error.is_some());
        ui.set_server_note(
            server_error
                .unwrap_or_else(|| {
                    if official {
                        "The official OpenRAC hub. Online play in every game logs in here and picks a game server from its list.".into()
                    } else {
                        "Your own hub: online play in every game logs in here and picks a game server from its list. Everyone you play with uses the same one.".into()
                    }
                })
                .into(),
        );
        ui.set_testing(self.testing.get());
        let address = openrac_launcher_service::core::multiplayer::address(
            draft.multiplayer_server_host.trim(),
            draft.multiplayer_server_port,
        );
        let tested = self.tested.borrow().clone().filter(|(at, _, _)| *at == address);
        ui.set_tested(tested.as_ref().map(|t| t.2.as_str()).unwrap_or_default().into());
        ui.set_tested_ok(tested.is_some_and(|t| t.1));

        ui.set_updates(true);
        ui.set_check_updates(draft.check_updates);
        ui.set_check_mod_updates(draft.check_mod_updates);
        ui.set_auto_update_mods(draft.auto_update_mods);
        ui.set_cleanup_deps(draft.cleanup_unused_deps);

        ui.set_detailed_log(draft.detailed_game_log);
        ui.set_renderer_shown(!android);
        ui.set_renderer(draft.renderer.as_str().into());
        ui.set_renderer_label(if cfg!(target_os = "macos") { "Metal" } else { "Vulkan" }.into());
        ui.set_editor_advanced(draft.save_editor_advanced);
        ui.set_developer(draft.developer);
        ui.set_incremental(draft.incremental_builds);
        ui.set_rac3(draft.rac3_dev);
        ui.set_discord_id(draft.discord_client_id.clone().unwrap_or_default().into());
        ui.set_config_file(launcher.paths().config_file().map(|f| f.display().to_string()).unwrap_or_default().into());

        let blocked = self.blocked(&draft, name_shown).unwrap_or_default();
        ui.set_blocked(blocked.into());
        ui.set_changed(self.changed(shell));
        ui.set_saving(self.saving.get());
    }

    #[cfg(not(target_os = "android"))]
    fn show_steam(&self, shell: &Shell) {
        use openrac_launcher_service::core::steam::State;
        let ui = shell.window().global::<Settings>();
        let state = *self.steam.borrow();
        ui.set_steam_shown(state.is_some_and(|s| s != State::NoSteam));
        let Some(state) = state else { return };
        ui.set_steam_line(
            match state {
                State::FlatpakSteam => "Flatpak Steam can't start OpenRAC. Use Steam's normal package to add it.",
                State::NoUser => "Sign in to Steam once to add OpenRAC to it.",
                State::Added => "In your Steam library.",
                _ => "Add OpenRAC to your Steam library, with its name and pictures.",
            }
            .into(),
        );
        ui.set_steam_button(
            match state {
                State::Added => "In Steam",
                State::Outdated => "Update in Steam",
                _ => "Add to Steam",
            }
            .into(),
        );
        ui.set_steam_enabled(matches!(state, State::NotAdded | State::Outdated));
        ui.set_steam_busy(self.steam_busy.get());
    }

    #[cfg(target_os = "android")]
    fn show_steam(&self, shell: &Shell) {
        shell.window().global::<Settings>().set_steam_shown(false);
    }
}
