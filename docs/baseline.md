# Baseline before the reorganisation (PLAN.md phase 0)

Taken on `develop` at e706882, 2026-10-04.

```
cargo test --workspace   1378 passed, 4 failed (environmental, below)
Lua specs (library, apps) 175 passed, 0 failed, 33 spec files
binary target/release/morf  48429776 bytes
```

Known failures, environmental, not regressions:

```
tests::a_named_shell_has_parts
tests::dbus_private::dbus_over_a_private_session_bus
tests::lib_dbus_services::services_over_a_private_session_bus
tests::lib_hyprland::hyprland_library_is_harmless_without_hyprland
```

caelestia specs fail where they failed before this work began (settings_theme, sound_theme, power_bar_theme, net_pages_theme, planner, planner_typography, lule_theme, lule, auth_desktop, auth_outputs, bar_theme, bottom_theme, connectivity_theme, graphs, keyboard_theme, network_typography, one caelestia_spec case; accessible_spec needs the a11y bus).

Per spec file:

```
 # 8 passed, 0 failed, 0 skipped in 0.23 s  library/tests/annotation_spec.lua
 # 9 passed, 0 failed, 0 skipped in 0.32 s  library/tests/capture_spec.lua
 # 2 passed, 0 failed, 0 skipped in 0.05 s  library/tests/channel_geometry_spec.lua
 # 6 passed, 0 failed, 0 skipped in 22.87 s  library/tests/default_kit_spec.lua
 # 3 passed, 0 failed, 0 skipped in 74.43 s  library/tests/default_widgets_gallery_spec.lua
 # 8 passed, 0 failed, 0 skipped in 0.19 s  library/tests/display_layout_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.08 s  library/tests/equalizer_spec.lua
 # 4 passed, 0 failed, 0 skipped in 0.12 s  library/tests/frecency_spec.lua
 # 17 passed, 0 failed, 0 skipped in 0.82 s  library/tests/hyprland_config_spec.lua
 # 2 passed, 0 failed, 0 skipped in 0.08 s  library/tests/kit_accessible_spec.lua
 # 8 passed, 0 failed, 0 skipped in 0.24 s  library/tests/kit_canvas_dock_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.32 s  library/tests/kit_collection_spec.lua
 # 3 passed, 0 failed, 0 skipped in 0.10 s  library/tests/kit_control_spec.lua
 # 4 passed, 0 failed, 0 skipped in 0.11 s  library/tests/kit_disclose_drag_nav_spec.lua
 # 7 passed, 0 failed, 0 skipped in 0.30 s  library/tests/kit_extensions_spec.lua
 # 3 passed, 0 failed, 0 skipped in 0.12 s  library/tests/kit_field_scroll_spec.lua
 # 4 passed, 0 failed, 0 skipped in 0.68 s  library/tests/kit_form_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.20 s  library/tests/kit_popup_spec.lua
 # 7 passed, 0 failed, 0 skipped in 1.53 s  library/tests/kit_roving_overflow_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.19 s  library/tests/kit_selection_spec.lua
 # 6 passed, 0 failed, 0 skipped in 2.48 s  library/tests/kit_sheet_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.21 s  library/tests/kit_spec.lua
 # 7 passed, 0 failed, 0 skipped in 0.26 s  library/tests/kit_transform_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.17 s  library/tests/kit_widgets_spec.lua
 # 1 passed, 0 failed, 0 skipped in 0.04 s  library/tests/lule_spec.lua
 # 6 passed, 0 failed, 0 skipped in 0.31 s  library/tests/lyrics_spec.lua
 # 2 passed, 0 failed, 0 skipped in 0.07 s  library/tests/rtl_spec.lua
 # 3 passed, 0 failed, 0 skipped in 0.01 s  library/tests/settings_pages_spec.lua
 # 8 passed, 0 failed, 0 skipped in 0.20 s  library/tests/settings_spec.lua
 # 5 passed, 0 failed, 0 skipped in 0.00 s  library/tests/spectrum_spec.lua
 # 2 passed, 0 failed, 0 skipped in 0.18 s  library/tests/taskwarrior_spec.lua
 # 4 passed, 0 failed, 0 skipped in 1.36 s  examples/apps/editor/tests/editor_app_spec.lua
 # 6 passed, 0 failed, 0 skipped in 1.67 s  examples/apps/settings/tests/settings_app_spec.lua
```

# After the hardening pass (PLAN.md sections 1-6)

Taken on `develop`, 2026-10-14, against v0.2.8 (36f881f8).

Lua specs: 151 files (101 caelestia, 38 library, 12 demos), 1053 passed, 0
failed, 1 skipped; caelestia_spec runs with `--no-dbus`, lyrics needs
python3 on PATH.

Desk layout in the sandbox, caelestia, `MONITORS="3840x2160 3840x2160
3456x2160"`, `tools/sandbox/cpu-idle.steps` and `cpu-active.steps`, two
alternating rounds each (v0.2.8 / develop):

```
                          v0.2.8          develop
rest                      4-6%            3-5%
threads                   42              37-38
rss                       941-952 MB      936-943 MB
sidebar in and out x4     4%              3-4%
workspace switch x4       4-8%            4%
osd x4                    11-12%          10-13%
dashboard x3              39-40%          36-39%
```

The shell's own CPU at this size is dominated by work these changes do not
touch; the per-frame savings show in the CPU benches
(`crates/graphics/morf-render/examples/`, `crates/core/morf-layout/examples/`):

```
layout update, 1041 nodes      28 ms        -> 1.9-3.9 ms   (kept Taffy trees)
draw list + damage, 921 nodes  3.0-4.0 ms   -> 0.49 ms      (copied subtrees)
damage diff, 921 nodes         1.04 ms      -> 0.058 ms     (copied runs)
backend frame, 921 nodes       1.11 ms      -> 0.59-0.72 ms (kept text, quads, fields)
box + shadow field             5.41/6.87 ms -> 5.25/6.60 ms (min/median, one-box pipeline)
```
