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

# Frame stalls (PLAN item 2, steps 1-2)

The rehearsal (`frame_stall` in `crates/engine/morf-lua/tests/engine/`): a
spring (`stiffness = 120, damping = 14`, `x` to 300) ticked at 16ms steps,
with an IPC handler that burns a fixed Lua loop fired once per frame before
the tick, as a host turn runs handlers before it paints. Release build:

```
spring settled            frame 114 of 150, stalled and unstalled alike
busy handler              3.60 ms/frame (120k-iteration loop, release)
                          66 ms/frame in a debug build, same loop
frame budget at 60Hz      16.7 ms; the accounting overrun line is 2.4x that
```

So a slow-but-legal handler is additive turn latency -- the spring keeps
its virtual-time schedule, but every frame's paint waits behind the
handler in the same turn. The cost lands in the `handlers` stage of the
new frame-time accounting (`host/turn/stats.rs`, surfaced with the motion
report, counters reset on report), not in paint.

# Motion-only fast path (PLAN item 2, steps 3-4)

Landed in `host/turn.rs`: backend events are drained up front, and a turn
whose events are only `Event::Frame`, with no supervisor commands and no
repaint debt, handles frame callbacks before `poll_services()`; every
other turn keeps the original ordering. Cage-sandbox rehearsal
(`tools/sandbox/nested.sh`, release build): a spring box (`x` 100 -> 1500)
beside a 16 ms timer burning a 900k-iteration Lua loop filmed 90 frames
in 5.31 s (about 59 ms a frame, handler-bound), the spring completed its
travel, an IPC `ping` during the film answered `pong`, and a separate
run clicking the counter demo incremented it. No wakeup was lost; frame
rate under this handler remains bounded by the handler itself, which the
accounting attributes to the handlers stage, not paint. Splitting Lua
off the host thread stays future work, informed by these numbers.

# Shared blur layers (PLAN item 6, step 3)

The scene: `examples/demos/render/blurred-cards.lua`, sixty cards on a
1920x1080 desk, each card a layer blurred by 4 holding two texts and a
bar. The bench draws the whole frame from nothing again and again
(`RenderEngine::forget` before each) and times it to the end of its
submission and to the GPU finishing it (`wait_idle`):

```
cargo run --release -p morf-cli --example frame_bench -- \
    examples/demos/render/blurred-cards.lua gpu-time 1920x1080 60
```

Adapter: llvmpipe (LLVM 21.1.8, 256 bits) through `nixVulkan`, the only
Vulkan driver the dev shell sees on this machine. Release builds of
8bbac48f (one target per blurred layer) and cfb8a785 (siblings blurred by
the same radius share one), five rounds alternating the two, 60 frames
each, while other builds kept the 24-core machine at a load average of
20-31: the fastest frame is the closest to the work itself, and the
medians carry the load.

```
                     one target per layer    one shared target
submitted, fastest   3.29-3.70 ms            1.10-1.23 ms
finished, fastest    74.9-87.0 ms            25.7-31.8 ms
finished, median     90.9-208.5 ms           32.8-133.2 ms
```

Per layer the frame took a target, a blur chain of four more, a content
pass and four blur passes, four uniform buffers and five bind groups:
300 offscreen passes for the sixty cards. Shared, the sixty are packed
apart in one target, drawn in one pass and blurred by one chain -- five
passes -- and composited back by sixty blits. The two pictures agree to
within 2 of 255 on every channel of every pixel (34,607 of 2,073,600
pixels differ at all), and `gpu/layer_share_tests.rs` holds a shared
target to what each layer's own target draws.

The glass material (step 2) adds three properties paint could read on
every `Rect`; it reads them only on a node one of them has moved on
(`Scene::may_have_glass`). Against a build that never reads them, the
CPU bench's draw list for the cards and for the glass cases stays within
the noise: 124.0-128.1 against 126.9-127.3 µs, and 18.7-18.9 against
19.5-19.8 µs (`DrawList (reused)`, three quiet rounds of four).
