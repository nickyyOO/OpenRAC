//! What a player reads when a tool stops. The tools say why as a code on a line of their own
//! (`openrac-error: project-outdated-gameplay`, extractor/errors.py); each code has a sentence
//! here in a player's words and the one step that helps. What the tool printed stays in the job's
//! log and behind "Show details", its folders cut to their names.
//!
//! extractor/test_errors.py fails when a tool raises a code that [`tool_error`] does not know.

use crate::progress::PROGRESS_LINE;
use crate::queue::Line;

/// The line a tool says its code on (extractor/errors.py LINE).
pub const ERROR_LINE: &str = "openrac-error: ";

/// The line the lighting bake describes its failure on, as JSON.
pub const BAKE_ERROR_LINE: &str = "openrac-bake-error: ";

/// The marker lines the launcher reads, never shown even in the details.
const MARKERS: [&str; 4] = [ERROR_LINE, "openrac-output: ", "openrac-carry: ", PROGRESS_LINE];

/// The one thing that helps, offered as the error's button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextStep {
    /// Update project: back up, read again, carry the edits over.
    Update,
    /// The same step again, once the player fixed what is said.
    Retry,
    /// The Your levels section: export again, delete.
    StartOver,
    /// Set the disc up again.
    SetUp,
    /// Open a level in OpenRAC Editor (it reads the levels first).
    Open,
    /// The install folder, in Settings.
    Settings,
    /// Something OpenRAC should hear about: Activity has what to send.
    Report,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolError {
    pub title: &'static str,
    pub line: &'static str,
    pub next: NextStep,
}

const fn error(title: &'static str, line: &'static str, next: NextStep) -> ToolError {
    ToolError { title, line, next }
}

/// What a failure without a known code says.
pub const UNKNOWN_ERROR: ToolError = error(
    "That step did not finish",
    "Try again. If it keeps happening, Show details says what went wrong, and exporting the levels again under Your levels usually helps.",
    NextStep::Retry,
);

