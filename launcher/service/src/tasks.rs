//! Running actions: one job at a time through the queue, chains of steps behind one button,
//! setting a game up from the player's disc, and what the launcher does when a job ends.
//!
//! Every method that waits (for a dialog, for a step to end) blocks its thread: the window calls
//! the `_soon` forms, which run on a thread of their own.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, MutexGuard};

use openrac_launcher_core::actions::{ActionView, Inputs, Kind, Scope};
use openrac_launcher_core::discord::{self, DiscordStatus};
use openrac_launcher_core::jobs::{self, Event, Started};
use openrac_launcher_core::{chd, disc_choice, enhanced, install, launcher_log, library, reports, saves};

use crate::failure::{bake_failure, extractor_error, tool_failure};
use crate::host::{Change, Notice};
use crate::launcher::{game_data, Launcher, Stopped};
use crate::progress::{is_play, tail_lines, PLAY_ACTIONS};
use crate::queue::{now_ms, Job, JobState, Queue};

/// Actions that are one request to the player: the game with mods or without.
const ACTION_GROUPS: &[&[&str]] = &[&PLAY_ACTIONS];

/// How a queued job starts, when it is not a plain action.
enum Starter {
    /// An action asked with a level, a file or an option.
    Action(Inputs),
    /// Setting a game up from a disc image.
    Install { game: String, image: PathBuf },
}

/// The bookkeeping around the queue that the player never sees.
#[derive(Default)]
pub(crate) struct Tasks {
    /// Requests whose start was asked for and is not answered yet (a dialog is open, a chain is
    /// between its steps): a second click on the same button finds its request here.
    asking: Mutex<HashSet<String>>,
    /// Actions whose warning was just accepted, for the start that follows.
    accepted: Mutex<HashSet<String>>,
    starters: Mutex<HashMap<u64, Starter>>,
    /// Who waits for a job to end (a chain's next step), by the job's key.
    waiting: Mutex<HashMap<u64, Sender<bool>>>,
    /// The job the Tasks page shows.
    selected: Mutex<Option<u64>>,
}

/// A request's key in `asking`: an action of a group is known by its group, whatever its inputs.
fn request_key(scope: &Scope, id: &str, inputs: &str) -> String {
    let group = ACTION_GROUPS.iter().find(|g| g.contains(&id));
    let id = group.map_or(id, |g| g[0]);
    format!("{}\n{id}\n{}", scope_name(scope), if group.is_some() { "" } else { inputs })
}

fn scope_name(scope: &Scope) -> &str {
    match scope {
        Scope::Version(key) => key,
        Scope::Repository => "repository",
    }
}

/// The inputs as one string: the same action with other inputs is another request.
fn inputs_key(inputs: Option<&Inputs>) -> String {
    inputs.map(|i| format!("{i:?}")).unwrap_or_default()
}

/// What Discord says a long job is doing; None for jobs it does not show.
fn task_of(action_id: &str) -> Option<&'static str> {
    match action_id {
        id if id.starts_with("install:") => Some("Setting up from the disc"),
        "port-build" | "build" => Some("Building the game"),
        "editor-extract" | "editor-import" => Some("Preparing the level editor"),
        _ => None,
    }
}

/// Marks a request as asked for while it lives: dropped, the request is answered.
struct Asking<'a> {
    set: &'a Mutex<HashSet<String>>,
    key: String,
}

impl Drop for Asking<'_> {
    fn drop(&mut self) {
        self.set.lock().unwrap().remove(&self.key);
    }
}

