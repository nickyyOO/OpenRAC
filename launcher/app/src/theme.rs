//! The window's colours: the theme picked in Settings and the picked game's accent, from core's
//! palettes.json, put on the `Theme` global.

use openrac_launcher_service::core::theme;
use slint::{Color, ComponentHandle};

use crate::{AppWindow, Theme};

/// Every colour token the window draws with, by its palettes.json name.
const TOKENS: &[&str] = &[
    "--ink",
    "--ink-deep",
    "--rail",
    "--panel",
    "--panel-hi",
    "--amber",
    "--gold",
    "--brand",
    "--glow",
    "--text",
    "--body",
    "--lav",
    "--soft",
    "--dim",
    "--line",
    "--line-strong",
    "--control",
    "--control-hi",
    "--control-card",
    "--control-card-hi",
    "--ok",
    "--warn",
    "--err",
];

/// `#rrggbb[aa]` as a colour.
fn color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some(Color::from_argb_u8(alpha, byte(0)?, byte(2)?, byte(4)?))
}

/// Puts theme `id` on the window, with the accent of `game` (its light, and with `game_colors`
/// its own gold and coal).
pub fn apply(window: &AppWindow, id: &str, game: Option<&str>, game_colors: bool) {
    let global = window.global::<Theme>();
    for (token, value) in theme::colors(id, game, game_colors, TOKENS) {
        let Some(c) = color(&value) else { continue };
        match token {
            "--ink" => global.set_ink(c),
            "--ink-deep" => global.set_ink_deep(c),
            "--rail" => global.set_rail(c),
            "--panel" => global.set_panel(c),
            "--panel-hi" => global.set_panel_hi(c),
            "--amber" => global.set_amber(c),
            "--gold" => global.set_gold(c),
            "--brand" => global.set_brand(c),
            "--glow" => global.set_glow(c),
            "--text" => global.set_text(c),
            "--body" => global.set_body(c),
            "--lav" => global.set_lav(c),
            "--soft" => global.set_soft(c),
            "--dim" => global.set_dim(c),
            "--line" => global.set_line(c),
            "--line-strong" => global.set_line_strong(c),
            "--control" => global.set_control(c),
            "--control-hi" => global.set_control_hi(c),
            "--control-card" => global.set_control_card(c),
            "--control-card-hi" => global.set_control_card_hi(c),
            "--ok" => global.set_ok(c),
            "--warn" => global.set_warn(c),
            "--err" => global.set_err(c),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_colours_core_hands_over() {
        assert_eq!(color("#f5c040"), Some(Color::from_rgb_u8(0xf5, 0xc0, 0x40)));
        assert_eq!(color("#ffffff17"), Some(Color::from_argb_u8(0x17, 255, 255, 255)));
        assert_eq!(color("f5c040"), None);
    }
}
