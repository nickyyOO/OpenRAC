//! The player's mods: listed, switched on and off, ordered, set, added and removed, kept current
//! (core plugins.rs, remote.rs, mod_updates.rs), and the built-in Enhanced graphics and Keyboard
//! and mouse entries. Every call blocks: the window calls them off its thread.

use std::path::{Path, PathBuf};

use openrac_launcher_core::enhanced::{self, EnhancedView};
use openrac_launcher_core::plugins::{self, Installed, PluginView};
use openrac_launcher_core::{keyboard_mouse, launcher_log, mod_updates, multiplayer, remote, saves};
use serde_json::Value;

use openrac_launcher_core::actions::{Inputs, Scope};
use openrac_launcher_core::library::VersionView;
use openrac_launcher_core::{install, texture_packs};

use crate::labels::grouped;
use crate::launcher::Launcher;
use crate::player::action;

/// What adding the player's files came to.
#[derive(Debug, Default)]
pub struct Added {
    /// In a sentence.
    pub text: String,
    pub failed: bool,
    /// A gameplay mod came in: their list is read again.
    pub plugin: bool,
    /// A pack came in: the packs' list is read again.
    pub pack: bool,
}

/// The starts of the refusals that mean a .zip is a texture pack that cannot be added; any other
/// refusal means it is something else (a mod, an overrides pack).
const PACK_REFUSALS: [&str; 4] = [
    "This pack is for ",
    "The pack has two pictures for the same texture",
    "The animation folder ",
    "The pack's manifest cannot be read",
];

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// A mod switched on, and the mods it needs that were switched on with it, by name.
pub struct TurnedOn {
    pub views: Vec<PluginView>,
    pub also_on: Vec<String>,
}

impl Launcher {
    /// The folder of a game's mods.
    pub fn mods_dir(&self, game: &str) -> Result<PathBuf, String> {
        let install = self.config().install_dir.ok_or("no install folder (Settings)")?;
        Ok(plugins::dir(&install, game))
    }

    /// The Multiplayer name, for the settings that default to it; empty when it is not valid.
    fn player_name(&self) -> String {
        let name = self.config().multiplayer_name;
        if multiplayer::is_valid(&name) {
            name
        } else {
            String::new()
        }
    }

    /// `work` on a game's mods folder, its answer's settings showing the Multiplayer name.
    fn with_mods(
        &self,
        game: &str,
        work: impl FnOnce(&Path) -> Result<Vec<PluginView>, String>,
    ) -> Result<Vec<PluginView>, String> {
        let mut views = work(&self.mods_dir(game)?)?;
        plugins::show_player_name(&mut views, &self.player_name());
        Ok(views)
    }

    /// A game's mods in load order, as they stand for its disc `version` (`pal`).
    pub fn mods(&self, game: &str, version: &str) -> Result<Vec<PluginView>, String> {
        self.with_mods(game, |dir| plugins::list(dir, game, version))
    }

    /// Switches a mod off.
    pub fn switch_mod_off(&self, game: &str, version: &str, id: &str) -> Result<Vec<PluginView>, String> {
        self.with_mods(game, |dir| plugins::set_enabled(dir, game, version, id, false))
    }

    /// Switches a mod on, with the mods it needs.
    pub fn switch_mod_on(&self, game: &str, version: &str, id: &str) -> Result<TurnedOn, String> {
        let dir = self.mods_dir(game)?;
        let (mut views, also_on) = plugins::turn_on(&dir, game, version, id)?;
        plugins::show_player_name(&mut views, &self.player_name());
        Ok(TurnedOn { views, also_on })
    }

    /// The mods that are on and need this one, by id and name.
    pub fn mod_dependents(&self, game: &str, id: &str) -> Result<Vec<(String, String)>, String> {
        plugins::dependents(&self.mods_dir(game)?, id)
    }

    /// Moves a mod one place up or down the load order.
    pub fn move_mod(&self, game: &str, version: &str, id: &str, up: bool) -> Result<Vec<PluginView>, String> {
        self.with_mods(game, |dir| plugins::move_mod(dir, game, version, id, up))
    }

