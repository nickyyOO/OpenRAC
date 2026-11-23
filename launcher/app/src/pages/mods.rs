//! The Mods tab's My mods: the game list with what each game's mods can be, and the picked game's
//! mods (its gameplay mods, the built-in entries, its tools) with their switches, order, updates
//! and removal, and adding one from a link, a file or a folder. What a mod is called and which
//! badge it shows is the service's (mod_words.rs); here only what goes where.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use openrac_launcher_service::core::enhanced::EnhancedView;
use openrac_launcher_service::core::library::VersionView;
use openrac_launcher_service::core::plugins::PluginView;
use openrac_launcher_service::core::remote::{Preview, Sources};
use openrac_launcher_service::{
    group_of, mod_badge, mod_game_state, mod_kind, mod_matches, moddable, player_version, Added, Launcher, ModGroup,
    Moddable,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::shell::{self, Shell};
use crate::{App, AppWindow, Games, Icons, LinkMod, MenuItem, ModFilter, ModRow, Mods, SideEntry, Tone};

const ENHANCED: &str = "enhanced-graphics";
const KEYBOARD: &str = "keyboard-mouse";
/// The toolbar shows from this many rows on: with fewer, the list is short enough as it is.
const TOOLBAR_FROM: usize = 6;

/// What was read for the version shown, and what the page keeps between reads.
#[derive(Default)]
pub struct ModsState {
    read: RefCell<Option<Read>>,
    /// Newer versions found by the last look, by mod.
    updates: RefCell<HashMap<String, String>>,
    /// The link read, and what it holds.
    link: RefCell<Option<(String, Preview)>>,
}

struct Read {
    key: String,
    mods: Result<Vec<PluginView>, String>,
    enhanced: Option<EnhancedView>,
    keyboard: Option<PluginView>,
    sources: Sources,
}

pub fn install(window: &AppWindow) {
    let mods = window.global::<Mods>();
    mods.on_pick(|key| shell::with(|s| s.pick_version(&key)));
    mods.on_open_url(|url| shell::with(|s| s.window().global::<App>().invoke_open_url(url)));
    mods.on_set_filter(|id| shell::with(|s| view_changed(s, |m| m.set_filter(id))));
    mods.on_set_sort(|id| shell::with(|s| view_changed(s, |m| m.set_sort(id))));
    mods.on_search(|text| shell::with(|s| view_changed(s, |m| m.set_query(text))));
    mods.on_turn_on_mods(|| shell::with(|s| s.launcher().map(|l| l.set_play_with_mods(true)).unwrap_or_default()));
    mods.on_set_on(|id, on| set_on(id.to_string(), on));
    mods.on_row_menu(|id, item| row_menu(id.to_string(), &item));
    mods.on_update(|id| update(vec![id.to_string()]));
    mods.on_update_all_now(|| shell::with(|s| update(s.mods.updates.borrow().keys().cloned().collect())));
    mods.on_check_updates(check_updates);
    mods.on_open_mod(|_| {});
    mods.on_add_file(|| {
        pick_then_add(|| crate::dialogs::pick_file("Choose a mod (.zip)", "Mod", &["zip", "openrac-map"]))
    });
    mods.on_add_folder(|| pick_then_add(|| crate::dialogs::pick_folder("Choose a mod's folder")));
    mods.on_add_link(|text| read_link(text.trim().to_string()));
    mods.on_install(|id| install_from_link(id.to_string()));
    mods.on_close_link(|| shell::with(|s| s.mods.close_link(s)));
}

/// The version the Mods tab shows: the one the game page plays, of the game picked.
fn shown(shell: &Shell) -> Option<VersionView> {
    let launcher = shell.launcher()?;
    let key = shell.window().global::<Games>().get_selected();
    let view = launcher.view();
    let library = view.library.as_ref()?;
    let game = library.games.iter().find(|g| g.versions.iter().any(|v| v.version.key == key.as_str()))?;
    player_version(game, &view).cloned()
}

/// Runs `work` off the window's thread with the launcher and the version shown, then shows what
/// it returned (the list as it now is, and what to say) if that version is still shown.
fn change(work: impl FnOnce(&Launcher, &VersionView) -> Result<(Vec<PluginView>, String), String> + Send + 'static) {
    shell::with(|s| {
        let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
        std::thread::spawn(move || {
            let done = work(&launcher, &v);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    let key = v.version.key;
                    match done {
                        Ok((views, said)) => {
                            s.mods.keep(&key, views);
                            s.mods.say(s, &said, false);
                        }
                        Err(e) => s.mods.say(s, &e, true),
                    }
                    s.mods.show(s);
                    s.read_game_details();
                })
            });
        });
    });
}

