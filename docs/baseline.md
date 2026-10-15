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

```
cargo test --workspace --no-fail-fast   1612 passed, 0 failed
Lua specs                               158 files, 1099 passed, 0 failed
morf check --strict                     caelestia shell, lock and greet: 0 errors, 0 warnings
```

The specs run on a private bus (`dbus-run-session` with
`tools/sandbox/dbus-session.conf`), caelestia_spec with `--no-dbus`, and
lyrics with python3 on PATH.

The desk layout, v0.2.8 against develop under identical conditions: caelestia
in the sandbox with `MONITORS="3840x2160 3840x2160 3456x2160"`, one run per
build per round, three rounds alternating, each run the same script
(`cpu-active.steps` between two `@wakes` reads) with `MORF_FRAME_LOG=1`.
Frames are the `frame on <output> took` lines, the first frame of a run left
out; CPU is the process's share in each `cpu-active` phase.

```
                              v0.2.8             develop
starting the GPU (A)          3.89-3.92 s        2.76-2.80 s
the first frame (A)           3.18-3.25 s        2.56-2.62 s
output A frames               136-144            139-147
output A CPU per frame        3.0-3.4 ms         3.3-3.8 ms
output B CPU per frame        2.4-2.8 ms         2.3-2.8 ms
output C median / p90         3.8-4.3 / 9.7-9.9  3.0-3.7 / 9.5-10.0 ms
cpu: rest                     3-4%               2-3%
cpu: sidebar in and out x4    3-5%               3-6%
cpu: workspace switch x4      3-4%               3-6%
cpu: osd x4                   12-17%             15%
cpu: dashboard x3             39-41%             38-42%
```

Start is a second shorter (the field pipeline variants are built when first
drawn, not all at start and again when the blend space is set). Frame cost
at this size is the same within the noise: it is spent where these changes do
not reach. Their per-frame savings are in the CPU benches
(`crates/graphics/morf-render/examples/`, `crates/core/morf-layout/examples/`):

```
layout update, 1041 nodes      28 ms        -> 1.9-3.9 ms   (kept Taffy trees)
draw list + damage, 921 nodes  3.0-4.0 ms   -> 0.49 ms      (copied subtrees)
damage diff, 921 nodes         1.04 ms      -> 0.058 ms     (copied runs)
backend frame, 921 nodes       1.11 ms      -> 0.59-0.72 ms (kept text, quads, fields)
box + shadow field             5.41/6.87 ms -> 5.25/6.60 ms (min/median, one-box pipeline)
```

Wakes per output loop at the desk layout (`tools/sandbox/wakes.steps`,
`MORF_TEST_HOOKS=1`, the `@wakes` counts): news one output asked for rings
that output's loop only. HEADLESS-A is the output the OSD and dashboard
open on; B and C keep their own rest rate (their clocks) throughout.

```
                          A            B            C
rest, 20 s                +1331        +113         +113
osd on A, 3 s             +211         +20          +20
dashboard on A, 5 s       +363         +29          +29
```

Before PLAN 4.5 every completion rang every loop, so B and C took A's
~66 wakes a second as well. In the A/B runs above, over the whole script A
was rung ~2950 times and B and C ~270 each.
