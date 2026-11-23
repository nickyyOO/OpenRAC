//! What a running job tells a player: a plain name for the step it is on, and how far along it is
//! when its output says so. The raw output stays behind "Details".

use crate::queue::{now_ms, Job, JobState, Line};

/// How long after it began the game still counts as "starting", in milliseconds.
pub const STARTING_MS: u64 = 10_000;

/// The line a tool says how far along it is on (extractor/progress_line.py).
pub const PROGRESS_LINE: &str = "openrac-progress: ";

/// The ids of the actions that start the game itself.
pub const PLAY_ACTIONS: [&str; 2] = ["play", "play-edited"];

pub fn is_play(action_id: &str) -> bool {
    PLAY_ACTIONS.contains(&action_id)
}

/// The step a player sees, by the job's action.
pub fn phase_label(job: &Job) -> String {
    if job.state == JobState::Queued {
        return "Waiting for another task to finish".into();
    }
    let id = job.action_id.as_str();
    if id.starts_with("install:") {
        return "Setting up from your disc".into();
    }
    match id {
        "port-build" => "Building the game for this computer",
        "port-part" => "Building the game from your disc",
        "port-levels" => "Reading the levels from your disc",
        "port-levels-check" => "Checking the levels",
        "probe-bake" => "Preparing the enhanced lighting",
        "play" | "play-edited" => {
            let starting = job.started_at.is_none_or(|at| now_ms().saturating_sub(at) <= STARTING_MS);
            if starting {
                "Starting the game"
            } else {
                "The game is running"
            }
        }
        _ => "Working on it",
    }
    .into()
}

/// How far along a job is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    /// 0 to 100 when the output says how far along the job is; None for a bar without an end.
    pub percent: Option<f32>,
    /// A short count in plain words when there is one but no total ("5 levels read").
    pub note: Option<String>,
    /// How long is left, in words, when the job says ("about 12 minutes left").
    pub left: Option<String>,
}

/// Each step a tool counts, in a player's words.
pub fn step_words(step: &str) -> Option<&'static str> {
    Some(match step {
        "read" => "Reading levels",
        "project" => "Making the level's project",
        "carry" => "Carrying your changes over",
        "pack" => "Packing",
        "visibility" => "Adding what the game can see",
        "textures" => "Showing your packs' textures",
        "audio" => "Writing the sounds",
        _ => return None,
    })
}

/// A progress line read: its step, and done/total when the step is counted.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolProgress {
    pub step: String,
    pub words: &'static str,
    pub done: Option<u64>,
    pub total: Option<u64>,
    pub level: Option<u64>,
}

pub fn parse_progress(line: &str) -> Option<ToolProgress> {
    let rest = line.strip_prefix(PROGRESS_LINE)?.trim();
    let mut parts = rest.split(' ');
    let step = parts.next()?;
    if step.is_empty() || !step.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let words = step_words(step)?;
    let (done, total) = match parts.next() {
        None => (None, None),
        Some(count) => {
            let (d, t) = count.split_once('/')?;
            (Some(d.parse::<u64>().ok()?), Some(t.parse::<u64>().ok()?))
        }
    };
    let level = match parts.next() {
        None => None,
        Some(n) => Some(n.parse::<u64>().ok()?),
    };
    if parts.next().is_some() || matches!((done, total), (Some(d), Some(t)) if d > t) {
        return None;
    }
    Some(ToolProgress { step: step.into(), words, done, total, level })
}

fn clamp(n: f32) -> f32 {
    n.clamp(0.0, 100.0)
}

/// `[1351/2273] Building C object ...` (ninja).
fn ninja(text: &str) -> Option<f32> {
    let rest = text.trim_start().strip_prefix('[')?;
    let (inside, _) = rest.split_once(']')?;
    let (done, total) = inside.split_once('/')?;
    let (done, total) = (done.trim().parse::<f32>().ok()?, total.trim().parse::<f32>().ok()?);
    (total > 0.0).then(|| clamp(done / total * 100.0))
}

/// `[ 45%] Building ...`, `copying 62 %`: a number of 1 to 3 digits before a percent sign, at the
/// start or after a space, `[` or `(`.
fn percent(text: &str) -> Option<f32> {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices('%') {
        let mut end = i;
        if end > 0 && bytes[end - 1] == b' ' {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }
        let number = &text[start..end];
        let whole = number.split('.').next().unwrap_or_default();
        let before_ok = start == 0 || matches!(bytes[start - 1], b' ' | b'[' | b'(' | b'\t');
        if !number.is_empty() && (1..=3).contains(&whole.len()) && before_ok {
            if let Ok(n) = number.parse::<f32>() {
                return Some(clamp(n));
            }
        }
    }
    None
}

