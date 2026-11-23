//! What the Settings page asks of the launcher: what was found on this machine, whether a path
//! works, and what is wrong with a multiplayer name or server, in a player's words.

use std::path::Path;
use std::time::Duration;

use openrac_launcher_core::detect::{self, Check, Detected, Tool};
use openrac_launcher_core::{multiplayer, plugins};

use crate::launcher::{start_folders, Launcher};

/// What a path setting holds, which says how it is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// The OpenRAC folder.
    Root,
    Python,
    Docker,
    /// The folder of disc images.
    Discs,
    /// Where the games are set up.
    Install,
    /// Where the memory cards are kept.
    Saves,
}

impl Launcher {
    /// The OpenRAC folders, Pythons and Dockers on this machine. Blocks.
    pub fn detect(&self) -> Detected {
        detect::detect_all(&start_folders(), self.bundled())
    }

    /// Whether `path` works as `kind`, in words. Blocks (a program is run for its version).
    pub fn check_path(&self, kind: PathKind, path: &Path) -> Check {
        let fine = |message: String| Check { ok: true, version: None, message };
        let shown = path.display();
        match kind {
            PathKind::Root => detect::check_root(path),
            PathKind::Python => detect::check_tool(Tool::Python, path),
            PathKind::Docker => detect::check_tool(Tool::Docker, path),
            PathKind::Discs => fine(format!("Disc images (.iso) are looked for in {shown}")),
            PathKind::Install => fine(format!("Games are set up in {shown}/active/<game>/data")),
            PathKind::Saves => fine(format!("Saves are kept in {shown}/<disc serial>")),
        }
    }

    /// Whether a mod that shows the multiplayer name is installed for one of the library's games.
    pub fn player_name_used(&self) -> bool {
        let Some(install) = self.config().install_dir else { return false };
        let view = self.view();
        let Some(library) = view.library.as_ref() else { return false };
        library.games.iter().any(|g| plugins::uses_player_name(&plugins::dir(&install, &g.id)))
    }

    /// Whether a multiplayer server answers at `host:port`, within 5 seconds. Blocks.
    pub fn test_server(&self, host: &str, port: u32) -> Result<(), String> {
        multiplayer::reachable(host, port, Duration::from_secs(5))
    }
}

/// What is wrong with a multiplayer name, if anything.
pub fn name_problem(name: &str) -> Option<String> {
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Some("Only letters, digits and underscores.".into());
    }
    if name.len() < multiplayer::MIN_LEN {
        return Some(format!("At least {} characters.", multiplayer::MIN_LEN));
    }
    if name.len() > multiplayer::MAX_LEN {
        return Some(format!("At most {} characters.", multiplayer::MAX_LEN));
    }
    None
}

/// What is wrong with a multiplayer server's host and port as typed, if anything.
pub fn server_problem(host: &str, port: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return Some("Type the server's host name or IP address.".into());
    }
    if host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err() {
        return Some("Put the port in its own field.".into());
    }
    if !multiplayer::is_valid_host(host) {
        return Some("Not a host name or an IP address.".into());
    }
    match port.trim().parse::<u32>() {
        Ok(p) if multiplayer::is_valid_port(p) => None,
        _ => Some("The port is a number from 1 to 65535.".into()),
    }
}

/// Whether `host:port` is the official hub.
pub fn is_official(host: &str, port: &str) -> bool {
    host.trim().eq_ignore_ascii_case(multiplayer::OFFICIAL_HOST)
        && port.trim().parse::<u16>().ok() == Some(multiplayer::OFFICIAL_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_say_what_is_wrong() {
        assert_eq!(name_problem("SwiftLombax427"), None);
        assert_eq!(name_problem("ab").as_deref(), Some("At least 3 characters."));
        assert_eq!(name_problem("a b c").as_deref(), Some("Only letters, digits and underscores."));
        assert_eq!(name_problem("abcdefghijklmnopq").as_deref(), Some("At most 16 characters."));
    }

    #[test]
    fn servers_say_what_is_wrong() {
        assert_eq!(server_problem("hub.openrac.dev", "7177"), None);
        assert_eq!(server_problem("::1", "7177"), None);
        assert_eq!(server_problem("", "7177").as_deref(), Some("Type the server's host name or IP address."));
        assert_eq!(server_problem("host:7177", "7177").as_deref(), Some("Put the port in its own field."));
        assert_eq!(server_problem("bad_host", "7177").as_deref(), Some("Not a host name or an IP address."));
        assert_eq!(server_problem("host", "0").as_deref(), Some("The port is a number from 1 to 65535."));
        assert!(is_official(" HUB.openrac.dev", "7177"));
        assert!(!is_official("hub.openrac.dev", "7000"));
    }
}
