//! What the service needs from the program around it: telling the window that something changed,
//! and the few questions only a person can answer (a dialog, a file to pick).

use std::path::PathBuf;

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