    /// Moves a mod `places` up (below 0: down) in the load order, one place at a time.
    pub fn move_mod_by(&self, game: &str, version: &str, id: &str, places: i32) -> Result<Vec<PluginView>, String> {
        let mut views = self.mods(game, version)?;
        for _ in 0..places.unsigned_abs() {
            views = self.move_mod(game, version, id, places > 0)?;
        }
        Ok(views)
    }

    /// Sets one of a mod's settings.
    pub fn set_mod_setting(
        &self,
        game: &str,
        version: &str,
        id: &str,
        key: &str,
        value: Value,
    ) -> Result<Vec<PluginView>, String> {
        let name = self.player_name();
        plugins::set_setting(&self.mods_dir(game)?, game, version, id, key, value, &name)
    }

    /// The dependencies removing this mod leaves unused.
    pub fn mod_orphans(&self, game: &str, id: &str) -> Result<Vec<mod_updates::Orphan>, String> {
        Ok(mod_updates::orphans(&self.mods_dir(game)?, id))
    }

    /// Removes a mod, and the unused dependencies in `also`.
    pub fn remove_mod(&self, game: &str, version: &str, id: &str, also: &[String]) -> Result<Vec<PluginView>, String> {
        let views = self.with_mods(game, |dir| {
            let left = mod_updates::remove_with(dir, game, version, id, also)?;
            remote::forget(dir, id)?;
            Ok(left)
        })?;
        launcher_log::line(&format!("{game}: mod {id} removed"));
        Ok(views)
    }

    /// Adds a mod from a .zip or a folder: a gameplay mod to the store, or the answer that it is a
    /// data pack.
    pub fn install_mod(&self, game: &str, version: &str, path: &Path) -> Result<Installed, String> {
        plugins::install(&self.mods_dir(game)?, game, version, path)
    }

    /// The mods a chosen folder holds: one mod, or a repository's checkout of several.
    pub fn mod_folders(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        plugins::mod_folders(path)
    }

    /// `work` with what reaches a game's mods on the web.
    fn with_remote<T>(&self, game: &str, version: &str, work: impl FnOnce(&remote::Context) -> T) -> Result<T, String> {
        let dir = self.mods_dir(game)?;
        let fetch = self.host().fetch();
        Ok(work(&remote::Context {
            dir: &dir,
            game,
            version,
            platform: plugins::platform_key(),
            fetch: &*fetch,
            hosts: remote::Hosts::default(),
        }))
    }

    /// What a mod's link holds: its mods, and whether each works here.
    pub fn preview_mod(&self, game: &str, version: &str, url: &str) -> Result<remote::Preview, String> {
        self.with_remote(game, version, |c| remote::preview(c, url))?
    }

    /// Installs one mod from a link.
    pub fn install_mod_from(
        &self,
        game: &str,
        version: &str,
        url: &str,
        id: &str,
    ) -> Result<remote::RemoteInstalled, String> {
        self.with_remote(game, version, |c| remote::install(c, url, id))?
    }

    /// Opens an installed mod's folder (its `path`), one inside the game's mods only.
    pub fn open_mod_folder(&self, game: &str, path: &str) -> Result<(), String> {
        let dir = self.mods_dir(game)?;
        let path = Path::new(path);
        if !path.starts_with(&dir) {
            return Err(format!("{} is not one of the game's mods", path.display()));
        }
        self.host().open_path(path)
    }

    /// Newer versions of the mods added from a link (those the index lists update from it).
    pub fn mod_updates(&self, game: &str, version: &str) -> Result<(Vec<remote::Update>, Vec<String>), String> {
        self.with_remote(game, version, remote::check_updates)
    }

    /// Updates one mod from its source.
    pub fn update_mod(&self, game: &str, version: &str, id: &str) -> Result<remote::RemoteInstalled, String> {
        self.with_remote(game, version, |c| remote::update(c, id))?
    }

    /// Where each installed mod came from.
    pub fn mod_sources(&self, game: &str) -> Result<remote::Sources, String> {
        Ok(remote::read_sources(&self.mods_dir(game)?))
    }

