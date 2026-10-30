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
  graphics/  morf-text  morf-vector  morf-image  morf-paint  morf-render
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

Processes, files, sockets and every `morf.<namespace>`: [`docs/IO.md`](docs/IO.md). Coming from 0.5: [`docs/MIGRATING.md`](docs/MIGRATING.md).

## Testing a configuration

`morf check`, `morf render` and `morf test` run a configuration with no compositor: nothing connects to Wayland and time is virtual. `check` loads it, lays out every surface and reports Lua errors and lint; `render` draws a surface to a PNG with the real GPU renderer; `test` runs Lua spec files that click, type, advance time and call IPC. See [`docs/TESTING.md`](docs/TESTING.md) and the specs in each shell's `tests/`, `examples/demos/tests/` and `library/tests/`.

```sh
morf check shell.lua --strict
nixVulkanIntel morf render shell.lua -o shell.png --surface screen
morf test examples/demos/tests/counter_spec.lua
```

## Release tarballs

The `morf-linux-*.tar.gz` assets on a GitHub release hold only the `morf`
executable and its Lua library. They do not include the libraries morf opens
at run time (Vulkan loader, PipeWire, PAM, udev); those come from the host
distribution. For a complete, self-contained install on any distribution, use
the Nix packages below: `.#morf` wraps the engine with every runtime library.

## Nix packages and binary cache

The flake provides Linux x86-64 and ARM64 packages:

- `morf` (also `default`): the engine, with its matching Lua library available
  automatically through the package wrapper.
- `morf-library`: the Lua modules, generated LuaLS definitions, and editor
  configuration template, without the engine executable or its runtime closure.

Both outputs come from the same build and version. The definitions are generated
by that engine rather than copied from potentially older checked-in files.

```sh
oslo make nix-build
oslo make nix-check

nix build .#morf
nix build .#morf-library --no-link --print-out-paths
```

After a release's cache workflow succeeds, replace `vX.Y.Z` below with that tag:

```sh
nix build github:paneworks/morf/vX.Y.Z
nix run github:paneworks/morf/vX.Y.Z -- --help
nix build github:paneworks/morf/vX.Y.Z#morf-library
```

The flake advertises `https://cache.bresilla.com/bresilla` and its public signing
key; direct users can accept them with `--accept-flake-config`. When using Morf
as an input, configure the cache on the consuming machine or top-level flake:
an input's cache settings are not automatically inherited. In `/etc/nix/nix.conf`:

```ini
extra-substituters = https://cache.bresilla.com/bresilla
extra-trusted-public-keys = bresilla:zsecaZYIrjCJQS4AuGPmEVU5PRkiMmB/Mds3KaCzTvE=
```

For another flake:

```nix
inputs.morf.url = "github:paneworks/morf/vX.Y.Z";
```

Use `inputs.morf.packages.${system}.morf` for the engine, or
`inputs.morf.packages.${system}.morf-library` for downstream Lua consumers.
The library root is `${morfLibrary}/share/morf/library`, where `morfLibrary`
is the latter package. It contains `lib/`, `types/`, and `luarc.template.json`.
Add that root to `MORF_RUNTIME_PATH` when explicitly selecting a library for
another Morf host; add the root and its `types/` directory to LuaLS's
`workspace.library`. The packaged engine already finds its bundled library.

The Lua modules use Morf's native APIs; this is not a replacement engine or a
standalone Lua interpreter package. Keep the engine and library versions aligned.
The library can be downloaded independently; building it from source also
compiles the engine needed to generate its definitions.

The default kit's named icons require the **Material Symbols Rounded** font on
the host. CI provisions a checksum-pinned test copy through `tools/test-fonts.sh`
and `FONTCONFIG_FILE`, without installing fonts into the system.

### NixOS integration

This flake provides the engine and its Lua library: `packages.morf` and
`packages.morf-library`. The desktop integration (greetd, the lock screen, idle,
phone gestures, scaling and wallpapers shared with the greeter) is not part of
this flake; it belongs to the system configuration that installs a shell.

### Cache publishing and retention

Only pushed `v*` tags publish, not ordinary branch pushes or pull requests.
CI builds natively on both architectures; the shared
[`attic-push`](https://github.com/bresilla/nixos/tree/main/.github/actions/attic-push)
action uploads only the paths cache.nixos.org does not have and sets the pins,
then both outputs are checked on fresh runners with all builders disabled.
Installed Lua tests run outside the checkout with isolated user directories and
D-Bus disabled.

Stable pins, listed at <https://cache.bresilla.com/-/pins>, keep only the pinned
build each:

```text
morf-x86_64-linux
morf-aarch64-linux
morf-library-x86_64-linux
morf-library-aarch64-linux
```

Package versions remain in the immutable store paths; there are no version-named
pins or `latest` aliases. Builds that are no longer pinned are removed from the
cache and may need rebuilding.

Repository setup requires the organization Actions secrets `ATTIC_TOKEN` (an
Attic token that may push to the `bresilla` cache) and `PIN_TOKEN` (the cache's
pins API token); `NETBIRD_SETUP_KEY` is only needed for uploads over 95 MB. Do
not publish these tokens.
