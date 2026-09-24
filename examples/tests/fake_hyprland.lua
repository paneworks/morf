-- A fake Hyprland for specs: both sockets under `<runtime>/hypr/<signature>`,
-- answering like a Lua-configured Hyprland (0.56+) or, with `flavour =
-- "hyprlang"`, a classic one. It is served from the spec's own runtime, so
-- call `serve()` often -- from a `test.wait` predicate -- and it answers
-- whatever the configuration asked since. `transcript` holds every command
-- that is not a `j/` query; `emit(line)` sends an event.
--
-- Monitor rules sent as `hl.monitor{...}` (or `keyword monitor`) change the
-- monitors it reports, and announce it with `monitoradded`, so a
-- configuration that reconciles can be watched settling.

local M = {}

function M.new(options)
  options = options or {}
  local fs = morf.fs
  local json = morf.json
  local self = { transcript = {}, flavour = options.flavour or "lua", listeners = {} }
  self.runtime = options.runtime or ("/tmp/morf-fake-hypr-" .. tostring(math.random(1, 1e9)))
  self.signature = options.signature or "fake"
  local dir = self.runtime .. "/hypr/" .. self.signature
  fs.mkdir(dir)
  pcall(fs.remove, dir .. "/.socket.sock")
  pcall(fs.remove, dir .. "/.socket2.sock")
  local requests = morf.socket_server(dir .. "/.socket.sock")
  local events = morf.socket_server(dir .. "/.socket2.sock")

  self.monitors = options.monitors or {
    { id = 0, name = "eDP-1", description = "BOE 0x0BCA", make = "BOE", model = "0x0BCA", serial = "",
      width = 1920, height = 1200, refreshRate = 60.0, x = 0, y = 0, scale = 1.25, transform = 0,
      focused = true, dpmsStatus = true, vrr = false, disabled = false, mirrorOf = "none",
      activeWorkspace = { id = 1, name = "1" }, specialWorkspace = { id = 0, name = "" },
      availableModes = { "1920x1200@60.00Hz", "1920x1200@48.00Hz", "1280x800@60.00Hz" } },
    { id = 1, name = "DP-2", description = "Dell Inc. DELL U2720Q 7XJ1K", make = "Dell Inc.", model = "DELL U2720Q",
      serial = "7XJ1K", width = 3840, height = 2160, refreshRate = 60.0, x = 1536, y = 0, scale = 1.5,
      transform = 0, focused = false, dpmsStatus = true, vrr = false, disabled = false, mirrorOf = "none",
      activeWorkspace = { id = 2, name = "2" }, specialWorkspace = { id = 0, name = "" },
      availableModes = { "3840x2160@60.00Hz", "2560x1440@59.95Hz", "1920x1080@60.00Hz" } },
  }
  self.workspaces = options.workspaces or {
    { id = 1, name = "1", monitor = "eDP-1", monitorID = 0, windows = 2 },
    { id = 2, name = "2", monitor = "DP-2", monitorID = 1, windows = 0 },
    { id = 3, name = "3", monitor = "DP-2", monitorID = 1, windows = 1 },
  }
  self.binds = options.binds or {
    { modmask = 64, key = "Return", description = "Applications · Open a terminal", dispatcher = "exec", arg = "kitty" },
    { modmask = 64, key = "SPACE", description = "Shell · Open the launcher", dispatcher = "exec", arg = "morf ipc call launcher" },
    { modmask = 64, key = "A", description = "Shell · Open the control centre", dispatcher = "exec", arg = "morf ipc call controls" },
    { modmask = 64, key = "Q", description = "Windows · Close the focused window", dispatcher = "killactive", arg = "" },
    { modmask = 64, key = "mouse:272", description = "Windows · Move with the mouse", dispatcher = "movewindow", arg = "" },
    { modmask = 0, key = "switch:Lid Switch", description = "Session · Lid", dispatcher = "exec", arg = "" },
  }
  self.plugins = options.plugins or {}

  function self.emit(line)
    for _, socket in ipairs(self.listeners) do pcall(socket.send, socket, line .. "\n") end
  end

  local later = {}
  local function monitor_rule(output, fields)
    local changed = false
    for _, m in ipairs(self.monitors) do
      if "desc:" .. m.description == output or m.name == output then
        changed = true
        for key, value in pairs(fields) do
          if key == "mirror" then
            m.mirrorOf = "none"
            for _, o in ipairs(self.monitors) do if o.name == value then m.mirrorOf = tostring(o.id) end end
          else
            m[key] = value
          end
        end
        if m.disabled then m.width, m.height = 0, 0 end
      end
    end
    if changed then later[#later + 1] = "monitoradded>>fake" end
  end

  local function one(command)
    local flags, rest = command:match("^([^/]*)/(.*)$")
    if not flags then rest = command end
    if rest == "monitors all" or rest == "monitors" then return json.encode(self.monitors) end
    if rest == "workspaces" then return json.encode(self.workspaces) end
    if rest == "clients" then return "[]" end
    if rest == "devices" then return '{"keyboards":[]}' end
    if rest == "submap" then return '"default"' end
    if rest == "binds" then return json.encode(self.binds) end
    local option = rest:match("^getoption (.+)$")
    if option then
      if option:match("^plugin:") and not self.plugins[option] then return "no such option" end
      return json.encode({ option = option, int = 0, set = true })
    end
    if rest:match("^eval ") then
      if self.flavour ~= "lua" then return "unknown request" end
      for body in rest:gmatch("hl%.monitor%(%{(.-)%}%)") do
        local fields = {}
        local disabled = body:match("disabled = (%a+)")
        if disabled then fields.disabled = disabled == "true" end
        local x, y = body:match('position = "(%-?%d+)x(%-?%d+)"')
        if x then fields.x, fields.y = tonumber(x), tonumber(y) end
        local scale = body:match("scale = ([%d%.]+)")
        if scale then fields.scale = tonumber(scale) end
        local mirror = body:match('mirror = "([^"]+)"')
        if mirror then fields.mirror = mirror end
        monitor_rule(body:match('output = "([^"]+)"') or "", fields)
      end
      return "ok"
    end
    if rest:match("^keyword ") then
      if self.flavour == "lua" then return "keyword is not supported with a Lua config" end
      local value = rest:match("^keyword monitor (.+)$")
      if value then
        local fields = {}
        local parts = {}
        for part in value:gmatch("[^,]+") do parts[#parts + 1] = part end
        if parts[2] == "disable" then fields.disabled = true
        else
          fields.disabled = false
          local x, y = (parts[3] or ""):match("^(%-?%d+)x(%-?%d+)$")
          if x then fields.x, fields.y = tonumber(x), tonumber(y) end
          fields.scale = tonumber(parts[4])
        end
        monitor_rule(parts[1], fields)
      end
      return "ok"
    end
    if rest:match("^reload") then
      later[#later + 1] = "configreloaded>>"
      return "ok"
    end
    if rest:match("^dispatch ") or rest:match("^setcursor ") then return "ok" end
    return "unknown request"
  end

  local function answer(payload)
    if payload:sub(1, 9) == "[[BATCH]]" then
      local out = {}
      for command in payload:sub(10):gmatch("[^;]+") do out[#out + 1] = one(command) end
      return table.concat(out, "\n\n\n")
    end
    return one(payload)
  end

  --- Answers what was asked since the last call; returns how many.
  function self.serve()
    local served = 0
    while true do
      local client = requests:accept()
      if not client then break end
      local data = ""
      while true do
        local chunk = client:receive(65536, #data == 0 and 200 or 10)
        if not chunk or chunk == "" then break end
        data = data .. chunk
      end
      if data:sub(1, 2) ~= "j/" then self.transcript[#self.transcript + 1] = data end
      client:send(answer(data))
      client:flush()
      client:close()
      served = served + 1
    end
    while true do
      local client = events:accept()
      if not client then break end
      self.listeners[#self.listeners + 1] = client
    end
    local pending = later
    later = {}
    for _, line in ipairs(pending) do self.emit(line) end
    return served
  end

  --- The transcript lines containing `text`.
  function self.sent(text)
    local out = {}
    for _, line in ipairs(self.transcript) do
      if line:find(text, 1, true) then out[#out + 1] = line end
    end
    return out
  end

  function self.close()
    requests:close()
    events:close()
    for _, socket in ipairs(self.listeners) do pcall(socket.close, socket) end
  end

  return self
end

return M
