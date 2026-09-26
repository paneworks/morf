# impasto, on morf

A port of [impasto](https://github.com/andreumassanet/impasto) — a Hyprland
shell written for Quickshell in QML — to morf, in Lua only. No shell scripts,
no Python: what the original did with `hyprctl`, `nmcli`, `wpctl`, `grim` or
a helper script is done here with morf's own APIs (`morf.fs`, `morf.time`,
`morf.http`, `morf.image`, `morf.audio`, `morf.clipboard`, `morf.dbus`,
`morf.screencopy`…) and the pure-Lua libraries in `library/lib`
(`hyprland`, `networkmanager`, `bluez`, `upower`, `mpris`, `logind`,
`sysinfo`, `weather`, `github`, `packages`, `palette`…). A program is still
run where nothing else can do its job (`pacman`, `hyprsunset`, `ddcutil`),
directly, never through a shell.

```sh
EXAMPLE=examples/impasto/init.lua oslo make run
```

Settings live in `~/.config/impasto-morf/settings.json` (only what differs
from the defaults in `services/settings.lua`).

## The compositor

What upstream pushed through CompositorService, MonitorService and
ShortcutService goes through `lib/hyprland_config.lua` (on top of
`lib/hyprland.lua`), for a Lua-configured Hyprland (`eval` of `hl.*` chunks)
or a hyprlang one (`keyword`s); the configuration files are never written,
and whatever is pushed is pushed again after every reload. Under any other
compositor the three services say so, the settings pages lock their
controls with a note, and nothing is sent.

- `services/compositor.lua`: keyboard layouts and the switch between them,
  key repeat, pointer sensitivity, the cursor (`setcursor`; its colour is a
  theme compiled with `hyprcursor-util` from `cursor-src` in the state
  folder, when both are there), shake to find (hypr-dynamic-cursors), the
  animation preset, the window shadow and hyprglass. An input option is
  pushed at start only once it differs from impasto's default, so an
  untouched one stays the compositor's.
- `services/displays.lua`: an arrangement per set of connected monitors
  (`displays` in the settings), pushed as monitor rules; the Displays page
  drags screens into place, mirrors, picks the main screen, switches them
  off and on, sets scale, rotation, VRR and mode. No screen lit is put right,
  workspaces left on a gone screen are brought back, and the lid goes
  through the arrangement. `morf ipc call displays` lists the screens;
  `morf ipc call display DP-2 off|on|scale 1.25|position 0x0|mode
  2560x1440@144|transform 1|vrr 1`, `display mirror on|off`, `display
  primary eDP-1`, `display forget` change it from a keybind.
- `services/shortcuts.lua`: the profile's `keys` (bind description ->
  combination), rebound from the Keys page, are written to
  `$XDG_STATE_HOME/impasto-morf/keys.tsv` and Hyprland is reloaded. For them
  to take, the Hyprland configuration needs one line:

  ```lua
  -- hyprland.lua (Lua config), near the top, before any bind
  pcall(dofile, (os.getenv("XDG_STATE_HOME") or (os.getenv("HOME") .. "/.local/state")) .. "/impasto-morf/keys.lua")
  ```

  ```ini
  # hyprland.conf (hyprlang), at the end, after every bind
  source = ~/.local/state/impasto-morf/keys.conf
  ```

Tested headless against a fake Hyprland served from the spec:
`morf test --private-bus examples/tests/impasto_hyprland_spec.lua` and
`morf test examples/tests/hyprland_config_spec.lua`.

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
| `games/` | `bar/island/games/*.qml` | the arcade's games, one file each (`common.lua` is their contract) |
| `bar/layers/` | `IslandRest`, `IslandSummary`, `OsdLayer`, `NotificationLayer` | the island's layers below a panel |
| `fonts/` | `home/.local/share/fonts` | Grape Nuts, the notes' hand (OFL); morf puts it on the font path |
| `profiles/` | `home/.local/share/impasto/profiles` | the three example profiles; wallpapers by file name, found in `wallpaperDir` |
| `bar/pieces/` | pieces of `Bar.qml` | what sits on the bar's sides |
| `pets/` | `components/Pet*.qml`, `components/pets/*` | the pets' four drawing styles, face, family, shelf |
| `bar/controls/` | `bar/island/controls/*.qml` | the control centre's blocks, shared with the details |
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

A panel can also ask for more of the island while it is open: `paper`
returns a colour to paint it instead of black (and drops its rim),
`padding` returns its inner margin, and `declared = true` lays the panel
out at its declared size from the first frame instead of resizing it with
the capsule. An open note uses all three: the island becomes the note.

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
- Any setting is too: `morf ipc call get <key>`, and
  `morf ipc call set <key> <json>` (`set idleLock 5`, `set clipboardHistory false`).