impl Launcher {
    pub(crate) fn queue(&self) -> MutexGuard<'_, Queue> {
        self.inner.queue.lock().unwrap()
    }

    fn tasks(&self) -> &Tasks {
        &self.inner.tasks
    }

    // ---- what the Tasks page reads -------------------------------------------------------------

    pub fn jobs(&self) -> Vec<Job> {
        self.queue().jobs().to_vec()
    }

    pub fn job(&self, key: u64) -> Option<Job> {
        self.queue().get(key).cloned()
    }

    pub fn running_job(&self) -> Option<Job> {
        self.queue().running().cloned()
    }

    /// Jobs not finished: running and queued.
    pub fn pending_jobs(&self) -> usize {
        self.queue().pending()
    }

    pub fn selected_job(&self) -> Option<u64> {
        *self.tasks().selected.lock().unwrap()
    }

    pub fn select_job(&self, key: Option<u64>) {
        *self.tasks().selected.lock().unwrap() = key;
        self.changed(Change::Jobs);
    }

    /// Whether a game runs now: mods load at its start, so changing them waits for it to close.
    pub fn game_running(&self) -> bool {
        self.android_game_running()
            || self.queue().jobs().iter().any(|j| is_play(&j.action_id) && j.state == JobState::Running)
    }

    pub fn clear_finished(&self) {
        self.queue().clear();
        self.changed(Change::Jobs);
    }

    /// A queued job is dropped; a running one is stopped with everything it started.
    pub fn cancel(&self, key: u64) {
        let Some(job) = self.job(key) else { return };
        match job.state {
            JobState::Queued => {
                self.queue().dequeue(key);
                self.changed(Change::Jobs);
                self.settle(key, false);
            }
            JobState::Running => {
                if let Some(id) = job.id {
                    let jobs = self.inner.jobs.clone();
                    self.spawn(move |_| jobs.cancel(id));
                }
            }
            _ => {}
        }
    }

    // ---- asking --------------------------------------------------------------------------------

    /// Whether asking for `id` in `scope` would repeat what was already asked for: a queued or
    /// running job of it (or of its group), or a start not yet answered. The job found is
    /// selected, and the player told, so a second click shows the first one's job.
    fn already_asked(&self, scope: &Scope, id: &str, inputs: &str) -> bool {
        if self.tasks().asking.lock().unwrap().contains(&request_key(scope, id, inputs)) {
            return true;
        }
        let Some(job) = self.queue().live(scope, id, inputs, ACTION_GROUPS).cloned() else { return false };
        self.select_job(Some(job.key));
        let said = if job.state == JobState::Running { "Already running" } else { "Already queued" };
        self.notify(Notice::info(format!("{said}: {}", job.title)));
        true
    }

    fn asking(&self, key: String) -> Asking<'_> {
        self.tasks().asking.lock().unwrap().insert(key.clone());
        Asking { set: &self.tasks().asking, key }
    }

    /// An action with a `warning` (a game far from finished) starts only once the player has read
    /// it and said go ahead. With `keep` the answer is kept for the start that follows (a build
    /// first, then the game): that start then asks nothing.
    pub fn accept_warning(&self, scope: &Scope, action: &ActionView, keep: bool) -> bool {
        let Some(warning) = &action.warning else { return true };
        let key = format!("{}\n{}", scope_name(scope), action.id);
        {
            let mut accepted = self.tasks().accepted.lock().unwrap();
            if accepted.contains(&key) {
                if !keep {
                    accepted.remove(&key);
                }
                return true;
            }
        }
        let yes = if action.kind == Kind::Play { "Play anyway" } else { "Run anyway" };
        if !self.host().confirm(warning, yes) {
            return false;
        }
        if keep {
            self.tasks().accepted.lock().unwrap().insert(key);
        }
        true
    }

    /// Drops an answer [`Launcher::accept_warning`] kept for a start that did not come.
    pub fn forget_warning(&self, scope: &Scope, id: &str) {
        self.tasks().accepted.lock().unwrap().remove(&format!("{}\n{id}", scope_name(scope)));
    }

    // ---- running -------------------------------------------------------------------------------

    /// Runs an action: a detached one (the game, the editor) at once, anything else through the
    /// queue. A second request for the same action while the first is queued, running or
    /// starting does nothing.
    pub fn run(&self, scope: &Scope, action: &ActionView, inputs: Option<Inputs>) {
        let key = inputs_key(inputs.as_ref());
        if self.already_asked(scope, &action.id, &key) {
            return;
        }
        let _asking = self.asking(request_key(scope, &action.id, &key));
        if !self.accept_warning(scope, action, false) {
            return;
        }
        if action.detached {
            match self.start_action(scope, &action.id, &inputs.unwrap_or_default()) {
                Ok(started) => self.notify(Notice::info(format!("Started: {}", started.title))),
                Err(e) => self.notify(Notice::error(e)),
            }
            self.update_presence();
            return;
        }
        let running = self.queue().running().is_some();
        self.enqueue(scope, &action.id, &action.label, &key, inputs.map(Starter::Action));
        self.notify(Notice::info(format!("{}: {}", if running { "Queued" } else { "Started" }, action.label)));
        self.pump();
    }

    /// [`Launcher::run`] on a thread of its own.
    pub fn run_soon(&self, scope: Scope, action: ActionView, inputs: Option<Inputs>) {
        self.spawn(move |l| {
            l.run(&scope, &action, inputs);
            Ok(())
        });
    }

    /// Runs actions one after another and stops at the first that fails: "extract the levels,
    /// import them, open the editor" behind one button. A detached step is started and not
    /// waited for. Whether every step ran.
    pub fn run_chain(&self, scope: &Scope, actions: &[ActionView], inputs: Option<Inputs>) -> bool {
        let key = inputs_key(inputs.as_ref());
        // The chain is one request, known by its last step (what the button is for).
        let Some(last) = actions.last() else { return true };
        if actions.iter().any(|a| self.already_asked(scope, &a.id, &key)) {
            return false;
        }
        let _asking = self.asking(request_key(scope, &last.id, &key));
        if !actions.iter().all(|a| self.accept_warning(scope, a, false)) {
            return false;
        }
        for action in actions {
            if action.detached {
                if let Err(e) = self.start_action(scope, &action.id, &inputs.clone().unwrap_or_default()) {
                    self.notify(Notice::error(e));
                    return false;
                }
                continue;
            }
            let job = self.enqueue(scope, &action.id, &action.label, &key, inputs.clone().map(Starter::Action));
            let ended = self.wait_for(job);
            self.pump();
            if !ended.recv().unwrap_or(false) {
                return false;
            }
        }
        true
    }

    /// [`Launcher::run_chain`] on a thread of its own.
    pub fn run_chain_soon(&self, scope: Scope, actions: Vec<ActionView>, inputs: Option<Inputs>) {
        self.spawn(move |l| {
            l.run_chain(&scope, &actions, inputs);
            Ok(())
        });
    }

    /// Adds a job to the queue and shows it; its key.
    fn enqueue(&self, scope: &Scope, id: &str, title: &str, inputs: &str, starter: Option<Starter>) -> u64 {
        let key = self.queue().add(scope.clone(), id, title, inputs);
        if let Some(starter) = starter {
            self.tasks().starters.lock().unwrap().insert(key, starter);
        }
        self.select_job(Some(key));
        key
    }

    /// Hears whether job `key` succeeded, once it ends.
    fn wait_for(&self, key: u64) -> Receiver<bool> {
        let (send, receive) = mpsc::channel();
        self.tasks().waiting.lock().unwrap().insert(key, send);
        receive
    }

    fn settle(&self, key: u64, succeeded: bool) {
        if let Some(waiter) = self.tasks().waiting.lock().unwrap().remove(&key) {
            let _ = waiter.send(succeeded);
        }
    }

    /// Starts the next queued job, if nothing is running.
    fn pump(&self) {
        let Some(job) = self.queue().next() else { return };
        self.changed(Change::Jobs);
        let starter = self.tasks().starters.lock().unwrap().remove(&job.key);
        let launcher = self.clone();
        std::thread::spawn(move || {
            let started = match starter {
                Some(Starter::Install { game, image }) => launcher.start_install(&game, &image),
                Some(Starter::Action(inputs)) => launcher.start_action(&job.scope, &job.action_id, &inputs),
                None => launcher.start_action(&job.scope, &job.action_id, &Inputs::default()),
            };
            launcher.on_started(&job, started);
        });
    }

    fn on_started(&self, job: &Job, started: Result<Started, String>) {
        match started {
            Ok(started) if started.detached => {
                self.queue().launched(job.key, &started.title, &started.command);
                self.changed(Change::Jobs);
                self.settle(job.key, true);
                self.update_presence();
                self.pump();
            }
            Ok(started) => {
                let ended = {
                    let mut queue = self.queue();
                    queue.started(job.key, started.id, &started.title, &started.command);
                    // The job may have ended before the start returned: its end was kept.
                    std::iter::from_fn(|| queue.take_ended()).collect::<Vec<_>>()
                };
                self.changed(Change::Jobs);
                if ended.is_empty() {
                    self.update_presence();
                }
                for job in ended {
                    self.handle_exit(&job);
                }
            }
            Err(e) => {
                self.queue().refused(job.key, &e);
                self.changed(Change::Jobs);
                self.notify(Notice::error(e));
                self.settle(job.key, false);
                self.update_presence();
                self.pump();
            }
        }
    }

    /// Starts action `id` of `scope`: a job whose events go into the queue, or a detached launch.
    fn start_action(&self, scope: &Scope, id: &str, inputs: &Inputs) -> Result<Started, String> {
        let config = self.config();
        let mut plan = library::plan(&config, scope, id, inputs)?;
        // The lighting bake reads the player's mods from a snapshot that lives as long as it runs.
        let mut bake_mods = None;
        if let (enhanced::BAKE_ACTION, Scope::Version(key)) = (id, scope) {
            bake_mods = enhanced::assemble_bake_mods(&config, key, true)?;
            if let Some(snapshot) = &bake_mods {
                plan.args.extend(["--mods-dir".into(), snapshot.path().to_string_lossy().into_owned()]);
            }
        }
        let play = library::is_play(&config, scope, id);
        // On Android the game is an activity of the launcher's own, not a program it runs.
        if play {
            if let Some(started) = self.host().launch_game(&plan) {
                launcher_log::line(&format!("play: {}", jobs::command_line(&plan)));
                return started;
            }
        }
        launcher_log::line(&format!("run: {}", jobs::command_line(&plan)));
        // The game named by its log's planets and bolts, for Discord.
        let game = match (play, scope) {
            (true, Scope::Version(key)) => key.split('/').next().map(str::to_string),
            _ => None,
        };
        let title = plan.title.clone();
        let launcher = self.clone();
        let sink = jobs::batched(move |events| launcher.on_events(events, game.as_deref(), &title, &bake_mods));
        let started = self.inner.jobs.start(&plan, config.root.as_deref(), sink);
        if let Err(e) = &started {
            launcher_log::line(&format!("could not start: {e}"));
        }
        started
    }

    /// A running job's output and end, a few times a second.
    fn on_events<T>(&self, events: Vec<Event>, game: Option<&str>, title: &str, _keep_alive: &T) {
        let presence = self.config().discord_rpc;
        for event in &events {
            match event {
                Event::Output { line, .. } => {
                    if let (Some(game), true) = (game, presence) {
                        if let Some(log) = discord::read_game_log(game, line) {
                            self.inner.presence.game_log(log);
                        }
                    }
                }
                Event::Exit { code, cancelled, .. } => {
                    let how = match (cancelled, code) {
                        (true, _) => "cancelled".to_string(),
                        (false, Some(c)) => format!("exit code {c}"),
                        (false, None) => "stopped by a signal (no exit code)".to_string(),
                    };
                    launcher_log::line(&format!("ended: {title}: {how}"));
                    if game.is_some() {
                        self.inner.presence.end_play();
                    }
                }
            }
        }
        let ended = {
            let mut queue = self.queue();
            for event in events {
                queue.apply(event);
            }
            std::iter::from_fn(|| queue.take_ended()).collect::<Vec<_>>()
        };
        self.changed(Change::Jobs);
        for job in ended {
            self.handle_exit(&job);
        }
    }

    /// A job ended: what the launcher does about it. Called once per job.
    fn handle_exit(&self, job: &Job) {
        let version = job.version().map(str::to_string);
        let played = version.is_some() && is_play(&job.action_id);
        if played {
            // The game may have saved and logged: the Saves list and reports read again.
            self.changed(Change::GameData);
        }
        match (job.state, version.as_deref()) {
            // The game stopping on its own is not a failed task: its page says what happened.
            (JobState::Failed, Some(_)) if played => self.game_stopped(job),
            (JobState::Failed, Some(_)) if job.action_id == enhanced::BAKE_ACTION => {
                let failure = bake_failure(&job.lines, job.code);
                self.notify(Notice {
                    details: Some(format!("{}\n{}\n\n{}", failure.explanation, failure.suggestion, failure.detail)),
                    ..Notice::error(failure.title)
                });
            }
            // Setting a game up says why it stopped itself (install_game).
            (JobState::Failed, _) if !job.action_id.starts_with("install:") => {
                let details = tool_failure(&job.lines).details;
                self.notify(Notice {
                    details: (!details.is_empty()).then_some(details),
                    ..Notice::error(format!("{} failed", job.title))
                });
            }
            _ => {}
        }
        if job.state == JobState::Succeeded {
            // Played to the end since: the notice folds to a line.
            if let (true, Some(version)) = (played, &version) {
                let mut stopped = self.inner.stopped.lock().unwrap();
                if let Some(s) = stopped.as_mut().filter(|s| version.starts_with(&format!("{}/", s.game))) {
                    s.played_since = true;
                    drop(stopped);
                    self.changed(Change::Stopped);
                }
            }
            // The levels were read again: Enhanced graphics' lighting is baked again for them.
            if let (Some(version), "port-levels-check") = (&version, job.action_id.as_str()) {
                self.bake_after_levels(version);
            }
        }
        self.settle(job.key, job.state == JobState::Succeeded);
        self.refresh_soon();
        self.update_presence();
        self.pump();
    }

    /// The game ended with an error status or a signal: its crash report, and the notice.
    fn game_stopped(&self, job: &Job) {
        let Some(version) = job.version() else { return };
        let game = version.split('/').next().unwrap_or(version).to_string();
        let since = (job.started_at.unwrap_or(job.queued_at) / 1000).saturating_sub(2);
        let crash = game_data(&self.config(), &game).and_then(|data| reports::latest_crash(&data, since));
        if let Some(e) = &crash {
            launcher_log::line(&format!(
                "the game stopped unexpectedly: {} ({})",
                e.file,
                e.reason.as_deref().unwrap_or("?")
            ));
        }
        let lines = tail_lines(&job.lines, 40);
        let crashed = crash.is_some();
        self.set_stopped(Some(Stopped {
            game: game.clone(),
            version: version.to_string(),
            code: job.code,
            at: job.ended_at.unwrap_or_else(now_ms),
            crash,
            lines,
            played_since: false,
        }));
        // On another page the player is told where to look.
        let viewing = self.viewing().is_some_and(|v| v.starts_with(&format!("{game}/")));
        if !viewing {
            self.notify(if crashed {
                Notice::error("The game crashed. Its page has a report you can send us.")
            } else {
                Notice::info("The game was closed. Its page has what happened.")
            });
        }
    }

    /// With Enhanced graphics on, bakes the lighting of the levels whose bake is missing or older
    /// than their export (the bake keeps the ones still current).
    fn bake_after_levels(&self, key: &str) {
        let Some((game, name)) = key.split_once('/') else { return };
        if !enhanced::supported(game, name) {
            return;
        }
        let config = self.config();
        let bake = enhanced::version_bake_status(&config, key);
        let view = enhanced::view(&saves::data_home(), game, name, bake);
        let bake = &view.bake;
        if !view.entry.enabled || !bake.available || bake.ready >= bake.levels {
            return;
        }
        let action = self.view_lock().library.as_ref().and_then(|l| {
            l.games
                .iter()
                .flat_map(|g| &g.versions)
                .find(|v| v.version.key == key)
                .and_then(|v| v.actions.iter().find(|a| a.id == enhanced::BAKE_ACTION && a.this_platform).cloned())
        });
        let scope = Scope::Version(key.to_string());
        let busy = self.queue().live(&scope, enhanced::BAKE_ACTION, "", &[]).is_some();
        if let (Some(action), false) = (action, busy) {
            self.run_soon(scope, action, None);
        }
    }

    // ---- setting a game up ---------------------------------------------------------------------

    /// Sets `game` up from the image of the player's own disc: the player picks the image (or
    /// the games folder has it: `found`), the extractor copies its files out and checks them.
    /// Whether it worked.
    pub fn install_game(&self, game: &str, title: &str, found: Option<PathBuf>) -> bool {
        let scope = Scope::Repository;
        let id = format!("install:{game}");
        // A second click while the disc is picked, unpacked or set up does nothing.
        if self.already_asked(&scope, &id, "") {
            return false;
        }
        let _asking = self.asking(request_key(&scope, &id, ""));
        let picked = found.or_else(|| {
            self.host().pick_disc_image(&format!("Select your legitimately obtained disc image of {title}"))
        });
        let Some(picked) = picked else { return false };
        let image = match self.prepare_disc(&picked) {
            Ok(image) => image,
            Err(e) => {
                self.notify(Notice::error(e));
                return false;
            }
        };
        let job = self.enqueue(
            &scope,
            &id,
            &format!("Set up {title} from your disc"),
            "",
            Some(Starter::Install { game: game.to_string(), image }),
        );
        let ended = self.wait_for(job);
        self.pump();
        let ok = ended.recv().unwrap_or(false);
        if ok {
            self.notify(Notice::info(format!("{title} is set up from your disc.")));
        } else {
            let why = self.job(job).and_then(|j| extractor_error(&j.lines));
            let why = why.unwrap_or_else(|| "Activity has what it printed.".into());
            self.notify(Notice::error(format!("Setting {title} up stopped: {why}")));
        }
        ok
    }

    /// [`Launcher::install_game`] on a thread of its own.
    pub fn install_game_soon(&self, game: String, title: String, found: Option<PathBuf>) {
        self.spawn(move |l| {
            l.install_game(&game, &title, found);
            Ok(())
        });
    }

    /// The plain image setting a game up reads: an .iso as it is, a CHD unpacked once into
    /// `<install>/discs/<name>.iso`, with its progress told at each quarter.
    fn prepare_disc(&self, image: &Path) -> Result<PathBuf, String> {
        if !chd::is_chd(image) {
            return Ok(image.to_path_buf());
        }
        let install = self.config().install_dir.ok_or("no install folder (Settings)")?;
        let name = image.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dest = install.join("discs").join(chd::iso_name(&name));
        let file = std::fs::File::open(image).map_err(|e| format!("{}: {e}", image.display()))?;
        let told = Mutex::new(None);
        chd::to_iso(file, &dest, |copied, total| {
            let quarter = (copied * 4).checked_div(total).unwrap_or(0);
            if told.lock().unwrap().replace(quarter) != Some(quarter) {
                self.notify(Notice::info(if quarter >= 4 {
                    format!("{name} is ready.")
                } else {
                    format!("Preparing {name}: {}%", quarter * 25)
                }));
            }
        })
        .map_err(|e| format!("cannot unpack the CHD image: {e}"))?;
        Ok(dest)
    }

    /// The extractor's job for `game` from `image`. A developer's other disc set up before is kept
    /// aside, and the image remembered as its version's, for the disc switch.
    fn start_install(&self, game: &str, image: &Path) -> Result<Started, String> {
        let config = self.config();
        let install_dir = config.install_dir.clone().ok_or("no install folder (Settings)")?;
        let plan = install::plan_extract(&config, &install_dir, game, image)?;
        if config.developer {
            let catalog = openrac_launcher_core::catalog::load(config.root()?)?;
            disc_choice::before_set_up(&config, &catalog, game, image)?;
            let mut current = self.config();
            if disc_choice::remember(&mut current, &catalog, game, image) {
                self.store_config(&current)?;
            }
        }
        launcher_log::line(&format!("run: {}", jobs::command_line(&plan)));
        let title = plan.title.clone();
        let launcher = self.clone();
        let sink = jobs::batched(move |events| launcher.on_events(events, None, &title, &()));
        self.inner.jobs.start(&plan, config.root.as_deref(), sink)
    }

    // ---- Discord ---------------------------------------------------------------------------------

    /// Discord's line: the game being played, else a long job for a game, else the game whose
    /// page is open, else idle.
    pub fn update_presence(&self) {
        if !self.config().discord_rpc {
            return;
        }
        let jobs = self.jobs();
        let view = self.view_lock();
        let games = view.library.as_ref().map(|l| l.games.as_slice()).unwrap_or_default();
        let versions = || games.iter().flat_map(|g| &g.versions);
        let started = |job: &Job| job.started_at.map(|ms| ms / 1000);

        let playing = jobs.iter().find(|j| is_play(&j.action_id) && j.state == JobState::Running);
        if let Some((job, v)) =
            playing.and_then(|j| Some((j, versions().find(|v| Some(v.version.key.as_str()) == j.version())?)))
        {
            return self.inner.presence.set_status(DiscordStatus::PlayingGame {
                title: v.version.title.clone(),
                region: v.version.region.clone(),
                game_id: v.version.game.clone(),
                start_time: started(job),
            });
        }
        let working = jobs.iter().find(|j| j.state == JobState::Running && task_of(&j.action_id).is_some());
        if let Some(job) = working {
            let game = games.iter().find(|g| match job.version() {
                Some(key) => g.versions.iter().any(|v| v.version.key == key),
                None => job.action_id == format!("install:{}", g.id),
            });
            if let Some(game) = game {
                return self.inner.presence.set_status(DiscordStatus::Working {
                    task: task_of(&job.action_id).unwrap_or_default().to_string(),
                    title: game.title.clone(),
                    game_id: game.id.clone(),
                    start_time: started(job),
                });
            }
        }
        let viewing = self.viewing();
        if let Some(v) = viewing.and_then(|key| versions().find(|v| v.version.key == key)) {
            return self.inner.presence.set_status(DiscordStatus::ViewingGame {
                title: v.version.title.clone(),
                region: v.version.region.clone(),
                progress_pct: v.version.progress.as_ref().map(|p| p.percent),
                game_id: v.version.game.clone(),
            });
        }
        self.inner.presence.set_status(DiscordStatus::Idle);
    }
}

/// On Android the game runs as an activity: its state comes from there, not from a job.
impl Launcher {
    fn android_game_running(&self) -> bool {
        self.android_game().is_some()
    }

    /// Android: the game's activity started (with the library it plays, "" when unknown) or ended.
    pub fn set_android_game(&self, library: Option<String>) {
        *self.inner.android_game.lock().unwrap() = library;
        self.changed(Change::Jobs);
        self.update_presence();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_with_mods_is_the_same_request_as_without() {
        let scope = Scope::Version("rac1/pal".into());
        assert_eq!(request_key(&scope, "play-edited", "x"), request_key(&scope, "play", ""));
        assert_ne!(request_key(&scope, "build", "a"), request_key(&scope, "build", "b"));
        assert_eq!(request_key(&Scope::Repository, "install:rac1", ""), "repository\ninstall:rac1\n");
    }

    #[test]
    fn discord_names_the_long_jobs_only() {
        assert_eq!(task_of("install:rac2"), Some("Setting up from the disc"));
        assert_eq!(task_of("port-build"), Some("Building the game"));
        assert_eq!(task_of("play"), None);
    }
}
