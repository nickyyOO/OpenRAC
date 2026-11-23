//! Activity: the jobs the launcher ran for the player, newest first.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use openrac_launcher_service::{is_play, job_duration, job_line, job_transcript, read_progress, Job, JobState};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::shell::{self, Shell};
use crate::{Activity, App, AppWindow, JobRow, Page};

/// The most lines of output written out on the page: a long build prints thousands.
const SHOWN: usize = 400;

#[derive(Default)]
pub struct ActivityState {
    /// Which jobs' output the player unfolded or folded; the rest follow their state.
    opened: RefCell<HashMap<u64, bool>>,
    /// Ticks while a job runs and the page shows, so its time counts up.
    clock: slint::Timer,
}

pub fn install(window: &AppWindow) {
    let ui = window.global::<Activity>();
    ui.on_cancel(|key| shell::with(|s| s.launcher().map(|l| l.cancel(key as u64)).unwrap_or_default()));
    ui.on_clear(|| shell::with(|s| s.launcher().map(|l| l.clear_finished()).unwrap_or_default()));
    ui.on_copy(|key| {
        shell::with(|s| {
            if let Some(job) = s.launcher().and_then(|l| l.job(key as u64)) {
                s.window().global::<App>().invoke_copy_text(job_transcript(&job).into());
            }
        })
    });
    ui.on_set_open(|key, open| {
        shell::with(|s| {
            s.activity.opened.borrow_mut().insert(key as u64, open);
            s.show_activity();
        })
    });
    ui.on_open_games(|| shell::with(|s| s.open_page(Page::Games)));
}

fn state(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

impl ActivityState {
    fn row(&self, job: &Job, selected: Option<u64>) -> JobRow {
        let open = self
            .opened
            .borrow()
            .get(&job.key)
            .copied()
            .unwrap_or(Some(job.key) == selected || job.state == JobState::Failed);
        let progress = read_progress(&job.action_id, &job.lines);
        let (output, cut) = if open {
            let from = job.lines.len().saturating_sub(SHOWN);
            (job.lines[from..].iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"), from)
        } else {
            (String::new(), 0)
        };
        JobRow {
            key: job.key as i32,
            title: job.title.as_str().into(),
            line: job_line(job).into(),
            time: job_duration(job).into(),
            state: state(job.state).into(),
            bar: !is_play(&job.action_id),
            percent: progress.percent.unwrap_or(-1.0),
            command: job.command.as_str().into(),
            output: output.into(),
            cut: cut as i32,
            open,
        }
    }

    pub fn show(&self, shell: &Shell) {
        let Some(launcher) = shell.launcher() else { return };
        let window = shell.window();
        if window.global::<App>().get_page() != Page::Activity {
            self.clock.stop();
            return;
        }
        let jobs = launcher.jobs();
        let selected = launcher.selected_job();
        let rows: Vec<JobRow> = jobs.iter().rev().map(|j| self.row(j, selected)).collect();
        let ui = window.global::<Activity>();
        ui.set_finished(jobs.iter().any(|j| !matches!(j.state, JobState::Queued | JobState::Running)));
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
        let running = jobs.iter().any(|j| j.state == JobState::Running);
        if running && !self.clock.running() {
            self.clock.start(slint::TimerMode::Repeated, Duration::from_secs(1), || shell::with(|s| s.show_activity()));
        } else if !running {
            self.clock.stop();
        }
    }
}
