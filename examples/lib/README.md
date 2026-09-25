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
- **Outputs**: `monitors` asks `j/monitors all`; the lit ones are the
  `monitors` model, and every one, disabled included, is `state.outputs`
  (keyed by `name`) and `outputs()`. Rows carry `serial`,
  `available_modes`, `vrr`, `mirror_of` and `dpms` too. `on("refreshed",
  fn(kind))` hears each refetched answer once it is in the state.
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

## hyprland_config.lua

Opt-in, on top of `hyprland.lua`: changes Hyprland's configuration at run
time and never writes its files. Works with a Lua config (0.56+, every
change an `eval` of `hl.*` chunks) and a hyprlang one (`keyword` commands in
one batch); `flavour(cb)` finds out which once, by evaluating a chunk that
does nothing.

Each change is a *plan* built by a pure, validated function, then sent:

```lua
local config = require("lib.hyprland_config")
config.apply(function(how)
  return config.options_plan({ { "input:kb_layout", "str", "us,de" },
                               { "input:repeat_rate", "int", 30 } }, how)
end, function(ok, replies) end)
```

- `options_plan(list, how)`, `animation_plan(spec, how)`,
  `monitors_plan(rules, how)` (disabled outputs first unless one is also
  lit, never through zero lit outputs), `monitor_rule(rule, how)`,
  `rehome_plan(lit, workspaces, home, how)` (workspaces stranded on a gone
  output, empty workspaces shown beside full ones), `cursor_plan(theme,
  size)`; each returns `nil, why` for anything invalid.
- `run(plan, cb)`, `apply(build, cb)`; `sent` keeps the last commands.
- `outputs()` (described: `mode`, `position`, `scale`, `mirror`, `vrr`,
  `modes`, `resolutions`), `parse_modes`, `group_modes`, `spell(bind)`,
  `plugin_available(option, cb)`, `reload(cb)`, `on_reload(fn)`.
- Without Hyprland, `available()` is false and nothing is sent.

## m3shapes

`lib/m3shapes.lua` draws Material 3's expressive shapes (cookie, clover,
sunny, burst, gem, pill, arch, heart, ...: `shapes.NAMES`) as `ui.Path`
outlines. Each is a polygon with per-corner rounding, cut into the same
number of cubics (`shapes.SEGMENTS`, 72) starting at the top, so
`morph_to` walks any one onto any other.

```lua
local shapes = require("lib.m3shapes")
ui.Path { width = 48, height = 48, view_box = { 0, 0, 100, 100 }, d = shapes.path("cookie9"), fill_color = accent }
shapes.Shape { width = 96, height = 96, shape = function() return which:get() end, color = accent, easing = "out_back" }
```

| call | what |
|------|------|
| `shapes.path(shape, { size, segments })` | SVG path data in a `size` square (100) |
| `shapes.Shape(props)` | a `ui.Path` that morphs when `shape` (a name or a function returning one) changes; `color`, `duration` (350), `easing`; other props pass through |
| `shapes.polygon(vertices, { rounding })`, `shapes.star(points, inner, opts)`, `shapes.regular(sides, opts)`, `shapes.lobes(count, inner, opts)` | outlines of your own, for `path` and `curves` |
| `shapes.curves(shape, segments)` | the normalised cubics themselves |

Outlines are made once per name and kept (a few milliseconds each).
`examples/m3shapes.lua` shows every one.

## material

`lib/material.lua` makes Material 3 colour schemes over `morf.color`'s HCT:
five tonal palettes placed around a source colour's hue, and every role
(`primary`, `onPrimaryContainer`, `surfaceContainerHigh`, `outlineVariant`,
the `*Fixed` roles, ...) at its tone for dark or light. This is what an M3
shell's colour tool does, in Lua.

```lua
local material = require("lib.material")
local s = material.scheme("#4a7fb5", { variant = "tonal_spot", mode = "dark" })
s.primary  s.surfaceContainer  s.palettes.tertiary(70)
material.from_image("~/Pictures/sea.jpg", { mode = "light" }, function(ok, s) ... end)
```