fn set_on(id: String, on: bool) {
    if id == ENHANCED {
        return shell::with(|s| {
            let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
            std::thread::spawn(move || {
                let done = launcher.set_enhanced(&v.version.game, &v.version.name, on);
                let _ = slint::invoke_from_event_loop(move || {
                    shell::with(|s| {
                        match done {
                            Ok(view) => s.mods.keep_enhanced(&v.version.key, view),
                            Err(e) => s.mods.say(s, &e, true),
                        }
                        s.mods.show(s);
                        s.read_game_details();
                    })
                });
            });
        });
    }
    let name = name_of(&id);
    change(move |l, v| l.switch_mod(&v.version.game, &v.version.name, &id, &name, on));
}

/// A mod's name as last read, for the questions about it.
fn name_of(id: &str) -> String {
    let mut name = id.to_string();
    shell::with(|s| {
        if let Some(m) = s.mods.find(id) {
            name = m.name;
        }
    });
    name
}

fn row_menu(id: String, item: &str) {
    let places = match item {
        "top" => i32::MAX,
        "up" => 1,
        "down" => -1,
        "bottom" => i32::MIN,
        "folder" => {
            return shell::with(|s| {
                let (Some(launcher), Some(v), Some(m)) = (s.launcher(), shown(s), s.mods.find(&id)) else { return };
                let game = v.version.game;
                launcher.spawn(move |l| l.open_mod_folder(&game, &m.path));
            });
        }
        "remove" => {
            let name = name_of(&id);
            return change(move |l, v| match l.remove_mod_asking(&v.version.game, &v.version.name, &id, &name)? {
                Some(done) => Ok(done),
                None => Ok((l.mods(&v.version.game, &v.version.name)?, String::new())),
            });
        }
        "update" => return update(vec![id]),
        _ => return,
    };
    // To the top or the bottom: as many places as there are mods to pass.
    let mut steps = places;
    shell::with(|s| {
        let read = s.mods.read.borrow();
        let Some(Ok(mods)) = read.as_ref().map(|r| &r.mods) else { return };
        let at = mods.iter().position(|m| m.id == id).unwrap_or_default() as i32;
        steps = match places {
            i32::MAX => at,
            i32::MIN => at + 1 - mods.len() as i32,
            n => n,
        };
    });
    change(move |l, v| Ok((l.move_mod_by(&v.version.game, &v.version.name, &id, steps)?, String::new())));
}

fn update(ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    shell::with(|s| s.window().global::<Mods>().set_checking(true));
    change(move |l, v| {
        let (game, version) = (&v.version.game, &v.version.name);
        let mut said = Vec::new();
        for id in &ids {
            match l.update_mod(game, version, id) {
                Ok(_) => {}
                Err(e) => said.push(format!("{id}: {e}")),
            }
        }
        let _ = slint::invoke_from_event_loop(move || {
            shell::with(|s| s.window().global::<Mods>().set_checking(false));
        });
        if !said.is_empty() {
            return Err(said.join(" "));
        }
        let text = match ids.len() {
            1 => "Updated. Its settings are kept.".to_string(),
            n => format!("Updated {n} mods. Their settings are kept."),
        };
        Ok((l.mods(game, version)?, text))
    });
    shell::with(|s| {
        // What was updated is no longer newer: the next look says again what is.
        s.mods.updates.borrow_mut().clear();
    });
}

fn check_updates() {
    shell::with(|s| {
        let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
        s.window().global::<Mods>().set_checking(true);
        std::thread::spawn(move || {
            let found = launcher.mod_updates(&v.version.game, &v.version.name);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    s.window().global::<Mods>().set_checking(false);
                    match found {
                        Ok((updates, errors)) => {
                            let none = updates.is_empty();
                            *s.mods.updates.borrow_mut() = updates.into_iter().map(|u| (u.id, u.available)).collect();
                            let text = match (none, errors.is_empty()) {
                                (_, false) => errors.join(" "),
                                (true, true) => "Every mod is up to date.".into(),
                                (false, true) => String::new(),
                            };
                            s.mods.say(s, &text, !errors.is_empty());
                        }
                        Err(e) => s.mods.say(s, &e, true),
                    }
                    s.mods.show(s);
                })
            });
        });
    });
}

