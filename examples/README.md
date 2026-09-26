# Examples

Two kinds: **shells**, whole desktops you can run as your own, and **demos**,
one file each, showing one thing the engine does. Both use the Lua library in
[`../library`](../library), which `require("lib.…")` finds from here without
installing anything.

```
examples/
  shells/NAME/         a whole shell, laid out as ~/.config/morf/NAME is
    shell/init.lua     the shell (and lock/, greet/ when it has them)
    tests/             its specs
    README.md
  demos/TOPIC/*.lua    one-file demos: sdf, motion, render, text, desktop
  demos/assets/        the demos' pictures
  demos/tests/         the demos' specs, and specs of the engine's own behaviour
```

The library's own specs are in [`../library/tests`](../library/tests).

## Running

```sh
morf examples/demos/sdf/sdf-blobs.lua              # a demo
morf examples/shells/caelestia/shell/init.lua      # a shell, from here
morf test examples/demos/tests/counter_spec.lua    # a spec
morf check examples/shells/impasto/shell/init.lua  # load it headless, report errors
```

To make a shell yours -- copied to `~/.config/morf/NAME` and run by a bare
`morf` -- after `make install`:

```sh
make apply --example caelestia     # then: morf, or morf -c caelestia
```

## Shells

| | |
|---|---|
| [`caelestia`](shells/caelestia) | a clean-room port of caelestia-dots/shell: a frame, drawers out of its edges, pills down its sides |
| [`chillpill`](shells/chillpill) | a port of ChillPill-Shell, the pill bar for Hyprland |
| [`impasto`](shells/impasto) | a port of andreumassanet/impasto: an island, a dock, a desk |
| [`panacea`](shells/panacea) | a port of Panacea: one capsule at the top edge is the whole shell |

## Demos

**sdf** -- distance fields: shapes that blend, morph and melt.
`sdf-gallery`, `sdf-foundation`, `sdf-field`, `sdf-compound`, `sdf-drawings`,
`sdf-glyph-morph`, `sdf-loaders`, `sdf-metaballs`, `sdf-blobs` (and its
`-blur`, `-chroma`, `-crt`, `-mouse` variants), `sdf-gravity`, `morph-stack`,
`m3shapes`, `path`, `masks`, `blend-compare`.

**motion** -- tweens, springs, keyframes and forces.
`motion-lab`, `keyframe-lab`, `physics-lab`, `transform`, `fluid-transform`,
`exit`, `drawers`.

**render** -- shaders, colour and the compositor.
`shader-gallery`, `lua-shader`, `crt-terminal`, `frosted-panel`,
`capture_gpu`, `material`, `palette`.

**text** -- type, input and terminals.
`font_axes`, `text-input`, `keyboard`, `terminal`, `fzf_launcher`.

**desktop** -- the pieces of a desktop.
`notifications`, `tray`, `audio`, `clipboard-history`, `overview`, `polkit`,
`greeter` (a login and lock screen), `crash` (what is shown when the shell
dies).
