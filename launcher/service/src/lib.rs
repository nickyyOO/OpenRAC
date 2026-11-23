//! The launcher without its window: what happens at start, the settings, the job queue, the
//! game's next step, Discord. The window (app/) shows it and asks it to act; Android's activity
//! and a desktop window are both a [`Host`] to it.

mod differences;
mod failure;
mod game;
mod host;
pub mod labels;
mod launcher;
mod mod_words;
mod mods;
mod paths;
mod player;
mod presence;
mod progress;
mod queue;
mod reports;
mod saves;
mod settings;
mod stopped;
mod tasks;

pub use failure::{bake_failure, has_path, hide_paths, tool_failure, BakeFailure, NextStep, ToolError, ToolFailure};
pub use host::{Change, Host, Notice, NoticeKind};
pub use launcher::{bundled_copy, game_data, Crashes, Launcher, Start, Stopped, View};
pub use mod_words::{
    badge as mod_badge, game_state as mod_game_state, group_of, kind as mod_kind, kind_hint, matches as mod_matches,
    moddable, state as mod_state, Label as ModLabel, ModGroup, Moddable,
};
pub use mods::{Added, TurnedOn};
pub use paths::Paths;
pub use player::{
    action, failed_step, found_disc, has_port, is_built, is_playing, missing_program, play_warning, player_state,
    player_version, rebuild_steps, update_step, Icon, PlayerKind, PlayerState, Situation, Tone, UpdateStep,
};
pub use progress::{
    elapsed, is_play, job_duration, job_line, job_transcript, phase_label, read_progress, summarize, tail_lines,
    JobSummary, Pace, Progress,
};
pub use queue::{Job, JobState, Line};
pub use saves::{backup_label, empty_slots, Card};
pub use settings::{is_official, name_problem, server_problem, PathKind};
pub use stopped::{crash_after_update, crash_mods_line, stop_text, StoppedMods};

/// The domain crate, for the window's own reads (the library's types, the settings).
pub use openrac_launcher_core as core;