    /// Keeps a mod at its version, or lets it update again.
    pub fn pin_mod(&self, game: &str, id: &str, pin: bool) -> Result<remote::Sources, String> {
        remote::set_pin(&self.mods_dir(game)?, id, pin)
    }

    // ---- the built-in entries ------------------------------------------------------------------

    /// Enhanced graphics for a version, or None for one that has none.
    pub fn enhanced(&self, game: &str, version: &str) -> Option<EnhancedView> {
        if !enhanced::supported(game, version) {
            return None;
        }
        let bake = enhanced::version_bake_status(&self.config(), &format!("{game}/{version}"));
        Some(enhanced::view(&saves::data_home(), game, version, bake))
    }

    pub fn set_enhanced(&self, game: &str, version: &str, on: bool) -> Result<Option<EnhancedView>, String> {
        enhanced::set_enabled(&saves::data_home(), game, version, on)?;
        launcher_log::line(&format!("{game}/{version}: enhanced graphics {}", if on { "on" } else { "off" }));
        Ok(self.enhanced(game, version))
    }

    pub fn set_enhanced_setting(
        &self,
        game: &str,
        version: &str,
        key: &str,
        value: &Value,
    ) -> Result<Option<EnhancedView>, String> {
        enhanced::set_setting(&saves::data_home(), game, version, key, value)?;
        Ok(self.enhanced(game, version))
    }

    /// Deletes every lighting bake of a version.
    pub fn delete_bakes(&self, game: &str, version: &str) -> Result<Option<EnhancedView>, String> {
        let key = format!("{game}/{version}");
        enhanced::delete_bakes(&self.config(), &key)?;
        launcher_log::line(&format!("{key}: lighting bakes deleted"));
        Ok(self.enhanced(game, version))
    }

    /// Keyboard and mouse for a version, or None for one that has none.
    pub fn keyboard_mouse(&self, game: &str, version: &str) -> Option<PluginView> {
        keyboard_mouse::supported(game, version).then(|| keyboard_mouse::view(&saves::data_home(), game, version))
    }

    pub fn set_keyboard_mouse_setting(
        &self,
        game: &str,
        version: &str,
        key: &str,
        value: &Value,
    ) -> Result<Option<PluginView>, String> {
        keyboard_mouse::set_setting(&saves::data_home(), game, version, key, value)?;
        Ok(self.keyboard_mouse(game, version))
    }

    // ---- adding the player's files -------------------------------------------------------------