/// The sentence for a tool's code, or None for a code this launcher does not know.
pub fn tool_error(code: &str) -> Option<ToolError> {
    use NextStep::*;
    Some(match code {
        "project-outdated-gameplay" => error(
            "Your levels need an update",
            "They were made before the editor moved to the 60 Hz version of the game that OpenRAC plays, so they cannot be packed as they are. Updating keeps a backup and carries your changes over.",
            Update,
        ),
        "project-outdated" => error(
            "Your levels need an update",
            "They were made by an older version of the editor. Updating keeps a backup and carries your changes over.",
            Update,
        ),
        "static-import" => error(
            "An imported model could not be packed",
            "Check where the models you imported into this level sit and their collision, save in OpenRAC Editor, then pack again.",
            Retry,
        ),
        "level-too-big" => error(
            "This level has grown too big",
            "Its objects no longer fit in the space the game keeps for them. Remove some of the objects you added, save in OpenRAC Editor, then pack again.",
            Retry,
        ),
        "pack-internal" => error(
            "Packing went wrong",
            "The packed level did not read back as it should, so nothing was written. Please report it, with the details.",
            Report,
        ),
        "pack-incomplete" => error(
            "Some level edits could not be packed",
            "Some changes in your levels could not be used in the game. Show details lists them; correct or remove those edits, then pack again.",
            Retry,
        ),
        "pack-folder-not-ours" => error(
            "The packed output folder holds something else",
            "Choose another output folder, or move the existing files elsewhere before packing.",
            Retry,
        ),
        "folder-not-allowed" => error(
            "The editor cannot write there",
            "OpenRAC only writes inside its own folders. Check the install folder in Settings.",
            Settings,
        ),
        "folder-exists" => error(
            "A folder is in the way",
            "Delete the level project under Your levels, then try again.",
            StartOver,
        ),
        "folder-not-ours" => error(
            "A folder holds something else",
            "One of the editor's folders holds files OpenRAC did not make. Move them somewhere else, or delete the level project under Your levels.",
            StartOver,
        ),
        "no-project" => error(
            "There are no levels to pack yet",
            "Open a level in OpenRAC Editor first: that reads the levels from your disc.",
            Open,
        ),
        "planet-unnamed" => error(
            "Your planet has no name yet",
            "In OpenRAC Editor, open Project > Project Settings > OpenRAC > Planet, type its name (one line, at most 40 characters), save, then try again.",
            Retry,
        ),
        "planet-invalid" => error(
            "Your planet could not be added",
            "A new planet needs exactly one Ratchet start and the Ship under Game/LevelSettings. Show details says what is missing; fix it in OpenRAC Editor, save, then try again.",
            Retry,
        ),
        "game-not-packable" => error(
            "Packing is not ready for this game yet",
            "You can open its levels in OpenRAC Editor and make texture and model packs.",
            None,
        ),
        "no-such-level" => error("That level is not on this disc", "Pick another level from the list.", None),
        "disc-data" => error(
            "Your disc image could not be read",
            "Part of it is not what OpenRAC expects. Make sure it is a full, unmodified copy of the game, then set it up again.",
            SetUp,
        ),
        "disk-full" => error("Your disk is full", "Free some space, then try again.", Retry),
        "file-locked" => error(
            "A file is in use",
            "Close OpenRAC Editor and any window showing the level files, then try again.",
            Retry,
        ),
        "file-missing" => error(
            "A file is missing",
            "Some of the files this step needs are gone. Export the levels again under Your levels.",
            StartOver,
        ),
        "file-error" => error(
            "A file could not be read or written",
            "Try again. If it keeps happening, export the levels again under Your levels.",
            Retry,
        ),
        "carry-failed" => error(
            "Your changes could not be carried over",
            "The new levels are as on your disc. Your old levels are kept in the backup, to redo your changes from.",
            None,
        ),
        "pack-invalid" => error("That pack could not be added", "It is not a pack OpenRAC reads, or it is damaged.", None),
        "asset-invalid" => error(
            "Your edited files could not be read",
            "Check the models and textures you changed, then import them again.",
            Retry,
        ),
        "internal" => error(
            "Something went wrong",
            "OpenRAC hit a problem it did not expect. Try again; if it happens again, please report it with the details.",
            Report,
        ),
        _ => return Option::None,
    })
}

/// A failed job in a player's words, with what the tool printed for "Show details".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolFailure {
    pub error: ToolError,
    /// The tool's code, or None when it said none.
    pub code: Option<String>,
    /// The last lines it printed, folders cut to their names: for "Show details" only.
    pub details: String,
}

/// The code a tool said, the last one if more.
pub fn error_code(lines: &[Line]) -> Option<String> {
    let said = lines.iter().rev().find(|l| l.text.starts_with(ERROR_LINE))?;
    let code = said.text[ERROR_LINE.len()..].trim();
    (!code.is_empty()).then(|| code.to_string())
}

/// A failed job of a tool, said for a player: never the tool's own words in the sentence.
pub fn tool_failure(lines: &[Line]) -> ToolFailure {
    let code = error_code(lines);
    let error = code.as_deref().and_then(tool_error).unwrap_or(UNKNOWN_ERROR);
    let shown: Vec<&str> = lines
        .iter()
        .map(|l| l.text.as_str())
        .filter(|l| !MARKERS.iter().any(|m| l.starts_with(m)) && !l.trim().is_empty())
        .collect();
    let shown = &shown[shown.len().saturating_sub(15)..];
    ToolFailure { error, code, details: hide_paths(&shown.join("\n")).0 }
}

/// The extractor's reason for refusing a disc: its last "error NNNN: ..." line
/// (tools/extractor.py), starting with a capital.
pub fn extractor_error(lines: &[Line]) -> Option<String> {
    let line = lines.iter().rev().map(|l| l.text.as_str()).find(|l| extractor_reason(l).is_some())?;
    let reason = extractor_reason(line)?;
    let mut chars = reason.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// What follows "error NNNN: " on an extractor's line.
fn extractor_reason(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("error ")?;
    let (digits, reason) = rest.split_at_checked(4)?;
    (digits.bytes().all(|b| b.is_ascii_digit())).then_some(())?;
    reason.strip_prefix(": ")
}

/// Why the lighting bake stopped, from its `openrac-bake-error:` record, or a plain notice when
/// it wrote none.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeFailure {
    pub code: String,
    pub title: String,
    pub explanation: String,
    pub suggestion: String,
    pub detail: String,
    pub stage: Option<String>,
    pub level: Option<String>,
    pub log: Option<String>,
    pub failed_levels: Option<u64>,
}

