//! The launcher without its window: what happens at start, the settings, the job queue, the
//! game's next step, Discord. The window (app/) shows it and asks it to act; Android's activity
//! and a desktop window are both a [`Host`] to it.

mod failure;
mod host;
mod launcher;
mod mods;
mod paths;
mod player;
mod presence;
mod progress;
mod queue;
mod tasks;

pub use failure::{bake_failure, has_path, hide_paths, tool_failure, BakeFailure, NextStep, ToolError, ToolFailure};
pub use host::{Change, Host, Notice, NoticeKind};
pub use launcher::{bundled_copy, game_data, Crashes, Launcher, Start, Stopped, View};
pub use paths::Paths;
pub use player::{
    found_disc, has_port, is_built, is_playing, missing_program, play_warning, player_state, player_version,
    rebuild_steps, update_step, Icon, PlayerKind, PlayerState, Situation, Tone, UpdateStep,
};
pub use progress::{is_play, phase_label, read_progress, Progress};
pub use queue::{Job, JobState, Line};

/// The domain crate, for the window's own reads (the library's types, the settings).
pub use openrac_launcher_core as core;
