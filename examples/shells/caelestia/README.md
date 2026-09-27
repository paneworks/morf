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
tabs. Its maximum size is 1696 × 751, with the same proportional reduction
on smaller screens (20% narrower and 40% shorter than the previous panel).
Assistant awaits a provider; Drop is a placeholder for the future
[termworks/drop](https://github.com/termworks/drop) messaging and file-sharing
integration. Open it directly with `morf ipc call bottom open drop`.
Click outside the workspace to close it from either tab. When opened by
hovering the bottom edge, it also closes after the pointer leaves it.
Add future pages in `shell/bottom.lua`.

Capture is a separate, compact bottom popup. Open it with PrintScreen,
the **Screenshot / Record** button in quick settings, or `capture toggle`
over IPC. It never opens from hovering the bottom edge. Choose region,
focused window or screen, an optional delay, then Screenshot or Record.
There is no capture history or gallery. Click outside to close it. Reopen
it to stop a recording or cancel a pending countdown. Stop signals only the recorder it started.

Bind PrintScreen in the compositor (Hyprland Lua config):

```lua
bind_exec("Print", ctx.home .. "/.local/bin/morf ipc call capture open")
```

Custom recording commands must stay in the foreground (use `exec` in a
shell wrapper) so the shell can track and stop them. The default tools are
`grim`, `slurp`, `wl-copy`, `hyprctl`, `jq`, and `gpu-screen-recorder`;
commands and the destination folder are set in `shell/config.lua`.

After changing the example, `oslo make apply --example caelestia` updates
the installed configuration (with backups); restart the shell afterward.
`oslo make install` updates the engine/library, not the installed example.

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

## Checks

```sh
morf test --no-dbus library/tests/taskwarrior_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/hover_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/panels_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/history_spec.lua
```

The library tests use a real Taskwarrior installation with a temporary
configuration and database, or skip the CLI test if it is unavailable.
Panel tests stub external commands and never change the user's tasks or
record the desktop.
