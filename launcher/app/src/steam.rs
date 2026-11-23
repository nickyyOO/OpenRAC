//! Starting OpenRAC, or one game, without the launcher's window: Steam's own entry (Linux and
//! Windows) and a desktop shortcut (Linux). Both start the launcher with `--play`.

use openrac_launcher_service::core::config::Config;
use openrac_launcher_service::core::steam::{self, State, Target};
use openrac_launcher_service::core::{catalog, desktop, shortcut};

/// Where Steam keeps its folders; Windows keeps its own in the registry.
fn roots() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    let registry = windows_registry::CURRENT_USER
        .open("Software\\Valve\\Steam")
        .and_then(|key| key.get_string("SteamPath"))
        .ok()
        .map(std::path::PathBuf::from);
    #[cfg(not(windows))]
    let registry = None;
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    steam::roots(home.as_deref(), registry)
}

/// Whether this system offers Steam's entry, and a desktop shortcut.
pub fn offers() -> (bool, bool) {
    (cfg!(any(target_os = "linux", windows)), cfg!(target_os = "linux"))
}

/// A game's title (the port's, as its menu entries carry), its id and its icon, if the checkout
/// has one. `key` must be a version of the catalogue.
fn game(config: &Config, key: &str) -> Result<(String, String, Option<Vec<u8>>), String> {
    let root = config.root()?;
    let catalog = catalog::load(root)?;
    let key = shortcut::version_for_arg(&catalog, key).ok_or_else(|| format!("no game {key}"))?;
    let id = shortcut::game_id(&key);
    let icon = std::fs::read(root.join("port/game").join(&id).join("icon.png")).ok();
    Ok((desktop::game_title(&catalog, &id), id, icon))
}

/// A game's own pictures for Steam's library: cover, wide, hero and logo.
fn pictures(id: &str) -> Option<[&'static [u8]; 4]> {
    macro_rules! pictures {
        ($game:literal) => {
            [
                include_bytes!(concat!("../steam/games/", $game, "/cover.jpg")).as_slice(),
                include_bytes!(concat!("../steam/games/", $game, "/wide.jpg")).as_slice(),
                include_bytes!(concat!("../steam/games/", $game, "/hero.jpg")).as_slice(),
                include_bytes!(concat!("../steam/games/", $game, "/logo.png")).as_slice(),
            ]
        };
    }
    match id.split('-').next()? {
        "rac1" => Some(pictures!("rac1")),
        "rac2" => Some(pictures!("rac2")),
        "rac3" => Some(pictures!("rac3")),
        "rac4" => Some(pictures!("rac4")),
        _ => None,
    }
}

type Entry = (Target, Vec<u8>, Option<[&'static [u8]; 4]>);

/// What an entry is for: the launcher, or the game `version` names.
fn entry(config: &Config, version: Option<&str>) -> Result<Entry, String> {
    let Some(key) = version else { return Ok((Target::launcher(), Vec::new(), None)) };
    let (title, id, icon) = game(config, key)?;
    let pictures = pictures(&id);
    let target = Target::game(&title, &id);
    let target = if pictures.is_some() { target.with_pictures() } else { target };
    Ok((target, icon.unwrap_or_default(), pictures))
}

/// Where the launcher, or with `version` that game, stands in the player's Steam library.
pub fn state(config: &Config, version: Option<&str>) -> Result<State, String> {
    let (target, _, _) = entry(config, version)?;
    let program = desktop::LauncherProgram::from_env().path;
    Ok(steam::state_for(&roots(), &program, &target))
}

/// Adds the launcher, or with `version` that game, to Steam's library with its pictures, or
/// updates it. Steam may be open.
pub fn add(config: &Config, version: Option<&str>) -> Result<State, String> {
    let (target, icon, pictures) = entry(config, version)?;
    let program = desktop::LauncherProgram::from_env().path;
    if version.is_some() {
        let [cover, wide, hero, logo] = pictures.unwrap_or([&[]; 4]);
        let art = steam::Art { cover, wide, hero, logo, icon: &icon };
        return steam::add_for(&roots(), &program, &target, &art);
    }
    let art = steam::Art {
        cover: include_bytes!("../steam/cover.jpg"),
        wide: include_bytes!("../steam/wide.jpg"),
        hero: include_bytes!("../steam/hero.jpg"),
        logo: include_bytes!("../steam/logo.png"),
        icon: include_bytes!("../icons/icon.png"),
    };
    steam::add(&roots(), &program, &art)
}

/// A `.desktop` shortcut on the Desktop that starts the game `version` names. Linux only.
/// Returns the path it wrote.
pub fn create_shortcut(config: &Config, version: &str) -> Result<String, String> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, version);
        Err("Shortcuts are made on Linux only.".into())
    }
    #[cfg(target_os = "linux")]
    {
        let (title, id, icon) = game(config, version)?;
        let places = desktop::Places::from_env().ok_or("No home folder.")?;
        let home = openrac_launcher_service::core::detect::home_dir().ok_or("No home folder.")?;
        let program = desktop::LauncherProgram::from_env().path;
        let dir = shortcut::desktop_dir(&home);
        let path = shortcut::write_shortcut(&dir, &places, &title, &id, &program, icon.as_deref())?;
        desktop::refresh(&places, desktop::Changed { icons: icon.is_some(), entries: false });
        Ok(path.display().to_string())
    }
}