| call | what |
|------|------|
| `material.scheme(source, opts)` | every role as a `morf.color`; `source` is a colour or a hue. `opts.variant`: `tonal_spot` (default), `vibrant`, `expressive`, `neutral`, `monochrome`, `fidelity`, `content`, `rainbow`, `fruit_salad`; `opts.mode`: `dark` or `light` |
| `material.palettes(source, variant)` | the six tonal palettes alone |
| `material.score(swatches, opts)` | a picture's best source colours, best first (a colour's share and its neighbours' within 15 degrees of hue, and its chroma; greys do not count; `#4285f4` when nothing has colour) |
| `material.from_image(path, opts, on_done)` | quantised off the loop, scored, and made a scheme: `on_done(true, s)` or `on_done(false, message)` |
| `material.hex(s)` | the roles as `#rrggbb` |
| `material.terminal(s)` | sixteen terminal colours and extras, in `lib/palette.lua`'s shape, for its writers |

The palette rules and the score follow Material Color Utilities
(Apache-2.0). There is one contrast level, the standard one.

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

## System services

Five services a shell's panels need, each one module, each exposing a
`morf.state` whose fields and lists bindings read directly, kept current by the
service's own D-Bus signals. When a service is not running, `available()` is
false and every list is empty; nothing raises. None of them ever *starts* a
service: an absent name is read as absent, never activated, because reading
NetworkManager or power-profiles-daemon into existence changes how the machine
runs.

Call `connect()` while the configuration loads: the state is made of signals,
and signals are declared up front. Every action returns a truthy value on
success and `nil, reason` on failure.

All five take the same test seam, `dbus = <table with a proxy function>`,
defaulting to `morf.dbus`. The tests in
`crates/morf-lua/src/tests/lib_dbus_services.rs` use it to run each library
against fake services written in Lua.

### `dbus_client` — the shared half

Proxies cached per address (each `morf.dbus.proxy` is its own bus connection),
subscriptions made once per address and shared by every handler on it,
`call`/`call1`/`get`/`get_all`/`set` (blocking; for quick reads),
`call_async`/`call1_async`/`set_async` (answered later through
`done(reply, err)`; for anything that may wait on a radio, polkit or a person),
`on_signal`/`on_properties` (handlers get `info` with the sender's unique name
last; both return a handle whose `close()` ends the subscription when its last
handler goes — the system bus caps match rules at 512 per connection),
`has_owner`/`owner_of`/`list_names`/`watch_name` (asked of the bus, never
activating anything), `managed_objects`, typed-value helpers (`u`, `o`, `x`,
`typed`) and `debounce`. Replies arrive as the list of a method's outputs (a
lone scalar bare); `call1` and `first` take the one output out.

### `networkmanager` — `org.freedesktop.NetworkManager`, system bus

```lua
local net = require("lib.networkmanager").connect()
net.state.available, .version, .state, .connectivity         -- "full", "portal", ...
net.state.networking_enabled, .wifi_enabled, .wifi_hardware_enabled
net.state.primary  { id, type, path }
net.state.wifi     { device, state, ssid, strength, frequency, security, connected, last_scan }
net.state.wired    { device, state, connected, carrier, speed, hw_address, ip4 }
net.state.devices            -- rows: path, interface, type, state, driver, managed, hw_address, ip4, connection
net.state.active_connections -- rows: path, id, uuid, type, state, default, vpn, devices, connection
net.state.access_points      -- one row per SSID: key, ssid, bssid, strength, frequency, band,
                             -- security ("open"/"owe"/"wep"/"wpa"/"wpa2"/"wpa3"/"enterprise"),
                             -- secure, in_use, known, path, device
net.state.known_connections  -- rows: path, id, uuid, type, ssid, autoconnect, timestamp, vpn
net.state.vpn_connections    -- rows: path, id, uuid, type, active, state

-- Actions never wait: each returns true once sent (or nil and why not) and
-- takes an optional last `done(result, err)`.
net.request_scan([interface], [done])
net.connect(ap_row_or_ssid, [password], [interface], [done(path)]) -- ActivateConnection if
                                                       -- saved, AddAndActivateConnection if new
net.disconnect([active id/uuid/path or interface], [done])  -- default: the Wi-Fi connection
net.forget(ssid, [done(count)])  -- returns how many profiles it asked to delete
net.set_wifi(enabled, [done])
net.activate(id_or_uuid, [done(path)]), net.deactivate(id_or_uuid, [done])
net.activate_vpn(id_or_uuid, [done]), net.deactivate_vpn(id_or_uuid, [done])
net.snapshot(), net.refresh()
```

