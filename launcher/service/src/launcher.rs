//! The launcher's state and its start-up: the settings, the library, the discs found, and the
//! place every other part (tasks.rs, player.rs) works on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use openrac_launcher_core::actions::Platform;
use openrac_launcher_core::catalog::Catalog;
use openrac_launcher_core::config::{self, Config};
use openrac_launcher_core::detect::{self, Tool};
use openrac_launcher_core::disc_choice::{self, DiscChoice};
use openrac_launcher_core::discs::{self, FoundDisc};
use openrac_launcher_core::jobs::Jobs;
use openrac_launcher_core::library::{self, Library};
use openrac_launcher_core::{install, launcher_log, multiplayer, reports, saves};

use crate::host::{Change, Host, Notice};
use crate::paths::Paths;
use crate::player::Acting;
use crate::presence::Presence;
use crate::queue::Queue;
use crate::tasks::Tasks;

/// What the launcher knows of the games: read at start, after every job, on focus.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub library: Option<Library>,
    /// Why the library could not be read (no checkout set, a broken game.json).
    pub library_error: Option<String>,
    /// The games found in the player's games folder.
    pub discs: Vec<FoundDisc>,
    /// Developers only: the games whose disc can be switched.
    pub disc_choices: Vec<DiscChoice>,
    /// Each set-up game's crash reports: how many, and how many in the last three days unseen.
    pub crashes: BTreeMap<String, Crashes>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Crashes {
    pub total: usize,
    pub recent: usize,
}

/// The game stopped unexpectedly: the notice on its page, until dismissed.
#[derive(Debug, Clone, PartialEq)]
pub struct Stopped {
    pub game: String,
    pub version: String,
    /// Its exit status; None when the system stopped it (a signal).
    pub code: Option<i32>,
    /// When it stopped, milliseconds since 1970.
    pub at: u64,
    pub crash: Option<reports::ReportEntry>,
    /// The game's last lines of output, for the details when there is no report.
    pub lines: Vec<String>,
    /// Played again since, and ended well: the notice is one line now.
    pub played_since: bool,
}

/// The launcher: cheap to clone, every clone the same launcher.
#[derive(Clone)]
pub struct Launcher {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub host: Arc<dyn Host>,
    pub paths: Paths,
    /// The copy of OpenRAC this launcher carries, if it was built with one.
    pub bundled: Option<PathBuf>,
    pub version: String,
    pub config: Mutex<Config>,
    pub view: Mutex<View>,
    pub stopped: Mutex<Option<Stopped>>,
    pub jobs: Jobs,
    pub queue: Mutex<Queue>,
    pub tasks: Tasks,
    pub acting: Acting,
    pub presence: Presence,
    /// The version whose page is open, for Discord and the stopped-game notice.
    pub viewing: Mutex<Option<String>>,
    /// Android: the game's activity runs, with the library it plays ("" when unknown).
    pub android_game: Mutex<Option<String>>,
}

/// How the launcher starts.
pub struct Start {
    pub paths: Paths,
    /// The launcher's version, as it compares with its updates.
    pub version: String,
    /// The copy of OpenRAC the package carries, when it was found (see [`bundled_copy`]).
    pub bundled: Option<PathBuf>,
}

impl Launcher {
    /// Starts the launcher: reads its settings, settles the install folder, the checkout and
    /// Python it works with, opens its log and Discord. Nothing here waits for the network.
    pub fn start(start: Start, host: Arc<dyn Host>) -> Launcher {
        let Start { paths, version, bundled } = start;
        let config_file = paths.config_file();
        let mut config = config_file.as_deref().map(config::load).unwrap_or_default();
        config.progress_cache = paths.progress_cache();
        saves::set_card_root(config.save_dir.clone());
        if Platform::current() != Platform::Android {
            install::settle_install_dir(
                &mut config,
                paths.home_dir.as_deref(),
                paths.local_data_dir.as_deref(),
                config_file.as_deref(),
            );
        }
        // The first start: a made-up multiplayer name, kept.
        if multiplayer::settle(&mut config) {
            if let Some(file) = config_file.as_deref() {
                let _ = config::save(file, &config);
            }
        }
        detect::settle(&mut config, bundled.as_deref());
        #[cfg(target_os = "linux")]
        if let Some(root) = &config.root {
            openrac_launcher_core::desktop::ensure_games(root);
        }
        if let Some(dir) = &paths.config_dir {
            launcher_log::init(dir);
            launcher_log::line(&format!("the launcher {version} started on {:?}", Platform::current()));
        }
        migrate_install_saves(&config);
        tidy_set_ups(&config);
        let presence = Presence::start(&config);
        let launcher = Launcher {
            inner: Arc::new(Inner {
                host,
                paths,
                bundled,
                version,
                config: Mutex::new(config),
                view: Mutex::new(View::default()),
                stopped: Mutex::new(None),
                jobs: Jobs::default(),
                queue: Mutex::new(Queue::default()),
                tasks: Tasks::default(),
                acting: Acting::default(),
                presence,
                viewing: Mutex::new(None),
                android_game: Mutex::new(None),
            }),
        };
        launcher.first_run();
        launcher
    }

