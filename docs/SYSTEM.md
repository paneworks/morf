# How morf is built

morf is one Rust workspace in five groups. A crate may depend on crates of
its own group or of the groups above it in this list, and only on the morf
crates its row names; `tools/layers.py --strict` (part of `make verify`)
fails on anything else.

```
group      crate          owns                                              may use
core       morf-value     values that cross every boundary: IpcValue,       (nothing)
                          Color and HCT, input regions, the accessible
                          tree, the renderer/window present contract
core       morf-scene     the scene graph, element properties, signals and  value
                          effects (reactive), retention, animation state
core       morf-layout    measuring and placing nodes                       scene value
graphics   morf-text      shaping, fonts, glyph rasters                     scene layout vector value
graphics   morf-vector    outlines and SVG                                  image value
graphics   morf-image     decoding, the image cache, XDG data dirs          value
graphics   morf-render    painting a laid-out scene with wgpu               scene layout text vector
                                                                            image value
platform   morf-app       our own windowing: window kinds, one event type,  value
                          outputs, input, cursors; Wayland and headless
                          backends behind the Backend trait
platform   morf-desktop   the desktop protocols on their own queue:         app value
                          capture, gamma, data-control clipboard,
                          workspaces, foreign toplevels, idle, output power
platform   morf-io        processes, files, sockets, D-Bus, HTTP            value
platform   morf-audio     PipeWire                                          io value
platform   morf-terminal  terminal emulation                                io scene value
platform   morf-system    services, desktop entries, menus                  io image value
engine     morf-shader    Lua-syntax shaders to WGSL                        value
engine     morf-runtime   what the engine does with no Lua: handlers,       scene layout text app value
                          timers, the reactive scheduler, events, gestures,
                          shortcuts, focus, wake causes, window
                          declarations and platform requests
engine     morf-kit       the widget archetypes                             value
engine     morf-lua       the Lua bindings (the morf.* tables) and the VM   everything above
frontend   morf-host      running a configuration: live windows, the loop,  everything above
                          the lock screen, the supervisor and workers,
                          capture, a11y, the app mode, the headless runner
frontend   morf-cli       the command line, the runners, the test host      host value
```

Three more rules hold everywhere: nothing below morf-lua names `luna`;
nothing outside morf-app and morf-desktop (and the frontend) names a
Wayland crate; and no core crate knows a compositor, a network manager or a
distribution -- those live in Lua libraries a configuration chooses.

## A turn of the loop

One `Host` (morf-host) runs an output: it owns the backend (any
`morf_app::Backend`), the desktop protocols where there is a compositor,
the live windows and what they are painted with. `Host::start` opens the
shell's surface, waits for its first configure, paints the first frame and
opens the declared windows; then the driver alternates `Host::wait` (sleep
on the backend until something is due) and `Host::turn`. The supervisor of
a live shell is reached through `Links`; a headless runner has none.

A backend (morf-app) delivers one `Event` type for every window: configures,
frames, pointer, keys, touch, drags. morf-host routes each to the window it
names (`Windows`, one map keyed by `WindowId`) and hands input to the
runtime. Handlers run; their writes mark effects dirty in the reactive
graph, and one flush re-runs them. What a handler asked of the platform --
a capture, the clipboard, gamma, a drag -- is queued as a request and
carried out by the host against morf-app or morf-desktop. The host lays the
scene out and paints the windows that owe a frame; morf-render draws into
the `RenderTarget` the backend handed out, presenting dmabufs through the
window's `BufferSink` where it can.

morf-desktop shares the Wayland connection but keeps its own event queue
and registry: smithay's toolkit implements every handler on one state type,
and a state type of another crate cannot be given them. Any read of the
socket fills its queue too, so the host dispatches it after each wake.

## Handlers

The runtime never holds a Lua closure. It holds `Handler`s: shared handles
whose last clone releases what they stand for. morf-lua keeps the closures
behind them (its handler store) and runs a handler when the runtime's code
calls for one.

## Headless

`morf check`, `render` and `test` run the same `Host` on morf-app's headless
backend: virtual outputs that configure every window opened, a virtual seat
that carries a test's clicks and keys to the host's own pointer and key
paths, and a clock that moves only when told (with the runtime's virtual
one), so the same spec fires the same timers and frame callbacks every run.
With no GPU the host's `Painter` is a text system: every window is laid
out, observed and given its input region and frames, and nothing is drawn;
`morf render` draws a surface on demand. What the host has not opened (a
hidden panel) the runner lays out itself, so `morf check` finds its
problems too.

## Lua

`library/lib` holds the shared Lua: `kit/` (the widget kit), `services/`
(system bindings any shell may use), `integrations/` (opinionated, opt-in:
Hyprland, weather, ...), `util/` and `testing/`. `library/types` is
generated by `morf types`.

Installation: [INSTALL.md](INSTALL.md). Writing UI: [UI.md](UI.md).
Testing: [TESTING.md](TESTING.md).