The whole tree is one `GetManagedObjects` on NetworkManager's ObjectManager,
re-read (debounced) when the manager, a device or the object set changes.
Access points are never subscribed one by one — their paths are never reused —
and their strengths arrive with each scan, through the device's `LastScan`;
devices are, for as long as they exist. Limitations: a password given for an
already-saved SSID creates a second profile beside the old one (rewriting a
saved profile means sending all of it back, fully typed); 802.1X networks need
a profile made elsewhere; an action's answer is awaited for up to
`action_timeout_ms` (5000) — without blocking — since the service may be
waiting on polkit.

### `bluez` — `org.bluez`, system bus

```lua
local bt = require("lib.bluez").connect()
bt.state.available, .adapter, .address, .powered, .discovering, .discoverable,
       .pairable, .connected_count
bt.state.adapters -- rows: path, name, address, powered, discovering, discoverable, pairable
bt.state.devices  -- rows: path, adapter, address, name, alias, named, icon, paired, bonded,
                  -- trusted, blocked, connected, rssi, in_range, battery (-1 unknown), has_battery

-- Every action takes an optional last `done(ok, err)` and never waits.
bt.set_powered(on, [adapter]), bt.set_discoverable(on), bt.set_pairable(on)
bt.start_discovery(), bt.stop_discovery()
bt.connect(dev), bt.disconnect(dev), bt.pair(dev), bt.cancel_pairing(dev)  -- true, "pending"
bt.trust(dev, [on]), bt.remove(dev)       -- dev: a row, a path or an address
```

Mirrors the ObjectManager tree incrementally (`InterfacesAdded`/`Removed`,
`PropertiesChanged` on adapters and on paired or connected devices). Strangers
found by a scan are refreshed by re-reading the tree every `discovery_poll_ms`
(2000) while discovery runs, rather than by a match rule each; a device's
subscription is closed when it is removed or unpaired. `Connect` and `Pair`
can take as long as a person does, so they are sent asynchronously
(`connect_timeout_ms`, 60000) and return `true, "pending"` at once: the result
arrives as `connected`/`paired` changing, and through `done`.
Pairing a device that needs a PIN requires an `org.bluez.Agent1`, which this
does not register.

### `upower` — `org.freedesktop.UPower` and power-profiles-daemon

```lua
local power = require("lib.upower").connect()
power.state.available, .on_battery, .lid_is_closed, .lid_is_present
power.state.display  { present, percentage, state, charging, time_to_empty, time_to_full,
                       icon_name, energy_rate, kind, warning_level }
power.state.devices      -- every device: path, kind ("battery", "mouse", "headset", ...),
                         -- model, vendor, percentage, state, charging, power_supply, online, ...
power.state.peripherals  -- batteries that are not the machine's own
power.state.profiles { available, active, degraded, service, list = rows { name, driver } }

power.set_profile("power-saver" | "balanced" | "performance")
power.devices(), power.refresh()
require("lib.upower").format_time(seconds)  -- "3 h 12 min"
```

Power profiles are read from `org.freedesktop.UPower.PowerProfiles`, falling
back to `net.hadess.PowerProfiles`, and only when one of them is already
running.

### `mpris` — `org.mpris.MediaPlayer2.*`, session bus

```lua
local media = require("lib.mpris").connect()
media.state.available, .count
media.state.players -- rows: name, identity, desktop_entry, status, playing, title, artist,
                    -- album, art_url, length, position, volume, can_*
media.state.active  { name, identity, desktop_entry, status, playing, title, artist,
                      album_artist, album, art_url, url, track_id, length, position,
                      rate, volume, shuffle, loop, can_play, can_pause, can_go_next,
                      can_go_previous, can_seek, can_control, can_raise }

media.play_pause([name]), .play, .pause, .stop, .next, .previous
media.seek(offset_seconds), media.set_position(seconds), media.set_volume(0..1)
media.set_shuffle(on), media.set_loop("none" | "track" | "playlist"), media.raise()
media.set_active(name_or_nil), media.active(), media.position([name]), media.players()
```

The active player is one that is playing (the most recent to start), else the
most recently changed, else the first by name; `set_active` pins one. Lengths
and positions are seconds. Position is interpolated between readings and
`Seeked`, and `state.active.position` is advanced by a timer (`tick_ms`, 1000)
while playing. Every player lives at the same object path; the engine routes a
signal only to the subscriptions naming its sender, so a player's change
re-reads that player alone (debounced), and its subscriptions close when it
leaves. The buttons are sent without waiting on the player. `playerctld` is
skipped by default (`ignore`).