    pub(crate) fn host(&self) -> &dyn Host {
        &*self.inner.host
    }

    pub fn version(&self) -> &str {
        &self.inner.version
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    /// The launcher works on the copy of OpenRAC it carries: only what a player does is offered.
    pub fn standalone(&self) -> bool {
        let root = self.config().root;
        self.inner.bundled.as_deref().is_some_and(|copy| detect::on_the_copy(root.as_deref(), copy))
    }

    pub fn bundled(&self) -> Option<&Path> {
        self.inner.bundled.as_deref()
    }

    pub fn config(&self) -> Config {
        self.inner.config.lock().unwrap().clone()
    }

    pub fn view(&self) -> View {
        self.inner.view.lock().unwrap().clone()
    }

    pub(crate) fn view_lock(&self) -> MutexGuard<'_, View> {
        self.inner.view.lock().unwrap()
    }

    pub fn stopped(&self) -> Option<Stopped> {
        self.inner.stopped.lock().unwrap().clone()
    }

    pub(crate) fn notify(&self, notice: Notice) {
        self.host().notify(notice);
    }

    pub(crate) fn changed(&self, change: Change) {
        self.host().changed(change);
    }

    /// Runs `work` on a thread of its own: anything that reads files, runs a tool or waits for
    /// the player. A failure becomes an error toast.
    pub fn spawn(&self, work: impl FnOnce(&Launcher) -> Result<(), String> + Send + 'static) {
        let launcher = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = work(&launcher) {
                launcher.notify(Notice::error(e));
            }
        });
    }

    // ---- settings ------------------------------------------------------------------------------

    /// Saves the settings, as the player changed them. The launcher's own paths are set again
    /// here, a new save folder takes the saves of the old one along, and Discord follows.
    pub fn save_config(&self, mut config: Config) -> Result<Config, String> {
        let file = self.inner.paths.config_file().ok_or("no per-user config folder")?;
        if !multiplayer::is_valid(&config.multiplayer_name) {
            return Err(format!(
                "the multiplayer name must be {} to {} letters, digits or underscores",
                multiplayer::MIN_LEN,
                multiplayer::MAX_LEN
            ));
        }
        config.multiplayer_server_host = config.multiplayer_server_host.trim().to_string();
        if !multiplayer::is_valid_host(&config.multiplayer_server_host) || config.multiplayer_server_port == 0 {
            return Err(
                "the multiplayer server must be a host name or an IP address, and a port from 1 to 65535".into()
            );
        }
        // Developer tools switched off: back to the carried copy.
        detect::settle(&mut config, self.inner.bundled.as_deref());
        config.progress_cache = self.inner.paths.progress_cache();
        config.save_dir = config.save_dir.take().filter(|dir| !dir.as_os_str().is_empty());
        let was = self.config();
        if config.save_dir != was.save_dir {
            let home = saves::data_home();
            let from = was.save_dir.clone().unwrap_or_else(|| home.join("memcard"));
            let to = config.save_dir.clone().unwrap_or_else(|| home.join("memcard"));
            let copied = saves::copy_cards(&from, &to)?;
            launcher_log::line(&format!("saves: {copied} files copied from {} to {}", from.display(), to.display()));
        }
        // The images the set-ups were made from are the launcher's to record, never the page's.
        config.disc_images = was.disc_images.clone();
        config::save(&file, &config)?;
        saves::set_card_root(config.save_dir.clone());
        *self.inner.config.lock().unwrap() = config.clone();
        self.inner.presence.configure(&config);
        self.changed(Change::Config);
        Ok(config)
    }

    /// Changes the settings with `edit` and saves them; a failure is a toast.
    pub fn update_config(&self, edit: impl FnOnce(&mut Config)) -> bool {
        let mut config = self.config();
        edit(&mut config);
        match self.save_config(config) {
            Ok(_) => true,
            Err(e) => {
                self.notify(Notice::error(e));
                false
            }
        }
    }

    /// Writes the settings as they are (the launcher's own changes, never the player's).
    pub(crate) fn store_config(&self, config: &Config) -> Result<(), String> {
        let file = self.inner.paths.config_file().ok_or("no per-user config folder")?;
        config::save(&file, config)?;
        *self.inner.config.lock().unwrap() = config.clone();
        self.changed(Change::Config);
        Ok(())
    }

    /// A first start with nothing to ask: when the OpenRAC folder and a Python were found, they
    /// are taken, with Docker when present, and the welcome screen is skipped.
    fn first_run(&self) {
        let config = self.config();
        if config.setup_complete {
            return;
        }
        let found = detect::detect_all(&start_folders(), self.inner.bundled.as_deref());
        let root = config.root.clone().or_else(|| found.roots.first().map(|c| c.path.clone()));
        let python = config.python.clone().or_else(|| found.pythons.first().map(|c| c.path.clone()));
        let (Some(root), Some(python)) = (root, python) else { return };
        let mut config = config;
        config.root = Some(root);
        config.python = Some(python);
        config.docker = config.docker.or_else(|| found.dockers.first().map(|c| c.path.clone()));
        config.setup_complete = true;
        if let Err(e) = self.save_config(config) {
            launcher_log::line(&format!("first run: {e}"));
        }
    }

    // ---- the library ---------------------------------------------------------------------------

    /// Reads the library, the discs and the crash counts again: after a job, when the window
    /// gets the focus, after the settings change. Blocks; call it off the window's thread.
    pub fn refresh(&self) {
        let config = self.config();
        let (library, error) = match config.root.as_ref() {
            None => (None, Some("Choose your OpenRAC folder in Settings.".to_string())),
            Some(_) => match library::library(&config) {
                Ok(library) => (Some(library), None),
                Err(e) => (None, Some(e)),
            },
        };
        let (discs, choices) = scan_discs(&config).unwrap_or_default();
        let crashes = library.as_ref().map(|l| count_crashes(&config, l)).unwrap_or_default();
        {
            let mut view = self.view_lock();
            view.library = library.or_else(|| view.library.take().filter(|_| error.is_none()));
            view.library_error = error;
            view.discs = discs;
            view.disc_choices = choices;
            view.crashes = crashes;
        }
        self.changed(Change::Library);
    }

    /// [`Launcher::refresh`] on a thread of its own.
    pub fn refresh_soon(&self) {
        let launcher = self.clone();
        std::thread::spawn(move || launcher.refresh());
    }

    /// The player has seen the crashes `game` has now: no notice or mark shows for them again.
    pub fn dismiss_crashes(&self, game: &str) {
        let mut config = self.config();
        let newest = game_data(&config, game)
            .map(|data| reports::list(&data))
            .unwrap_or_default()
            .iter()
            .filter(|e| e.kind == reports::ReportKind::Crash)
            .map(|e| e.time)
            .max();
        let seen = config.crashes_seen.entry(game.to_string()).or_default();
        *seen = (*seen).max(newest.unwrap_or_default());
        if let Err(e) = self.store_config(&config) {
            self.notify(Notice::error(e));
        }
        self.refresh();
    }

    /// Puts the stopped-game notice away; its crash counts as seen.
    pub fn dismiss_stopped(&self) {
        let game = self.inner.stopped.lock().unwrap().take().map(|s| s.game);
        self.changed(Change::Stopped);
        if let Some(game) = game {
            self.dismiss_crashes(&game);
        }
    }

    /// The version whose page is open now, or None.
    pub fn viewing(&self) -> Option<String> {
        self.inner.viewing.lock().unwrap().clone()
    }

    /// The window shows the page of `version` (or no game's page): Discord follows.
    pub fn set_viewing(&self, version: Option<String>) {
        *self.inner.viewing.lock().unwrap() = version;
        self.update_presence();
    }

    /// Android: the library of the game whose activity runs, "" when unknown.
    pub fn android_game(&self) -> Option<String> {
        self.inner.android_game.lock().unwrap().clone()
    }

    pub(crate) fn set_stopped(&self, stopped: Option<Stopped>) {
        *self.inner.stopped.lock().unwrap() = stopped;
        self.changed(Change::Stopped);
    }

    /// The version a player means by `game` (see [`crate::player::player_version`]).
    pub fn player_version(&self, game: &str) -> Option<String> {
        let view = self.view_lock();
        let game = view.library.as_ref()?.games.iter().find(|g| g.id == game)?;
        crate::player::player_version(game, &view).map(|v| v.version.key.clone())
    }
}

