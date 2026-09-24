# examples/lib

Pure-Lua libraries a configuration can `require("lib.<name>")`. morf's core
stays compositor- and service-agnostic; anything that speaks one program's
protocol lives here, built only on the engine's generic APIs.

## hyprland.lua

Hyprland over its own two sockets in
`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`: `.socket.sock` for
requests (one per connection, `j/` for JSON, `[[BATCH]]` for several) and
`.socket2.sock` for the `EVENT>>DATA` stream. No `hyprctl` is spawned. Both
sockets are drained from one repeating `morf.timer` (`poll_ms`, default 30),
because the engine does not watch arbitrary sockets for readiness; an idle
tick blocks at most 1 ms per open socket.

```lua
local hyprland = require("lib.hyprland")

-- Reactive state, current from events; bind to it directly.
ui.Repeater { model = hyprland.state.workspaces, delegate = function(ws) ... end }
ui.Text { text = function() return hyprland.state.active_window.title end }

hyprland.on("openwindow", function(address, workspace, class, title) ... end)
local sub = hyprland.on("urgent", function(address) ... end)
sub:off()

hyprland.dispatch("workspace", 3)
hyprland.json("j/activeworkspace", function(value, err) ... end)
```

- **State** (`hyprland.state`, a `morf.state`): `monitors`, `workspaces`,
  `clients` are list models keyed by `id`, `id` and `address`;
  `active_workspace {id, name}`, `active_window {address, class, title}`,
  `focused_monitor`, `keyboard`, `keyboard_layout` (the main keyboard's),
  `submap`, `fullscreen`, `urgent` (address of a window asking for attention
  until it is focused), `connected`. Monitor rows carry their own
  `active_workspace`; workspace rows carry `windows`, `visible`, `active`,
  `urgent`. Events carrying their news are applied at once; the rest mark the
  answers they invalidate, refetched once per tick however many events came.
- **Lookups**: `monitor(name|id)`, `workspace(id|name)`, `client(address)`,
  `workspace_windows(id)`, `occupied(id)`, `monitor_workspace(name?)`,
  `snapshot()`.
- **Events**: `on(name, fn)` with parsed fields (numbers for ids, `0x`
  addresses, booleans for flags; titles and names keep their commas) for the
  documented events, `"*"` for `(name, data)`, and `"connected"` /
  `"disconnected"` for the stream. `parse_event(name, data)` exposes the
  parser. Malformed and overlong lines are logged and skipped.
- **Requests**: `request(cmd, cb(reply, err))`, `json(cmd, cb(value, err))`,
  `batch(cmds, cb(replies, err))`; queued (`max_queue`) with at most
  `max_in_flight` connections open, each with a timeout and a size bound.
- **Commands**: `dispatch(d, arg, cb(ok, reply))`, `keyword(k, v, cb)`,
  `eval(lua, cb(reply))` (`/eval`, as `hyprctl eval` sends it, for
  Lua-configured Hyprland 0.56+), `reload(config_only, cb)`, and the readers
  `binds`, `getoption`, `options(names)`, `cursor_position`, `devices`,
  `version`, `layers`.
- **Lifecycle**: starts itself just after `require` when the environment
  names an instance; `start{ runtime_dir, signature, poll_ms, ... }` in the
  same chunk chooses instead. When the stream drops it reconnects with
  backoff, and, unless a signature was pinned, follows a restarted
  compositor to its new instance directory. `available()` is false outside
  Hyprland, where every call answers `nil, "unavailable"` and the state stays
  empty. `stop()` closes everything.

Tested against a fake compositor in
`crates/morf-lua/src/tests/lib_hyprland.rs`.

## palette

`lib/palette.lua` derives a whole desk's colours from a wallpaper: the
thirteen tokens impasto uses (`background surface surfaceHover border text
textMuted accent accentHover accentText red green yellow blue`), a
sixteen-colour terminal set, and writers for the programs that read them.
It is a port of the substance of impasto's `theme_manager.py`, built on
`morf.image.palette`, `morf.color`, `morf.fs` and `morf.json`.

```lua
local palette = require("lib.palette")

palette.from_image("~/Pictures/sea.jpg", { mode = "dark" }, function(ok, p)
  if not ok then return morf.log.warn(p) end
  local dir = morf.cache_path("colors")
  palette.write.kitty(p, dir .. "/kitty.conf")
  palette.write.pywal(p, dir .. "/colors.json")
  theme.accent = p.accent
end)
```

### A palette

A table of `morf.color` values under the token names, plus `terminal`
(`color0`..`color15`, `foreground`, `background`, `cursor`, `cursor_text`,
`selection`, `selection_foreground`) and `mode`, `picked` (the swatch the
accent came from), `swatches` (the picture's most common colours, hex),
`source`, and `cached`.

### Making one

| call | what |
| --- | --- |
| `palette.from_image(path, opts, on_done)` | quantises the picture off the loop, answers `on_done(true, p)` or `on_done(false, message)` |
| `palette.derive(swatches, opts)` | the same from `{ color, fraction }` swatches, synchronously |
| `palette.from_accent(color, opts)` | as if the picture were only that colour |
| `palette.from_tokens(tokens)` | a fixed scheme's thirteen tokens made a whole palette |
| `palette.presets`, `palette.preset(id)` | impasto's nine schemes: `catppuccin_mocha`, `catppuccin_latte`, `tokyo_night`, `gruvbox_dark`, `nord`, `rose_pine`, `cyberpunk`, `bauhaus`, `crimson` |
| `palette.blend(a, b, t)` | every colour mixed in OkLab, for animating between two |
| `palette.check(p)` | every contrast floor, measured: `report, all_ok` |
| `palette.to_hex(p)`, `palette.from_hex(t)` | to and from plain hex strings |

Options: `mode` (`"dark"` by default, `"light"`, or `"auto"` from the
picture's lightness), `accent` (use this instead of picking), `fallback_accent`
(`#89b4fa`), `population_weight` (1), `min_population` (0.004), `hue_pull`
(0.25), `hue_shift` (12 degrees), `cyan_is_accent` (true), `count` (16
swatches), `cache_dir` / `cache = false`.

### How the colours are chosen

- **Accent.** impasto's rule: among swatches with HSV saturation over 0.2
  and luma between 0.2 and 0.85, the highest `2*sat + (1 - |luma - 0.55|)`;
  added to it are `3 * OkLCh chroma` (so a near-black navy does not count
  as saturated) and `sqrt(share of the picture)` (so a speck of neon does
  not beat the sky). Failing that, the most saturated swatch over 0.1;
  failing that, the neutral accent. Its hue is kept; its lightness is
  brought into 0.64..0.84 (dark) or 0.42..0.62 (light) and its chroma into
  0.07..0.2.
- **Grounds** are near-black (OkLCh L 0.18 / 0.215 / 0.265 / 0.33) or
  near-white, tinted towards the accent's hue by a fifth of its chroma.
- **Type** starts near-white (or near-black) and is lifted until text reads
  at 7:1 on the background and 4.5:1 on the surfaces, muted text 4.5:1.
- **accentText** is the ground's dark or a near-white, whichever reads
  better on the accent; if neither reaches 4.5:1 the accent moves instead.
  `accentHover` is a step further from `accentText`, held to the same floor.
- **Semantic colours** start at recognisable OkLCh hues (red 25, green 145,
  yellow 92, blue 255) and are pulled a quarter of the way towards the
  accent's hue, never more than twelve degrees; their chroma follows the
  picture's own vividness (half to full), and each is lifted to 4.5:1.
- **Terminal**: normal slots 1..6 at 4.5:1 on the terminal background,
  bright slots a step further and at 7:1 (impasto's `lift` and
  `brighten`); cyan carries the accent (impasto's choice; `cyan_is_accent =
  false` for a real cyan); magenta is the red turned to 330 degrees.
  On a light ground `color0` is the ink and `color15` the paper.

Lifting moves only OkLCh lightness, keeping hue, and giving up chroma only
where sRGB cannot show it. Every floor is listed in `palette.FLOORS` and
checked by `palette.check`. The same image and options always give the same
palette.

### Caching

`from_image` caches by path, modification time, size and options, as JSON
in `opts.cache_dir` (default `morf.cache_path("palette")`). A cached
palette is answered before `from_image` returns. A fresh one is answered a
few loop turns later: the derivation, the cache write and the caller each
run in a turn of their own, because each handler has a fuel budget.

### Writers

`palette.build.<name>(p)` returns the file as a string; `palette.write.<name>(p,
path)` writes it atomically and returns `true` or `nil, message`. Names:
`kitty` (an include), `foot` (an include), `alacritty` (TOML import), `btop`
(a theme file), `cava` (its `[color]` section), `gtk3`, `gtk4`, `gtk`
(`@define-color` lines), `json`, `lua`, `pywal` (`colors.json`).

Nothing is ever reloaded. `palette.reload_hints.<name>` says what each
program needs (`signal`, `process`, `command`, `note`) for a configuration
that wants to do it itself.

### Templates

```lua
palette.render("accent = {{accent}}\nbar = {{surface | mix accent 0.2 | strip}}\n", p)
palette.write.template(text, p, path)
```

`{{name}}` is a token, a `terminal.<slot>` or a field; colours are written
`#rrggbb` unless a format follows: `.hex .strip .hexa .xhex .rgb .rgba .css
.hsl .oklch .r .g .b .name`. Filters follow a bar and chain: `lighten n`,
`darken n`, `saturate n`, `desaturate n`, `rotate deg`, `alpha a`, `mix
<colour or token> t [space]`, `complement`, `invert`, `gray`, `text_color`,
any format name, `upper`, `lower`. An unknown name or filter raises.
