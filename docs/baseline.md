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
                              v0.2.8                develop
starting the GPU (A)          3.60-3.63 s           2.55-2.56 s
the first frame (A)           2.95-3.04 s           2.44-2.47 s
output A frames               142-145               161-168
output A CPU per frame        2.89-3.04 ms          2.01-2.18 ms
output A median / p90         1.47-1.70 / 4.4-5.1   0.71-0.75 / 3.9-4.0 ms
output B CPU per frame        2.19-2.76 ms          1.31-1.69 ms
output B median / p90         1.09-1.20 / 2.4-2.7   0.50-0.56 / 0.93-0.95 ms
output C CPU per frame        5.22-5.53 ms          4.88-5.15 ms
output C median / p90         4.4-4.8 / 8.9-9.4     2.5-3.2 / 8.7-8.8 ms
cpu: rest                     3-4%                  4%
cpu: sidebar in and out x4    3-5%                  2-4%
cpu: workspace switch x4      2-5%                  3-4%
cpu: osd x4                   11-12%                13-14%
cpu: dashboard x3             42-44%                38-40%
```

Start is a second shorter: the field pipeline variants are built when first
drawn, not all at start and again when the blend space is set. A frame is
about a third cheaper on the outputs with something on them and about half on
the quieter ones: at this size the per-frame cost was the walks every frame
repeated whether or not anything had moved -- every text input shaped and
observed again, and the input region found again from the whole tree. Text
inputs now sync only when focused or when their paint stamp or box moved
(487-506 ms over a run, now 25-35 ms), and the input region is kept until
an input stamp moves, which only transforms, stretches and layout move
(516-556 ms over a run, now 316-326 ms). The savings in layout, the draw list
and the damage diff (PLAN 4.1, 4.2) are in the CPU benches
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

Each PLAN item on its own at the desk layout: the commit before it and the
commit that adds it, the same script (`cpu-active.steps`, `cpu-idle.steps`
for 4.3) with `MORF_FRAME_LOG=2 MORF_FRAME_SPLIT_MS=0`, two runs each; stage
totals over a run, CPU per frame on each output, the first frame left out.

```
item                         before                       after
4.2 kept Taffy trees         layout 294-297 ms            layout 288-300 ms
  9f4a5a3b -> 286a502b       A 2.92-3.00 ms/frame         A 2.79-3.28 ms/frame
4.1 copied commands, damage  render 1917-2067 ms          render 1651-1752 ms
  286a502b -> 543f4dc7       C 5.32-5.45 ms/frame         C 5.04-5.17 ms/frame
4.1 kept fields and quads    render 1632-1724 ms          render 1593-1744 ms
  924ead69 -> a4fa4375       C 4.98-5.03 ms/frame         C 5.02-5.13 ms/frame
4.4 one-box field pipeline   render 1748-1769 ms          render 1576-1623 ms
  a4fa4375 -> 22d2998b       C 5.06-5.23 ms/frame         C 4.86-5.09 ms/frame
4.3 capped text caches       rss 923-929 MB               rss 927-929 MB
  2c15f150 -> 7ae5207d       (idle script)
4.5 wakes by asker           every completion rang        A ~2950, B and C ~270
                             every output's loop          each over the A/B run
