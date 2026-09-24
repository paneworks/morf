-- Hyprland, over its own two sockets.
--
-- morf's core does not know which compositor it runs under, and should not:
-- what Hyprland calls a workspace, a submap or a window address is Hyprland's
-- vocabulary, so it lives here, in a library a configuration may require or
-- ignore. Nothing below needs more than `morf.connect`,
-- `morf.request_socket`, `morf.json`, `morf.state`, `morf.timer`,
-- `morf.fs` and `morf.log`; nothing spawns
-- `hyprctl`, because a process per question is slower than the question.
--
-- Both sockets live in `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`:
--
--   `.socket.sock`   one request per connection. The client writes a command
--                    (`j/monitors`, `/dispatch workspace 2`, `[[BATCH]]a;b`),
--                    the compositor writes the answer and closes. `j/` asks
--                    for JSON; a batch answers each command followed by three
--                    newlines.
--   `.socket2.sock`  a stream of `EVENT>>DATA` lines, one per change, for as
--                    long as the connection is open.
--
-- The engine watches both sockets and calls back as lines and answers
-- arrive, so nothing here polls: an idle compositor costs nothing, and a
-- change is seen when it is written, not at the next tick.
--
-- What a configuration gets:
--
--   hyprland.state       a `morf.state`, kept current from the event stream:
--                        `monitors`, `workspaces`, `clients` (list models keyed
--                        by `id`, `id` and `address`), `active_workspace`
--                        {id, name}, `active_window` {address, class, title},
--                        `focused_monitor`, `keyboard`, `keyboard_layout`,
--                        `submap`, `fullscreen`, `urgent`, `connected`.
--   hyprland.on(name, fn)  typed events, `fn` gets the parsed fields;
--                        returns a handle with `:off()`. `"*"` sees every
--                        event as (name, raw data); `"refreshed"` hears
--                        `(kind)` once a refetched answer is in the state.
--   hyprland.request / json / batch  asynchronous questions, queued, with a
--                        bound on how many connections are open at once.
--   dispatch, keyword, eval, reload, and the read-only helpers at the end.
--
-- Under anything but Hyprland `available()` is false, no timer runs, the
-- state stays empty and every call answers its callback with `nil,
-- "unavailable"`: a configuration can require this unconditionally.

local morf = require("morf")

local hyprland = {}

local json = morf.json
local log = morf.log

-- --------------------------------------------------------------- settings --

local defaults = {
  poll_ms = 30,             -- accepted and ignored: nothing polls any more
  max_in_flight = 4,        -- request connections open at once
  max_queue = 256,          -- requests waiting for a connection
  request_timeout_ms = 5000,
  reply_limit = 4 * 1024 * 1024,   -- a raw reply; JSON decode caps at 1 MiB
  line_limit = 64 * 1024,          -- one event line
  reconnect_min_ms = 250,
  reconnect_max_ms = 8000,
}

local BATCH_SEPARATOR = "\n\n\n"

local settings = {}
for key, value in pairs(defaults) do settings[key] = value end

-- ------------------------------------------------------------------ state --

hyprland.state = morf.state {
  connected = false,
  monitors = {},
  -- Every connected output, lit or not (`monitors all`), keyed by `name`;
  -- `monitors` keeps only the lit ones.
  outputs = {},
  workspaces = {},
  clients = {},
  active_workspace = { id = 0, name = "" },
  active_window = { address = "", class = "", title = "" },
  focused_monitor = "",
  keyboard = "",
  keyboard_layout = "",
  submap = "",
  fullscreen = false,
  urgent = "",
}

local state = hyprland.state

-- Plain copies of what the models hold, for lookups from logic: a model is
-- for a Repeater, a table is for `for`.
local rows = { monitors = {}, outputs = {}, workspaces = {}, clients = {} }
-- The last decoded answers, the source every row is rebuilt from.
local latest = { monitors = nil, workspaces = nil, clients = nil }
-- Urgency is not in any request's answer, only in the event, so it is kept
-- here until the window is focused.
local urgent = {}
local main_keyboard = nil

-- ------------------------------------------------------------------ paths --

local instance = { directory = nil, signature = nil, runtime = nil, pinned = false }

local function env(name)
  local value = morf.env(name)
  if value == "" then return nil end
  return value
end

local function socket_path(name)
  return instance.directory and (instance.directory .. "/" .. name) or nil
end

-- When Hyprland restarts it comes back under a new signature, and the one in
-- our environment points at a directory that is gone. Unless the caller
-- pinned an instance, the newest directory under `hypr/` that has an event
-- socket is the compositor now running.
local function rediscover()
  if instance.pinned or not instance.runtime then return false end
  local root = instance.runtime .. "/hypr"
  local entries = morf.fs.list(root)
  if type(entries) ~= "table" then return false end
  local best, best_time = nil, -1
  for _, entry in ipairs(entries) do
    if entry.is_dir and morf.fs.exists(root .. "/" .. entry.name .. "/.socket2.sock") then
      local modified = tonumber(entry.modified) or 0
      if modified > best_time then best, best_time = entry.name, modified end
    end
  end
  if best and best ~= instance.signature then
    log.info("hyprland: following the compositor to instance", best)
    instance.signature = best
    instance.directory = root .. "/" .. best
    return true
  end
  return false