    /// Adds each path in turn, stopping at the first that fails: a shared map to OpenRAC Editor's
    /// import, a texture pack's .zip into the player's texture pack, a gameplay mod to the game's
    /// mods, a data pack to its overrides. The player never has to know which is which.
    pub fn add_mods(&self, v: &VersionView, paths: &[PathBuf]) -> Added {
        let mut result = Added::default();
        let scope = Scope::Version(v.version.key.clone());
        let failed = |result: Added, text: String| Added { text, failed: true, ..result };
        let mut each = Vec::new();
        for path in paths {
            let name = name_of(path);
            if name.to_lowercase().ends_with(".openrac-map") {
                let Some(importer) = action(v, "editor-import-map").filter(|a| a.runnable) else {
                    return failed(
                        result,
                        format!("{name} is a map: download OpenRAC Editor (Mods, Level editor) to add it."),
                    );
                };
                let inputs = Inputs { path: Some(path.display().to_string()), ..Default::default() };
                if !self.run_chain(&scope, std::slice::from_ref(importer), Some(inputs)) {
                    return failed(
                        result,
                        format!("{name}: {}", self.why_stopped(&v.version.key, "editor-import-map")),
                    );
                }
                result.pack = true;
                result.text = format!("Added the map {name} as a new planet: board the ship and pick it.");
                continue;
            }
            match self.mod_folders(path) {
                Ok(found) => each.extend(found),
                Err(e) => return failed(result, format!("{name}: {e}")),
            }
        }
        let mut count = 0;
        for path in &each {
            let name = name_of(path);
            match self.add_texture_pack(v, path) {
                Some(Ok(text)) => {
                    result.pack = true;
                    result.text = text;
                    continue;
                }
                Some(Err(e)) => return failed(result, e),
                None => {}
            }
            match self.install_mod(&v.version.game, &v.version.name, path) {
                Err(e) => return failed(result, format!("{name}: {e}")),
                Ok(Installed::Plugin { name, version, replaced, enabled, .. }) => {
                    result.plugin = true;
                    count += 1;
                    result.text = match (replaced, enabled) {
                        (true, _) => format!("Updated {name} to {version}; its settings are kept."),
                        (false, true) => format!("Added {name}. It is on from the game's next start."),
                        (false, false) => format!("Added {name}. It is off: its row says why."),
                    };
                }
                Ok(Installed::Pack) => {
                    let Some(adder) = action(v, "overrides-add").filter(|a| a.runnable) else {
                        return failed(
                            result,
                            format!("{name} is a texture and model pack, which cannot be added here yet."),
                        );
                    };
                    let inputs = Inputs { path: Some(path.display().to_string()), ..Default::default() };
                    if !self.run_chain(&scope, std::slice::from_ref(adder), Some(inputs)) {
                        return failed(
                            result,
                            format!("{name}: {}", self.why_stopped(&v.version.key, "overrides-add")),
                        );
                    }
                    result.pack = true;
                    result.text = format!("Added {name}.");
                }
            }
        }
        if count > 1 {
            result.text = format!("Added or updated {count} mods; their settings are kept.");
        }
        result
    }

    /// Installs mod `id` of the repository at `url` (its preview's `label` names it): a gameplay
    /// mod into the store, a data pack's files to the packs.
    pub fn add_from_link(&self, v: &VersionView, url: &str, label: &str, id: &str) -> Added {
        let (game, version) = (&v.version.game, &v.version.name);
        match self.install_mod_from(game, version, url, id) {
            Err(e) => Added { text: format!("{id}: {e}"), failed: true, ..Added::default() },
            Ok(remote::RemoteInstalled::Pack { path, .. }) => self.add_mods(v, &[PathBuf::from(path)]),
            Ok(remote::RemoteInstalled::Plugin { name, version, replaced, enabled, .. }) => Added {
                text: match (replaced, enabled) {
                    (true, _) => format!("Installed {name} {version} from {label}; its settings are kept."),
                    (false, true) => format!("Installed {name} from {label}. It is on from the game's next start."),
                    (false, false) => format!("Installed {name} from {label}. It is off: its row says why."),
                },
                plugin: true,
                ..Added::default()
            },
        }
    }

