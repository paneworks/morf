# examples/lib

Pure-Lua libraries for morf configurations, loaded with `require("lib.<name>")`
from a configuration beside this folder. The engine stays agnostic: it offers a
generic D-Bus client and server (`morf.dbus`), signals, state and timers, and
everything that is *about* a particular service lives here, where a shell can
read it, copy it and change it.

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
