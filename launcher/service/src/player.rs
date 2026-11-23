//! What a player can do with a game right now: one answer, used by Android's disc shelf and the
//! game's page, so the main button says and does the same thing in both.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openrac_launcher_core::actions::{ActionView, Platform, Scope, State};
use openrac_launcher_core::disc_choice::DiscChoice;
use openrac_launcher_core::library::{GameView, VersionView};

use crate::host::Notice;
use crate::launcher::{Launcher, View};
use crate::progress::{phase_label, PLAY_ACTIONS};
use crate::queue::Job;

/// As core/src/actions.rs RAC3_GATE: why rac3's actions are blocked while its switch is off.
pub const RAC3_GATE: &str = "needs the Up Your Arsenal switch in Settings, under Developer";
/// The one-line note under rac3's actions while the switch is off.
pub const RAC3_NOTE: &str = "Up Your Arsenal is in development: turn it on in Settings, under Developer.";

/// The steps that get a game ready: while one runs, the button stops it.
const BUILD_STEPS: [&str; 6] =
    ["disc-files", "port-build", "port-rebuild", "port-part", "port-levels", "port-levels-check"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerKind {
    /// The disc is being read.
    Installing,
    /// Nothing is set up yet: choose the disc image.
    SetUp,
    /// Set up, but there is no port to play here.
    Unavailable,
    /// The player's disc is another version of this game.
    OtherDisc,
    Playing,
    /// Android: the game runs beside the launcher, in a window of its own.
    Return,
    Building,
    Play,
    Build,
    /// Built, but the levels the game draws are not read from the disc yet.
    Levels,
    /// Not built, and Rebuild can make it again.
    Rebuild,
    /// A port exists but something it needs is missing.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Disc,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Info,
}

/// The main button of a game.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub kind: PlayerKind,
    /// The button's words.
    pub label: String,
    pub icon: Icon,
    /// One line under the button: what it will do, or why it cannot; while a job runs, its step.
    pub hint: String,
    /// The page's main action (amber), not a stop or a dead end.
    pub primary: bool,
    pub disabled: bool,
    /// The job the button stops, while one runs.
    pub job: Option<Job>,
    /// A few words for a list: "Ready to play".
    pub status: String,
    pub tone: Tone,
}

impl PlayerState {
    fn new(kind: PlayerKind, label: &str, icon: Icon, status: &str, tone: Tone) -> PlayerState {
        PlayerState {
            kind,
            label: label.into(),
            icon,
            hint: String::new(),
            primary: false,
            disabled: false,
            job: None,
            status: status.into(),
            tone,
        }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// A running or queued job's button: it stops the job.
    fn stopping(kind: PlayerKind, job: &Job, stop: &str, status: &str, tone: Tone) -> Self {
        let queued = job.state == crate::queue::JobState::Queued;
        let mut state = PlayerState::new(kind, if queued { "Waiting…" } else { stop }, Icon::Stop, status, tone);
        state.hint = phase_label(job);
        state.job = Some(job.clone());
        state
    }

    /// The next step can be taken from beside a disc (Android's shelf).
    pub fn offers_step(&self) -> bool {
        !self.disabled && self.kind != PlayerKind::Installing
    }
}

/// The player's own state of the world a button is read from.
pub struct Situation<'a> {
    pub view: &'a View,
    pub jobs: &'a [Job],
    /// Android: the game's activity runs, with the library it plays.
    pub android_game: Option<&'a str>,
}

fn action<'a>(v: &'a VersionView, id: &str) -> Option<&'a ActionView> {
    v.actions.iter().find(|a| a.id == id && a.this_platform)
}

fn live_job<'a>(jobs: &'a [Job], key: &str, ids: &[&str]) -> Option<&'a Job> {
    jobs.iter().find(|j| j.state.is_live() && j.version() == Some(key) && ids.contains(&j.action_id.as_str()))
}

/// The launcher is running `v`'s game (with or without the player's mods).
pub fn is_playing(jobs: &[Job], v: &VersionView) -> bool {
    live_job(jobs, &v.version.key, &PLAY_ACTIONS).is_some()
}

fn choice_for<'a>(choices: &'a [DiscChoice], game: &str) -> Option<&'a DiscChoice> {
    choices.iter().find(|c| c.game == game)
}