/// The bake job's last record that reads as one. Text and paths in it are only ever shown.
pub fn bake_failure(lines: &[Line], code: Option<i32>) -> BakeFailure {
    for line in lines.iter().rev() {
        let Some(json) = line.text.strip_prefix(BAKE_ERROR_LINE) else { continue };
        if let Some(failure) = read_bake_record(json) {
            return failure;
        }
    }
    let tail: Vec<&str> = lines[lines.len().saturating_sub(20)..].iter().map(|l| l.text.as_str()).collect();
    BakeFailure {
        code: "unreported".into(),
        title: "Lighting preparation could not finish".into(),
        explanation: "The lighting tool stopped without a detailed error report.".into(),
        suggestion:
            "Open Activity for its output. Check you have the latest launcher, then retry or send us the output.".into(),
        detail: format!(
            "Exit status: {}\n{}",
            code.map_or("unavailable".to_string(), |c| c.to_string()),
            tail.join("\n")
        ),
        stage: None,
        level: None,
        log: None,
        failed_levels: None,
    }
}

fn read_bake_record(json: &str) -> Option<BakeFailure> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let record = value.as_object()?;
    if record.get("version")?.as_u64()? != 1 {
        return None;
    }
    let text = |key: &str| record.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let head = |s: String, n: usize| s.chars().take(n).collect::<String>();
    let detail = text("detail")?;
    let detail_len = detail.chars().count();
    Some(BakeFailure {
        code: head(text("code")?, 80),
        title: head(text("title")?, 240),
        explanation: head(text("explanation")?, 1000),
        suggestion: head(text("suggestion")?, 1000),
        detail: detail.chars().skip(detail_len.saturating_sub(12_000)).collect(),
        stage: text("stage"),
        level: text("level"),
        log: text("log"),
        failed_levels: record.get("failed_levels").and_then(|v| v.as_u64()),
    })
}

// ---- folders out of sight ------------------------------------------------------------------------