- Idle: lock, screen off and suspend are each their own `morf.idle`
  subscription at the minutes in Settings; changing one re-arms it at once.

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

## The desk

Modules on the wallpaper, on a grid, in four shapes (2x2, 4x2, 4x4, 8x2
cells) and two themes (Modern figures, Analogue objects). The rows are
`desktopWidgets` in the settings; `services/desktop.lua` owns them, the
grid, collisions and the arranging state.

A morf layer surface keeps its layer for life, so the original's one desk
surface (raised to the top layer while arranging) is three here:

- at rest the widgets are drawn into the wallpaper's own background
  surface (`desktop/wallpaper.lua` → `desktop/desk.lua` `rest`), under
  every window; its input is the widgets' controls and the right button on
  the wallpaper, which opens the desk's menu;
- while arranging, `impasto-desktop` on the top layer holds the board:
  the wallpaper under a grid, every widget with its handles (drag to move,
  corner to reshape, badge to remove, wheel to cycle shapes, click for the
  inspector), the note decks on the edges (`desktop/arrange/decks.lua`:
  grip to slide, tabs to reorder, move, or pull onto the grid or the card),
  the card of modules and the photo picker; Escape, a right click, the
  keyboard going to the island, or another workspace ends it, on every
  screen (each screen's process says so on the session bus);
- `impasto-desktop-menu`, on the top layer only while the menu is open.

The note decks at rest are `deck/deck.lua`, one small surface per edge on
the desk's board; their rows are the desk's too.

The faces read `desktop/sources.lua`, which prefers the shell's services
(`services.audio`, `battery`, `media`, `tasks`, `notes`, `updates`, ...)
and stands in with the libraries where one cannot load, and
`services/weather|stats|github|claude.lua`, which read the lua-stdlib
libraries (`lib.weather`, `lib.sysinfo`, `lib.github`, `lib.claude_usage`,
`lib.packages`).

`morf ipc call desk <verb>`: `edit`, `done`, `add <module> [col row]`,
`remove <key>`, `select <key>`, `pick <key>`, `family <key> <2x2|4x2|4x4|8x2>`,
`menu [key]`, `theme <modern|analogue>`, `style <capsule|accent|outline|bare>`,
`list`; and for a bench without a pointer, `day <key> [yyyy-mm-dd]` (a
calendar's day view), `note_edge <key> <edge>`, `note_grid <note> <col> <row>`,
`deck_edge <key> <edge>`, `deck_add <edge>`, `along <deck> <0..1>`,
`takes_new <deck> <1|0>`, `light <edge>` and `view <photo key>`.

## Testing

Only in a nested, headless compositor — never on the desktop the shell is
being written on:

```sh
IMPASTO_DRY_RUN=1 WLR_BACKENDS=headless dbus-run-session -- \
  cage -- sh -c 'morf examples/impasto/init.lua & sleep 6; morf ipc call controls; grim out.png'
```

Verbs for a state the pointer would otherwise have to reach:
`notes.open <key>`, `notes.new`, `board.open <key>`, `board.new`,
`board.pick` (the month over the open task's day), `board.move <key>
<lane> <slot>`, `deck.peek <key>`, `deck.reveal`, `deck.rest`,
`deck.place <key> <edge>`, `module.notes`, `module.tasks`.
`IMPASTO_DRY_RUN=1` makes every action that would change the machine --
the radios, a connection, the volume, the backlight, the power profile,
the player, the session -- log what it would do instead
(`services/act.lua`), so a test can press anything. `morf ipc call detail
<id>` opens a module's detail, `glance` the summary, `controls_edit`
arranging.

Unset the host's compositor sockets first (`HYPRLAND_INSTANCE_SIGNATURE`,
`SWAYSOCK`, `NIRI_SOCKET`), or `lib.hyprland` in the nested shell talks to
the live Hyprland; give it `XDG_CONFIG_HOME` of its own and a private
session bus whose configuration lists no activatable services
(`dbus-run-session --config-file=...`), or startup waits on the settings
portal. `IMPASTO_INLINE_WALLPAPER=1` draws the wallpaper, the desk, its
arranging board and its menu inside the main surface, since cage has no
layer shell. The headless output is 1280x720, which the desk's grid makes
11 by 5 cells.

Against a test Hyprland of its own (one nested in the headless cage, with
its own `HYPRLAND_INSTANCE_SIGNATURE` under a private `XDG_RUNTIME_DIR`),
`IMPASTO_LIVE_COMPOSITOR=1` next to `IMPASTO_DRY_RUN=1` lets what goes to the
compositor through (options, monitor rules, reloads, the keys file) while
the machine is still left alone. cage 0.3.1 offers xdg_wm_base v5 and
Hyprland 0.56's Wayland backend binds v6, so it needs a proxy between them
that advertises v6 (v6 only adds a toplevel state), and its screens are
headless outputs made with `hyprctl output create headless`. Binds typed
with wtype fire only with `input:resolve_binds_by_sym = true` (wtype sends
a keymap of its own), and a wtype held open (`wtype -s 400000`) keeps the
seat's keyboard, so the first key of each call is not lost.
The settings window: `settings [section] [part]` opens it on a page (or
closes it when no page is named), `settings_scroll <pixels>` scrolls the
page, `layout_drop <from> <index-or-id> <over> <at>` makes the drop a drag
in the bar's layout editor would (once the Bar page has been open),
`profile <name>` switches profiles, `profile_export <name> <path>` and
`profile_import <path>` write and read one. `appearance` and `palette`
open the island's strips, `appearance_key <left|right|up|down|return>`
presses a key in them. `IMPASTO_SETTINGS_TIMING=1` logs how long each
page took to build and lay out. Wallpaper thumbnails are cut into
`$XDG_CACHE_HOME/impasto-morf/thumbs`, so a test with its own config home
wants its own cache home too.
Capture, recording and the picker are the shortcuts' verbs: `capture
[region|window|screen] [photo|video] [file|clipboard|editor|text]`, `record
[toggle|start|stop]`, `picker`; a test drives them with `capture.select x y
w h`, `capture.take`, `capture.cancel`, `capture.last`, `picker.hover x y`,
`picker.take x y`, `picker.last`. A dry run pretends a recording instead of
starting the encoder. Captures go to `$IMPASTO_CAPTURES`, else
`$XDG_PICTURES_DIR`, else the XDG pictures folder; recordings to
`$IMPASTO_RECORDINGS`, `$XDG_VIDEOS_DIR` or the videos folder -- point them
at a temporary folder on a test bench. `stats.warm [n]` fills the graphs'
history, `keys.sample` gives the key sheet a bind list where there is no
Hyprland, `updates.sample` a pending list, and `packages.view <id>`,
`packages.query <text>`, `packages.filter <id>`, `packages.key <key>` drive
the packages panel. Its Install, Remove and Update everything open a
terminal only on a click, and never on a dry run.

## Status

| part | state |
|---|---|
| settings, theme, kit | ported |
| island state, island, bar (grouped, spread) | ported |
| rest layer (clock) | ported |
| pets (service, four styles, panel `pet`, detail `pet.detail`, bar piece `pet`) | ported |
| lock screen, lock/idle/session services, session panel | ported |
| dock (`dock/`, `services/dock.lua`, `services/auto/dock.lua`) | ported |
| arcade: `bar/panels/games.lua`, `games/`, `services/games.lua` | ported (eleven games) |
| notes: service, panel (deck and paper), sticky, module | ported |
| edge decks (`deck/`, `services/deck.lua`) | ported, one small surface per edge |
| tasks: service, board, task row, day picker, module | ported; `tasks.days_with_tasks` for the calendar |
| launcher (`bar/panels/launcher.lua`, `services/launcher.lua`, `services/calc.lua`; `=` `>` `@` `!` `'`) | ported |
| overview (`bar/panels/overview.lua`), workspaces piece, launcher and overview buttons | ported |
| timer (`services/timer.lua`, on the island: `bar/modules/timer.lua`), clipboard history (`services/clipboard.lua`) | ported |
| rest layer (clock and activities), glance | ported |
| network, Bluetooth, audio, battery, brightness, media, system, OSD, modules services | ported |
| control centre (blocks, toggles, arranging), Wi-Fi and Bluetooth lists | ported |
| battery, volume, brightness, network, Bluetooth, media, notifications, calendar modules | ported |
| desk: `desktop/`, `services/desktop.lua`, `services/auto/desktop.lua` | ported (both themes, arranging, spectrum) |
| settings window (`settings/`, eleven sections), its components (`components/setting.lua` and the small pieces beside it) | ported; input, displays and keys are pushed to Hyprland at run time (see below), its files never written |
| profiles (`services/profiles.lua`, three examples in `profiles/`), import and export | ported |
| island appearance panel (`bar/panels/appearance.lua`, `appearance` and `palette`) | ported |
| capture (`capture/`, `services/capture.lua`), recorder, colour picker (`capture/picker.lua`, `services/picker.lua`), their tiles and verbs (`services/auto/capture.lua`) | ported |
| system statistics (panel `stats`, module `stats`, `services/stats.lua`), key sheet (panel `keys`), packages and updates (panel `packages`, module `updates`) | ported |
| everything else | in progress |
