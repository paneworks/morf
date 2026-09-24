# impasto, on morf

A port of [impasto](https://github.com/andreumassanet/impasto) — a Hyprland
shell written for Quickshell in QML — to morf, in Lua only. No shell scripts,
no Python: what the original did with `hyprctl`, `nmcli`, `wpctl`, `grim` or
a helper script is done here with morf's own APIs (`morf.fs`, `morf.time`,
`morf.http`, `morf.image`, `morf.audio`, `morf.clipboard`, `morf.dbus`,
`morf.screencopy`…) and the pure-Lua libraries in `examples/lib`
(`hyprland`, `networkmanager`, `bluez`, `upower`, `mpris`, `logind`,
`sysinfo`, `weather`, `github`, `packages`, `palette`…). A program is still
run where nothing else can do its job (`pacman`, `hyprsunset`, `ddcutil`),
directly, never through a shell.

```sh
EXAMPLE=examples/impasto/init.lua oslo make run
```

Settings live in `~/.config/impasto-morf/settings.json` (only what differs
from the defaults in `services/settings.lua`).

## Layout

Folders follow the original's, one Lua file per QML file where that makes
sense:

| here | original | what |
|---|---|---|
| `theme.lua` | `theme/Theme.qml` | colour, size, type and motion tokens; live palette |
| `services/` | `services/*.qml` | state of the machine and of the shell |
| `components/` | `components/*.qml` | small shared pieces (`kit.lua` is the base) |
| `bar/bar.lua` | `bar/Bar.qml` | the bar's styles around the island |
| `bar/island.lua` | `bar/widgets/DynamicIsland.qml` | the morphing capsule |
| `bar/island_state.lua` | `bar/island/IslandState.qml` | which layer is showing |
| `bar/modules/` | `bar/modules/*.qml` | module chips and their details |
| `bar/panels/` | `bar/island/*Panel.qml` | what the island opens into |
| `bar/layers/` | `IslandRest`, `IslandSummary`, `OsdLayer`, `NotificationLayer` | the island's layers below a panel |
| `bar/pieces/` | pieces of `Bar.qml` | what sits on the bar's sides |
| `pets/` | `components/Pet*.qml`, `components/pets/*` | the pets' four drawing styles, face, family, shelf |
| `desktop/`, `dock/`, `deck/`, `lock/`, `capture/`, `settings/` | same | the other surfaces |

## How the parts plug in

`init.lua` requires every file in `services/auto`, `bar/modules`,
`bar/pieces`, `bar/layers` and `bar/panels` before it builds the bar. Each
file registers itself; none needs a line in `init.lua`:

```lua
-- bar/panels/stats.lua
local island = require("bar.island")
island.register("stats", {
  size = function() return 940, 614 end,   -- declared: the capsule gets there first
  build = function(island) return ui.Item { ... } end,
})

-- bar/pieces/battery.lua
local bar = require("bar.bar")
bar.register("battery", { build = function() return node end })

-- bar/layers/osd.lua
island.register_layer("osd", { size = function() return 260, 32, 10 end, build = function() ... end })
```

A file that fails to load is logged and listed by `morf ipc call failed`;
the rest of the shell still starts.

Conventions:

- Colours come from `theme.color.<name>` (palette entries are functions, so
  a binding follows the wallpaper: `color = theme.color.accent`); sizes from
  `theme`; durations from `theme.behave("fast" | "medium" | "morph")`.
- Settings are read with `settings.<key>` inside bindings, written with
  `settings.set(key, value)`. A new setting gets its default in
  `services/settings.lua`.
- Bindings are given at construction (`x = function() ... end`), never
  assigned to a node afterwards.
- Nothing is polled faster than it changes; services start their timers only
  once something reads them.
- Every panel is reachable over IPC: `morf ipc call <panel>` toggles it.

## The lock

The lock is its own process. `morf ipc call lock` (or the session panel,
or the idle timeout) closes the island, photographs the desk with
`morf.screencopy.save`, blurs a quarter-size copy with `morf.image.process`,
and starts

```sh
morf examples/impasto/init.lua -- lock
```

which sets `morf.surface.session_lock = true`: morf runs that file as an
ext-session-lock client, one surface per output, until PAM (or a face)
answers and the file clears the flag. The compositor keeps the session
locked if that process dies. `-- lock window` shows the same screen as a
plain overlay that holds nothing, to look at it.

## Testing

Only in a nested, headless compositor — never on the desktop the shell is
being written on:

```sh
WLR_BACKENDS=headless cage -- sh -c 'morf examples/impasto/init.lua & sleep 4; morf ipc call controls; grim out.png'
```

## Status

| part | state |
|---|---|
| settings, theme, kit | ported |
| island state, island, bar (grouped, spread) | ported |
| rest layer (clock) | ported |
| pets (service, four styles, panel `pet`, detail `pet.detail`, bar piece `pet`) | ported |
| lock screen, lock/idle/session services, session panel | ported |
| dock (`dock/`, `services/dock.lua`, `services/auto/dock.lua`) | ported |
| everything else | in progress |