/// The image of `game` to set it up from: the one in the games folder (true), or a developer's
/// chosen disc the launcher knows from before (false).
pub fn found_disc(view: &View, game: &str) -> Option<(PathBuf, bool)> {
    let choice = choice_for(&view.disc_choices, game);
    let chosen = choice.map(|c| c.chosen.as_str());
    let found = view.discs.iter().find(|d| d.game == game && chosen.is_none_or(|c| d.version == c));
    if let Some(found) = found {
        return Some((found.path.clone(), true));
    }
    choice.and_then(|c| c.images.get(&c.chosen)).map(|p| (p.clone(), false))
}

fn playable(v: &VersionView) -> bool {
    action(v, "play").is_some_and(|a| a.state != State::Planned)
}

/// The version a player means by "this game": a developer's chosen disc, else the one set up,
/// else the first whose disc image was found, else the first that can be played, else the first.
pub fn player_version<'a>(game: &'a GameView, view: &View) -> Option<&'a VersionView> {
    let chosen = choice_for(&view.disc_choices, &game.id).map(|c| c.chosen.as_str());
    let own = game.install.as_ref().and_then(|i| i.extracted.as_ref()).map(|b| b.version.as_str());
    let found = |v: &&VersionView| view.discs.iter().any(|d| d.game == game.id && d.version == v.version.key);
    let by_key = |key: Option<&str>| game.versions.iter().find(|v| Some(v.version.key.as_str()) == key);
    by_key(chosen)
        .or_else(|| by_key(own))
        .or_else(|| game.versions.iter().find(found))
        .or_else(|| game.versions.iter().find(|v| playable(v)))
        .or_else(|| game.versions.first())
}

fn builds(v: &VersionView) -> Vec<&ActionView> {
    ["port-rebuild", "port-build"].iter().filter_map(|id| action(v, id)).collect()
}

/// Play waits for the game's program, which is not there, and nothing here builds it.
pub fn missing_program(v: &VersionView) -> bool {
    action(v, "play").is_some_and(|a| a.missing_artifact) && builds(v).is_empty()
}

/// The game is built for this computer (on Android, the APK's), its levels ready or not.
pub fn is_built(game: &GameView, v: &VersionView) -> bool {
    game.install.as_ref().is_some_and(|i| i.compiled) || action(v, "play").is_some_and(|a| a.done)
}

/// What Rebuild runs: the build from scratch where this platform builds, else (Android) the levels
/// read from the disc again and checked. None when it cannot run, or when nothing would make Play
/// work (no program and nothing to build it).
pub fn rebuild_steps(v: &VersionView) -> Option<Vec<ActionView>> {
    let builds = builds(v);
    if !builds.is_empty() {
        return builds.into_iter().find(|a| a.runnable).map(|a| vec![a.clone()]);
    }
    if missing_program(v) {
        return None;
    }
    let levels = action(v, "port-levels").filter(|a| a.runnable)?;
    let part = action(v, "port-part").filter(|a| a.runnable);
    let check = action(v, "port-levels-check");
    Some(part.into_iter().chain([levels]).chain(check).cloned().collect())
}

/// What the "an update is ready" notice runs for a game built by an older OpenRAC.
#[derive(Debug, Clone)]
pub struct UpdateStep {
    pub action: ActionView,
    pub incremental: bool,
    /// Why it builds from scratch despite the Incremental builds setting.
    pub clean_because: Option<String>,
}

/// The build from scratch, or with Incremental builds (developers) the incremental one unless the
/// build folder is unusable. None when neither can run.
pub fn update_step(v: &VersionView, incremental: bool) -> Option<UpdateStep> {
    let usable = |a: Option<&ActionView>| a.filter(|a| a.this_platform && a.runnable).cloned();
    let full = usable(action(v, "port-rebuild"));
    let clean_because = v.port_clean_reason.clone().filter(|_| incremental && full.is_some());
    if incremental && clean_because.is_none() {
        if let Some(build) = usable(action(v, "port-build")) {
            return Some(UpdateStep { action: build, incremental: true, clean_because: None });
        }
    }
    full.map(|action| UpdateStep { action, incremental: false, clean_because })
}