### `logind` — `org.freedesktop.login1`, system bus

```lua
local login = require("lib.logind").connect()
login.state.available
login.state.session { id, path, user, uid, type, class, seat, vt, active, locked, idle,
                      remote, state, desktop, service }
login.state.can     { suspend, hibernate, hybrid_sleep, suspend_then_hibernate, reboot,
                      power_off }  -- "yes" | "no" | "challenge" | "na"
login.state.lid_closed, .handle_lid_switch, .docked, .idle_hint,
            .preparing_for_sleep, .preparing_for_shutdown
login.state.brightness { device, value, max, percent }
login.state.inhibitors -- rows: what, who, why, mode, uid, pid

login.on_lock(fn), login.on_unlock(fn)
login.on_prepare_for_sleep(fn(going)), login.on_prepare_for_shutdown(fn(going))
-- Actions never wait on logind (it may be waiting on polkit): each returns true
-- once sent and takes an optional last `done(ok, err)`.
login.lock(), login.unlock(), login.set_locked_hint(on), login.set_idle_hint(on)
login.suspend([interactive]), .hibernate, .hybrid_sleep, .suspend_then_hibernate,
      .reboot, .power_off
login.can("suspend")                         -- asked now
login.inhibit(what, who, why, mode, [done])  -- handle with release(), held once granted
login.set_brightness(fraction, [device]), login.set_brightness_raw(value, [device])
login.backlights()
```

`Inhibit` answers with a file descriptor, which the engine hands over as an
opaque handle (see `morf_io::dbus_types`): it can be held and closed, nothing
more. The lock ends when the handle is released, when it is dropped and
collected, or when the shell dies — never outliving it. Brightness is read from
`/sys/class/backlight` with `morf.fs` and written with `Session.SetBrightness`,
which an active session may do unprivileged; it is re-read on udev backlight
events.

### `notifications` — serving `org.freedesktop.Notifications`