/// A game's data folder, where its logs and crash reports are.
pub fn game_data(config: &Config, game: &str) -> Option<PathBuf> {
    if game.is_empty() || game.contains(['/', '\\', '.']) {
        return None;
    }
    config.install_dir.as_deref().map(|install| install::data_dir(install, game))
}

/// Where to look for a checkout: next to the launcher and the folder it was started from.
pub fn start_folders() -> Vec<PathBuf> {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from));
    exe.into_iter().chain(std::env::current_dir().ok()).collect()
}

/// The copy of OpenRAC the package carries in `resources`, when its Python runs here (an Intel
/// Mac has the Apple silicon one). A contributor's debug build works on its checkout instead,
/// unless OPENRAC_STANDALONE is set.
pub fn bundled_copy(resources: Option<&Path>, config_file: Option<&Path>) -> Option<PathBuf> {
    let copy = resources.and_then(detect::bundled_root)?;
    let contributor = cfg!(debug_assertions) && std::env::var_os("OPENRAC_STANDALONE").is_none();
    if contributor {
        let mut config = config_file.map(config::load).unwrap_or_default();
        detect::leave_copy(&mut config, &copy, &start_folders());
        return None;
    }
    detect::bundled_python(&copy).is_some_and(|python| detect::check_tool(Tool::Python, &python).ok).then_some(copy)
}

