-- Workspaces, from Hyprland's own sockets.
--
-- Two sockets, both under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`:
-- `.socket2.sock` streams events as `name>>data` lines, and `.socket.sock`
-- answers one request per connection -- `j/monitors`, `j/workspaces`, or a
-- Lua expression after `eval`. The engine watches both and calls back when
-- a line or an answer arrives, so nothing here polls.
--
-- The pill follows one monitor: whichever `MORF_MONITOR` names, else the
-- focused one, and it moves when focus does. A workspace is "occupied" when
-- it has windows, which is what the disc under a number means.

local morf = require("morf")
local io = require("morf.io")
local core = require("morf.core")
local config = require("config")

local hypr = {}

hypr.state = morf.state {
  rows = {},         -- { id, label, exists, active, urgent }
  active = 0,
  active_label = "1",
  monitor = "",
  keymap = "",       -- "US", "RU": the main keyboard's layout, shortened
}
-- The same rows as a plain list, for logic that walks them.
hypr.rows = {}

local pinned = core.env("MORF_MONITOR")
if pinned == "" then pinned = nil end

-- ------------------------------------------------------------------ paths --

local signature = core.env("HYPRLAND_INSTANCE_SIGNATURE")

local function find_sockets()
  if not signature or signature == "" then return nil, nil end
  local runtime = core.env("XDG_RUNTIME_DIR")
  local directories = {}
  if runtime and runtime ~= "" then directories[#directories + 1] = runtime .. "/hypr/" .. signature end
  directories[#directories + 1] = "/tmp/hypr/" .. signature
  for _, directory in ipairs(directories) do
    for _, suffix in ipairs { ".sock", "" } do
      -- A connect that works is the test; the stream itself is opened below.
      local ok, socket = pcall(io.socket, directory .. "/.socket2" .. suffix)
      if ok and socket then
        pcall(socket.close, socket)
        return directory .. "/.socket2" .. suffix, directory .. "/.socket" .. suffix
      end
    end
  end
  return nil, nil
end

local events_path, command_path = find_sockets()
hypr.available = command_path ~= nil

local RECEIVE_LIMIT = 64 * 1024

-- ---------------------------------------------------------------- requests --

-- One connection per request, at most one per key in flight. `on_reply`
-- gets the answer, or nil when there was none.
local requests = {}

local function request(key, payload, on_reply)
  if not command_path or requests[key] then return false end
  local ok, handle = pcall(morf.request_socket, command_path, payload, function(reply)
    requests[key] = nil
    if on_reply then on_reply(reply) end
  end, { timeout_ms = 2000 })
  if not ok then return false end
  requests[key] = handle
  return true
end

--- One request, answered now: for the few things needed before the first
--- frame, such as the gap the compositor keeps between the reserved zone
--- and the windows. Waits at most `timeout_ms`.
function hypr.ask(payload, timeout_ms)
  if not command_path then return nil end
  local ok, socket = pcall(io.socket, command_path)
  if not ok or not socket then return nil end
  local sent = pcall(socket.send, socket, payload)
  if sent then sent = pcall(socket.flush, socket) end
  local reply = ""
  if sent then
    for _ = 1, 8 do
      local received, chunk = pcall(socket.receive, socket, RECEIVE_LIMIT, timeout_ms or 100)
      if not received or chunk == nil or chunk == "" then break end
      reply = reply .. chunk
    end
  end
  pcall(socket.close, socket)
  return reply ~= "" and reply or nil
end

--- The compositor's outer gap on the top edge, in pixels; 0 when unknown.
function hypr.gaps_out()
  local reply = hypr.ask("j/getoption general:gaps_out", 150)
  if not reply then return 0 end
  local ok, option = pcall(io.json.decode, reply)
  if not ok or type(option) ~= "table" then return 0 end
  -- `css` is top, right, bottom, left; `int` is one number for all.
  local top = tostring(option.css or ""):match("^%s*(%d+)")
  return tonumber(top) or tonumber(option["int"]) or 0
end

--- Sends one Lua expression to the compositor. The reply is not needed.
function hypr.eval(expression)
  return request("eval:" .. expression, "eval " .. expression, nil)
end

-- ------------------------------------------------------------------- state --

local pending = { monitors = nil, workspaces = nil }
local last_fingerprint = nil

local function label_of(workspace)
  local name = tostring(workspace.name or "")
  -- `laptop:2`, `samsung:10`: the part after the colon is what the person
  -- calls it.
  local short = name:match(":([^:]+)$") or name
  if short == "" then short = tostring(workspace.id) end
  return short
end

local function rebuild()
  if not pending.monitors or not pending.workspaces then return end
  local ok_monitors, monitors = pcall(io.json.decode, pending.monitors)
  local ok_workspaces, workspaces = pcall(io.json.decode, pending.workspaces)
  pending.monitors, pending.workspaces = nil, nil
  if not ok_monitors or not ok_workspaces then return end
  if type(monitors) ~= "table" or type(workspaces) ~= "table" then return end

  local monitor = nil
  for _, entry in ipairs(monitors) do
    if pinned and entry.name == pinned then monitor = entry end
  end
  if not monitor then
    for _, entry in ipairs(monitors) do
      if entry.focused then monitor = entry end
    end
  end
  monitor = monitor or monitors[1]
  if not monitor then return end
  local active = monitor.activeWorkspace and monitor.activeWorkspace.id or 0

  local mine = {}
  for _, workspace in ipairs(workspaces) do
    if workspace.monitor == monitor.name and type(workspace.id) == "number" and workspace.id > 0 then
      mine[#mine + 1] = workspace
    end
  end
  table.sort(mine, function(a, b) return a.id < b.id end)

  -- Up to `maxWorkspaces` of them, plus any beyond that are active or have
  -- windows -- the same shape as the original's five numbers, which grow
  -- when something lives further out.
  local rows, pieces = {}, {}
  for index, workspace in ipairs(mine) do
    local windows = tonumber(workspace.windows) or 0
    local is_active = workspace.id == active
    if index <= config.maxWorkspaces or is_active or windows > 0 then
      rows[#rows + 1] = {
        id = workspace.id,
        label = label_of(workspace),
        exists = windows > 0,
        active = is_active,
        urgent = false,
      }
      pieces[#pieces + 1] = workspace.id .. ":" .. windows .. (is_active and "*" or "")
    end
  end
  local fingerprint = monitor.name .. "|" .. table.concat(pieces, ",")
  if fingerprint == last_fingerprint then return end
  last_fingerprint = fingerprint

  hypr.rows = rows
  hypr.state.rows:replace(rows, "id")
  hypr.state.active = active
  hypr.state.monitor = monitor.name
  for _, row in ipairs(rows) do
    if row.active then hypr.state.active_label = row.label end
  end
end

--- `English (US)` is `US`, `Russian` is `RU`: the first two letters of
--- what is in the brackets, or of the name.
local function short_layout(name)
  name = tostring(name or "")
  local inner = name:match("%((%a+)%)")
  if inner then return inner:sub(1, 2):upper() end
  return name:sub(1, 2):upper()
end

local function refresh_keymap()
  request("devices", "j/devices", function(reply)
    if not reply then return end
    local ok, devices = pcall(io.json.decode, reply)
    if not ok or type(devices) ~= "table" then return end
    for _, keyboard in ipairs(devices.keyboards or {}) do
      if keyboard.main then
        hypr.state.keymap = short_layout(keyboard.active_keymap)
        return
      end
    end
    local first = (devices.keyboards or {})[1]
    if first then hypr.state.keymap = short_layout(first.active_keymap) end
  end)
end
hypr.refresh_keymap = refresh_keymap

-- Asked again once the answers in flight are in when something changed
-- meanwhile, so the last event of a burst is never lost to the first.
local refreshing = 0
local refresh_again = false

local function refresh()
  if refreshing > 0 then
    refresh_again = true
    return
  end
  local function answered()
    refreshing = refreshing - 1
    if refreshing == 0 and refresh_again then
      refresh_again = false
      refresh()
    end
  end
  refreshing = 2
  local function ask(key, payload)
    local asked = request(key, payload, function(reply)
      pending[key] = reply
      if reply then rebuild() end
      answered()
    end)
    if not asked then answered() end
  end
  ask("monitors", "j/monitors")
  ask("workspaces", "j/workspaces")
end
hypr.refresh = refresh

-- ------------------------------------------------------------------ events --

local WATCHED = {
  workspace = true, workspacev2 = true, focusedmon = true, focusedmonv2 = true,
  openwindow = true, closewindow = true, movewindow = true, movewindowv2 = true,
  createworkspace = true, createworkspacev2 = true,
  destroyworkspace = true, destroyworkspacev2 = true,
  moveworkspace = true, moveworkspacev2 = true, renameworkspace = true,
  urgent = true, monitoradded = true, monitorremoved = true,
}

hypr.on_event = nil

local function on_line(line)
  local name, data = line:match("^([^>]+)>>(.*)$")
  if name and WATCHED[name] then refresh() end
  if name == "activelayout" then refresh_keymap() end
  if name and hypr.on_event then hypr.on_event(name, data or "") end
end

--- Nothing to do any more: the sockets call back. Kept for callers.
function hypr.tick() end

-- Without an event stream the state still follows, just later.
local fallback = nil
local function follow_slowly()
  if not fallback then fallback = morf.timer(2000, refresh, true) end
end

if events_path then
  morf.connect {
    path = events_path,
    on_line = on_line,
    -- The compositor closed the stream; it is not coming back.
    on_close = follow_slowly,
  }
else
  follow_slowly()
end

-- ------------------------------------------------------------------- verbs --

--- Goes to workspace `id`.
function hypr.go_to(id)
  hypr.eval(string.format('hl.dispatch(hl.dsp.focus({ workspace = "%d" }))', id))
  refresh()
end

--- The next or previous workspace among the ones shown, wrapping.
function hypr.step(by)
  local rows = hypr.rows
  local count = #rows
  if count == 0 then return end
  local index = 1
  for i, row in ipairs(rows) do
    if row.active then index = i break end
  end
  local target = rows[((index - 1 + by) % count) + 1]
  if target then hypr.go_to(target.id) end
end

refresh()
refresh_keymap()

--- Focuses a window by its address.
function hypr.focus_window(address)
  hypr.eval(string.format('hl.dispatch(hl.dsp.focus({ window = "address:%s" }))', address))
end

return hypr