end

-- ------------------------------------------------------------- utilities --

local function protected(what, fn, ...)
  local ok, err = pcall(fn, ...)
  if not ok then log.warn("hyprland:", what, "failed:", tostring(err)) end
end

local function close_socket(socket)
  if socket then pcall(socket.close, socket) end
end

local function cancel(timer)
  if timer then pcall(timer.cancel, timer) end
end

local function number_or(value, fallback)
  local n = tonumber(value)
  if n == nil then return fallback end
  return n
end

local function text(value)
  if type(value) == "string" then return value end
  if type(value) == "number" or type(value) == "boolean" then return tostring(value) end
  return ""
end

local function flag(value)
  return value == true or value == 1 or value == "1"
end

-- Event addresses come without `0x`, JSON ones with it; one spelling is kept.
local function address_of(value)
  value = text(value)
  if value == "" or value == "0x0" then return "" end
  if value:sub(1, 2) == "0x" then return value end
  return "0x" .. value
end

-- Splits on commas into `count` fields, the last taking the rest: titles and
-- names may contain commas, and they are always last.
local function split(data, count)
  local fields, position = {}, 1
  for _ = 1, count - 1 do
    local comma = data:find(",", position, true)
    if not comma then break end
    fields[#fields + 1] = data:sub(position, comma - 1)
    position = comma + 1
  end
  fields[#fields + 1] = data:sub(position)
  return fields
end

-- The same from the right, for `NAME,MONITOR` where the name is the part
-- that may hold a comma and the monitor never does.
local function split_right(data, count)
  local fields, finish = {}, #data
  for _ = 1, count - 1 do
    local comma = nil
    for index = finish, 1, -1 do
      if data:byte(index) == 44 then comma = index break end
    end
    if not comma then break end
    table.insert(fields, 1, data:sub(comma + 1, finish))
    finish = comma - 1
  end
  table.insert(fields, 1, data:sub(1, finish))
  return fields
end

-- --------------------------------------------------------------- requests --

local queue = {}
local in_flight = {}
local pump_requests

local function unavailable(callback)
  if callback then protected("callback", callback, nil, "unavailable") end
end

local function finish(entry, reply, err)
  close_socket(entry.socket)
  entry.socket = nil
  if entry.callback then protected("request callback", entry.callback, reply, err) end
end

local function forget(entry)
  for index = #in_flight, 1, -1 do
    if in_flight[index] == entry then table.remove(in_flight, index) end
  end
end

-- One connection, one command, one answer: the engine connects, writes,
-- collects until the compositor closes, and calls back once.
local function begin(entry)
  local path = socket_path(".socket.sock")
  if not path then return finish(entry, nil, "unavailable") end
  local ok, socket = pcall(morf.request_socket, path, entry.payload, function(reply, err)
    forget(entry)
    entry.socket = nil
    if reply == nil and err ~= "timed out" then
      if tostring(err):find("exceeds", 1, true) then
        err = "reply exceeds " .. settings.reply_limit .. " bytes"
      else
        err = "request failed: " .. tostring(err)
      end
    end
    finish(entry, reply, err)
    pump_requests()
  end, { timeout_ms = settings.request_timeout_ms, max_bytes = settings.reply_limit })
  if not ok then
    return finish(entry, nil, "could not connect: " .. tostring(socket))
  end
  entry.socket = socket
  in_flight[#in_flight + 1] = entry
end

pump_requests = function()
  while #in_flight < settings.max_in_flight and #queue > 0 do
    begin(table.remove(queue, 1))
  end
end

--- Sends one command and calls `callback(reply, err)` with the raw answer
--- once the compositor closes the connection. Returns false when the command
--- was refused before it was sent (no Hyprland, a full queue, too long).
function hyprland.request(command, callback)
  if not instance.directory then
    unavailable(callback)
    return false
  end
  command = text(command)
  if command == "" or #command > 64 * 1024 then
    if callback then protected("callback", callback, nil, "command is empty or over 64 KiB") end
    return false
  end
  if #queue >= settings.max_queue then
    log.warn("hyprland: request queue is full, dropping", command:sub(1, 64))
    if callback then protected("callback", callback, nil, "queue full") end
    return false
  end
  queue[#queue + 1] = { payload = command, callback = callback }
  pump_requests()
  return true
end

-- Hyprland (0.56) writes a keyboard with no layout active -- a virtual
-- keyboard such as wtype's, wayvnc's or KDE Connect's -- as
-- `"active_layout_index": none`, which is not JSON, and one such field would
-- lose every device. A bare `none` where a value goes is read as null.
local function repaired(reply)
  return (reply:gsub("(:%s*)none(%s*[,}%]])", "%1null%2"))
end
hyprland.repaired_json = repaired

local function decode(reply)
  if type(reply) ~= "string" then return nil, "no reply" end
  local ok, value = pcall(json.decode, reply)
  if not ok and reply:find(":%s*none") then ok, value = pcall(json.decode, repaired(reply)) end
  if not ok then
    -- Hyprland answers a bad command in prose, even when JSON was asked for.
    local first = reply:gsub("^%s+", ""):sub(1, 200)
    return nil, first ~= "" and first or tostring(value)
  end
  return value
end

--- Like `request`, with `j/` added when missing and the answer decoded:
--- `callback(value, err)`.
function hyprland.json(command, callback)
  command = text(command)
  if command:sub(1, 2) ~= "j/" then command = "j/" .. command:gsub("^/", "") end
  return hyprland.request(command, function(reply, err)
    if not callback then return end
    if not reply then return callback(nil, err) end
    local value, problem = decode(reply)
    callback(value, problem)
  end)
end

--- Several commands on one connection. `callback(replies, err)` gets one
--- raw answer per command, in order. A command may not contain `;`, which is
--- the batch's separator -- `eval` chunks usually do, so send those alone.
function hyprland.batch(commands, callback)
  local list = {}
  for _, command in ipairs(commands or {}) do
    command = text(command)
    if command:find(";", 1, true) then
      if callback then protected("callback", callback, nil, "batched command contains ';'") end
      return false
    end
    list[#list + 1] = command
  end
  if #list == 0 then
    if callback then protected("callback", callback, {}, nil) end
    return true
  end
  return hyprland.request("[[BATCH]]" .. table.concat(list, ";"), function(reply, err)
    if not callback then return end
    if not reply then return callback(nil, err) end
    local replies, position = {}, 1
    while #replies < #list do
      local at = reply:find(BATCH_SEPARATOR, position, true)
      if not at or #replies == #list - 1 then
        -- The last answer, or a reply that did not split as expected: what
        -- is left is the remaining answer, trailing separator removed.
        local rest = reply:sub(position)
        if rest:sub(-#BATCH_SEPARATOR) == BATCH_SEPARATOR then
          rest = rest:sub(1, -#BATCH_SEPARATOR - 1)
        end
        replies[#replies + 1] = rest
        break
      end
      replies[#replies + 1] = reply:sub(position, at - 1)
      position = at + #BATCH_SEPARATOR
    end
    callback(replies, nil)
  end)
end

-- ---------------------------------------------------------------- rebuild --

local function mode_list(value)
  local out = {}
  if type(value) ~= "table" then return out end
  for _, mode in ipairs(value) do
    if type(mode) == "string" then out[#out + 1] = mode end
  end
  return out
end

local function monitor_row(monitor)
  local active = type(monitor.activeWorkspace) == "table" and monitor.activeWorkspace or {}
  local special = type(monitor.specialWorkspace) == "table" and monitor.specialWorkspace or {}
  return {
    id = number_or(monitor.id, -1),
    name = text(monitor.name),
    description = text(monitor.description),
    make = text(monitor.make),
    model = text(monitor.model),
    x = number_or(monitor.x, 0),
    y = number_or(monitor.y, 0),
    width = number_or(monitor.width, 0),
    height = number_or(monitor.height, 0),
    scale = number_or(monitor.scale, 1),
    transform = number_or(monitor.transform, 0),
    refresh_rate = number_or(monitor.refreshRate, 0),
    focused = monitor.focused == true,
    disabled = monitor.disabled == true,
    serial = text(monitor.serial),
    -- "2560x1440@144.00Hz", as the monitor reported them.
    available_modes = mode_list(monitor.availableModes),
    vrr = monitor.vrr == true,
    -- The id of the output this one mirrors, -1 for none.
    mirror_of = number_or(monitor.mirrorOf, -1),
    -- Unlike `disabled`, DPMS off keeps the output in the layout; it is dark.
    dpms = monitor.dpmsStatus ~= false,
    active_workspace = number_or(active.id, 0),
    active_workspace_name = text(active.name),
    special_workspace = number_or(special.id, 0),
    special_workspace_name = text(special.name),
    -- What layer surfaces keep clear on each edge: left, top, right, bottom.
    reserved = {
      number_or(type(monitor.reserved) == "table" and monitor.reserved[1], 0),
      number_or(type(monitor.reserved) == "table" and monitor.reserved[2], 0),
      number_or(type(monitor.reserved) == "table" and monitor.reserved[3], 0),
      number_or(type(monitor.reserved) == "table" and monitor.reserved[4], 0),
    },
  }
end

local function client_row(client, active_address)
  local workspace = type(client.workspace) == "table" and client.workspace or {}
  local at = type(client.at) == "table" and client.at or {}
  local size = type(client.size) == "table" and client.size or {}
  local address = address_of(client.address)
  return {
    address = address,
    class = text(client.class),
    title = text(client.title),
    initial_class = text(client.initialClass),
    initial_title = text(client.initialTitle),
    workspace = number_or(workspace.id, 0),
    workspace_name = text(workspace.name),
    monitor = number_or(client.monitor, -1),
    x = number_or(at[1], 0),
    y = number_or(at[2], 0),
    width = number_or(size[1], 0),
    height = number_or(size[2], 0),
    floating = client.floating == true,
    pinned = client.pinned == true,
    hidden = client.hidden == true,
    mapped = client.mapped ~= false,
    fullscreen = number_or(client.fullscreen, 0),
    xwayland = client.xwayland == true,
    pid = number_or(client.pid, 0),
    focus_history = number_or(client.focusHistoryID, -1),
    active = address ~= "" and address == active_address,
    urgent = urgent[address] == true,
  }
end

local function workspace_row(workspace, shown, focused_id, urgent_workspaces)
  local id = number_or(workspace.id, 0)
  return {
    id = id,
    name = text(workspace.name),
    monitor = text(workspace.monitor),
    monitor_id = number_or(workspace.monitorID, -1),
    windows = number_or(workspace.windows, 0),
    fullscreen = workspace.hasfullscreen == true,
    last_window = address_of(workspace.lastwindow),
    last_window_title = text(workspace.lastwindowtitle),
    persistent = workspace.ispersistent == true,
    special = id < 0,
    visible = shown[id] == true,
    active = id == focused_id,
    urgent = urgent_workspaces[id] == true,
  }
end

local function by_id(a, b) return a.id < b.id end

-- Rebuilds every row from the latest answers and hands them to the models.
-- The three lists are cross-referenced (a workspace is urgent when a window
-- on it is, a window is active when its address is the focused one), so they
-- are rebuilt together; `replace` keyed by identity keeps unchanged rows'
-- nodes, so a rebuild that changes one title patches one delegate.
local function rebuild()
  local active_address = state.active_window.address
  local focused_id = state.active_workspace.id

  if latest.monitors then
    local list, shown, focused, every = {}, {}, nil, {}
    for _, monitor in ipairs(latest.monitors) do
      if type(monitor) == "table" then
        local row = monitor_row(monitor)
        every[#every + 1] = row
      end
    end
    table.sort(every, function(a, b) return a.name < b.name end)
    for _, row in ipairs(every) do
      if not row.disabled then
        list[#list + 1] = row
        shown[row.active_workspace] = true
        if row.special_workspace ~= 0 then shown[row.special_workspace] = true end
        if row.focused then focused = row end
      end
    end
    table.sort(list, by_id)
    rows.monitors = list
    rows.outputs = every
    rows.shown = shown
    if focused then
      state.focused_monitor = focused.name
      state.active_workspace.id = focused.active_workspace
      state.active_workspace.name = focused.active_workspace_name
      focused_id = focused.active_workspace
    end
    state.monitors:replace(list, "id")
    state.outputs:replace(every, "name")
  end

  local urgent_workspaces = {}
  if latest.clients then
    local list = {}
    for _, client in ipairs(latest.clients) do
      if type(client) == "table" then
        local row = client_row(client, active_address)
        if row.address ~= "" then
          list[#list + 1] = row
          if row.urgent then urgent_workspaces[row.workspace] = true end
        end
      end
    end
    table.sort(list, function(a, b)
      if a.workspace ~= b.workspace then return a.workspace < b.workspace end
      return a.address < b.address
    end)
    rows.clients = list
    state.clients:replace(list, "address")
  end

  if latest.workspaces then
    local list, shown = {}, rows.shown or {}
    for _, workspace in ipairs(latest.workspaces) do
      if type(workspace) == "table" then
        list[#list + 1] = workspace_row(workspace, shown, focused_id, urgent_workspaces)
      end
    end
    table.sort(list, by_id)
    rows.workspaces = list
    state.workspaces:replace(list, "id")
    local fullscreen = false
    for _, row in ipairs(list) do
      if row.id == focused_id then fullscreen = row.fullscreen end
    end
    state.fullscreen = fullscreen
  end
end

-- ------------------------------------------------------------- refetching --

-- Which answers an event makes stale. An event that carries its own news
-- (the new layout, the new submap, the focused address) is applied directly
-- and invalidates nothing it did not change.
local INVALIDATES = {
  workspace = { monitors = true, workspaces = true },
  workspacev2 = { monitors = true, workspaces = true },
  focusedmon = { monitors = true, workspaces = true },
  focusedmonv2 = { monitors = true, workspaces = true },
  createworkspace = { workspaces = true },
  createworkspacev2 = { workspaces = true },
  destroyworkspace = { workspaces = true },
  destroyworkspacev2 = { workspaces = true },
  moveworkspace = { monitors = true, workspaces = true, clients = true },
  moveworkspacev2 = { monitors = true, workspaces = true, clients = true },
  renameworkspace = { monitors = true, workspaces = true, clients = true },
  activespecial = { monitors = true, workspaces = true },
  activespecialv2 = { monitors = true, workspaces = true },
  monitoradded = { monitors = true, workspaces = true },
  monitoraddedv2 = { monitors = true, workspaces = true },
  monitorremoved = { monitors = true, workspaces = true, clients = true },
  monitorremovedv2 = { monitors = true, workspaces = true, clients = true },
  openwindow = { workspaces = true, clients = true },
  closewindow = { workspaces = true, clients = true },
  movewindow = { workspaces = true, clients = true },
  movewindowv2 = { workspaces = true, clients = true },
  changefloatingmode = { clients = true },
  fullscreen = { workspaces = true, clients = true },
  pin = { clients = true },
  minimized = { clients = true },
  togglegroup = { clients = true },
  moveintogroup = { clients = true },
  moveoutofgroup = { clients = true },
  configreloaded = { monitors = true, workspaces = true, clients = true, devices = true },
}

local dirty = {}
local fetching = {}
-- Bumped when an event sets what an answer would: a submap answer asked for
-- before `submap>>resize` arrived is older news than the event, and must not
-- undo it.
local generation = { submap = 0, devices = 0 }

local QUERIES = {
  -- `all`, so a disabled output is listed too (in `outputs`) and can be
  -- lit again from a settings page.
  monitors = "j/monitors all",
  workspaces = "j/workspaces",
  clients = "j/clients",
  devices = "j/devices",
  submap = "j/submap",
}

local function apply_devices(devices)
  if type(devices) ~= "table" or type(devices.keyboards) ~= "table" then return end
  local chosen = nil
  for _, keyboard in ipairs(devices.keyboards) do
    if type(keyboard) == "table" and keyboard.main == true then chosen = keyboard end
  end
  chosen = chosen or devices.keyboards[1]
  if type(chosen) ~= "table" then return end
  main_keyboard = text(chosen.name)
  state.keyboard = main_keyboard
  state.keyboard_layout = text(chosen.active_keymap)
end

local hyprland_flush
local emit

local function fetch(kind)
  if fetching[kind] then
    -- Already asked; ask again once this answer is in, since the event that
    -- dirtied it may have come after the compositor wrote that answer.
    fetching[kind] = "again"
    return
  end
  fetching[kind] = true
  local asked = generation[kind]
  hyprland.json(QUERIES[kind], function(value, err)
    local again = fetching[kind] == "again"
    fetching[kind] = nil
    if again then
      -- An event dirtied it while this answer was on its way.
      dirty[kind] = true
      hyprland_flush()
    end
    if asked ~= generation[kind] then return end
    if value == nil then
      log.debug("hyprland: refreshing", kind, "failed:", tostring(err))
      return
    end
    if kind == "devices" then
      apply_devices(value)
    elseif kind == "submap" then
      local name = text(value)
      state.submap = name == "default" and "" or name
    elseif type(value) == "table" then
      latest[kind] = value
      rebuild()
    end
    -- After the state holds the answer: a consumer that needs the whole
    -- fresh list (a settings page, a reconciler) hears it here.
    emit("refreshed", kind)
  end)
end

-- Called after every event line. A burst of events (a workspace switch is
-- four or five) is still one or two refetches per answer it touched: the
-- first line asks, and the rest only mark the answer to be asked again
-- once that one is in.
local function flush_dirty()
  for kind in pairs(dirty) do
    dirty[kind] = nil
    fetch(kind)
  end
end

hyprland_flush = function()
  if instance.directory then flush_dirty() end
end

local function invalidate_all()
  for kind in pairs(QUERIES) do dirty[kind] = true end
end

-- ----------------------------------------------------------------- events --

local function boolean(data) return data == "1" end

local function one(data) return data end

-- How each documented event's data becomes arguments. Anything not listed
-- is passed as its raw data string.
local PARSERS = {
  workspace = one,
  workspacev2 = function(data)
    local f = split(data, 2)
    return tonumber(f[1]), f[2] or ""
  end,
  focusedmon = function(data)
    local f = split(data, 2)
    return f[1], f[2] or ""
  end,
  focusedmonv2 = function(data)
    local f = split(data, 2)
    return f[1], tonumber(f[2])
  end,
  activewindow = function(data)
    local f = split(data, 2)
    return f[1], f[2] or ""
  end,
  activewindowv2 = function(data) return address_of(data:gsub(",", "")) end,
  fullscreen = boolean,
  monitoradded = one,
  monitorremoved = one,
  monitoraddedv2 = function(data)
    local f = split(data, 3)
    return tonumber(f[1]), f[2] or "", f[3] or ""
  end,
  monitorremovedv2 = function(data)
    local f = split(data, 3)
    return tonumber(f[1]), f[2] or "", f[3] or ""
  end,
  createworkspace = one,
  destroyworkspace = one,
  createworkspacev2 = function(data)
    local f = split(data, 2)
    return tonumber(f[1]), f[2] or ""
  end,
  destroyworkspacev2 = function(data)
    local f = split(data, 2)
    return tonumber(f[1]), f[2] or ""
  end,
  moveworkspace = function(data)
    local f = split_right(data, 2)
    return f[1], f[2] or ""
  end,
  moveworkspacev2 = function(data)
    local head = split(data, 2)
    local tail = split_right(head[2] or "", 2)
    return tonumber(head[1]), tail[1], tail[2] or ""
  end,
  renameworkspace = function(data)
    local f = split(data, 2)
    return tonumber(f[1]), f[2] or ""
  end,
  activespecial = function(data)
    local f = split_right(data, 2)
    return f[1], f[2] or ""
  end,
  activespecialv2 = function(data)
    local head = split(data, 2)
    local tail = split_right(head[2] or "", 2)
    return tonumber(head[1]), tail[1], tail[2] or ""
  end,
  activelayout = function(data)
    local f = split(data, 2)
    return f[1], f[2] or ""
  end,
  openwindow = function(data)
    local f = split(data, 4)
    return address_of(f[1]), f[2] or "", f[3] or "", f[4] or ""
  end,
  closewindow = function(data) return address_of(data) end,
  movewindow = function(data)
    local f = split(data, 2)
    return address_of(f[1]), f[2] or ""
  end,
  movewindowv2 = function(data)
    local f = split(data, 3)
    return address_of(f[1]), tonumber(f[2]), f[3] or ""
  end,
  openlayer = one,
  closelayer = one,
  submap = one,
  changefloatingmode = function(data)
    local f = split(data, 2)
    return address_of(f[1]), f[2] == "1"
  end,
  urgent = function(data) return address_of(data) end,
  screencast = function(data)
    local f = split(data, 2)
    return f[1] == "1", tonumber(f[2])
  end,
  windowtitle = function(data) return address_of(data) end,
  windowtitlev2 = function(data)
    local f = split(data, 2)
    return address_of(f[1]), f[2] or ""
  end,
  togglegroup = function(data)
    local f = split(data, 2)
    local members = {}
    for address in (f[2] or ""):gmatch("[^,]+") do members[#members + 1] = address_of(address) end
    return f[1] == "1", members
  end,
  moveintogroup = function(data) return address_of(data) end,
  moveoutofgroup = function(data) return address_of(data) end,
  ignoregrouplock = boolean,
  lockgroups = boolean,
  configreloaded = function() end,
  pin = function(data)
    local f = split(data, 2)
    return address_of(f[1]), f[2] == "1"
  end,
  minimized = function(data)
    local f = split(data, 2)
    return address_of(f[1]), f[2] == "1"
  end,
  bell = function(data) return address_of(data) end,
  custom = one,
}

--- The parsed arguments `on(name, …)` would receive for one event line's
--- name and data; exposed so a configuration can test its handlers.
function hyprland.parse_event(name, data)
  local parser = PARSERS[name] or one
  return parser(text(data))
end

local listeners = {}

--- Calls `fn(args...)` for every `name` event, with the fields parsed
--- (ids as numbers, addresses with `0x`, flags as booleans). `"*"` receives
--- `(name, data)` for every event; `"connected"` and `"disconnected"` report
--- the event socket. Returns a handle whose `:off()` stops it.
function hyprland.on(name, fn)
  local list = listeners[name]
  if not list then
    list = {}
    listeners[name] = list
  end
  local entry = { fn = fn }
  list[#list + 1] = entry
  return {
    off = function()
      for index = #list, 1, -1 do
        if list[index] == entry then table.remove(list, index) end
      end
    end,
  }
end

emit = function(name, ...)
  local list = listeners[name]
  if not list or #list == 0 then return end
  -- A copy, so a handler that calls `:off()` does not skip its neighbour.
  local copy = {}
  for index, entry in ipairs(list) do copy[index] = entry end
  for _, entry in ipairs(copy) do protected("handler for " .. name, entry.fn, ...) end
end

-- Applies what an event says outright before anything is refetched, so the
-- indicator moves on the event rather than a round trip later.
local function apply_event(name, a, b)
  if name == "workspacev2" then
    if a then
      state.active_workspace.id = a
      state.active_workspace.name = b
    end
  elseif name == "focusedmon" then
    state.focused_monitor = a
  elseif name == "activewindow" then
    state.active_window.class = a
    state.active_window.title = b
  elseif name == "activewindowv2" then
    state.active_window.address = a
    if a == "" then
      state.active_window.class = ""
      state.active_window.title = ""
    end
    if urgent[a] then
      urgent[a] = nil
      if state.urgent == a then state.urgent = "" end
    end
    rebuild()
  elseif name == "windowtitlev2" then
    if a == state.active_window.address then state.active_window.title = b end
    for _, client in ipairs(latest.clients or {}) do
      if type(client) == "table" and address_of(client.address) == a then
        client.title = b
      end
    end
    rebuild()
  elseif name == "fullscreen" then
    state.fullscreen = a
  elseif name == "urgent" then
    if a ~= "" and a ~= state.active_window.address then
      urgent[a] = true
      state.urgent = a
      rebuild()
    end
  elseif name == "closewindow" then
    if urgent[a] then
      urgent[a] = nil
      if state.urgent == a then state.urgent = "" end
    end
  elseif name == "submap" then
    generation.submap = generation.submap + 1
    state.submap = a
  elseif name == "activelayout" then
    -- Every keyboard announces its own layout; the one shown is the main
    -- keyboard's, which is the only one the person is typing on.
    if main_keyboard == nil or a == main_keyboard then
      generation.devices = generation.devices + 1
      state.keyboard = a
      state.keyboard_layout = b
    end
  end
end

local function handle_line(line)
  local separator = line:find(">>", 1, true)
  if not separator or separator == 1 then
    log.debug("hyprland: ignoring malformed event line", line:sub(1, 80))
    return
  end
  local name = line:sub(1, separator - 1)
  local data = line:sub(separator + 2)
  local parser = PARSERS[name] or one
  local ok, a, b, c, d = pcall(parser, data)
  if not ok then
    log.debug("hyprland: could not parse", name, tostring(a))
    return
  end
  protected("applying " .. name, apply_event, name, a, b, c, d)
  local stale = INVALIDATES[name]
  if stale then
    for kind in pairs(stale) do dirty[kind] = true end
  end
  emit("*", name, data)
  emit(name, a, b, c, d)
end

-- ------------------------------------------------------------- connection --

local events = nil       -- the `.socket2.sock` connection
local retry = nil        -- the timer that tries it again
local backoff_ms = defaults.reconnect_min_ms
local failures = 0
local connect

local function schedule_reconnect()
  cancel(retry)
  retry = morf.timer(backoff_ms, function()
    retry = nil
    protected("reconnect", connect)
  end, false)
  backoff_ms = math.min(backoff_ms * 2, settings.reconnect_max_ms)
end

local function disconnect(why)
  close_socket(events)
  events = nil
  if state.connected then
    state.connected = false
    log.info("hyprland: event stream closed:", why)
    emit("disconnected", why)
  end
  schedule_reconnect()
end

local function on_line(line)
  -- The engine cuts a line one byte past the limit, so an overlong one is
  -- recognisable here and dropped whole rather than parsed in part.
  if #line > settings.line_limit then
    log.warn("hyprland: dropping an event line over", settings.line_limit, "bytes")
    return
  end
  if #line > 0 then handle_line(line) end
  flush_dirty()
end

local function unreachable(why)
  events = nil
  failures = failures + 1
  if failures == 1 then
    log.warn("hyprland: event socket unreachable, retrying:", tostring(why))
  end
  -- The instance may have been replaced; look once before backing off.
  if failures == 1 and rediscover() then return connect() end
  schedule_reconnect()
end

connect = function()
  local path = socket_path(".socket2.sock")
  if not path then return end
  local up = false
  local ok, made = pcall(morf.connect, {
    path = path,
    max_line = settings.line_limit + 1,
    on_connect = function()
      up = true
      if failures > 0 then log.info("hyprland: event socket reachable again") end
      failures = 0
      backoff_ms = settings.reconnect_min_ms
      state.connected = true
      -- Whatever happened while no one was listening is unknown, so
      -- everything is asked again.
      invalidate_all()
      emit("connected")
      flush_dirty()
    end,
    on_line = function(line) protected("event", on_line, line) end,
    on_close = function(reason)
      if up then
        disconnect(reason == "eof" and "compositor closed the stream" or reason)
      else
        unreachable(reason)
      end
    end,
  })
  if not ok then return unreachable(made) end
  -- Held from the start, so `stop` can close a connect still under way.
  events = made
end

-- ------------------------------------------------------------- lifecycle --

local started = false

--- Starts talking to Hyprland. Called by itself shortly after `require`
--- when the environment names an instance; call it first to choose one:
---   runtime_dir   instead of `$XDG_RUNTIME_DIR`
---   signature     instead of `$HYPRLAND_INSTANCE_SIGNATURE` (and pins it:
---                 no following a restarted compositor to a new instance)
---   poll_ms, max_in_flight, max_queue, request_timeout_ms, reply_limit,
---   line_limit, reconnect_min_ms, reconnect_max_ms  (see `defaults`)
--- Returns whether an instance was found.
function hyprland.start(options)
  options = options or {}
  hyprland.stop()
  for key, value in pairs(defaults) do
    settings[key] = options[key] ~= nil and options[key] or value
  end
  instance.runtime = options.runtime_dir or env("XDG_RUNTIME_DIR")
  instance.signature = options.signature or env("HYPRLAND_INSTANCE_SIGNATURE")
  instance.pinned = options.signature ~= nil
  started = true
  if not instance.runtime or not instance.signature or instance.signature == "" then
    instance.directory = nil
    return false
  end
  instance.directory = instance.runtime .. "/hypr/" .. instance.signature
  backoff_ms = settings.reconnect_min_ms
  failures = 0
  connect()
  flush_dirty()
  return true
end

--- Closes every socket, stops trying to reconnect and answers pending
--- requests with `nil, "stopped"`. The state keeps its last values.
function hyprland.stop()
  cancel(retry)
  retry = nil
  if events then
    close_socket(events)
    events = nil
  end
  local pending = {}
  for _, entry in ipairs(in_flight) do pending[#pending + 1] = entry end
  for _, entry in ipairs(queue) do pending[#pending + 1] = entry end
  in_flight, queue = {}, {}
  for _, entry in ipairs(pending) do finish(entry, nil, "stopped") end
  dirty, fetching = {}, {}
  instance.directory = nil
  started = false
  state.connected = false
end

--- Whether there is a Hyprland instance to talk to. `state.connected` says
--- whether its event stream is open right now.
function hyprland.available()
  if started then return instance.directory ~= nil end
  return env("HYPRLAND_INSTANCE_SIGNATURE") ~= nil and env("XDG_RUNTIME_DIR") ~= nil
end

--- Asks for everything again, as after a reconnect.
function hyprland.refresh()
  invalidate_all()
  if started then hyprland_flush() end
end

-- A deferred start, so a configuration that calls `start{...}` in the same
-- chunk as the `require` gets its options rather than a second connection.
if hyprland.available() then
  morf.timer(1, function()
    if not started then protected("start", hyprland.start) end
  end, false)
end

-- --------------------------------------------------------------- lookups --

local function copy(row)
  local out = {}
  for key, value in pairs(row) do out[key] = value end
  return out
end

--- A monitor's row by name or id, or nil.
function hyprland.monitor(key)
  for _, row in ipairs(rows.monitors) do
    if row.name == key or row.id == key then return copy(row) end
  end
end

--- Every connected output, lit or not (`monitors all`), as plain rows;
--- the lit ones are also `monitor(...)`'s.
function hyprland.outputs()
  local out = {}
  for index, row in ipairs(rows.outputs) do out[index] = copy(row) end
  return out
end

--- A workspace's row by id or name, or nil.
function hyprland.workspace(key)
  for _, row in ipairs(rows.workspaces) do
    if row.id == key or row.name == key then return copy(row) end
  end
end

--- A window's row by address (with or without `0x`), or nil.
function hyprland.client(address)
  address = address_of(address)
  for _, row in ipairs(rows.clients) do
    if row.address == address then return copy(row) end
  end
end

--- The windows on workspace `id`, as rows.
function hyprland.workspace_windows(id)
  local out = {}
  for _, row in ipairs(rows.clients) do
    if row.workspace == id then out[#out + 1] = copy(row) end
  end
  return out
end

--- Whether workspace `id` has windows.
function hyprland.occupied(id)
  for _, row in ipairs(rows.workspaces) do
    if row.id == id then return row.windows > 0 end
  end
  return false
end

--- The workspace id shown on monitor `name` (the focused one when omitted).
function hyprland.monitor_workspace(name)
  for _, row in ipairs(rows.monitors) do
    if (name and row.name == name) or (not name and row.focused) then
      return row.active_workspace
    end
  end
end

--- Plain copies of the three lists, for logic that walks them.
function hyprland.snapshot()
  local out = {}
  for kind, list in pairs(rows) do
    if kind ~= "shown" then
      out[kind] = {}
      for index, row in ipairs(list) do out[kind][index] = copy(row) end
    end
  end
  return out
end

-- -------------------------------------------------------------- commands --

-- Commands answer `ok` on success and prose otherwise; callbacks get both.
local function command(payload, callback)
  return hyprland.request(payload, function(reply, err)
    if not callback then
      if reply and reply ~= "ok" then log.warn("hyprland:", payload:sub(1, 80), "->", reply) end
      return
    end
    if not reply then return callback(false, err) end
    callback(reply == "ok", reply)
  end)
end

--- Runs a dispatcher: `dispatch("workspace", "2")`. On a Lua-configured
--- Hyprland (0.56+) `eval("hl.dispatch(...)")` is the native spelling; the
--- plain dispatcher path is kept for the rest. `callback(ok, reply)`.
function hyprland.dispatch(dispatcher, argument, callback)
  local payload = "/dispatch " .. text(dispatcher)
  if argument ~= nil and text(argument) ~= "" then payload = payload .. " " .. text(argument) end
  return command(payload, callback)
end

--- Sets a config keyword at run time: `keyword("general:gaps_out", 8)`.
function hyprland.keyword(name, value, callback)
  return command("/keyword " .. text(name) .. " " .. text(value), callback)
end

--- Runs a Lua chunk inside a Lua-configured Hyprland, as `hyprctl eval`
--- does. `callback(reply, err)` gets whatever the compositor printed.
function hyprland.eval(chunk, callback)
  return hyprland.request("/eval " .. text(chunk), callback)
end

--- Reloads the compositor's configuration; `config_only` keeps monitors.
function hyprland.reload(config_only, callback)
  return command(config_only and "/reload config-only" or "/reload", callback)
end

--- `callback(binds, err)`: the list `hyprctl binds` prints.
function hyprland.binds(callback) return hyprland.json("j/binds", callback) end

--- `callback(option, err)`: one option, `{option, int|float|str|css|..., set}`.
function hyprland.getoption(name, callback)
  return hyprland.json("j/getoption " .. text(name), callback)
end

--- `callback(map, err)`: several options on one connection, by name.
function hyprland.options(names, callback)
  local commands = {}
  for index, name in ipairs(names or {}) do commands[index] = "j/getoption " .. text(name) end
  return hyprland.batch(commands, function(replies, err)
    if not callback then return end
    if not replies then return callback(nil, err) end
    local out = {}
    for index, name in ipairs(names) do out[name] = decode(replies[index]) end
    callback(out, nil)
  end)
end

--- `callback(x, y)`, in the global layout's pixels; nil when unknown.
function hyprland.cursor_position(callback)
  return hyprland.json("j/cursorpos", function(value, err)
    if not callback then return end
    if type(value) ~= "table" then return callback(nil, nil, err) end
    callback(tonumber(value.x), tonumber(value.y))
  end)
end

--- `callback(devices, err)`: mice, keyboards, tablets, touch, switches.
function hyprland.devices(callback) return hyprland.json("j/devices", callback) end

--- `callback(version, err)`: `{version, tag, commit, branch, ...}`.
function hyprland.version(callback) return hyprland.json("j/version", callback) end

--- `callback(layers, err)`: per monitor, per level, the layer surfaces.
function hyprland.layers(callback) return hyprland.json("j/layers", callback) end

return hyprland
