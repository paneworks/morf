-- Workspaces and windows, as the bar, the overview and the launcher read
-- them.
--
-- The part of HyprlandService.qml those three use, over lib.hyprland. The
-- library's list models are for Repeaters and a binding cannot follow them,
-- so this keeps plain copies and a revision signal a binding can: every
-- event from the compositor schedules one refresh a moment later, once the
-- library has refetched what the event made stale.
--
-- Outside Hyprland the library reports unavailable and everything here is
-- empty; the pieces draw their empty states. `morf ipc call
-- workspaces_demo on` fills it with a made-up desk instead, for looking at
-- the overview in a nested compositor; while it is on, nothing is ever sent
-- to a compositor.

local settings = require("services.settings")

local ok_lib, hyprland = pcall(require, "lib.integrations.hyprland")
if not ok_lib then
  morf.log("warn", "impasto: lib.hyprland did not load: " .. tostring(hyprland))
  hyprland = nil
end

local M = {}

M.revision = morf.signal("impasto.workspaces.revision", 0)
M.demo = morf.signal("impasto.workspaces.demo", false)

local mirror = { clients = {}, workspaces = {}, monitors = {} }

local counter = 0
local function bump() counter = counter + 1 M.revision:set(counter) end

local function refresh()
  if M.demo:get() or not hyprland then return end
  local snapshot = hyprland.snapshot()
  mirror.clients = snapshot.clients or {}
  mirror.workspaces = snapshot.workspaces or {}
  mirror.monitors = snapshot.monitors or {}
  bump()
end

local pending = false
local function refresh_soon()
  if pending then return end
  pending = true
  -- After the library's own refetch: it drains once per poll tick (30 ms)
  -- and the answer comes back on the next.
  morf.timer(90, function()
    pending = false
    refresh()
  end, false)
end

if hyprland then
  hyprland.on("*", refresh_soon)
  hyprland.on("connected", refresh_soon)
  hyprland.on("disconnected", refresh_soon)
end

--- Asks the compositor for everything again (the launcher and the overview
--- do when they open).
function M.reload()
  if hyprland and hyprland.available() then
    hyprland.refresh()
    refresh_soon()
  end
end

--- Whether there is a compositor to ask, or the demo.
function M.available()
  if M.demo:get() then return true end
  return hyprland ~= nil and hyprland.available()
end

-- ------------------------------------------------------------- readings --

--- Every window, as rows: address, class, title, workspace, x, y, width,
--- height, floating. A binding that calls it follows them.
function M.clients()
  M.revision:get()
  return mirror.clients
end

function M.monitors()
  M.revision:get()
  return mirror.monitors
end

