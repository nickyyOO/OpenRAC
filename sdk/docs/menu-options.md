# Menu options

A mod can turn an option of the port's menu on or off with `menu_option_set` (C) or
`mod.menu_option_set` (C++).

- `OPENRAC_MENU_OPTION_ON`: the option can be used, even where the port greys it out.
- `OPENRAC_MENU_OPTION_OFF`: the option is greyed out. It is shown, but cannot be changed.
- `OPENRAC_MENU_OPTION_DEFAULT`: the port decides again.

The last call for an option wins. When a mod unloads, its calls go with it.

Example: Ratchet & Clank never vibrates the controller, so the port greys out Vibration there.
A mod that vibrates the controller itself (`set_motors`) turns it back on:

```cpp
mod.menu_option_set("rumble_strength", OPENRAC_MENU_OPTION_ON);
```

## Names

Names never change. A port that does not know a name ignores it. Panels, notes and Quit are not
listed: they cannot be turned off.

- Graphics: `full_screen`, `display_mode`, `monitor`, `menu_size`, `picture`, `gameplay_fov`, `hud`, `render_scale`, `msaa`, `anisotropy`, `draw_distance`, `active_distance`, `frame_rate`, `frame_cap`, `vsync`, `frame_counter`, `hdr`, `hdr_paper_white`, `hdr_peak`, `hdr_highlights`, `hdr_sdr_hue`, `hdr_sdr_chroma`, `hdr_blowout`, `low_latency`, `responsive_controls`, `gpu_breadcrumbs`
- Enhanced: `enhanced_graphics`, `enhanced_low`, `enhanced_medium`, `enhanced_high`, `shadow_quality`, `shadow_filter`, `occlusion_quality`, `texture_upscaling`, `effects_detail`, `upscaling`, `sharpening`, `motion_blur`, `motion_blur_quality`, `color_correction`, `brightness`, `enhanced_reset`, `occlusion_hi_z`, `enhanced_advanced`, `enhanced_materials`
- Game: `volume`, `mute_in_background`, `skip_cutscenes`
- Controls: `controls_file`, `controls_player`, `controls_player_reset`, `trigger_point`, `face_buttons`, `pad_background`, `rumble_strength`, `rumble_style`, `rumble_triggers`, `rumble_phone`, `players_reset`, `touch_action_buttons`, `touch_button_size`, `touch_camera_speed`, `touch_move_controls`, `controls_reset`
- Developer: `debug_overlay`, `moby_picker`, `object_viewer`, `meat_proxy_review`, `level_select`, `keep_paused`, `step_tick`, `speed`, `screenshot`, `record_gameplay`, `record_length`, `record_sweep`, `record_folder`, `slow_cpu`, `freecam`, `freecam_mode`, `freecam_inspect`, `freecam_speed`, `freecam_reset`, `freecam_exit`