/// Asks for a file or folder off the window's thread (the dialog waits there), then adds it.
fn pick_then_add(pick: impl FnOnce() -> Option<PathBuf> + Send + 'static) {
    std::thread::spawn(move || {
        if let Some(path) = pick() {
            let _ = slint::invoke_from_event_loop(move || add(vec![path]));
        }
    });
}

/// Adds the player's files to the version shown when they were picked.
fn add(paths: Vec<PathBuf>) {
    shell::with(|s| {
        let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
        let busy = match paths.as_slice() {
            [one] => format!("Adding {}…", one.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
            many => format!("Adding {} mods…", many.len()),
        };
        s.window().global::<Mods>().set_adding(busy.into());
        s.mods.say_added(s, "", false);
        std::thread::spawn(move || {
            let added = launcher.add_mods(&v, &paths);
            let _ = slint::invoke_from_event_loop(move || shell::with(|s| s.mods.added(s, added)));
        });
    });
}

fn read_link(link: String) {
    if link.is_empty() {
        return;
    }
    shell::with(|s| {
        let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
        s.mods.close_link(s);
        s.mods.say_added(s, "", false);
        s.window().global::<Mods>().set_adding("Reading the link…".into());
        std::thread::spawn(move || {
            let preview = launcher.preview_mod(&v.version.game, &v.version.name, &link);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    s.window().global::<Mods>().set_adding("".into());
                    match preview {
                        Ok(p) => *s.mods.link.borrow_mut() = Some((link, p)),
                        Err(e) => s.mods.say_added(s, &e, true),
                    }
                    s.mods.show_link(s);
                })
            });
        });
    });
}

fn install_from_link(id: String) {
    shell::with(|s| {
        let (Some(launcher), Some(v)) = (s.launcher().cloned(), shown(s)) else { return };
        let Some((url, label)) = s.mods.link.borrow().as_ref().map(|(u, p)| (u.clone(), p.label.clone())) else {
            return;
        };
        s.window().global::<Mods>().set_installing(id.as_str().into());
        std::thread::spawn(move || {
            let added = launcher.add_from_link(&v, &url, &label, &id);
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    s.window().global::<Mods>().set_installing("".into());
                    if !added.failed {
                        s.mods.installed_from_link(s, &id);
                    }
                    s.mods.added(s, added);
                })
            });
        });
    });
}

/// The search, kind or order changed: the rows are shown again.
fn view_changed(shell: &Shell, set: impl FnOnce(&Mods)) {
    set(&shell.window().global::<Mods>());
    shell.mods.show(shell);
}

/// The menu of a mod's row: update, move, its folder, remove.
fn menu(icons: &Icons, update: Option<&str>, at: usize, count: usize, movable: bool) -> Vec<MenuItem> {
    let item = |id: &str, label: String, icon: SharedString, danger: bool| MenuItem {
        id: id.into(),
        label: label.into(),
        icon,
        danger,
    };
    let mut items = Vec::new();
    if let Some(version) = update {
        items.push(item("update", format!("Update to {version}"), icons.get_refresh(), false));
    }
    if movable {
        if at > 1 {
            items.push(item("top", "Move to the top".into(), icons.get_up(), false));
        }
        if at > 0 {
            items.push(item("up", "Move up".into(), icons.get_up(), false));
        }
        if at + 1 < count {
            items.push(item("down", "Move down".into(), icons.get_down(), false));
        }
        if at + 2 < count {
            items.push(item("bottom", "Move to the bottom".into(), icons.get_down(), false));
        }
    }
    items.push(item("folder", "Open its folder".into(), icons.get_folder(), false));
    items.push(item("remove", "Remove".into(), icons.get_trash(), true));
    items
}

fn tone(tone: &str) -> Tone {
    match tone {
        "ok" => Tone::Ok,
        "warn" => Tone::Warn,
        "err" => Tone::Err,
        _ => Tone::Plain,
    }
}

impl ModsState {
    /// Another game is shown: nothing of the last one's stays.
    pub fn reset(&self, window: &AppWindow) {
        self.read.borrow_mut().take();
        self.updates.borrow_mut().clear();
        self.link.borrow_mut().take();
        let mods = window.global::<Mods>();
        mods.set_read(false);
        mods.set_said("".into());
        mods.set_add_said("".into());
        mods.set_query("".into());
        mods.set_filter("all".into());
        mods.set_sort("order".into());
        mods.set_revision(mods.get_revision() + 1);
        mods.set_link_mods(ModelRc::default());
    }