/// Something of the levels is still to do: reading them, or (where the version has it) the check.
fn levels_needed(levels: Option<&ActionView>, check: Option<&ActionView>) -> bool {
    if levels.is_some_and(|l| l.runnable && !l.done) {
        return true;
    }
    check.is_some_and(|c| !c.done && (c.runnable || levels.is_some_and(|l| l.runnable)))
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The main button for version `v` of `game`.
pub fn player_state(game: &GameView, v: &VersionView, now: &Situation) -> PlayerState {
    use PlayerKind as K;
    let platform = Platform::current();
    let play = action(v, "play");
    let set_up = game.install.as_ref().and_then(|i| i.extracted.as_ref());
    let other = set_up.filter(|s| s.version != v.version.key);
    let install_id = format!("install:{}", v.version.game);
    let installing = now.jobs.iter().find(|j| j.action_id == install_id && j.state.is_live());
    // Android: a game whose Play the APK leaves out, as it carries no library for it.
    let not_in_app = platform == Platform::Android
        && play.is_none()
        && v.actions.iter().any(|a| a.id == "play" && a.state != State::Planned);
    let no_build_hint = format!(
        "This app does not carry {}: an APK built with it (tools/build_android.py) builds it from your disc.",
        game.title
    );

    if let Some(job) = installing {
        return PlayerState::stopping(K::Installing, job, "Stop", "Setting up…", Tone::Warn);
    }
    if platform == Platform::Ios && play.is_none() {
        return PlayerState::new(
            K::Unavailable,
            "Not available on iOS",
            Icon::Play,
            "Not available on iOS",
            Tone::Info,
        )
        .hint("This iOS version does not include an engine for this game.")
        .disabled();
    }
    if set_up.is_none() {
        let planned = play.is_none_or(|p| p.state == State::Planned);
        let found = found_disc(now.view, &v.version.game);
        let hint = if not_in_app {
            format!("{no_build_hint} You can set your disc up now.")
        } else if planned {
            "This game cannot be played yet. You can set your disc up now, ready for when it can.".into()
        } else if let Some((path, in_folder)) = &found {
            let where_ = if *in_folder { "Found in your games folder" } else { "Your disc image" };
            format!("{where_}: {}", file_name(path))
        } else {
            "Choose the image (.iso or .chd) of your own disc. OpenRAC never downloads a game.".into()
        };
        let label = if found.is_some() { "Set up" } else { "Set up from your disc" };
        let state = if planned {
            PlayerState::new(K::SetUp, label, Icon::Disc, "Not playable yet", Tone::Info)
        } else {
            PlayerState::new(K::SetUp, label, Icon::Disc, "Not set up", Tone::Warn).primary()
        };
        return state.hint(hint);
    }
    let Some(play) = play.filter(|p| p.state != State::Planned) else {
        return PlayerState::new(K::Unavailable, "Not playable yet", Icon::Play, "Not playable yet", Tone::Info)
            .hint("Your disc is set up. This game's PC port is not ready yet.")
            .disabled();
    };
    if not_in_app {
        return PlayerState::new(
            K::Unavailable,
            "Not playable here yet",
            Icon::Play,
            "Not playable here yet",
            Tone::Info,
        )
        .hint(no_build_hint)
        .disabled();
    }
    if other.is_some() {
        return PlayerState::new(K::OtherDisc, "Play", Icon::Play, "Set up from another disc", Tone::Info)
            .hint("This is set up from another disc. Set up from this one to play it.")
            .disabled();
    }
    let ours = format!("openrac-{}", v.version.game);
    if platform == Platform::Android && now.android_game.is_some_and(|lib| lib.is_empty() || lib.starts_with(&ours)) {
        return PlayerState::new(K::Return, "Return to game", Icon::Play, "Running", Tone::Ok)
            .hint("The game is running. It waits where you left it while the launcher is open.")
            .primary();
    }
    if let Some(job) = live_job(now.jobs, &v.version.key, &PLAY_ACTIONS) {
        return PlayerState::stopping(K::Playing, job, "Stop the game", "Running", Tone::Ok);
    }
    if let Some(job) = live_job(now.jobs, &v.version.key, &BUILD_STEPS) {
        let id = job.action_id.as_str();
        let compiles = id == "port-part" || (id.starts_with("port-") && id.ends_with("build"));
        let status = if compiles { "Building…" } else { "Getting ready…" };
        return PlayerState::stopping(K::Building, job, "Stop", status, Tone::Warn);
    }
    if play.runnable {
        // Play never builds: an outdated game is recommended a rebuild, in a notice on its page.
        return PlayerState::new(K::Play, "Play", Icon::Play, "Ready to play", Tone::Ok).primary();
    }
    let ready = |id: &str| action(v, id).is_some_and(|a| a.runnable && !a.done);
    if ready("disc-files") {
        return PlayerState::new(K::Levels, "Unpack and play", Icon::Disc, "Needs the disc's files", Tone::Warn)
            .hint("Writes your disc's data out as the files the game reads, then gets the rest ready and starts it. A few minutes and about as much space as the disc, once.")
            .primary();
    }
    if ready("port-part") {
        return PlayerState::new(K::Build, "Build and play", Icon::Build, "Needs building", Tone::Warn)
            .hint("Makes the game's code that is still in assembly from your disc and compiles it on this device (nothing is downloaded), reads its levels, then starts it. A few minutes, the first time only.")
            .primary();
    }
    // The levels are ready when the game itself has read every one of them (the check's stamp).
    let levels = action(v, "port-levels");
    if play.done && levels_needed(levels, action(v, "port-levels-check")) {
        let read = levels.is_some_and(|l| l.done);
        let (label, hint) = if read {
            ("Check and play", "Makes sure every level was read from your disc whole (reading them again if not), then starts the game. A few seconds.")
        } else {
            ("Get levels and play", "Reads the levels from your disc so the game can draw them, then starts the game. About half a minute and 1 GB, once.")
        };
        return PlayerState::new(K::Levels, label, Icon::Disc, "Needs its levels", Tone::Warn).hint(hint).primary();
    }
    if action(v, "port-build").is_some_and(|b| b.runnable) && !play.done {
        return PlayerState::new(K::Build, "Build and play", Icon::Build, "Needs building", Tone::Warn)
            .hint("Makes the game for this computer from OpenRAC's code and your disc, reads its levels from your disc, then starts it. A few minutes, the first time only.")
            .primary();
    }
    if !is_built(game, v) && rebuild_steps(v).is_some() {
        return PlayerState::new(K::Rebuild, "Rebuild", Icon::Build, "Needs rebuilding", Tone::Warn)
            .hint("Makes the game again from the start, so it can be played. Your saves are kept.")
            .primary();
    }
    if missing_program(v) {
        return PlayerState::new(K::Blocked, "Play", Icon::Play, "Needs a newer launcher", Tone::Warn)
            .hint("This launcher has no program for this game and cannot build it. Update the launcher (Settings): a newer one builds it from your disc.")
            .disabled();
    }
    if play.blockers.iter().any(|b| b == RAC3_GATE) {
        return PlayerState::new(K::Blocked, "Play", Icon::Play, "In development", Tone::Warn)
            .hint(RAC3_NOTE)
            .disabled();
    }
    let needs: Vec<&str> = play.blockers.iter().map(|b| b.strip_prefix("needs ").unwrap_or(b)).collect();
    let hint = if needs.is_empty() {
        "Not ready yet.".to_string()
    } else {
        format!("Not ready: it needs {}.", needs.join(", "))
    };
    PlayerState::new(K::Blocked, "Play", Icon::Play, "Not ready", Tone::Warn).hint(hint).disabled()
}

/// Whether a version has a port to play (its Play is not only planned).
pub fn has_port(v: &VersionView) -> bool {
    v.actions.iter().any(|a| a.id == "play" && a.state != State::Planned)
}

/// What Play warns of before it starts, on this platform.
pub fn play_warning(v: &VersionView) -> Option<&str> {
    action(v, "play").and_then(|a| a.warning.as_deref())
}

// ---- doing what the button says -----------------------------------------------------------------

/// The versions whose button was pressed and whose request still goes on, and the ones whose
/// visibility could not be added (not tried again until the launcher starts again).
#[derive(Default)]
pub(crate) struct Acting {
    pressed: Mutex<HashSet<String>>,
    no_visibility: Mutex<HashSet<String>>,
}

impl Launcher {
    /// The main button for version `key`, as the world is now.
    pub fn player_state(&self, key: &str) -> Option<PlayerState> {
        let jobs = self.jobs();
        let game = self.android_game();
        let view = self.view_lock();
        let library = view.library.as_ref()?;
        let g = library.games.iter().find(|g| g.versions.iter().any(|v| v.version.key == key))?;
        let v = g.versions.iter().find(|v| v.version.key == key)?;
        Some(player_state(g, v, &Situation { view: &view, jobs: &jobs, android_game: game.as_deref() }))
    }

    /// The version as the library has it now (it is read again after every step).
    fn fresh(&self, key: &str) -> Option<VersionView> {
        let view = self.view_lock();
        view.library.as_ref()?.games.iter().flat_map(|g| &g.versions).find(|v| v.version.key == key).cloned()
    }

    fn fresh_action(&self, key: &str, id: &str) -> Option<ActionView> {
        self.fresh(key).and_then(|v| action(&v, id).cloned())
    }

    /// Does what the main button of `key` says, on a thread of its own: stops its job, or gets
    /// the game ready and starts it. A second press while the first goes on does nothing.
    pub fn player_act(&self, key: &str) {
        let Some(state) = self.player_state(key) else { return };
        if let Some(job) = &state.job {
            return self.cancel(job.key);
        }
        let key = key.to_string();
        self.spawn(move |l| {
            if !l.inner.acting.pressed.lock().unwrap().insert(key.clone()) {
                return Ok(());
            }
            l.act(&key, &state);
            l.inner.acting.pressed.lock().unwrap().remove(&key);
            Ok(())
        });
    }

    fn act(&self, key: &str, state: &PlayerState) {
        let Some(v) = self.fresh(key) else { return };
        let scope = Scope::Version(key.to_string());
        match state.kind {
            PlayerKind::Return => self.host().show_game(),
            PlayerKind::SetUp => {
                let found = found_disc(&self.view(), &v.version.game).map(|(path, _)| path);
                self.set_up(&v.version.game, &v.version.title, found);
            }
            PlayerKind::Build | PlayerKind::Levels => {
                // One press gets the game ready and starts it. A game that warns before it starts
                // asks first, before minutes of building.
                if let Some(start) = action(&v, "play") {
                    if !self.accept_warning(&scope, start, true) {
                        return;
                    }
                }
                if self.prepare(key) {
                    self.play(key);
                }
                self.forget_warning(&scope, "play");
            }
            PlayerKind::Rebuild => {
                self.rebuild(key);
            }
            PlayerKind::Play => {
                self.add_visibility(key);
                self.play(key);
            }
            _ => {}
        }
    }

    /// Sets a game up and goes on to what playing needs next (the build, then the levels),
    /// stopping when it is ready to play. `image`: from the games folder; else the player picks.
    pub fn set_up(&self, game: &str, title: &str, image: Option<PathBuf>) -> bool {
        if !self.install_game(game, title, image) {
            return false;
        }
        self.refresh();
        let key = {
            let view = self.view_lock();
            let g = view.library.as_ref().and_then(|l| l.games.iter().find(|g| g.id == game));
            g.and_then(|g| player_version(g, &view)).map(|v| v.version.key.clone())
        };
        if let Some(key) = key.filter(|k| self.fresh_action(k, "play").is_some_and(|a| a.state != State::Planned)) {
            self.prepare(&key);
        }
        true
    }

    /// The player names the folder their PlayStation 2 games are in (on Android, picks an image,
    /// which is copied into the app's own games folder); every game OpenRAC knows in it that can
    /// be played and is not set up yet is set up, one after the other. How many games the folder
    /// holds, or None when nothing was chosen. Blocks.
    pub fn choose_games_folder(&self) -> Option<usize> {
        if Platform::current() == Platform::Android {
            self.host().pick_disc_image("Select your legitimately obtained disc image")?;
        } else {
            let dir = self.host().pick_folder("Choose the folder with your PlayStation 2 games")?;
            if !self.update_config(|c| c.discs_dir = Some(dir)) {
                return None;
            }
        }
        self.refresh();
        let view = self.view();
        if view.discs.is_empty() {
            self.notify(Notice::error("No Ratchet & Clank disc image (.iso or .chd) was found in that folder."));
            return Some(0);
        }
        for disc in &view.discs {
            let wanted = self.view_lock().library.as_ref().and_then(|l| {
                let game = l.games.iter().find(|g| g.id == disc.game)?;
                let set_up = game.install.as_ref().is_some_and(|i| i.extracted.is_some());
                let version = game.versions.iter().find(|v| v.version.key == disc.version)?;
                (!set_up && playable(version)).then_some(())
            });
            if wanted.is_some() {
                // The disc a developer chose, when the folder holds two (for a player, this one).
                let image = found_disc(&self.view(), &disc.game).map_or_else(|| disc.path.clone(), |(p, _)| p);
                self.set_up(&disc.game, &disc.title, Some(image));
            }
        }
        Some(view.discs.len())
    }

    /// Runs one step of `key` and reads the library again; whether it ran.
    fn step(&self, key: &str, actions: &[ActionView]) -> bool {
        let ok = self.run_chain(&Scope::Version(key.to_string()), actions, None);
        self.refresh();
        ok
    }

    /// Everything playing still needs, in order: the disc's files, the game built if it is not
    /// yet, then the levels read and checked. A game already built is not built again here.
    /// Whether the game can be played.
    pub fn prepare(&self, key: &str) -> bool {
        let Some(v) = self.fresh(key) else { return false };
        let ready = |a: &Option<ActionView>| a.as_ref().is_some_and(|a| a.runnable && !a.done);
        let files = action(&v, "disc-files").cloned();
        if ready(&files) && !self.step(key, &[files.unwrap()]) {
            return false;
        }
        let build = self.fresh_action(key, "port-build").filter(|b| b.runnable);
        let built = action(&v, "play").is_some_and(|p| p.done);
        if let (Some(build), false) = (build, built) {
            if !self.step(key, &[build]) {
                return false;
            }
        }
        let part = self.fresh_action(key, "port-part");
        if ready(&part) && !self.step(key, &[part.unwrap()]) {
            return false;
        }
        // Levels only when Play waits for them: a game that reads none starts as it is.
        let Some(now) = self.fresh(key) else { return false };
        let waits = !action(&now, "play").is_some_and(|p| p.runnable);
        if waits
            && levels_needed(action(&now, "port-levels"), action(&now, "port-levels-check"))
            && !self.prepare_levels(key)
        {
            return false;
        }
        self.fresh_action(key, "play").is_some_and(|p| p.runnable)
    }

    /// Reads the levels if that is still to do, then has the game check them. Levels that fail
    /// the check are read again, once, and checked again.
    fn prepare_levels(&self, key: &str) -> bool {
        let levels = self.fresh_action(key, "port-levels");
        if self.fresh_action(key, "port-levels-check").is_none() {
            return levels.is_some_and(|l| self.step(key, &[l]));
        }
        if let Some(levels) = levels.filter(|l| !l.done) {
            if !self.step(key, &[levels]) {
                return false;
            }
        }
        self.add_visibility(key);
        if let Some(check) = self.fresh_action(key, "port-levels-check") {
            if self.step(key, &[check]) {
                return true;
            }
        }
        // The check named levels it cannot draw: read them all again, in the old ones' place.
        let Some(reread) = self.fresh_action(key, "port-levels").filter(|l| l.runnable) else { return false };
        self.notify(Notice::info(
            "Your levels need reading again (an older version read them, or some are incomplete). Reading them from your disc…",
        ));
        if !self.step(key, &[reread]) {
            return false;
        }
        let ok = self.fresh_action(key, "port-levels-check").is_some_and(|check| self.step(key, &[check]));
        if !ok {
            self.notify(Notice::error("The levels still cannot be drawn: see Activity for which, and why."));
        }
        ok
    }

    /// Levels read before OpenRAC kept the game's record of what each spot can see get it added
    /// (a few seconds, once). Playing does not need it: without it the game draws all in view.
    fn add_visibility(&self, key: &str) {
        let Some(add) = self.fresh_action(key, "port-levels-visibility").filter(|a| a.runnable && !a.done) else {
            return;
        };
        if self.inner.acting.no_visibility.lock().unwrap().contains(key) {
            return;
        }
        if !self.step(key, &[add]) {
            self.inner.acting.no_visibility.lock().unwrap().insert(key.to_string());
        }
    }

    /// Rebuilds the version (rebuild_steps). Whether every step ran.
    pub fn rebuild(&self, key: &str) -> bool {
        let Some(steps) = self.fresh(key).and_then(|v| rebuild_steps(&v)) else { return false };
        self.step(key, &steps)
    }

    /// Starts the version. With the Mods switch under Play on, the mods come too: the packs that
    /// are on are put together first (overrides-stack), then the game starts with them
    /// (play-edited). A version without those actions plays as it is.
    fn play(&self, key: &str) {
        let Some(v) = self.fresh(key) else { return };
        let Some(start) = action(&v, "play").filter(|a| a.runnable) else { return };
        let with_mods = self.config().play_with_mods;
        if with_mods {
            // Mods are brought up to date first, never holding Play long.
            self.update_mods_before_play(key);
        }
        self.host().launching(&v.version.game, &v.version.title);
        let scope = Scope::Version(key.to_string());
        match action(&v, "play-edited").filter(|a| with_mods && a.runnable) {
            Some(modded) => {
                let steps: Vec<ActionView> =
                    action(&v, "overrides-stack").into_iter().chain([modded]).cloned().collect();
                self.run_chain(&scope, &steps, None);
            }
            None => self.run(&scope, start, None),
        }
    }
}
