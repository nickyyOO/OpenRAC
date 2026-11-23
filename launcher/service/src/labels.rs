//! Words for what the catalogue and the games name by code: the names the launcher shows, a
//! disc's region, each game's levels, a large number.

/// Names the launcher shows in place of the catalogue's (games/<game>/game.json).
pub fn shown_title(title: &str) -> &str {
    match title {
        "Ratchet & Clank: Secret Agent Clank" => "Secret Agent Clank",
        _ => title,
    }
}

/// A game's name without the series in front, for a narrow list: "Going Commando",
/// "Deadlocked"; the first game keeps its name.
pub fn short_title(title: &str) -> &str {
    title.strip_prefix("Ratchet & Clank: ").or_else(|| title.strip_prefix("Ratchet: ")).unwrap_or(title)
}

/// A game's name in two lines, the series and the game ("Ratchet & Clank:", "Going Commando");
/// "Ratchet: Deadlocked" is one name and keeps one line (the second is empty).
pub fn title_lines(title: &str) -> (&str, &str) {
    match title.strip_prefix("Ratchet & Clank: ") {
        Some(game) => ("Ratchet & Clank:", game),
        None => (title, ""),
    }
}

/// A disc's short name: `pal` -> "PAL", `ntsc` -> "NTSC-U", `pal1` -> "PAL v1.00".
pub fn region_label(name: &str) -> String {
    match name {
        "pal" => "PAL".into(),
        "pal1" => "PAL v1.00".into(),
        "ntsc" => "NTSC-U".into(),
        "ntsc2" => "NTSC-U v2.00".into(),
        "jp" => "NTSC-J".into(),
        _ => name.to_uppercase(),
    }
}

/// Ratchet & Clank's levels, by the number the game and the level editor give them.
const RAC1_LEVELS: &[&str] = &[
    "Veldin",
    "Novalis",
    "Aridia",
    "Kerwan",
    "Eudora",
    "Rilgar",
    "Blarg Station",
    "Umbris",
    "Batalia",
    "Gaspar",
    "Orxon",
    "Pokitaru",
    "Hoven",
    "Gemlik Base",
    "Oltanis",
    "Quartu",
    "Kalebo III",
    "Drek's Fleet",
    "Veldin (the return)",
];

/// rac2's levels, by number.
const RAC2_LEVELS: &[(u32, &str)] = &[
    (0, "Aranos Tutorial"),
    (1, "Oozla"),
    (2, "Maktar Nebula"),
    (3, "Endako"),
    (4, "Barlow"),
    (5, "Feltzin System"),
    (6, "Notak"),
    (7, "Siberius"),
    (8, "Tabora"),
    (9, "Dobbo"),
    (10, "Hrugis Cloud"),
    (11, "Joba"),
    (12, "Todano"),
    (13, "Boldan"),
    (14, "Aranos Prison"),
    (15, "Gorn"),
    (16, "Snivelak"),
    (17, "Smolg"),
    (18, "Damosel"),
    (19, "Grelbin"),
    (20, "Yeedil"),
    (22, "Dobbo Orbit"),
    (23, "Damosel Orbit"),
    (24, "Ship Shack"),
    (25, "Wupash Nebula"),
    (26, "Jamming Array"),
    (30, "Insomniac Museum"),
];

/// rac3's levels, by number.
const RAC3_LEVELS: &[(u32, &str)] = &[
    (1, "Veldin"),
    (2, "Florana"),
    (3, "Starship Phoenix"),
    (4, "Marcadia"),
    (5, "Daxx"),
    (6, "Starship Phoenix (Under Attack)"),
    (7, "Annihilation Nation"),
    (8, "Aquatos"),
    (9, "Tyhrranosis"),
    (10, "Zeldrin Starport"),
    (11, "Obani Moons"),
    (12, "Rilgar"),
    (13, "Holostar Studios Ratchet"),
    (14, "Koros"),
    (16, "Kerwan"),
    (17, "Crash Site"),
    (18, "Aridia"),
    (19, "Thran Asteroid Belt"),
    (20, "Final Boss"),
    (21, "Obani Draco"),
    (22, "Mylon"),
    (23, "Holostar Studios Clank"),
    (24, "Insomniac Museum"),
    (26, "Kerwan Ranger Missions"),
    (27, "Aquatos Base"),
    (28, "Aquatos Sewers"),
    (29, "Tyhrranosis Ranger Missions"),
    (30, "Vid-Comic Unnamed"),
    (31, "Vid-Comic 1"),
    (32, "Vid-Comic 4"),
    (33, "Vid-Comic 2"),
    (34, "Vid-Comic 3"),
    (35, "Vid-Comic 5"),
    (36, "Vid-Comic 1 Special Edition"),
    (39, "Multiplayer Menu"),
    (40, "Bakisi Isles"),
    (41, "Hoven Gorge"),
    (42, "Outpost X12"),
    (43, "Korgon Outpost"),
    (44, "Metropolis"),
    (45, "Blackwater City"),
    (46, "Command Center"),
    (47, "Blackwater Docks"),
    (48, "Aquatos Sewers (multiplayer)"),
    (49, "Marcadia Palace"),
    (50, "Bakisi Isles (split screen)"),
    (51, "Hoven Gorge (split screen)"),
    (52, "Outpost X12 (split screen)"),
    (53, "Korgon Outpost (split screen)"),
    (54, "Metropolis (split screen)"),
    (55, "Blackwater City (split screen)"),
];