    /// Reads the shown version's mods off the window's thread, then shows them.
    pub fn read(&self, shell: &Shell) {
        let (Some(launcher), Some(v)) = (shell.launcher().cloned(), shown(shell)) else { return };
        std::thread::spawn(move || {
            let (game, version) = (&v.version.game, &v.version.name);
            let read = Read {
                key: v.version.key.clone(),
                mods: launcher.mods(game, version),
                enhanced: launcher.enhanced(game, version),
                keyboard: launcher.keyboard_mouse(game, version),
                sources: launcher.mod_sources(game).unwrap_or_default(),
            };
            let _ = slint::invoke_from_event_loop(move || {
                shell::with(|s| {
                    if shown(s).is_some_and(|v| v.version.key == read.key) {
                        *s.mods.read.borrow_mut() = Some(read);
                        s.mods.show(s);
                    }
                })
            });
        });
    }

    fn find(&self, id: &str) -> Option<PluginView> {
        let read = self.read.borrow();
        let read = read.as_ref()?;
        let mods = read.mods.as_ref().ok()?;
        mods.iter()
            .chain(read.enhanced.as_ref().map(|e| &e.entry))
            .chain(read.keyboard.as_ref())
            .find(|m| m.id == id)
            .cloned()
    }

    fn keep(&self, key: &str, views: Vec<PluginView>) {
        if let Some(read) = self.read.borrow_mut().as_mut().filter(|r| r.key == key) {
            read.mods = Ok(views);
        }
    }

    fn keep_enhanced(&self, key: &str, view: Option<EnhancedView>) {
        if let Some(read) = self.read.borrow_mut().as_mut().filter(|r| r.key == key) {
            read.enhanced = view;
        }
    }

    fn say(&self, shell: &Shell, text: &str, failed: bool) {
        let mods = shell.window().global::<Mods>();
        mods.set_said(text.into());
        mods.set_said_failed(failed);
    }

    /// Says how adding went, under the add box.
    fn say_added(&self, shell: &Shell, text: &str, failed: bool) {
        let mods = shell.window().global::<Mods>();
        mods.set_add_said(text.into());
        mods.set_add_said_failed(failed);
    }

    fn added(&self, shell: &Shell, added: Added) {
        shell.window().global::<Mods>().set_adding("".into());
        self.say_added(shell, &added.text, added.failed);
        if added.plugin || added.pack {
            self.read(shell);
            shell.read_game_details();
        }
    }

    fn close_link(&self, shell: &Shell) {
        self.link.borrow_mut().take();
        self.show_link(shell);
    }

    /// A mod of the link is installed: a repository of several keeps its preview for the others.
    fn installed_from_link(&self, shell: &Shell, id: &str) {
        let mut link = self.link.borrow_mut();
        if let Some((_, preview)) = link.as_mut() {
            if preview.mods.len() > 1 {
                for m in preview.mods.iter_mut().filter(|m| m.id == id) {
                    m.installed = Some(m.version.clone());
                }
            } else {
                link.take();
            }
        }
        drop(link);
        self.show_link(shell);
    }

    fn show_link(&self, shell: &Shell) {
        let mods = shell.window().global::<Mods>();
        let link = self.link.borrow();
        let Some((_, p)) = link.as_ref() else {
            mods.set_link_mods(ModelRc::default());
            return;
        };
        let from = if p.owner.is_empty() {
            format!("In {}", p.label)
        } else {
            format!("In {}, published by {}", p.label, p.owner)
        };
        mods.set_link_from(from.into());
        mods.set_link_note(p.note.clone().unwrap_or_default().into());
        mods.set_link_native(p.mods.iter().any(|m| m.kind == "native" && m.compatible));
        let rows: Vec<LinkMod> = p
            .mods
            .iter()
            .map(|m| {
                let kind = mod_kind(&m.kind);
                let installed = m.installed.as_deref();
                LinkMod {
                    id: m.id.as_str().into(),
                    name: m.name.as_str().into(),
                    meta: [m.version.as_str(), m.author.as_str()]
                        .iter()
                        .filter(|w| !w.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" · ")
                        .into(),
                    line: m.description.as_str().into(),
                    kind: match installed {
                        Some(v) => format!("{} · {v} installed", kind.text),
                        None => kind.text.into(),
                    }
                    .into(),
                    kind_tone: tone(kind.tone),
                    fits: m.compatible,
                    why: m.why.clone().unwrap_or_default().into(),
                    button: match installed {
                        Some(v) if v == m.version => "Reinstall".into(),
                        Some(_) => format!("Update to {}", m.version).into(),
                        None => "Install".into(),
                    },
                }
            })
            .collect();
        mods.set_link_mods(ModelRc::new(VecModel::from(rows)));
    }

