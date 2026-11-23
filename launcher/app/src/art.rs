//! The games' pictures: each game's background, its tile and the print of its disc, carried in
//! the program and decoded once, when first shown.

use std::cell::RefCell;
use std::collections::HashMap;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

macro_rules! img {
    ($path:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../public/img/", $path))
    };
}

/// A game's picture under its page's band, by OpenRAC's game id.
fn background(game: &str) -> Option<&'static [u8]> {
    Some(match game {
        "rac1" => img!("rac1-bg.webp"),
        "rac2" => img!("gc-bg.webp"),
        "rac3" => img!("uya-bg.webp"),
        "rac4" => img!("deadlocked-bg.webp"),
        "rac5" => img!("size-matters-bg.webp"),
        "rac6" => img!("sac-bg.webp"),
        _ => return None,
    })
}

/// The print of a game's real disc in a region (`ntsc`, `pal`, `jp`).
fn print(game: &str, region: &str) -> Option<&'static [u8]> {
    Some(match (game, region) {
        ("rac1", "pal") => img!("discs/rac1-pal.webp"),
        ("rac1", "jp") => img!("discs/rac1-jp.webp"),
        ("rac1", _) => img!("discs/rac1-ntsc.webp"),
        ("rac2", "pal") => img!("discs/rac2-pal.webp"),
        ("rac2", "jp") => img!("discs/rac2-jp.webp"),
        ("rac2", _) => img!("discs/rac2-ntsc.webp"),
        ("rac3", "pal") => img!("discs/rac3-pal.webp"),
        ("rac3", "jp") => img!("discs/rac3-jp.webp"),
        ("rac3", _) => img!("discs/rac3-ntsc.webp"),
        ("rac4", "pal") => img!("discs/rac4-pal.webp"),
        ("rac4", "jp") => img!("discs/rac4-jp.webp"),
        ("rac4", _) => img!("discs/rac4-ntsc.webp"),
        ("rac5", "pal") => img!("discs/rac5-pal.webp"),
        ("rac5", "jp") => img!("discs/rac5-jp.webp"),
        ("rac5", _) => img!("discs/rac5-ntsc.webp"),
        // Secret Agent Clank had no PS2 release in Asia.
        ("rac6", "pal") => img!("discs/rac6-pal.webp"),
        ("rac6", _) => img!("discs/rac6-ntsc.webp"),
        _ => return None,
    })
}

/// The print region of a version by its name (`pal1`: pal, `jp`: jp, `ntsc2`: ntsc). Korean discs
/// show the Japanese print: no scan of one exists.
fn region(version: &str) -> &'static str {
    if version.starts_with("pal") {
        "pal"
    } else if ["jp", "ntsc-j", "kr", "ko", "ntsc-k"].contains(&version) {
        "jp"
    } else {
        "ntsc"
    }
}

thread_local! {
    /// Decoded pictures, kept: a page that shows again draws at once.
    static DECODED: RefCell<HashMap<usize, Image>> = RefCell::new(HashMap::new());
}

/// `bytes` decoded, once (Slint images live on the window's thread).
fn decode(bytes: &'static [u8]) -> Image {
    let key = bytes.as_ptr() as usize;
    DECODED.with(|kept| {
        kept.borrow_mut()
            .entry(key)
            .or_insert_with(|| match image::load_from_memory(bytes) {
                Ok(picture) => {
                    let rgba = picture.into_rgba8();
                    let buffer =
                        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height());
                    Image::from_rgba8(buffer)
                }
                Err(_) => Image::default(),
            })
            .clone()
    })
}

/// A game's picture, or an empty image for a game without one (its colours show instead).
pub fn game_background(game: &str) -> Image {
    background(game).map(decode).unwrap_or_default()
}

/// The print of `game`'s disc in `version` (a version of its game.json, `pal`), or an empty image.
pub fn disc(game: &str, version: &str) -> Image {
    print(game, region(version)).map(decode).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_shows_its_regions_print() {
        assert_eq!(region("pal1"), "pal");
        assert_eq!(region("ntsc2"), "ntsc");
        assert_eq!(region("kr"), "jp");
        assert!(print("rac6", region("jp")).is_some());
        assert!(print("rac9", "ntsc").is_none());
    }
}
