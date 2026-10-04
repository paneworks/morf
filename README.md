<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo-light.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/logo-dark.svg">
    <img src="docs/logo-dark.svg" alt="morf" width="520">
  </picture>
</p>

<p align="center"><em>Rendering and shell engine implemented in Rust and configured in Lua</em></p>

It exposes native scene, layout, rendering, input, surface, IO, and service primitives through Rust and Lua APIs. Widgets and complete shells are downstream projects.

The `morf-lua` crate embeds [Luna](https://github.com/onix-os/luna) as a bounded configuration and extension interface. Built-in engine modules are preloaded by Rust; morf does not ship a Lua implementation tree.

```
crates/
  core/      morf-value  morf-scene  morf-layout
  graphics/  morf-text  morf-vector  morf-image  morf-render
  platform/  morf-app  morf-desktop  morf-io  morf-audio  morf-terminal  morf-system
  engine/    morf-shader  morf-runtime  morf-kit  morf-lua
  frontend/  morf-host  morf-cli
```

What each crate owns and may depend on: [`docs/SYSTEM.md`](docs/SYSTEM.md).

```sh
oslo make build
oslo make run
oslo make test
oslo make verify
```

Run a configuration directly with:

```sh
cargo run --package morf-cli -- shell.lua
```

Run the interactive transformation example with:

```sh
EXAMPLE=examples/demos/motion/fluid-transform.lua oslo make run
```

Clicking its shape animates square-to-circle radius, color, origin-aware non-uniform scale, skew, rotation, shadows, and spring translation. The animation clock and interpolation remain in Rust; Lua only changes targets.

Run the combined animation, polygon morph, and signed-distance-field example:

```sh
EXAMPLE=examples/demos/sdf/morph-stack.lua oslo make run
```

Animato advances the native tween and spring state — the timing — and signeddistance fields decide what a frame looks like — the view. Analytic fields are composed and morphed in the fragment shader; reusable raster masks are converted to cached distance fields. Morf still owns the compositor frame clock.

Use `--no-plugin` to load the configuration without auto-sourced plugins. Use `--clean` to also exclude external Lua roots; modules beside the selected config remain available.

An infinite Lua loop is terminated when its fuel budget is exhausted rather than hanging the process.

## Writing UI

How nodes are sized and placed, how state reaches them, and what makes a frame: [`docs/UI.md`](docs/UI.md). The demos under `examples/demos/` are the runnable versions of each section; whole shells are under `examples/shells/` (see [`examples/README.md`](examples/README.md)).

## Testing a configuration

`morf check`, `morf render` and `morf test` run a configuration with no compositor: nothing connects to Wayland and time is virtual. `check` loads it, lays out every surface and reports Lua errors and lint; `render` draws a surface to a PNG with the real GPU renderer; `test` runs Lua spec files that click, type, advance time and call IPC. See [`docs/TESTING.md`](docs/TESTING.md) and the specs in each shell's `tests/`, `examples/demos/tests/` and `library/tests/`.

```sh
morf check shell.lua --strict
nixVulkanIntel morf render shell.lua -o shell.png --surface screen
morf test examples/demos/tests/counter_spec.lua
```