    /// The game list, with what each game's mods can be.
    pub fn show_list(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let view = launcher.view();
        let jobs = launcher.jobs();
        let games = view.library.as_ref().map(|l| l.games.as_slice()).unwrap_or_default();
        let entries: Vec<SideEntry> = games
            .iter()
            .filter_map(|g| {
                let v = player_version(g, &view)?;
                let state = mod_game_state(v, &jobs);
                Some(SideEntry {
                    key: v.version.key.as_str().into(),
                    name: g.title.as_str().into(),
                    disc: crate::art::disc(&g.id, &v.version.name),
                    icon: "".into(),
                    state: state.text.into(),
                    tone: match state.tone {
                        "ok" => Tone::Ok,
                        "warn" => Tone::Warn,
                        _ => Tone::Info,
                    },
                    mark: false,
                    group: "".into(),
                })
            })
            .collect();
        shell.window().global::<Mods>().set_entries(ModelRc::new(VecModel::from(entries)));
    }

    /// The picked game's page: what its mods can be, and its rows as the toolbar picks them.
    pub fn show(&self, shell: &Shell) {
        let window = shell.window();
        let mods = window.global::<Mods>();
        let Some(launcher) = shell.launcher() else { return };
        let Some(v) = shown(shell) else { return };
        let title = launcher
            .view()
            .library
            .as_ref()
            .and_then(|l| l.games.iter().find(|g| g.id == v.version.game).map(|g| g.title.clone()))
            .unwrap_or_default();
        mods.set_title(title.into());
        let (mode, mode_title, mode_line) = match moddable(&v) {
            Moddable::NotYet => {
                ("none", "No mods for this game yet", "Mods arrive once the level editor can read its levels.")
            }
            Moddable::Levels { early: true } if !playable(&v) => (
                "early",
                "Early access",
                "Its levels open in OpenRAC Editor and you can make texture and model packs. Packing level edits and playing come later.",
            ),
            _ => ("mods", "", ""),
        };
        mods.set_mode(mode.into());
        mods.set_mode_title(mode_title.into());
        mods.set_mode_line(mode_line.into());
        let purist = !launcher.config().play_with_mods;
        let read = self.read.borrow();
        let Some(read) = read.as_ref().filter(|r| r.key == v.version.key) else {
            mods.set_read(false);
            return;
        };
        let list: &[PluginView] = read.mods.as_deref().unwrap_or_default();
        let updates = self.updates.borrow();
        // A pinned mod is kept at its version: no update is offered.
        let update_of = |m: &PluginView| {
            let pinned = read.sources.mods.get(&m.id).is_some_and(|s| s.pin);
            updates.get(&m.id).filter(|_| !pinned).map(String::as_str)
        };
        let (query, filter, sort) =
            (mods.get_query().to_string(), mods.get_filter().to_string(), mods.get_sort().to_string());
        let shows = |group: &str| filter == "all" || filter == group;
        let found = |m: &PluginView| mod_matches(&query, &[&m.name, &m.description, &m.author, &m.id]);
        let movable = sort == "order" && query.trim().is_empty();
        let arrange = |group: ModGroup| {
            let mut rows: Vec<&PluginView> = list.iter().filter(|m| group_of(&m.kind) == group && found(m)).collect();
            if sort == "name" {
                rows.sort_by_key(|m| m.name.to_lowercase());
            }
            rows
        };
        let built_in: Vec<&PluginView> =
            read.keyboard.iter().chain(read.enhanced.as_ref().map(|e| &e.entry)).filter(|m| found(m)).collect();
        let gameplay = arrange(ModGroup::Gameplay);
        let tools = arrange(ModGroup::Tools);
        let all_gameplay = list.iter().filter(|m| group_of(&m.kind) == ModGroup::Gameplay).count();
        let icons = window.global::<Icons>();
        let row = |m: &PluginView, at: usize, count: usize, movable: bool| {
            let update = update_of(m);
            let (badge, badge_tone, hint) = mod_badge(m, update, purist);
            let fixed = m.kind == "tool" || m.id == KEYBOARD;
            ModRow {
                id: m.id.as_str().into(),
                name: m.name.as_str().into(),
                line: m.description.as_str().into(),
                group: match group_of(&m.kind) {
                    ModGroup::Gameplay => "gameplay",
                    ModGroup::Looks => "looks",
                    ModGroup::Tools => "tools",
                }
                .into(),
                on: m.enabled || fixed,
                switchable: !fixed && (m.enabled || m.blocked.is_empty()),
                badge: badge.into(),
                badge_tone: tone(badge_tone),
                badge_hint: hint.into(),
                open_text: if m.settings.is_empty() && m.id != ENHANCED { "Details" } else { "Settings" }.into(),
                update: update.unwrap_or_default().into(),
                menu: if m.id == ENHANCED || m.id == KEYBOARD {
                    ModelRc::default()
                } else {
                    ModelRc::new(VecModel::from(menu(&icons, update, at, count, movable)))
                },
            }
        };
        let mut rows: Vec<ModRow> = Vec::new();
        if shows("looks") {
            rows.extend(built_in.iter().map(|m| row(m, 0, 1, false)));
        }
        if shows("gameplay") {
            rows.extend(gameplay.iter().enumerate().map(|(i, m)| row(m, i, gameplay.len(), movable)));
        }
        let tool_rows: Vec<ModRow> =
            if shows("tools") { tools.iter().map(|m| row(m, 0, 1, false)).collect() } else { Vec::new() };
        mods.set_heading(
            match filter.as_str() {
                "tools" => "Tools",
                "looks" => "Built in",
                _ => "Gameplay mods",
            }
            .into(),
        );
        let searching = !query.trim().is_empty();
        let empty = match &read.mods {
            Err(e) => e.clone(),
            Ok(_) if searching && rows.is_empty() && tool_rows.is_empty() => {
                format!("Nothing matches “{}”.", query.trim())
            }
            Ok(_) if all_gameplay == 0 && shows("gameplay") && !searching => {
                "No gameplay mods yet. Paste a mod's link above, or choose its file.".into()
            }
            Ok(_) if rows.is_empty() && tool_rows.is_empty() => "None of this kind yet.".into(),
            Ok(_) => String::new(),
        };
        let order_line = if !shows("gameplay") || gameplay.is_empty() {
            ""
        } else if !movable {
            "Sorted for viewing only: switch to Load order, with no search, to move mods."
        } else if gameplay.len() > 1 {
            "Load order: a mod lower in the list goes after the ones above it."
        } else {
            ""
        };
        mods.set_order_line(order_line.into());
        mods.set_empty(empty.into());
        mods.set_gameplay(ModelRc::new(VecModel::from(rows)));
        mods.set_tools(ModelRc::new(VecModel::from(tool_rows)));
        // The toolbar's kinds, each with its count: one with none is left out (All always shows).
        let looks = built_in_count(read);
        let tools_count = list.iter().filter(|m| group_of(&m.kind) == ModGroup::Tools).count();
        let total = all_gameplay + looks + tools_count;
        let filters: Vec<ModFilter> = [
            ("all", "All", total),
            ("gameplay", "Gameplay", all_gameplay),
            ("looks", "Graphics and sound", looks),
            ("tools", "Tools", tools_count),
        ]
        .into_iter()
        .filter(|(id, _, n)| *id == "all" || *n > 0)
        .map(|(id, label, n)| ModFilter { id: id.into(), label: label.into(), count: n as i32 })
        .collect();
        mods.set_filters(ModelRc::new(VecModel::from(filters)));
        mods.set_toolbar(total >= TOOLBAR_FROM || searching || filter != "all" || sort != "order");
        let on = list.iter().filter(|m| m.enabled && m.kind != "tool").count()
            + usize::from(read.enhanced.as_ref().is_some_and(|e| e.entry.enabled));
        mods.set_purist(purist && on > 0);
        let pending = list.iter().filter(|m| update_of(m).is_some()).count();
        mods.set_update_all(if pending > 1 { format!("Update all ({pending})").into() } else { "".into() });
        mods.set_read(true);
    }
}

fn built_in_count(read: &Read) -> usize {
    usize::from(read.enhanced.is_some()) + usize::from(read.keyboard.is_some())
}

fn playable(v: &VersionView) -> bool {
    openrac_launcher_service::action(v, "play")
        .is_some_and(|a| a.state != openrac_launcher_service::core::actions::State::Planned)
}