A notification server (see the file's header). Each entry carries `id`, `app`,
`icon`, `summary`, `body`, `actions`, `urgency`, `timeout_ms`, `hints`, and the
hints read out: `image_path`, `image_data` (`width`, `height`, `rowstride`,
`has_alpha`, `bits_per_sample`, `channels`, `data`), `category`,
`desktop_entry`, `resident` (stays after an action) and `transient`.
Replacement ids keep a notification's place in the list; closes report the
spec's reasons (expired, dismissed, closed by the app).

## poll

The shared machinery for the libraries below that watch something.

- `poll.source { sample = function(done) ... end, interval = ms, linger = n, initial = v }`
  makes a value that polls **only while something reads it**. `source:get()`
  is a tracked read (a binding that calls it re-runs on each sample) and
  starts the timer; a sample nobody re-reads counts as idle, and after
  `linger` idle samples the timer stops until the next read. Also
  `pin(bool)`, `refresh()`, `set_interval(ms)`, `running()`, `stop()`,
  `publish(value)`, and the fields `value`, `error`, `updated`, `samples`.
- `poll.job(body, on_done, { slice })` runs `body(spend)` as a coroutine over
  timer ticks. A handler gets 100k Lua instructions and a native call costs a
  few dozen, so anything proportional to the machine calls `spend(n)` and is
  resumed next tick when its slice is used.
- `poll.run(argv, on_done, { timeout_ms, max_bytes })` runs a program
  directly (no shell) and calls back with `{ ok, code, stdout, stderr, error }`.
- `poll.which(name)` finds a program on `PATH`.
- `poll.ring(n)` is a ring buffer (`push`, `list`) for sparklines.
- `poll.cache_dir()`, `poll.cache_read(path, ttl)`, `poll.cache_write(path, value)`
  keep JSON with the time it was fetched.

## sysinfo

The machine, from `/proc` and `/sys` only.

```lua
local sysinfo = require("lib.sysinfo")
ui.Text { text = function() return ("CPU %d%%"):format(sysinfo.cpu().usage) end }
ui.Text { text = function()
  local t = sysinfo.temperatures().cpu
  return t and ("%d°C"):format(t) or ""
end }
sysinfo.configure { intervals = { cpu = 1000, processes = 3000 }, history = 120, top = 8 }
```

| reader | returns | default interval |
| --- | --- | --- |
| `cpu()` | `usage`, `cores[i] = {name, usage, frequency}`, `count`, `frequency` (MHz, mean), `load = {1, 5, 15}`, `running`, `threads`, `model` | 2 s |
| `memory()` | bytes: `total`, `available`, `used`, `free`, `cached`, `percent`, `swap = {total, used, free, percent}` | 3 s |
| `disks()` | real filesystems from `/proc/mounts` (one row per device), `{device, mount, type, total, used, free, available, percent}` | 30 s |
| `temperatures()` | `cpu` (°C, the package sensor when there is one), `cpu_sensor`, `sensors[i] = {chip, label, celsius, critical}` from hwmon and thermal zones | 5 s |
| `gpu()` | `busy` and `cards` where the driver exposes `gpu_busy_percent` (amdgpu); Intel reads as nil | 2 s |
| `network()` | `default` interface (from the routing table), `rx_rate`/`tx_rate` in bytes/s for it, `interfaces[i]` | 2 s |
| `battery()` | `present`, `percent`, `status`, `charging`, `power` (W), `time_left`/`time_to_full` (s), `ac`, `batteries` -- for machines without UPower | 30 s |
| `backlight()` | `brightness`, `max`, `percent`, `writable`, `set(percent)` only when writable, `devices` | 5 s |
| `system()` | `os`, `os_id`, `os_version`, `kernel`, `hostname`, `user`, `uptime` | 60 s |
| `processes()` | `count`, `by_cpu`, `by_memory`: the top few `{pid, name, state, cpu, memory, memory_percent}` | 5 s, in slices |

`sysinfo.history(name)` gives the last samples of `cpu`, `load`, `memory`,
`swap`, `temperature`, `gpu`, `rx`, `tx` or `coreN`, oldest first.
`sysinfo.set_brightness(percent, device)` writes the backlight when its file
is writable (world-writable, root, or group-writable with the user in
`video`); nothing here asks for privileges. `sysinfo.sample(name)` samples
one section now. `sysinfo.sources[name]` are the poll sources, for `pin`.
`sysinfo.configure { root = "/some/folder" }` reads a fake `/proc` and `/sys`
from a folder, which is how the tests run.

The tables handed out are shared and replaced whole on every sample: read
them, do not change them.

## weather

The weather from keyless APIs: Open-Meteo (geocoding and forecast), with
wttr.in as the fallback and as the answer when no place is given (it guesses
from the address).

```lua
local weather = require("lib.weather")
local here = weather.new { location = "Wageningen", units = "metric" }
ui.Text { text = function()
  local now = here:get()
  if not now.available then return "" end
  return ("%s %d%s %s"):format(now.glyph, now.temperature, now.units.temperature, now.condition)
end }
```

`weather.new(options)`: `location` (a name, geocoded once) or `latitude`,
`longitude` and `name`; `units` (`"metric"` or `"imperial"`); `interval` (ms
between refreshes while read, default 30 min); `ttl` (seconds an answer is
fresh, default 30 min); `cache_dir`; `fallback` (default true); and
`geocoding_url`, `forecast_url`, `wttr_url` to point elsewhere.

`here:get()` is a tracked read of `{ available, source, place, temperature,
feels_like, humidity, wind_speed, wind_direction, code, condition, icon,
glyph, is_day, high, low, hourly, daily, units, updated, stale }`. `hourly`
is the next 24 hours and `daily` seven days, each entry with `time`,
temperatures, `precipitation` (%), `code`, `condition`, `icon`, `glyph`.
`icon` is a freedesktop icon name (`weather-showers`, `weather-clear-night`),
`glyph` a Unicode symbol. `here:refresh()` asks again past the cache.
Answers are cached on disk; offline, the last one comes back with `stale =
true`. `weather.condition(code, is_day)` maps any WMO code.

## github

A public user's contribution calendar: a year of days, each with a count and
the 0-4 shade GitHub draws it in.

```lua
local github = require("lib.github")
local me = github.new { user = "torvalds" }          -- or { user = ..., token = "ghp_..." }
ui.Text { text = function()
  local c = me:get()
  return c.available and ("%d contributions, %d-day streak"):format(c.total, c.current_streak) or ""
end }
```

Without a token the calendar is read from the public page
`github.com/users/<user>/contributions`, by what identifies a day (a cell's
`data-date` and `data-level`, the tooltip naming it with the count) rather
than by where it sits in the markup. With a `token` it comes from the GraphQL
API. Either way it is cached on disk (`ttl`, default an hour) and refreshed
every `interval` (default an hour) while read.

`me:get()` is a tracked read of `{ available, user, source, days = { {date,
count, level, weekday} }, weeks, total, current_streak, longest_streak, today,
max, updated, stale }`. `weeks` are columns of seven, Sunday first, the way the
calendar is drawn (the first column padded with nils). `total` is the number
GitHub states when it states one (it includes private contributions). The
current streak runs back from today, or from yesterday while today is still
empty. Options: `user`, `token`, `interval`, `ttl`, `cache_dir`, `today`,
`base_url`, `api_url`. `github.parse_html`, `github.parse_graphql` and
`github.summarise` are there for other uses.

## packages

Pending updates and installed counts from the package managers that are
there, by running their programs directly (an argument list, never a shell)
and reading what they print. Only queries: nothing syncs the system's
database, installs, or asks for privileges.

- **pacman**: `checkupdates` when pacman-contrib is installed (it syncs a
  private copy of the database, so the answer is current), else `pacman -Qu`
  against the last sync; `pacman -Q` for the installed count.
- **AUR**: the foreign packages from `pacman -Qm`, their versions asked of
  the AUR RPC (`https://aur.archlinux.org/rpc/v5/info`, a hundred names per
  request) over `morf.http`, compared the way pacman compares versions.
- **flatpak**: `flatpak remote-ls --updates` and `flatpak list`.

```lua
local packages = require("lib.packages")
local updates = packages.new { interval = 60 * 60 * 1000 }
ui.Text { text = function()
  local state = updates:get()
  return state.checking and "…" or (state.total .. " updates")
end }
```

`updates:get()` is a tracked read of `{ total, managers, pacman = { installed,
updates, count, via }, aur = { foreign, updates, count }, flatpak = {
installed, updates, count }, errors, checking, updated }`; each `updates` is a
list of `{ name, old, new }`, and a manager that is not installed is absent.
`updates:refresh()` checks now. Options: `interval`, `aur` and `flatpak`
(both default true), `aur_url`, and `run(argv, on_done)` / `which(name)` to
replace how programs are run and found (the tests do). `packages.vercmp(a,
b)` is pacman's version comparison.

## claude_usage

Claude Code's token usage from its own transcripts, a port of impasto's
`claude_usage.py`: tokens in the current five-hour billing block and in the
last seven days, read from `~/.claude/projects/**/*.jsonl` with `morf.fs`.

```lua
local claude_usage = require("lib.claude_usage")
local usage = claude_usage.new {}
ui.Text { text = function()
  local u = usage:get()
  return u.available and ("%dk / 5h"):format(u.block_tokens // 1000) or ""
end }
```

A token count is input + output + cache-creation tokens, as the original
counts them (cache reads re-send the same context every turn and are left
out). A turn written as several lines with one request id is counted once.
The block starts on the hour of its first message within reach of a
five-hour window.

`usage:get()` is a tracked read of `{ available, block_start, block_end,
block_tokens, block_messages, week_tokens, week_messages, peak_block_tokens,
peak_week_tokens, files, scanning, skipped, updated }`. Reading is
incremental: every file's offset, size and mtime are kept on disk
(`state_path`), an unchanged file is not opened, a grown one is read from
where the last pass stopped, and the work is a job in slices. A pass reads at
most `pass_bytes` (32 MiB); while a backlog remains, `scanning` is true and
the next pass follows a second later, files written within the block first.
Files up to `max_read` (4 MiB) are read whole with `morf.fs.read`; larger ones
a `chunk` at a time by running `dd` directly for the byte range (`morf.fs`
has no ranged read). Options: `dir`, `state_path`, `interval` (60 s),
`keep_hours` (five weeks), `pass_bytes`, `max_read`, `chunk`, `read_range`,
`now`.

`claude_usage.limits(options, on_done)` reports the plan's 5-hour and 7-day
utilisation from the `anthropic-ratelimit-unified-*` headers, which only a
real API request returns: it sends the smallest one (one output token) with
Claude Code's OAuth token from `~/.claude/.credentials.json`. It is never
called by the library itself.
