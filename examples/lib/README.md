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