    /// A texture pack's .zip added to the player's own pack, or why it is refused; None when the
    /// file is not a texture pack at all.
    fn add_texture_pack(&self, v: &VersionView, path: &Path) -> Option<Result<String, String>> {
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            return None;
        }
        let name = name_of(path);
        let data = install::data_dir(&self.config().install_dir?, &v.version.game);
        let preview = match texture_packs::preview_import(&data, &v.version.game, &v.version.name, path) {
            Ok(p) => p,
            Err(why) if PACK_REFUSALS.iter().any(|start| why.starts_with(start)) => {
                return Some(Err(format!("{name}: {why}")))
            }
            Err(_) => return None,
        };
        Some(
            match texture_packs::import(&data, &v.version.game, &v.version.name, path, texture_packs::ImportMode::Merge)
            {
                Ok((textures, _)) => {
                    launcher_log::line(&format!(
                        "{}: imported a texture pack of {textures} picture(s) (Merge)",
                        v.version.key
                    ));
                    let n = format!(
                        "{} {}",
                        grouped(u64::from(textures)),
                        if textures == 1 { "texture" } else { "textures" }
                    );
                    Ok(match preview.replacing {
                        0 => format!("Added {n} to your texture pack."),
                        r => format!("Added {n} to your texture pack, {r} of them in place of yours."),
                    })
                }
                Err(e) => Err(format!("{name}: {e}")),
            },
        )
    }

    /// Why the last run of `id` stopped, in a player's words.
    fn why_stopped(&self, key: &str, id: &str) -> String {
        let jobs = self.jobs();
        let job = jobs.iter().rev().find(|j| j.action_id == id && matches!(&j.scope, Scope::Version(k) if k == key));
        let line = crate::failure::tool_failure(job.map(|j| j.lines.as_slice()).unwrap_or_default()).error.line;
        let mut chars = line.chars();
        chars
            .next()
            .map(|c| c.to_lowercase().collect::<String>() + chars.as_str())
            .unwrap_or_else(|| "it could not be added".into())
    }

    // ---- with the player's answers -------------------------------------------------------------

    /// Switches a mod on with what it needs, or off with the mods that need it once the player
    /// says yes. The list as it now is, and what to say about it.
    pub fn switch_mod(
        &self,
        game: &str,
        version: &str,
        id: &str,
        name: &str,
        on: bool,
    ) -> Result<(Vec<PluginView>, String), String> {
        if on {
            let turned = self.switch_mod_on(game, version, id)?;
            let said = if turned.also_on.is_empty() {
                String::new()
            } else {
                format!("Also turned on {}.", names(&turned.also_on))
            };
            return Ok((turned.views, said));
        }
        let needers = self.mod_dependents(game, id).unwrap_or_default();
        if !needers.is_empty() {
            let who: Vec<String> = needers.iter().map(|(_, n)| n.clone()).collect();
            let (verb, them) = if who.len() == 1 { ("needs", "it") } else { ("need", "them") };
            if !self.host().confirm(&format!("{} {verb} {name}. Turn off {them} too?", names(&who)), "Turn off") {
                return Ok((self.mods(game, version)?, String::new()));
            }
            for (needer, _) in &needers {
                let _ = self.switch_mod_off(game, version, needer);
            }
        }
        Ok((self.switch_mod_off(game, version, id)?, String::new()))
    }

    /// Removes a mod once the player says yes, with the dependencies only it needed: the
    /// launcher's own installs at once (Clean up unused dependencies), the player's own after a
    /// question. None when the player kept it.
    pub fn remove_mod_asking(
        &self,
        game: &str,
        version: &str,
        id: &str,
        name: &str,
    ) -> Result<Option<(Vec<PluginView>, String)>, String> {
        if !self.host().confirm(&format!("Remove {name}? The launcher's copy and its settings are deleted."), "Remove")
        {
            return Ok(None);
        }
        let orphans =
            if self.config().cleanup_unused_deps { self.mod_orphans(game, id).unwrap_or_default() } else { Vec::new() };
        let mut also: Vec<String> = orphans.iter().filter(|o| o.auto).map(|o| o.id.clone()).collect();
        let mine: Vec<&mod_updates::Orphan> = orphans.iter().filter(|o| !o.auto).collect();
        if !mine.is_empty() {
            let who: Vec<String> = mine.iter().map(|o| o.name.clone()).collect();
            let it = if who.len() == 1 { "it" } else { "them" };
            if self.host().confirm(&format!("Also remove {}? Nothing else uses {it}.", names(&who)), "Remove") {
                also.extend(mine.iter().map(|o| o.id.clone()));
            }
        }
        let views = self.remove_mod(game, version, id, &also)?;
        let said = match also.len() {
            0 => String::new(),
            1 => "Also removed 1 unused mod.".into(),
            n => format!("Also removed {n} unused mods."),
        };
        Ok(Some((views, said)))
    }

    /// Before Play: the game's mod updates, waited for a short while at most; Play goes on
    /// whatever happens.
    pub(crate) fn update_mods_before_play(&self, _key: &str) {}
}

/// "A", "A and B", "A, B and C".
pub fn names(list: &[String]) -> String {
    match list {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_read_as_a_list() {
        let n = |l: &[&str]| super::names(&l.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(n(&["A"]), "A");
        assert_eq!(n(&["A", "B"]), "A and B");
        assert_eq!(n(&["A", "B", "C"]), "A, B and C");
    }
}