/// rac4's levels, by number.
const RAC4_LEVELS: &[(u32, &str)] = &[
    (0, "Multiplayer Menu"),
    (1, "DreadZone Station"),
    (2, "Catacrom Graveyard"),
    (4, "Sarathos Swamp"),
    (5, "Dark Cathedral"),
    (6, "Temple of Shaar"),
    (7, "Valix Lighthouse"),
    (8, "Mining Facility"),
    (10, "Torval Ruins"),
    (11, "Tempus Station"),
    (13, "Maraxus Prison"),
    (14, "Ghost Station"),
    (15, "Control Level"),
    (21, "DreadZone Station (split screen)"),
    (22, "Catacrom Graveyard (split screen)"),
    (24, "Sarathos Swamp (split screen)"),
    (25, "Dark Cathedral (split screen)"),
    (26, "Temple of Shaar (split screen)"),
    (27, "Valix Lighthouse (split screen)"),
    (28, "Mining Facility (split screen)"),
    (30, "Torval Ruins (split screen)"),
    (31, "Tempus Station (split screen)"),
    (33, "Maraxus Prison (split screen)"),
    (34, "Ghost Station (split screen)"),
    (35, "Control Level (split screen)"),
    (41, "Battledome Tower"),
    (42, "Catacrom Graveyard (multiplayer)"),
    (44, "Sarathos Swamp (multiplayer)"),
    (45, "Dark Cathedral (multiplayer)"),
    (46, "Temple of Shaar (multiplayer)"),
    (47, "Valix Lighthouse (multiplayer)"),
    (48, "Mining Facility (multiplayer)"),
    (50, "Torval Ruins (multiplayer)"),
    (51, "Tempus Station (multiplayer)"),
    (53, "Maraxus Prison (multiplayer)"),
    (54, "Ghost Station (multiplayer)"),
    (61, "Battledome Tower (split screen)"),
    (62, "Catacrom Graveyard (multiplayer, split screen)"),
    (64, "Sarathos Swamp (multiplayer, split screen)"),
    (65, "Dark Cathedral (multiplayer, split screen)"),
    (66, "Temple of Shaar (multiplayer, split screen)"),
    (67, "Valix Lighthouse (multiplayer, split screen)"),
    (68, "Mining Facility (multiplayer, split screen)"),
    (70, "Torval Ruins (multiplayer, split screen)"),
    (71, "Tempus Station (multiplayer, split screen)"),
    (73, "Maraxus Prison (multiplayer, split screen)"),
    (74, "Ghost Station (multiplayer, split screen)"),
];

/// A game's levels in the order of their numbers.
pub fn levels_of(game: &str) -> Vec<(u32, &'static str)> {
    let later: &[(u32, &str)] = match game {
        "rac2" => RAC2_LEVELS,
        "rac3" => RAC3_LEVELS,
        "rac4" => RAC4_LEVELS,
        _ => return RAC1_LEVELS.iter().enumerate().map(|(n, name)| (n as u32, *name)).collect(),
    };
    let mut levels = later.to_vec();
    levels.sort_by_key(|(n, _)| *n);
    levels
}

/// A level's name, or "Level N" for one the list does not have.
pub fn level_name(game: &str, number: u32) -> String {
    levels_of(game)
        .into_iter()
        .find(|(n, _)| *n == number)
        .map_or_else(|| format!("Level {number}"), |(_, name)| name.into())
}

/// 2202320 -> "2 202 320" (narrow no-break spaces).
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_as_the_launcher_shows_them() {
        assert_eq!(shown_title("Ratchet & Clank: Secret Agent Clank"), "Secret Agent Clank");
        assert_eq!(short_title("Ratchet: Deadlocked"), "Deadlocked");
        assert_eq!(title_lines("Ratchet & Clank: Going Commando"), ("Ratchet & Clank:", "Going Commando"));
        assert_eq!(title_lines("Ratchet: Deadlocked"), ("Ratchet: Deadlocked", ""));
        assert_eq!(region_label("ntsc"), "NTSC-U");
        assert_eq!(level_name("rac1", 8), "Batalia");
        assert_eq!(level_name("rac1", 99), "Level 99");
    }
}