/// `(about 12 minutes left)` at the end of a line: the probe bake.
fn left(text: &str) -> Option<String> {
    let inner = text.trim_end().strip_suffix(')')?;
    let start = inner.rfind('(')?;
    let words = &inner[start + 1..];
    ((words.starts_with("about ") || words.starts_with("less than ")) && words.ends_with(" left"))
        .then(|| words.to_string())
}

/// How far along a job is, from what it printed. The newest counter wins.
pub fn read_progress(action_id: &str, lines: &[Line]) -> Progress {
    for line in lines.iter().rev().take(200) {
        let text = line.text.as_str();
        if text.starts_with(PROGRESS_LINE) {
            let Some(said) = parse_progress(text) else { continue };
            return Progress { percent: tool_percent(&said), ..Progress::default() };
        }
        if let Some(p) = ninja(text) {
            return Progress { percent: Some(p), ..Progress::default() };
        }
        if let Some(p) = percent(text) {
            return Progress { percent: Some(p), left: left(text), ..Progress::default() };
        }
    }
    if action_id == "port-levels" {
        let done = lines.iter().filter(|l| is_level_line(&l.text)).count();
        if done > 0 {
            let noun = if done == 1 { "level" } else { "levels" };
            return Progress { note: Some(format!("{done} {noun} read")), ..Progress::default() };
        }
    }
    Progress::default()
}

/// `level 12:` (extractor/extract.py, one line per level finished).
fn is_level_line(text: &str) -> bool {
    text.strip_prefix("level ")
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A count's share, 0 to 100; None when the step counts nothing.
pub fn tool_percent(p: &ToolProgress) -> Option<f32> {
    match (p.done, p.total) {
        (Some(done), Some(total)) if total > 0 => Some(clamp(done as f32 / total as f32 * 100.0)),
        _ => None,
    }
}

/// The last `count` lines of output, without progress lines: the Details box.
pub fn tail_lines(lines: &[Line], count: usize) -> Vec<String> {
    let kept: Vec<&Line> = lines.iter().filter(|l| !l.text.starts_with(PROGRESS_LINE)).collect();
    kept[kept.len().saturating_sub(count)..].iter().map(|l| l.text.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrac_launcher_core::jobs::Stream;

    fn lines(texts: &[&str]) -> Vec<Line> {
        texts.iter().map(|t| Line { stream: Stream::Stdout, text: t.to_string() }).collect()
    }

    #[test]
    fn reads_ninja_and_percent_lines() {
        assert_eq!(read_progress("x", &lines(&["[50/200] Building C object"])).percent, Some(25.0));
        assert_eq!(read_progress("x", &lines(&["[ 45%] Building"])).percent, Some(45.0));
        assert_eq!(read_progress("x", &lines(&["copying 62 %"])).percent, Some(62.0));
        let bake = read_progress("x", &lines(&["Lighting 42.5% (about 12 minutes left)"]));
        assert_eq!(bake.percent, Some(42.5));
        assert_eq!(bake.left.as_deref(), Some("about 12 minutes left"));
        assert_eq!(read_progress("x", &lines(&["version 1.2"])).percent, None);
    }

    #[test]
    fn counts_levels_read() {
        let p = read_progress("port-levels", &lines(&["level 1: ok", "level 2: ok"]));
        assert_eq!(p.note.as_deref(), Some("2 levels read"));
    }

    #[test]
    fn parses_tool_progress_lines() {
        let p = parse_progress("openrac-progress: read 3/10 7").unwrap();
        assert_eq!((p.done, p.total, p.level), (Some(3), Some(10), Some(7)));
        assert_eq!(p.words, "Reading levels");
        assert!(parse_progress("openrac-progress: read 11/10").is_none());
        assert!(parse_progress("openrac-progress: dance").is_none());
        assert!(parse_progress("openrac-progress: pack").is_some());
    }

    #[test]
    fn tail_skips_progress_lines() {
        let tail = tail_lines(&lines(&["a", "openrac-progress: pack", "b", "c"]), 2);
        assert_eq!(tail, ["b", "c"]);
    }
}
