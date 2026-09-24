-- Brightness, one entry per screen that can be dimmed.
--
-- Port of BrightnessService.qml. The laptop panel is the backlight, read
-- from sysfs by `lib.logind` (which re-reads it on udev events, so the
-- hardware keys reach the OSD too) and set through logind's
-- Session.SetBrightness, which an active session may do unprivileged. An
-- external monitor is set over DDC/CI with `ddcutil`, run directly by argv
-- and only when it is installed and a screen other than a panel is
-- connected; without it the monitor simply is not listed. Which monitor is
-- on which I2C bus is read from sysfs, and read again when outputs change.
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

local fs = morf.fs
local live = require("services.live")
local SHARED = fs.join(morf.env("XDG_RUNTIME_DIR") or "/tmp", "impasto-morf")
-- What the last reading of each bus said, for the other screens: only one
-- of them asks the monitors at load (services/live.lua).
local LEVELS = fs.join(SHARED, "ddc-levels.json")

local function load_levels()
  local ok, levels = pcall(morf.json.decode, fs.read(LEVELS) or "{}")
  return ok and type(levels) == "table" and levels or {}
end

local function save_level(bus, level, maximum)
  fs.mkdir(SHARED)
  local levels = load_levels()
  levels[bus] = { level = level, maximum = maximum }
  fs.write(LEVELS, morf.json.encode(levels))
end

-- One ddcutil on a bus at a time, whichever screen asks: two conversations
-- on one I2C bus garble each other. The claim is a directory, made by
-- exactly one runtime; one older than ten seconds belonged to a ddcutil
-- that is gone.
local function claim(bus)
  local dir = fs.join(SHARED, "ddc-" .. bus .. ".busy")
  fs.mkdir(SHARED)
  if fs.mkdir(dir, { parents = false }) then return dir end
  local stat = fs.stat(dir)
  local now = os.time()
  if stat and stat.modified and now - math.floor(tonumber(stat.modified) or now) > 10 then
    fs.remove(dir, { recursive = true })
    if fs.mkdir(dir, { parents = false }) then return dir end
  end
  return nil
end

-- Runs ddcutil on a slot's bus once the bus is free.
local function on_bus(slot, what, argv, on_done, options)
  local held = claim(slot.bus)
  if not held then
    morf.timer(250, function() on_bus(slot, what, argv, on_done, options) end, false)
    return
  end
  act.collect(what, ddcutil, argv, function(text, ok)
    fs.remove(held, { recursive = true })
    on_done(text, ok)
  end, options)
end

local read

local function flush(slot)
  if slot.wanted < 0 or slot.busy then return end
  slot.written = slot.wanted
  slot.busy = true
  local value = math.floor(slot.written / 100 * slot.ceiling + 0.5)
  on_bus(slot, "setting " .. slot.name() .. " to " .. slot.written .. "%",
    { "--bus", slot.bus, "--noverify", "--skip-ddc-checks", "setvcp", "10", tostring(value) },
    function(_, ok)
      slot.busy = false
      if slot.wanted == slot.written or not ok then slot.wanted = -1 end
      if ok then
        save_level(slot.bus, value, slot.ceiling)
        flush(slot)
      elseif not act.dry then
        -- The monitor said no, or did not answer: show what it has.
        read(slot)
      end
    end, { mutates = true })
end

-- "VCP 10 C <level> <maximum>"
read = function(slot)
  if slot.bus == "" or slot.busy or not ddcutil then return end
  slot.busy = true
  on_bus(slot, "reading " .. slot.name(),
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
      save_level(slot.bus, level, maximum)
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
--- "card1-DP-1" where the compositor says "DP-1". Kept for that text;
--- detection itself reads sysfs (`M.scan`).
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

--- Connector -> I2C bus, from sysfs: `/sys/class/drm/card*-<connector>/ddc`
--- is the adapter the monitor's DDC lines are on, and its `i2c-dev` names
--- the `/dev/i2c-N` ddcutil opens. `root` is for tests.
function M.scan(root)
  root = root or "/sys/class/drm"
  local found = {}
  for _, entry in ipairs(fs.list(root) or {}) do
    local connector = entry.name:match("^card%d+%-(.+)$")
    if connector and not is_panel(connector) then
      local base = fs.join(root, entry.name)
      local status = (fs.read(fs.join(base, "status")) or ""):match("^%s*(%S+)")
      if status == "connected" then
        -- The link names the adapter (`../../i2c-5`); its i2c-dev entry is
        -- the same number, looked at when the link cannot be read.
        local ok, link = pcall(fs.read_link, fs.join(base, "ddc"))
        local bus = ok and link and tostring(link):match("i2c%-(%d+)/?$") or nil
        if not bus then
          for _, dev in ipairs(fs.list(fs.join(base, "ddc", "i2c-dev")) or {}) do
            bus = dev.name:match("^i2c%-(%d+)$")
            if bus then break end
          end
        end
        if bus then found[#found + 1] = { connector = connector, bus = bus } end
      end
    end
  end
  table.sort(found, function(a, b) return a.connector < b.connector end)
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

local function adopt(slot, known)
  local level, maximum = known and tonumber(known.level), known and tonumber(known.maximum)
  if not (level and maximum and maximum > 0) then return false end
  slot.ceiling = maximum
  slot.level:set(math.floor(level / maximum * 100 + 0.5))
  slot.answering:set(true)
  return true
end

--- Which monitors have a DDC bus, from sysfs. One screen then asks them for
--- their levels; the others take the levels it wrote down. A desk with only
--- a laptop panel never runs ddcutil.
function M.detect()
  if not ddcutil or not external() then
    for _, slot in ipairs(ddc) do
      slot.answering:set(false)
      slot.connector:set("")
    end
    return
  end
  local found = M.scan()
  local asks = live.here()
  local levels = asks and {} or load_levels()
  for index, slot in ipairs(ddc) do
    local entry = found[index]
    slot.bus = entry and entry.bus or ""
    slot.title_signal:set(entry and title_of(entry.connector) or "")
    slot.connector:set(entry and entry.connector or "")
    if not entry then
      slot.answering:set(false)
    elseif asks then
      read(slot)
    elseif not adopt(slot, levels[entry.bus]) then
      -- The asking screen has not written yet: look again shortly.
      morf.timer(3000, function()
        if slot.bus == entry.bus then adopt(slot, load_levels()[entry.bus]) end
      end, false)
    end
  end
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

-- The focused screen, from Hyprland when another part of the shell has it
-- running; requiring it here would start its socket poll for one name.
local function focused_name()
  local hyprland = type(package) == "table" and package.loaded and package.loaded["lib.hyprland"]
  if type(hyprland) ~= "table" or not hyprland.available or not hyprland.available() then return "" end
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

-- A moment after load, so the first frame is not waiting on I2C; again
-- whenever a monitor comes or goes (morf.screens is kept current in place).
morf.timer(1500, M.detect, false)
local function outputs()
  local names = {}
  for _, screen in ipairs(morf.screens or {}) do names[#names + 1] = tostring(screen.name) end
  table.sort(names)
  return table.concat(names, ",")
end
local seen_outputs = outputs()
morf.timer(2000, function()
  local now = outputs()
  if now == seen_outputs then return end
  seen_outputs = now
  morf.timer(1500, M.detect, false)
end, true)

return M