/// The games OpenRAC knows among the images in the games folder, and the discs a developer can
/// choose between.
fn scan_discs(config: &Config) -> Result<(Vec<FoundDisc>, Vec<DiscChoice>), String> {
    if config.root.is_none() || (config.discs_dir.is_none() && !config.developer) {
        return Ok(Default::default());
    }
    let (catalog, found) = scan(config)?;
    let choices = disc_choice::choices(config, &catalog, &found);
    Ok((found, choices))
}

/// The games folder looked through, and a developer's `baserom/` folders after it.
pub(crate) fn scan(config: &Config) -> Result<(Catalog, Vec<FoundDisc>), String> {
    let catalog = openrac_launcher_core::catalog::load(config.root()?)?;
    let mut found = config.discs_dir.as_deref().map(|dir| discs::scan(dir, &catalog)).unwrap_or_default();
    if config.developer {
        let root = config.root()?;
        let mut dirs = vec![root.join("baserom")];
        if let Ok(games) = std::fs::read_dir(root.join("games")) {
            let mut games: Vec<_> = games.flatten().map(|e| e.path().join("baserom")).collect();
            games.sort();
            dirs.extend(games);
        }
        for dir in dirs.iter().filter(|d| d.is_dir()) {
            for disc in discs::scan(dir, &catalog) {
                if !found.iter().any(|f| f.version == disc.version) {
                    found.push(disc);
                }
            }
        }
    }
    Ok((catalog, found))
}

/// How many crash reports each set-up game has, and how many of the last three days are unseen.
fn count_crashes(config: &Config, library: &Library) -> BTreeMap<String, Crashes> {
    let recent_since = crate::queue::now_ms() / 1000 - 3 * 86_400;
    let mut counts = BTreeMap::new();
    for game in library.games.iter().filter(|g| g.install.as_ref().is_some_and(|i| i.extracted.is_some())) {
        let Some(data) = game_data(config, &game.id) else { continue };
        let crashes: Vec<_> =
            reports::list(&data).into_iter().filter(|e| e.kind == reports::ReportKind::Crash).collect();
        let seen = config.crashes_seen.get(&game.id).copied().unwrap_or_default();
        let recent = crashes.iter().filter(|e| e.time > seen && e.time >= recent_since).count();
        counts.insert(game.id.clone(), Crashes { total: crashes.len(), recent });
    }
    counts
}

/// Memory cards and settings that some development builds kept in the install folder go to the
/// data folder's migrated/. Android never kept them there.
fn migrate_install_saves(config: &Config) {
    let Some(install) = config.install_dir.as_deref().filter(|_| Platform::current() != Platform::Android) else {
        return;
    };
    match saves::migrate_install_saves(install, &saves::data_home()) {
        Ok(0) => {}
        Ok(copied) => launcher_log::line(&format!("saves: {copied} files from {} are in migrated/", install.display())),
        Err(e) => launcher_log::line(&format!("saves: not migrated: {e}")),
    }
}

/// Frees what older set-ups hold and nothing reads, in the background.
fn tidy_set_ups(config: &Config) {
    let Some(install) = config.install_dir.clone() else { return };
    let discs = config.discs_dir.clone().filter(|_| Platform::current() == Platform::Android);
    std::thread::spawn(move || {
        let done = install::tidy(&install, discs.as_deref());
        for line in &done.lines {
            launcher_log::line(&format!("set-up: {line}"));
        }
        if done.bytes > 0 {
            launcher_log::line(&format!("set-up: {} MB freed", done.bytes >> 20));
        }
    });
}
