//! The job queue: jobs run one at a time, in the order they were asked for, so two builds never
//! share the build container or a game's folder. Events from the running program are matched to
//! jobs by id, including events that arrive before the start call has returned.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use openrac_launcher_core::actions::Scope;
use openrac_launcher_core::jobs::{Event, Stream};

/// Older lines are dropped past this many, so a chatty build cannot exhaust memory.
pub const MAX_LINES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobState {
    /// Queued or running: not finished.
    pub fn is_live(self) -> bool {
        matches!(self, JobState::Queued | JobState::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub stream: Stream,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Job {
    /// The launcher's own number for the job.
    pub key: u64,
    pub scope: Scope,
    pub action_id: String,
    pub title: String,
    pub state: JobState,
    /// The running program's id, once started.
    pub id: Option<u32>,
    pub command: String,
    pub lines: Vec<Line>,
    pub code: Option<i32>,
    pub queued_at: u64,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    /// The folders it wrote, from its exit event.
    pub outputs: Vec<String>,
    /// What it was asked with (a level, a file), as one string: the same action with other inputs
    /// is another job.
    pub inputs: String,
}

impl Job {
    /// The version the job is for (`rac1/pal`), if any.
    pub fn version(&self) -> Option<&str> {
        match &self.scope {
            Scope::Version(key) => Some(key),
            Scope::Repository => None,
        }
    }
}

/// Now, in milliseconds since 1970.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Actions that are one thing to the player: a second of any of them while one is queued or
/// running is the same request again (the game, and the game with mods).
pub type Groups<'a> = &'a [&'a [&'a str]];

#[derive(Default)]
pub struct Queue {
    jobs: Vec<Job>,
    next_key: u64,
    /// Events for ids no job has yet (the start call has not returned).
    early: HashMap<u32, Vec<Event>>,
    /// The jobs that ended, waiting to be acted on (take_ended).
    ended: Vec<u64>,
}

impl Queue {
    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn get(&self, key: u64) -> Option<&Job> {
        self.jobs.iter().find(|j| j.key == key)
    }

    fn get_mut(&mut self, key: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.key == key)
    }

    pub fn add(&mut self, scope: Scope, action_id: &str, title: &str, inputs: &str) -> u64 {
        self.next_key += 1;
        self.jobs.push(Job {
            key: self.next_key,
            scope,
            action_id: action_id.to_string(),
            title: title.to_string(),
            state: JobState::Queued,
            id: None,
            command: String::new(),
            lines: Vec::new(),
            code: None,
            queued_at: now_ms(),
            started_at: None,
            ended_at: None,
            outputs: Vec::new(),
            inputs: inputs.to_string(),
        });
        self.next_key
    }

    /// The queued or running job that asking for `action_id` in `scope` with `inputs` would
    /// repeat: the same action with the same inputs, or any action of its group.
    pub fn live(&self, scope: &Scope, action_id: &str, inputs: &str, groups: Groups) -> Option<&Job> {
        let group = groups.iter().find(|g| g.contains(&action_id));
        self.jobs.iter().find(|j| {
            j.state.is_live()
                && &j.scope == scope
                && match group {
                    Some(group) => group.contains(&j.action_id.as_str()),
                    None => j.action_id == action_id && j.inputs == inputs,
                }
        })
    }

    pub fn running(&self) -> Option<&Job> {
        self.jobs.iter().find(|j| j.state == JobState::Running)
    }

    /// The job to start now: the oldest queued one, if nothing runs. It is marked running.
    pub fn next(&mut self) -> Option<Job> {
        if self.running().is_some() {
            return None;
        }
        let job = self.jobs.iter_mut().find(|j| j.state == JobState::Queued)?;
        job.state = JobState::Running;
        Some(job.clone())
    }

    /// Jobs not finished: running and queued.
    pub fn pending(&self) -> usize {
        self.jobs.iter().filter(|j| j.state.is_live()).count()
    }

    pub fn started(&mut self, key: u64, id: u32, title: &str, command: &str) {
        let Some(job) = self.get_mut(key) else { return };
        job.state = JobState::Running;
        job.id = Some(id);
        job.title = title.to_string();
        job.command = command.to_string();
        job.started_at = Some(now_ms());
        for event in self.early.remove(&id).unwrap_or_default() {
            self.apply(event);
        }
    }

    /// A detached start (the editor, Android's game): there is no process to follow, so the job
    /// is done once it started.
    pub fn launched(&mut self, key: u64, title: &str, command: &str) {
        let Some(job) = self.get_mut(key) else { return };
        let now = now_ms();
        job.state = JobState::Succeeded;
        job.title = title.to_string();
        job.command = command.to_string();
        job.started_at = Some(now);
        job.ended_at = Some(now);
    }

    /// The start call failed: the job never ran.
    pub fn refused(&mut self, key: u64, error: &str) {
        let Some(job) = self.get_mut(key) else { return };
        job.state = JobState::Failed;
        job.lines.push(Line { stream: Stream::Stderr, text: error.to_string() });
        job.ended_at = Some(now_ms());
    }

    /// A queued job is dropped; a running one is the program runner's to stop.
    pub fn dequeue(&mut self, key: u64) -> bool {
        let Some(job) = self.get_mut(key).filter(|j| j.state == JobState::Queued) else { return false };
        job.state = JobState::Cancelled;
        job.ended_at = Some(now_ms());
        true
    }

    /// Applies one event from a running program.
    pub fn apply(&mut self, event: Event) {
        let id = match &event {
            Event::Output { id, .. } | Event::Exit { id, .. } => *id,
        };
        let Some(job) = self.jobs.iter_mut().find(|j| j.id == Some(id)) else {
            self.early.entry(id).or_default().push(event);
            return;
        };
        match event {
            Event::Output { stream, line, .. } => {
                job.lines.push(Line { stream, text: line });
                if job.lines.len() > MAX_LINES {
                    let extra = job.lines.len() - MAX_LINES;
                    job.lines.drain(..extra);
                }
            }
            Event::Exit { code, cancelled, outputs, .. } => {
                job.code = code;
                job.outputs = outputs;
                job.ended_at = Some(now_ms());
                job.state = match (cancelled, code) {
                    (true, _) => JobState::Cancelled,
                    (false, Some(0)) => JobState::Succeeded,
                    _ => JobState::Failed,
                };
                self.ended.push(job.key);
            }
        }
    }

    /// Forgets finished jobs.
    pub fn clear(&mut self) {
        self.jobs.retain(|j| j.state.is_live());
        let jobs = &self.jobs;
        self.ended.retain(|key| jobs.iter().any(|j| j.key == *key));
    }

    /// The job that ended, if one has and nobody has taken it yet. A job's end is reported here
    /// and only here, so it is acted on once, whenever its exit event came.
    pub fn take_ended(&mut self) -> Option<Job> {
        if self.ended.is_empty() {
            return None;
        }
        let key = self.ended.remove(0);
        self.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exit(id: u32, code: i32) -> Event {
        Event::Exit { id, code: Some(code), cancelled: false, outputs: vec![] }
    }

    fn output(id: u32, line: &str) -> Event {
        Event::Output { id, stream: Stream::Stdout, line: line.into() }
    }

    #[test]
    fn runs_one_job_at_a_time_in_order() {
        let mut q = Queue::default();
        let a = q.add(Scope::Repository, "a", "A", "");
        let b = q.add(Scope::Repository, "b", "B", "");
        assert_eq!(q.next().map(|j| j.key), Some(a));
        assert!(q.next().is_none(), "nothing starts while A runs");
        q.started(a, 1, "A", "a");
        q.apply(exit(1, 0));
        assert_eq!(q.take_ended().map(|j| j.state), Some(JobState::Succeeded));
        assert_eq!(q.next().map(|j| j.key), Some(b));
    }

    #[test]
    fn keeps_events_that_come_before_the_start_returns() {
        let mut q = Queue::default();
        let a = q.add(Scope::Repository, "a", "A", "");
        q.next();
        q.apply(output(7, "early"));
        q.apply(exit(7, 2));
        assert!(q.take_ended().is_none());
        q.started(a, 7, "A", "a");
        let ended = q.take_ended().unwrap();
        assert_eq!(ended.state, JobState::Failed);
        assert_eq!(ended.lines[0].text, "early");
    }

    #[test]
    fn a_group_counts_as_one_request() {
        let mut q = Queue::default();
        let scope = Scope::Version("rac1/pal".into());
        q.add(scope.clone(), "play", "Play", "");
        let groups: Groups = &[&["play", "play-edited"]];
        assert!(q.live(&scope, "play-edited", "", groups).is_some());
        assert!(q.live(&scope, "port-build", "", groups).is_none());
        assert!(q.live(&Scope::Version("rac2/ntsc".into()), "play", "", groups).is_none());
    }

    #[test]
    fn drops_the_oldest_lines() {
        let mut q = Queue::default();
        let a = q.add(Scope::Repository, "a", "A", "");
        q.next();
        q.started(a, 1, "A", "a");
        for i in 0..MAX_LINES + 10 {
            q.apply(output(1, &i.to_string()));
        }
        let job = q.get(a).unwrap();
        assert_eq!(job.lines.len(), MAX_LINES);
        assert_eq!(job.lines[0].text, "10");
    }

    #[test]
    fn clear_keeps_live_jobs() {
        let mut q = Queue::default();
        let a = q.add(Scope::Repository, "a", "A", "");
        q.add(Scope::Repository, "b", "B", "");
        q.next();
        q.refused(a, "no");
        q.clear();
        assert_eq!(q.jobs().len(), 1);
    }
}