```

At this size caelestia's layout is mostly not Flex or Grid, so the kept Taffy
trees (4.2) change little; they pay in the layout benches. Copying unchanged
commands and diffing only between copied runs (4.1) take about a seventh off
the render stage, the one-box pipeline (4.4) about a tenth; keeping field
instances and glyph quads saves what its bench said, ~0.1 ms a frame, inside
the noise here. The text caches (4.3) never reach their caps in a short
script; the caps bound a long session's memory, which their unit tests
cover.

# Frame stalls (PLAN item 2)

Taken on a branch off `develop` (cce07898), 2026-10-16, release builds, on
an Intel Core i7-13700KF with an RTX 4080 and no integrated GPU, Ubuntu
26.04. The machine was building other branches all the while (load average
13-25 on 24 threads), so every figure is the range over three rounds of 6 s,
the two builds alternated phase by phase under the same load. Before is the
tree just ahead of the fast path (50b37e6c) with the accounting's refresh
fix (0f572786) applied; after is that fix's commit.

## The rehearsal and the accounting

`examples/demos/motion/frame-stall.lua`: a 120 px box on a spring
(`stiffness = 90, damping = 11`), swung between the ends of a strip every
700 ms so it is always moving, and a timer every `--interval` ms that burns
`--budget` Lua instructions, split over as many timers as a handler's 1M
budget needs, all due together. An instruction costs about 9 ns here:
1.3M is 10-13 ms a firing, 2.8M 23-29 ms, 4.5M 36-39 ms, 67M 600-650 ms
(the handlers stage over the firings in each run).

`morf check --frames` runs it three ways: virtual (the headless runner's
clock, frames back to back: what a frame costs, stage by stage); `--paced`
(the wall clock, a refresh every 16.7 ms, each frame callback delivered at
the first refresh after the paint that asked for it, paints held back
until then, as a compositor holds them); and `--paced --gpu` (the same,
painting with wgpu offscreen on the RTX 4080 through nixGL's
`nixVulkanNvidia`). The frame accounting (`host/turn/stats.rs`) reads a
frame as paint to paint while motion carries on, late past 1.5x the
refresh interval, an overrun past 2.4x, each overrun owned by the stage
that held most of it: services, handlers, flush, motion, layout, paint,
or waiting (no stage: the loop asleep, or the runner between turns).

The cage sandbox (`tools/sandbox/frame-stall.steps`, one `@frames` read a
phase) does not run on this machine. With no integrated GPU cage needs
NVIDIA's GL (a nixGL wrapper gets it to GLES on the 4080), but the nested
Hyprland then fails to allocate buffers for its headless outputs
(`GBM: Failed to allocate a GBM buffer: bo null`) and the shell's layer
surface is never configured. The paced GPU run stands in for it here; the
steps file is for a machine with an Intel or AMD render node.

## What a stall costs, before the fast path

Virtual, before (50b37e6c), per 6 s: every millisecond of the stall is
charged to handlers, and the flush after the handler stays under 0.1 ms in
every run of every mode, so the flush does not dominate and the plan's
narrower variant (a bounded flush) does not apply.

```
stall a firing          frames  mean / worst ms     handlers ms   flush  paint
none                    332     0.0 / 0.0           0.0           0.0    0.3-0.4
12 ms every 16 ms       332     12.0-12.6 / 22-38   3971-4182     0.0    1.5-1.7
600 ms every 2 s        332     5.7-5.9 / 649-703   1890-1949     0.0    0.5-0.6
```

The premise check: 600 ms stalls (67M every 2 s) make frames of 627-1069
ms, two overruns in every 6 s, each owned by handlers, in all three modes.
Frames do not hold at refresh, so the premise stands.

## Before and after the fast path

Mean and worst frame (ms), late frames and overruns, per 6 s:

```
paced                   before                           after
stall a firing          mean       worst    late  over   mean       worst    late  over
none                    16.6       17-27    0-1   0      16.6       17-20    0     0
12 ms every 16 ms       16.7-17.8  51-79    14-15 1-3    16.8-17.5  44-84    13-16 1-2
25 ms every 100 ms      18.6-20.4  60-98    53    52     18.3-18.4  45-67    40-45 28-30
40 ms every 100 ms      22.6-22.9  84-110   53    53     22.4-23.0  61-81    53    39-45
600 ms every 2 s        21.4-22.2  627-703  2     2      21.7-22.2  697-757  2     2

paced, GPU              before                           after
none                    16.7       34       1     0      16.7       34       1     0
12 ms every 16 ms       16.8-17.9  70-81    16-27 1-3    16.8-17.5  36-86    15-16 0-2
25 ms every 100 ms      18.3-21.0  78-133   53    52     18.3-21.3  49-77    42-53 28-37
40 ms every 100 ms      23.2-29.7  75-173   52-53 51-53  23.2-25.3  95-98    53    44-51
600 ms every 2 s        21.5-25.0  636-1069 3     2      21.3-23.7  606-858  3     2
```

(The one late GPU frame unstalled is the run's first, which builds the
pipelines.) A 25 ms stall every 100 ms makes 29-46 % fewer overruns and a
40 ms one 4-26 % fewer, mostly with lower worst frames: a frame whose
callback is already due is drawn before the timer runs instead of after
it. Stalls under a refresh change nothing (they never overran),
and every stall still makes the frame after it late: the loop that runs the
handler is the loop that paints.

Against the plan's bar: mean frame time stays within one refresh of the
unstalled run (16.6 against at most 25.3 ms) for every stall up to 40 ms a
firing, before and after alike; worst frames do not (45-98 ms against
17-34), and a 600 ms stall is a 600-860 ms frame either way. That shortfall
is the follow-up, splitting logic evaluation off the host thread, with these
numbers as its baseline.

## Without losing a wakeup

`crates/frontend/morf-cli/src/tests/motion_turn.rs` drives the real host on
the headless backend, paced: a timer due on a motion frame fires in that
turn after its paint and is painted on the next callback; a timer's colour
change after a still frame is painted by the turn after (dropping the
post-paint repaint fails this test; dropping the fast path fails four);
a click during the motion takes the full turn and the spring goes on; a
service thread ringing takes the full turn; and the spring's path frame
by frame is the same paced and unpaced. `examples/demos/tests/
frame_stall_spec.lua` swings the spring with and without 1M instructions a
frame: the path is the same frame for frame, the frames went through
motion-only turns, handlers hold over ten times paint and the flush, no
overrun is paint's, and a calibrated tenth-of-a-second stall is an overrun
owned by handlers. It fails against the build without the fast path.