--- The windows on one workspace.
function M.clients_on(id)
  local out = {}
  for _, client in ipairs(M.clients()) do
    if client.workspace == id then out[#out + 1] = client end
  end
  return out
end

--- The workspace being worked on.
function M.active_id()
  if M.demo:get() then
    M.revision:get()
    return mirror.active or 1
  end
  if not hyprland then return 0 end
  return hyprland.state.active_workspace.id or 0
end

function M.occupied(id)
  for _, client in ipairs(M.clients()) do
    if client.workspace == id then return true end
  end
  return false
end

--- The fixed slots, plus any higher workspace that is occupied or active,
--- as ids in order.
function M.visible_ids()
  local slots = math.max(1, settings.workspaceCount)
  local most = math.max(slots, settings.workspaceMax)
  local ids, seen = {}, {}
  for id = 1, slots do ids[#ids + 1] = id seen[id] = true end
  local extra = { M.active_id() }
  for _, client in ipairs(M.clients()) do extra[#extra + 1] = client.workspace end
  for _, id in ipairs(extra) do
    if id > slots and id <= most and not seen[id] then
      ids[#ids + 1] = id
      seen[id] = true
    end
  end
  table.sort(ids)
  return ids
end

function M.is_visible(id)
  for _, shown in ipairs(M.visible_ids()) do
    if shown == id then return true end
  end
  return false
end

--- The monitor a workspace is on (its row), or the focused one.
function M.monitor_of(id)
  local monitors = M.monitors()
  local name
  if not M.demo:get() and hyprland then
    local row = hyprland.workspace(id)
    name = row and row.monitor
  end
  local focused
  for _, monitor in ipairs(monitors) do
    if name and monitor.name == name then return monitor end
    if monitor.focused then focused = monitor end
  end
  return focused or monitors[1]
end

-- -------------------------------------------------------------- commands --

-- Hyprland takes a classic dispatcher on its socket; a Lua-configured one
-- (0.56+) wants `hl.dsp...`, which impasto sends. The classic one is tried
-- first and the Lua one only if it is refused.
local function dispatch(classic, argument, lua)
  if M.demo:get() then
    morf.log("info", "impasto: (demo) would dispatch " .. classic .. " " .. tostring(argument))
    return
  end
  if not hyprland or not hyprland.available() then return end
  hyprland.dispatch(classic, argument, function(ok)
    if not ok and lua then hyprland.dispatch(lua) end
  end)
end

--- Brings workspace `id` to the screen being worked on.
function M.focus(id)
  dispatch("focusworkspaceoncurrentmonitor", id,
    ("hl.dsp.focus({ workspace = %d, on_current_monitor = true })"):format(id))
  if M.demo:get() then mirror.active = id bump() end
end

function M.focus_window(address)
  dispatch("focuswindow", "address:" .. address,
    ('hl.dsp.focus({ window = "address:%s" })'):format(address))
end

--- Moves a window to another workspace without following it.
function M.move_window(address, id)
  dispatch("movetoworkspacesilent", id .. ",address:" .. address,
    ('hl.dsp.window.move({ workspace = %d, window = "address:%s", follow = false })'):format(id, address))
  if M.demo:get() then
    for _, client in ipairs(mirror.clients) do
      if client.address == address then client.workspace = id end
    end
    bump()
  end
  refresh_soon()
end

function M.close_window(address)
  dispatch("closewindow", "address:" .. address,
    ('hl.dsp.window.close({ window = "address:%s" })'):format(address))
  refresh_soon()
end

-- ------------------------------------------------------------------ demo --

-- A made-up desk: one 1920x1200 screen, a few windows on three workspaces.
local function demo_desk()
  local W, H, top = 1920, 1200, 56
  local function client(address, class, title, workspace, x, y, w, h, floating)
    return {
      address = address, class = class, title = title, workspace = workspace,
      x = x, y = y, width = w, height = h, floating = floating or false,
    }
  end
  local g = 10
  local half = (W - 3 * g) // 2
  local full = H - top - 2 * g
  return {
    active = 1,
    monitors = { { id = 0, name = "DEMO-1", x = 0, y = 0, width = W, height = H, scale = 1, focused = true } },
    clients = {
      client("0xd1", "firefox", "impasto — GitHub", 1, g, top + g, half, full),
      client("0xd2", "kitty", "~/mold — nvim", 1, 2 * g + half, top + g, half, (full - g) // 2),
      client("0xd3", "kitty", "~/mold — cargo build", 1, 2 * g + half, top + 2 * g + (full - g) // 2, half, (full - g) // 2),
      client("0xd4", "org.gnome.Nautilus", "Pictures", 2, g, top + g, W - 2 * g, full),
      client("0xd5", "org.gnome.Calculator", "Calculator", 2, 1300, 300, 360, 520, true),
      client("0xd6", "code", "island.lua — mold", 4, g, top + g, W - 2 * g, full),
    },
  }
end

function M.set_demo(on)
  M.demo:set(on and true or false)
  if on then
    local desk = demo_desk()
    mirror.clients, mirror.monitors, mirror.workspaces = desk.clients, desk.monitors, {}
    mirror.active = desk.active
    bump()
  else
    mirror.active = nil
    mirror.clients, mirror.monitors, mirror.workspaces = {}, {}, {}
    refresh()
    bump()
  end
end

morf.ipc.workspaces_demo = function(arg)
  M.set_demo(arg ~= "off")
  return M.demo:get() and "demo" or "live"
end

-- The first answers arrive a moment after the library starts.
if hyprland and hyprland.available() then refresh_soon() end

return M
