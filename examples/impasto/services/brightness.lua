-- Brightness, one entry per screen that can be dimmed.
--
-- Port of BrightnessService.qml. The laptop panel is the backlight, read
-- from sysfs by `lib.logind` (which re-reads it on udev events, so the
-- hardware keys reach the OSD too) and set through logind's
-- Session.SetBrightness, which an active session may do unprivileged. An
-- external monitor is set over DDC/CI with `ddcutil`, run directly by argv
-- and only when it is installed and a screen other than a panel is
-- connected; without it the monitor simply is not listed.
--
-- The keys and every single reading follow the focused screen, or the first
-- that can be dimmed.

local logind = require("lib.logind")
local act = require("services.act")

local M = {}

local login = logind.connect()
M.lib = login
local b = login.state.brightness

local function is_panel(name)
  name = tostring(name or "")
  return name:match("^eDP%-") ~= nil or name:match("^LVDS%-") ~= nil or name:match("^DSI%-") ~= nil
end
M.is_panel = is_panel

function M.icon_for(percent)
  if percent < 34 then return "󰃞" end
  return percent < 67 and "󰃟" or "󰃠"
end

-- A level was set here, or the backlight moved on its own: `adjusted` counts
-- the events and `adjusted_display` names the screen, for the OSD.
M.adjusted = morf.signal("impasto.brightness.adjusted", 0)
M.adjusted_display = morf.signal("impasto.brightness.adjusted_display", "")

local adjustments = 0
local function adjusted(name)
  adjustments = adjustments + 1
  M.adjusted_display:set(name)
  M.adjusted:set(adjustments)
end

-- ------------------------------------------------------------- backlight --

local backlight = {
  name = "backlight",
  title = "Built-in",
  backlight = true,
}
function backlight.available() return (b.max or 0) > 0 end
function backlight.percent() return b.percent or 0 end
function backlight.icon() return M.icon_for(backlight.percent()) end
local wanted_backlight = -1
function backlight.set_percent(value)
  if not backlight.available() then return end
  local clamped = math.max(1, math.min(100, math.floor(value + 0.5)))
  if clamped == backlight.percent() then return end
  wanted_backlight = clamped
  act.run("setting the backlight to " .. clamped .. "%", login.set_brightness, clamped / 100)
end
function backlight.step(delta)
  local from = wanted_backlight >= 0 and wanted_backlight or backlight.percent()
  local target = math.max(1, math.min(100, from + delta))
  if target == from then adjusted("backlight") else backlight.set_percent(target) end
end
function backlight.read() login.refresh() end

-- The backlight moving, from here or from the keys, is an adjustment.
local last_value = nil
morf.effect("impasto.brightness.backlight", function()
  local value = b.value
  if last_value ~= nil and value ~= last_value then
    wanted_backlight = -1
    adjusted("backlight")
  end
  last_value = value
end)

-- ------------------------------------------------------------------ DDC --

-- Fixed slots: signals are declared while the configuration loads, and a
-- desk rarely has more than a few monitors.
local SLOTS = 4
local ddc = {}
for index = 1, SLOTS do
  local slot = {
    index = index,
    connector = morf.signal("impasto.brightness.ddc.connector." .. index, ""),
    bus = "",
    title_signal = morf.signal("impasto.brightness.ddc.title." .. index, ""),
    level = morf.signal("impasto.brightness.ddc.level." .. index, 0),
    answering = morf.signal("impasto.brightness.ddc.answering." .. index, false),
    ceiling = 0, wanted = -1, written = -1, busy = false,
  }
  slot.name = function() return slot.connector:get() end
  function slot.title() return slot.title_signal:get() end
  function slot.available() return slot.connector:get() ~= "" and slot.answering:get() end
  function slot.percent() return slot.level:get() end
  function slot.icon() return M.icon_for(slot.percent()) end
  ddc[index] = slot
end

local ddcutil = act.which("ddcutil")
M.ddc_available = ddcutil ~= nil

local function flush(slot)
  if slot.wanted < 0 or slot.busy then return end
  slot.written = slot.wanted
  slot.busy = true
  local value = tostring(math.floor(slot.written / 100 * slot.ceiling + 0.5))
  act.collect("setting " .. slot.name() .. " to " .. slot.written .. "%", ddcutil,
    { "--bus", slot.bus, "--noverify", "--skip-ddc-checks", "setvcp", "10", value },
    function(_, ok)
      slot.busy = false
      if slot.wanted == slot.written or not ok then slot.wanted = -1 end
      if ok then flush(slot) end
    end, { mutates = true })
end

