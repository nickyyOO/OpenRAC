//! What the Mods tab says about a mod, in a player's words: its state, what it runs, the one badge
//! its row shows, and its group. Tones are "ok", "off", "warn" and "err".

use openrac_launcher_core::actions::State;
use openrac_launcher_core::library::VersionView;
use openrac_launcher_core::plugins::PluginView;
use openrac_launcher_core::status::DiscState;

use crate::player::action;
use crate::queue::Job;

/// A few words and their tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    pub text: &'static str,
    pub tone: &'static str,
}

const fn label(text: &'static str, tone: &'static str) -> Label {
    Label { text, tone }
}

/// The groups My mods holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModGroup {
    Gameplay,
    /// Graphics and sound: the built-in entries and the packs.
    Looks,
    /// Mods that are only a page in the launcher.
    Tools,
}

pub fn group_of(kind: &str) -> ModGroup {
    match kind {
        "tool" => ModGroup::Tools,
        "built-in" | "always-on" => ModGroup::Looks,
        _ => ModGroup::Gameplay,
    }
}

/// A mod's state. With the Mods switch under Play off (`purist`), the game loads none.
pub fn state(m: &PluginView, purist: bool) -> Label {
    match (m.enabled, purist, m.state.as_str()) {
        (false, _, _) => label("Off", "off"),
        (true, true, _) => label("Mods off at Play", "off"),
        (true, false, "ready") => label("On", "ok"),
        (true, false, "failed") => label("Not loaded", "err"),
        (true, false, "refused") => label("Cannot load", "err"),
        _ => label("Off", "off"),
    }
}

/// What a mod runs.
pub fn kind(kind: &str) -> Label {
    match kind {
        "native" => label("Runs code", "warn"),
        "script" => label("Sandboxed script", "ok"),
        "tool" => label("Launcher page", "ok"),
        "built-in" => label("Experimental", "warn"),
        "always-on" => label("Part of the game", "ok"),
        _ => label("Textures and models", "off"),
    }
}

/// What a mod's kind means, in a sentence.
pub fn kind_hint(kind: &str) -> &'static str {
    match kind {
        "native" => "A program: it runs on this computer with the game's rights. Install it only from people you trust.",
        "script" => "Runs in the game's sandbox: it reaches only what the game lets scripts change, never your files.",
        "tool" => "A page of its own in the launcher: it reads the game's levels and saved textures, and saves only where you choose.",
        "built-in" => "Part of the game, off unless you turn it on. Still being worked on.",
        "always-on" => "Part of the game and always on: it is used the moment you play with it.",
        _ => "Data only: it replaces textures, models or levels, and runs nothing.",
    }
}

/// The one badge a mod's row shows, most pressing first: it cannot load, an update, what it runs.
/// Its text, tone and hint.
pub fn badge(m: &PluginView, update: Option<&str>, purist: bool) -> (String, &'static str, String) {
    if m.enabled && !purist && (m.state == "refused" || m.state == "failed") {
        let s = state(m, purist);
        return (s.text.into(), s.tone, m.reason.clone());
    }
    if let Some(version) = update {
        return (
            format!("Update {version}"),
            "warn",
            "A newer version is out: update it with its Update button.".into(),
        );
    }
    let k = kind(&m.kind);
    (k.text.into(), k.tone, kind_hint(&m.kind).into())
}

/// What a game's mods can be, for the Mods tab: its levels open in the editor and make packs
/// (`levels`), or it has gameplay mods only, or none yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moddable {
    /// Levels can be read: packs, and the level editor where it supports the game.
    Levels {
        early: bool,
    },
    GameplayOnly,
    NotYet,
}

fn ready(v: &VersionView, id: &str) -> bool {
    action(v, id).is_some_and(|a| a.state != State::Planned)
}

pub fn moddable(v: &VersionView) -> Moddable {
    if ready(v, "assets-export") {
        // Early access: the levels are read, but edits are not packed back into the game.
        let early = action(v, "play-edited").is_none()
            || (action(v, "editor-extract").is_some() && action(v, "editor-pack").is_none());
        Moddable::Levels { early }
    } else if ready(v, "play") {
        Moddable::GameplayOnly
    } else {
        Moddable::NotYet
    }
}

/// The steps that make something from the game's files: the game is "Working…" while one runs.
const MAKING: [&str; 8] = [
    "editor-extract",
    "editor-import",
    "editor-pack",
    "assets-export",
    "assets-import",
    "audio-export",
    "audio-export-all",
    "audio-import",
];

/// What the Mods tab's game list says of a game.
pub fn game_state(v: &VersionView, jobs: &[Job]) -> Label {
    let working = jobs.iter().any(|j| {
        j.state.is_live() && j.version() == Some(v.version.key.as_str()) && MAKING.contains(&j.action_id.as_str())
    });
    let found = v.status.disc.state == DiscState::Found;
    match moddable(v) {
        Moddable::NotYet => label("Not moddable yet", "off"),
        Moddable::GameplayOnly => label("Gameplay mods", "ok"),
        Moddable::Levels { early: true } if !ready(v, "play") && found && !working => label("Early access", "off"),
        Moddable::Levels { .. } if working => label("Working…", "warn"),
        Moddable::Levels { .. } if !found => label("Set it up first", "warn"),
        Moddable::Levels { .. } => label("Ready to mod", "ok"),
    }
}

/// Whether a row's words hold every word of the search, in any case and order.
pub fn matches(query: &str, words: &[&str]) -> bool {
    let text = words.join(" ").to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| text.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: &str, enabled: bool, state: &str) -> PluginView {
        PluginView { kind: kind.into(), enabled, state: state.into(), reason: "why".into(), ..Default::default() }
    }

    #[test]
    fn the_badge_says_the_most_pressing_thing() {
        assert_eq!(badge(&view("native", true, "refused"), Some("1.1"), false).0, "Cannot load");
        assert_eq!(badge(&view("native", true, "refused"), Some("1.1"), true).0, "Update 1.1");
        assert_eq!(badge(&view("script", false, "off"), None, false).0, "Sandboxed script");
        assert_eq!(state(&view("script", true, "ready"), true).text, "Mods off at Play");
    }

    #[test]
    fn a_search_finds_every_word() {
        assert!(matches("lock STRAFE", &["Lock-strafe", "Strafe as in the later games"]));
        assert!(!matches("lock camera", &["Lock-strafe"]));
        assert_eq!(group_of("tool"), ModGroup::Tools);
        assert_eq!(group_of("always-on"), ModGroup::Looks);
    }
}
