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
subscriptions made once per address (they cannot be taken back, and the system
bus caps match rules at 512 per connection), `call`/`call1`/`get`/`get_all`/
`set`, `on_signal`/`on_properties`, `watch_name`, `managed_objects`, typed-value
helpers (`u`, `o`, `x`, `typed`) and `debounce`. Replies arrive as the list of a
method's outputs (a lone scalar bare); `call1` and `first` take the one output
out.

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

net.request_scan([interface])
net.connect(ap_row_or_ssid, [password], [interface])  -- ActivateConnection if saved,
                                                       -- AddAndActivateConnection if new
net.disconnect([active id/uuid/path or interface])     -- default: the Wi-Fi connection
net.forget(ssid)                                       -- deletes every saved profile for it
net.set_wifi(enabled)
net.activate(id_or_uuid), net.deactivate(id_or_uuid)
net.activate_vpn(id_or_uuid), net.deactivate_vpn(id_or_uuid)
net.snapshot(), net.refresh()
```

The whole tree is one `GetManagedObjects` on NetworkManager's ObjectManager,
re-read (debounced) when the manager, a device or the object set changes.
Access points are never subscribed one by one — their paths are never reused —
and their strengths arrive with each scan, through the device's `LastScan`.
Limitations: a password given for an already-saved SSID creates a second
profile beside the old one (rewriting a saved profile means sending all of it
back, fully typed); 802.1X networks need a profile made elsewhere; an action
waits up to `action_timeout_ms` (5000) on the service, which may be waiting on
polkit.

### `bluez` — `org.bluez`, system bus

```lua
local bt = require("lib.bluez").connect()
bt.state.available, .adapter, .address, .powered, .discovering, .discoverable,
       .pairable, .connected_count
bt.state.adapters -- rows: path, name, address, powered, discovering, discoverable, pairable
bt.state.devices  -- rows: path, adapter, address, name, alias, named, icon, paired, bonded,
                  -- trusted, blocked, connected, rssi, in_range, battery (-1 unknown), has_battery

bt.set_powered(on, [adapter]), bt.set_discoverable(on), bt.set_pairable(on)
bt.start_discovery(), bt.stop_discovery()
bt.connect(dev), bt.disconnect(dev), bt.pair(dev), bt.cancel_pairing(dev)
bt.trust(dev, [on]), bt.remove(dev)       -- dev: a row, a path or an address
```

Mirrors the ObjectManager tree incrementally (`InterfacesAdded`/`Removed`,
`PropertiesChanged` on adapters and on paired or connected devices). Strangers
found by a scan are refreshed by re-reading the tree every `discovery_poll_ms`
(2000) while discovery runs, rather than by a match rule each. `Connect` and
`Pair` can take seconds and the engine's calls block the drawing thread, so
actions wait `action_timeout_ms` (2500) and then return `true, "pending"`:
BlueZ carries on and the result arrives as `connected`/`paired` changing.
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
while playing. Every player lives at the same object path and a signal handler
is not told the sender, so any player's change re-reads all of them
(debounced). `playerctld` is skipped by default (`ignore`).

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
login.lock(), login.unlock(), login.set_locked_hint(on), login.set_idle_hint(on)
login.suspend([interactive]), .hibernate, .hybrid_sleep, .suspend_then_hibernate,
      .reboot, .power_off
login.can("suspend")                         -- asked now
login.inhibit(what, who, why, mode)          -- handle with release()
login.set_brightness(fraction, [device]), login.set_brightness_raw(value, [device])
login.backlights()
```

`Inhibit` answers with a file descriptor, and the engine deliberately never
passes fds into a configuration (see `morf_io::dbus_types`). So `inhibit`
holds the lock through a child process, `systemd-inhibit ... cat`: `cat` waits
on a pipe from the shell, so the lock ends when the handle is released or when
the shell dies, never outliving it. Brightness is read from
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
