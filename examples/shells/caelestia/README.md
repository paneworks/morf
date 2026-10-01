# Caelestia

Run the example with `morf examples/shells/caelestia/shell/init.lua`.
The top, bottom, left and right panel triggers wait for a 600 ms hover
before opening, including at the outermost pixel. Leaving early cancels
the pending opening. IPC also opens them:

```sh
morf ipc call capture toggle
morf ipc call bottom open
morf ipc call assistant open
morf ipc call tasks open
morf ipc call calendar open
morf ipc call close
```

The bottom edge opens a large workspace with **Assistant** and **Drop**
tabs. Material's maximum size is 1696 × 751, with the same proportional
reduction on smaller screens (20% narrower and 40% shorter than the previous
panel). Tsugumori keeps that desktop footprint and uses the available width
on compact screens, with vertically scrollable pages.
Assistant awaits a provider; Drop is a placeholder for the future
[termworks/drop](https://github.com/termworks/drop) messaging and file-sharing
integration. Open it directly with `morf ipc call bottom open drop`.
Click outside the workspace to close it from either tab. When opened by
hovering the bottom edge, it also closes after the pointer leaves it.
Add future pages in `shell/bottom_model.lua` and register their builders in
each visual theme. Assistant, Drop and the workspace container have separate
Material and Tsugumori layouts.

**Lule** lives in the top dashboard, in a 992 × 588 panel. The wallpaper
gets 60% of the top row, with a narrower color pane beside it and a control
strip below; all color swatches and Apply stay visible. **Wallpaper folder**
shows the collection used by Images, Shuffle, the arrows, and Random & apply.
Edit that field and press **Use folder** (or Enter) to save a different folder;
`~/` paths work. This remembers `lule.folder` in the shell's `caelestia.json`
settings across restarts and monitors. Invalid folders leave the collection
unchanged. Initially it uses `LULE_W`, then the current wallpaper's folder.
This is the panel's collection setting; standalone Lule still uses its own
`lule.wallpaper` config / `LULE_W` environment setting.
Use Images, Shuffle or the arrows to preview an image.
The folder is rescanned when opening the tab or Images, shuffling, using the
arrows, or choosing Random & apply, so added and removed images are picked up
automatically. Use folder is only needed to change the collection's directory.
Choose dark/light and a palette method, then press
**Apply wallpaper & colors**. **Random & apply** chooses another image and
applies it immediately using the selected appearance and palette method.
Images and Shuffle remain preview actions.
The swatches show the currently applied ANSI palette and special colors;
click a swatch to copy its hex value. Open directly with `morf ipc call lule open`
or click the top dashboard’s Lule tab. With no arguments, `morf ipc call lule`
still reports the terminal/accent diagnostics.

The tab runs `lule create --image=… --theme=… --palette=… -- set` through
`lib.lule.generate`, preserving Lule's config, templates and desktop hooks.
Current Lule uses `~/.config/lule/init.lua` (or `LULE_C`); a separate legacy
`lule_colors` script is not run a second time. The bundled tab icon is Lule's
original SVG geometry: outlined when inactive, filled with the accent when
selected. Its license is in `shell/assets/`.

Previews resize on an image worker only while the Lule page is visible.
They overwrite one small scratch JPEG per output and clear on closing;
opening the shell does not decode an entire wallpaper collection.

PrintScreen, the **Screenshot / Record** button in quick settings, or
`morf ipc call capture open` opens the compact bottom capture panel. Choose
region, window or screen, an optional delay, then Screenshot or Record.
The panel closes before capture. Recording uses its existing commands.
Screenshots freeze the desktop at its normal size: drag a region, click a
window, or press `S` (or Enter) for the current monitor. The screen target
already selects that monitor.

After selection, a compact floating editing menu appears beside the capture,
with Copy, Save, undo/redo, Draw and More. Draw expands the tool palette;
More includes Select again, Upload, Settings and Cancel. The toolbar can be
dragged. Handles appear after selection for resizing. Window snapping uses
Hyprland or Sway geometry, or the floating window geometry available from Niri.

The editor supplies select, rectangle, ellipse, line, arrow, pen, highlight,
text, numbered steps, blur, pixelate and zoom. Select moves annotations;
Delete removes the selected one. `F` switches rectangle/ellipse fill, the
wheel changes stroke/text size while drawing, and each tool remembers its
colour, width and fill across launches. `Ctrl+Z`/`Ctrl+Shift+Z` undo/redo;
`Ctrl+C`, `Ctrl+S` and `Ctrl+U` copy, save and request upload. Escape or
Cancel dismisses immediately; clicking outside the selected region also
closes the editor. The tool palette wraps on small screens. Both themes use their own controls and title animations.

Settings expose blur strength, pixel size, zoom factor, a custom hex colour,
save folder, an internal save/folder chooser, copy-on-save, save-on-copy,
pointer inclusion and key rebinding. Preferences live in `shell.json`;
defaults belong to `shell/config.lua`. Hyprland shortcut changes apply
immediately and are restored after reload. Set `capture.keybind_file` to a
dedicated include file if you want to write the generated binding there too.
The default compositor binding remains:

```lua
bind_exec("Print", "/usr/bin/morf ipc call capture open")
```

Upload requires a separate confirmation. It sends only the rendered image,
with metadata removed by re-encoding, to `capture.upload_endpoint` (default
Litterbox); the default link is public and expires after 72 hours. The URL
is copied to the clipboard. Nothing is uploaded automatically.

Acquisition uses Morf's native Wayland screencopy; KDE uses Spectacle.
Image composition/effects run on workers without ImageMagick. `wl-copy`
keeps the image clipboard alive after the shell exits; uploads use `curl`.
Live drawing uses native Path geometry; committed previews keep a decoded
monitor image and publish pixels directly, without a PNG round trip for each
edit. Lua owns tool selection, undo history and themed controls; Rust builds
and rasterizes the annotation geometry. Save/Copy still encode an export.
Temporary captures/previews are cleaned when editing ends, with bounded
undo and preview caches. There is no screenshot history or gallery.
`morf ipc call capture-editor cancel` closes editing across outputs;
`morf ipc call screenshot screen quick` retains the old command-based
capture. Set `capture.editor = false` to use that path by default.

Recording still uses the configured commands; custom recorders must stay in
the foreground (use `exec` in a wrapper) so Morf can track and stop them.
Quick capture/recording commands may use `grim`, `slurp`, `hyprctl`, `jq`
and `gpu-screen-recorder`. Reopen the popup to stop the recorder it started
or cancel a pending countdown.

`oslo make install` invokes sudo to install `/usr/bin/morf` and the shared
library in `/usr/share/morf/library`, then retires the old local executable.
`oslo make apply` also invokes sudo: it updates the selected shell both in
`~/.config/morf/` and `/etc/xdg/morf/`, and configures greetd to run `morf greet`.
Use `--example caelestia` to select it explicitly. Existing files are backed up.
Your appearance preferences stay in your own configuration directory.

Run `morf shell`, `morf lock`, or `morf greet`. Preview without authenticating
with `morf lock -- window preview` or `morf greet -- preview`.
See [system installation](../../../docs/SYSTEM.md) for the full workflow.

The left panel has **Tasks** and **Calendar** tabs. Tasks uses the installed
`task` executable and the user's normal Taskwarrior configuration, including
`TASKRC` and `TASKDATA`. Its reusable client is `library/lib/taskwarrior.lua`.
No second task database or sync service is created. Tasks refresh when the
panel opens, every ten seconds while open, and after a successful change.

Click a task to edit its description, project, priority, scheduled date and
time, deadline, waiting date, tags, recurrence, recurrence end, or dependencies.
Dates accept Taskwarrior expressions such as `tomorrow`, or an ISO date/time
such as `2026-10-01T09:00`. A repeating task needs a due date. Editing one
occurrence does not propagate changes to its siblings. The editor also
offers start/stop, completion, and deletion with a second click to confirm.

Calendar shows a month and tasks scheduled or due on the selected day, in
local time. “Plan a task” preselects that day. Work-mail meetings are not
connected yet; the calendar explicitly shows that state.

Performance and battery graphs collect the latest 60 samples while their
tabs are closed. Old samples are overwritten in memory; restarting the
shell resets them. No history files are written.
Reloading the configuration also starts a new history. CPU history spans
two minutes; memory and battery span three minutes. After a restart or
reload, the graphs fill from the right as samples arrive.

`morf ipc call dashboard-history` reports sampling counters and errors without
waking idle sources. Pass an output name (for example `dashboard-history DP-6`)
to inspect that monitor. The counters should increase with the drawer closed.

## Visual themes (in progress)

The shell, lock and greeter select a visual package with
`CAELESTIA_STYLE=material` or `CAELESTIA_STYLE=tsugumori`. Without that override,
selection comes from `theme` in `$XDG_CONFIG_HOME/morf/caelestia/appearance.json`
(or the file named by `CAELESTIA_APPEARANCE`); Material is the default.
Changing the selection currently requires restarting the preview. Wallpaper
and Lule colors remain independent of that selection.

Both themes use the same content builders in `themes/layouts/`: sections,
control order, navigation and information grouping stay the same. Useful
section titles are shared additions too. Tsugumori supplies typography,
framed controls, subtle wallpaper-accent borders, title decoding, hover
feedback and covered transitions. Its approved edge pills remain a visual
override. Every tab has its icon at the right end, including Lule's custom SVG.

Shell services and authentication remain outside the visual components.
Preview mode disables task changes and real authentication actions. The
conversion and broad verification are still in progress; see
[THEMING.md](THEMING.md). Develop and review in the isolated sandbox before
installing.

## Checks

```sh
morf test --no-dbus library/tests/lule_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/lule_spec.lua
morf test --no-dbus library/tests/taskwarrior_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/hover_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/panels_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/history_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/heading_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/panel_headings_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/planner_typography_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/planner_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/side_panel_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/bar_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/keyboard_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/auth_keyboard_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/auth_desktop_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/network_typography_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/history_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/frame_rail_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/bottom_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/settings_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/sound_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/connectivity_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/power_bar_theme_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/net_pages_theme_spec.lua
```

The library tests use a real Taskwarrior installation with a temporary
configuration and database, or skip the CLI test if it is unavailable.
Panel tests stub external commands and never change the user's tasks or
record the desktop.

Keyring prompts use morf when the optional `morf-keyring` helper is installed
on PATH. See [the bridge build and test instructions](../../../tools/keyring/README.md).
The bridge keeps GNOME Keyring as the secret store and displays unlock,
new-password and confirmation dialogs using the current theme. It starts on
the primary output, waits for existing GNOME prompts to finish, and leaves
GNOME's original prompter available when the shell is stopped. Inspect its
status with `morf ipc call keyring`.

The **Shell theme** controls at the bottom of Lule switch between Material and
Tsugumori without restarting the process. Open drawers use a feathered wipe while
the frame stays visible and its corners and seams morph between styles. Restored
cards start settled so their entrance animations do not compete with the reveal.
Font-only changes use a dissolve. The selected theme is saved in
`$XDG_CONFIG_HOME/morf/caelestia/appearance.json` (`CAELESTIA_APPEARANCE` overrides
that path); `CAELESTIA_STYLE` remains a startup override for previews.

The switch retains open drawers, selected tabs, Taskwarrior editor drafts,
Lule selections, notification history, and bounded performance/battery samples
in memory. These snapshots are released after the switch and never written to
disk. Finish an active authentication prompt, capture, task operation, or Lule
apply before switching. IPC also supports `morf ipc call appearance material`
and `morf ipc call appearance tsugumori`; calling `appearance` alone reports the
current theme and transition status.

The adjacent **Font** picker searches installed font families and previews each
face. Choose **Theme default** to restore Material or Tsugumori's own typeface.
The shell font is saved as `font` in the same appearance file, stays selected
when changing themes, and uses the same smooth, state-preserving transition.
Icons retain their symbol font. Installed families are scanned with `fc-list`
when opening the picker, then cached until the next shell reload. IPC accepts
`morf ipc call appearance 'font:Goku'`; `font:` resets the font choice.

Lock controls follow the pointer's output. Each output uses its own logical
dimensions, including space for pattern input and the on-screen keyboard;
other outputs show the clock and background. The lock has one private password
draft across all outputs. The greeter stays on Cage's main display with one
private draft and one authentication conversation.
Both views rebuild their presentation after a resize or rotation while retaining
controller state. Small outputs keep readable controls and scroll the sheet;
pattern input fits the visible area. Replacing a keyboard cancels held keys and
pending repeats. Short greeter outputs reserve a footer for power controls.
The lock's live monitor list drops disconnected displays, so its controls move
to a remaining output and recover when a display reconnects.

Connecting headphones shows a centered status badge for 3.2 seconds on the active
output. Earbuds get a separate icon when the name or form-factor metadata identifies
them; generic analog jacks use headphones because they cannot distinguish the
physical accessory. One `pactl subscribe` watcher reads audio-route changes,
including wired jack availability, without idle polling. Native audio metadata is
the fallback when PulseAudio compatibility is unavailable. Devices present at
startup, volume changes and brief Bluetooth profile handoffs stay quiet.
Preview with `morf ipc call headphones-demo` or
`morf ipc call headphones-demo earbuds`.

The Cage greeter uses one login window on Cage's main display. `make apply`
sets `cage -m last`, so secondary outputs are disabled during login. The view
uses its configured fullscreen window size and never follows the pointer or
rebuilds in response to monitor metadata. Locking remains independent.