/// Players share screenshots, so a message never shows a folder in plain view: every absolute
/// path in `text` (/a/b, ~/a, C:\a, \\server\share) is cut to its last part. Says whether there
/// was one. Links (//host), repositories (a/b) and fractions (1/2) stay.
pub fn hide_paths(text: &str) -> (String, bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut hidden = false;
    let mut i = 0;
    while i < chars.len() {
        match path_at(&chars, i) {
            Some(end) => {
                hidden = true;
                let path: String = chars[i..end].iter().collect();
                let kept = path.trim_end_matches('.');
                out.push_str(&last_part(kept));
                out.push_str(&path[kept.len()..]);
                i = end;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    (out, hidden)
}

/// Whether `text` names a folder or file by its full path.
pub fn has_path(text: &str) -> bool {
    hide_paths(text).1
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_part(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '/' | '\\' | ':' | ';' | ',' | '\'' | '"' | '(' | ')' | '<' | '>' | '`')
}

/// Where the path starting at `i` ends, if one starts there.
fn path_at(chars: &[char], i: usize) -> Option<usize> {
    let before = i.checked_sub(1).map(|p| chars[p]);
    if before.is_some_and(|c| is_word(c) || matches!(c, ':' | '/' | '.' | '~' | '\\')) {
        return None;
    }
    let at = |k: usize| chars.get(i + k).copied();
    let sep = |c: Option<char>| matches!(c, Some('/' | '\\'));
    let start = if at(0) == Some('~') && at(1) == Some('/') {
        2
    } else if at(0) == Some('/') {
        // "/a/…": a first part, and a slash after it.
        let first = chars[i + 1..].iter().take_while(|c| !c.is_whitespace() && **c != '/').count();
        if first == 0 || chars.get(i + 1 + first) != Some(&'/') {
            return None;
        }
        1
    } else if at(0).is_some_and(|c| c.is_ascii_alphabetic()) && at(1) == Some(':') && sep(at(2)) {
        3
    } else if at(0) == Some('\\') && at(1) == Some('\\') {
        2
    } else {
        return None;
    };
    let mut end = i + start;
    loop {
        let part = chars[end..].iter().take_while(|c| is_part(**c)).count();
        end += part;
        if part > 0 && chars.get(end).is_some_and(|c| matches!(c, '/' | '\\')) {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

/// A path's last part ("plugins.json"), or "a folder" for a root.
fn last_part(path: &str) -> String {
    path.split(['/', '\\']).rfind(|p| !p.is_empty() && *p != "~" && !is_drive(p)).unwrap_or("a folder").to_string()
}

fn is_drive(part: &str) -> bool {
    let b = part.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrac_launcher_core::jobs::Stream;

    fn lines(texts: &[&str]) -> Vec<Line> {
        texts.iter().map(|t| Line { stream: Stream::Stderr, text: t.to_string() }).collect()
    }

    #[test]
    fn a_folder_is_cut_to_its_last_part() {
        let cases = [
            ("cannot read /home/alice/.local/share/dev.openrac.launcher/plugins/difficulty", "cannot read difficulty"),
            ("/Users/bob/OpenRAC/active/rac1/data/plugins.json: not valid JSON.", "plugins.json: not valid JSON."),
            ("C:\\Users\\carol\\AppData\\Roaming\\OpenRAC\\mods\\x.zip is not a mod", "x.zip is not a mod"),
            ("in ~/Downloads/lock-strafe, no manifest", "in lock-strafe, no manifest"),
        ];
        for (text, want) in cases {
            assert_eq!(hide_paths(text), (want.to_string(), true), "{text}");
        }
    }

    #[test]
    fn links_repositories_and_fractions_stay() {
        for text in [
            "Installed Lock-strafe from OpenRAC/openrac-mods.",
            "This repository is private: https://github.com/OpenRAC/openrac-mods/tree/master/x",
            "Hold L2 or R2, 1/2 of the time",
            "not for this disc (rac1/pal)",
            "Needs OpenRAC with mod interface 2.0; this one has 1.0.",
        ] {
            assert_eq!(hide_paths(text), (text.to_string(), false), "{text}");
        }
    }

    #[test]
    fn a_known_code_says_its_sentence_and_hides_the_markers() {
        let failure = tool_failure(&lines(&["reading /home/a/levels/x.bin", "openrac-error: disk-full", ""]));
        assert_eq!(failure.code.as_deref(), Some("disk-full"));
        assert_eq!(failure.error.next, NextStep::Retry);
        assert_eq!(failure.details, "reading x.bin");
        assert_eq!(tool_failure(&lines(&["openrac-error: what"])).error, UNKNOWN_ERROR);
    }

    #[test]
    fn the_extractor_says_why_it_refused_a_disc() {
        let said = extractor_error(&lines(&["x", "error 1203: this is not a disc OpenRAC knows", "y"]));
        assert_eq!(said.as_deref(), Some("This is not a disc OpenRAC knows"));
        assert_eq!(extractor_error(&lines(&["error 12: no"])), None);
    }

    #[test]
    fn a_bake_record_is_read_and_a_missing_one_said_plainly() {
        let record = r#"openrac-bake-error: {"version":1,"code":"gpu","title":"T","explanation":"E","suggestion":"S","detail":"D","failed_levels":2}"#;
        let failure = bake_failure(&lines(&["a", record, "b"]), Some(1));
        assert_eq!((failure.code.as_str(), failure.failed_levels), ("gpu", Some(2)));
        let plain = bake_failure(&lines(&["openrac-bake-error: {broken", "last"]), None);
        assert_eq!(plain.code, "unreported");
        assert!(plain.detail.starts_with("Exit status: unavailable\n"));
    }
}