-- "VCP 10 C <level> <maximum>"
local function read(slot)
  if slot.bus == "" or slot.busy or not ddcutil then return end
  slot.busy = true
  act.collect("reading " .. slot.name(), ddcutil,
    { "--bus", slot.bus, "--skip-ddc-checks", "getvcp", "10", "--brief" },
    function(text)
      slot.busy = false
      local words = {}
      for word in text:gmatch("%S+") do words[#words + 1] = word end
      local level, maximum = tonumber(words[4]), tonumber(words[5])
      local answering = words[1] == "VCP" and words[3] == "C" and level and maximum and maximum > 0
      slot.answering:set(answering and true or false)
      if not answering then return end
      slot.ceiling = maximum
      if slot.wanted < 0 then slot.level:set(math.floor(level / maximum * 100 + 0.5)) end
      flush(slot)
    end)
end

for _, slot in ipairs(ddc) do
  function slot.read() read(slot) end
  function slot.set_percent(value)
    if not slot.available() then return end
    local clamped = math.max(1, math.min(100, math.floor(value + 0.5)))
    if clamped == (slot.wanted >= 0 and slot.wanted or slot.percent()) then return end
    slot.wanted = clamped
    slot.level:set(clamped)
    adjusted(slot.name())
    flush(slot)
  end
  function slot.step(delta)
    local from = slot.wanted >= 0 and slot.wanted or slot.percent()
    local target = math.max(1, math.min(100, from + delta))
    if target == from then adjusted(slot.name()) else slot.set_percent(target) end
  end
end

--- Connector -> I2C bus, from `ddcutil detect --brief`. Its connector is
--- "card1-DP-1" where the compositor says "DP-1".
function M.parse(text)
  local found = {}
  for block in (text .. "\n\n"):gmatch("(.-)\n%s*\n") do
    if block:match("^%s*Display %d+") then
      local bus = block:match("I2C bus:%s*/dev/i2c%-(%d+)")
      local connector = block:match("DRM connector:%s*card%d+%-(%S+)")
      if bus and connector then found[#found + 1] = { connector = connector, bus = bus } end
    end
  end
  return found
end

local function external()
  for _, screen in ipairs(morf.screens or {}) do
    if not is_panel(screen.name or "") then return true end
  end
  return false
end

local function title_of(connector)
  for _, screen in ipairs(morf.screens or {}) do
    if screen.name == connector and (screen.model or "") ~= "" then return screen.model end
  end
  return connector
end

--- Asks ddcutil which monitors answer. A desk with only a laptop panel
--- never runs it.
function M.detect()
  if not ddcutil or not external() then return end
  act.collect("detecting monitors", ddcutil, { "detect", "--brief" }, function(text)
    local found = M.parse(text)
    for index, slot in ipairs(ddc) do
      local entry = found[index]
      slot.bus = entry and entry.bus or ""
      slot.title_signal:set(entry and title_of(entry.connector) or "")
      slot.connector:set(entry and entry.connector or "")
      if entry then read(slot) else slot.answering:set(false) end
    end
  end, { timeout_ms = 20000 })
end

-- ---------------------------------------------------------------- screens --

--- Every entry that can be dimmed now: the backlight first, then monitors.
function M.dimmable()
  local out = {}
  if backlight.available() then out[#out + 1] = backlight end
  for _, slot in ipairs(ddc) do
    if slot.available() then out[#out + 1] = slot end
  end
  return out
end

local hyprland
do
  local ok, lib = pcall(require, "lib.hyprland")
  if ok then hyprland = lib end
end

local function focused_name()
  if not hyprland or not hyprland.available or not hyprland.available() then return "" end
  local ok, name = pcall(function() return hyprland.state.focused_monitor end)
  if ok and type(name) == "string" then return name end
  if ok and type(name) == "table" then return name.name or "" end
  return ""
end

local function name_of(display)
  if display.backlight then
    for _, screen in ipairs(morf.screens or {}) do
      if is_panel(screen.name or "") then return screen.name end
    end
    return "backlight"
  end
  return display.name()
end

--- The display the keys and the single readings follow.
function M.current()
  local list = M.dimmable()
  local focused = focused_name()
  for _, display in ipairs(list) do
    if focused ~= "" and name_of(display) == focused then return display end
  end
  return list[1]
end

function M.display(name)
  if name == "backlight" then return backlight end
  for _, slot in ipairs(ddc) do
    if slot.name() == name then return slot end
  end
end

function M.available() return M.current() ~= nil end
function M.percent() local c = M.current() return c and c.percent() or 0 end
function M.icon() return M.icon_for(M.percent()) end
function M.set_percent(value) local c = M.current() if c then c.set_percent(value) end end
function M.step(delta) local c = M.current() if c then c.step(delta) end end

--- A monitor's own buttons change it without telling anyone, so whatever
--- shows the levels asks again when it opens.
function M.refresh()
  for _, slot in ipairs(ddc) do read(slot) end
end

-- A moment after load, so the first frame is not waiting on I2C.
morf.timer(1500, M.detect, false)

return M
