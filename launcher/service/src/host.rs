//! What the service needs from the program around it: telling the window that something changed,
//! and the few questions only a person can answer (a dialog, a file to pick).

use std::path::{Path, PathBuf};

use openrac_launcher_core::actions::Plan;
use openrac_launcher_core::jobs::Started;

/// What changed, so the window reads it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Change {
    /// The settings.
    Config,
    /// The games, their actions, the discs found, the crash counts.
    Library,
    /// The job list or a job's output.
    Jobs,
    /// The notice of a game that stopped unexpectedly.
    Stopped,
    /// The game's saves or logs may be new (the game ended).
    GameData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

/// A message for the player: a toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    /// A failed task's last output lines, folders cut to their names.
    pub details: Option<String>,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Notice {
        Notice { kind: NoticeKind::Info, text: text.into(), details: None }
    }

    pub fn error(text: impl Into<String>) -> Notice {
        Notice { kind: NoticeKind::Error, text: text.into(), details: None }
    }
}

/// The program around the service: the window on a desktop, the activity on Android. Every method
/// may be called from any thread. The ones that wait for the player block the calling thread,
/// which is never the window's.
pub trait Host: Send + Sync + 'static {
    /// Something changed: the window reads it again.
    fn changed(&self, change: Change);

    /// A message for the player.
    fn notify(&self, notice: Notice);

    /// Asks the player to go on, in a dialog: true for `yes`.
    fn confirm(&self, text: &str, yes: &str) -> bool;

    /// The player's disc image (.iso or .chd), or None when they picked nothing.
    fn pick_disc_image(&self, title: &str) -> Option<PathBuf>;

    /// A folder, or None.
    fn pick_folder(&self, title: &str) -> Option<PathBuf>;

    /// A file of one of `extensions` (named `kind` in the dialog), the dialog opening in `start`
    /// when given; None when the player picked nothing.
    fn pick_file(&self, title: &str, kind: &str, extensions: &[&str], start: Option<&Path>) -> Option<PathBuf>;

    /// Where to save a file named `name`, or None.
    fn save_file(&self, title: &str, name: &str) -> Option<PathBuf>;

    /// Shows a folder or opens a file with the system's program for it (Android: its Files app).
    fn open_path(&self, path: &Path) -> Result<(), String>;

    /// Opens a web page in the system's browser.
    fn open_url(&self, url: &str) -> Result<(), String>;

    /// Puts text on the clipboard.
    fn copy(&self, text: &str);

    /// What fetches mods and the mod hub from the web: curl, through the launcher's cache of
    /// GitHub's answers (Android fetches through its own side).
    fn fetch(&self) -> Box<dyn openrac_launcher_core::remote::Fetch> {
        Box::new(openrac_launcher_core::remote::Cached::shared(openrac_launcher_core::remote::Curl))
    }

    /// The folder a file made for the player (a report's .zip) is made in.
    fn hand_over_folder(&self) -> Option<PathBuf> {
        openrac_launcher_core::reports::save_folder()
    }

    /// A file made for the player at `made`: a desktop keeps it there and shows its folder (Android
    /// asks where to save it). Returns where it went in the player's words, or None when they
    /// backed out.
    fn hand_over(&self, made: &Path) -> Result<Option<String>, String> {
        if let Some(folder) = made.parent() {
            self.open_path(folder)?;
        }
        let home = openrac_launcher_core::reports::home();
        Ok(Some(openrac_launcher_core::reports::tidy(&made.to_string_lossy(), home.as_deref())))
    }

    /// Starts the game where the system does it, not as a child process (Android's game
    /// activity). None: start it as a job, as a desktop does.
    fn launch_game(&self, _plan: &Plan) -> Option<Result<Started, String>> {
        None
    }

    /// Android: the game is starting, its disc spins meanwhile.
    fn launching(&self, _game: &str, _title: &str) {}

    /// Android: back to the game's activity, which waits where the player left it.
    fn show_game(&self) {}
}
