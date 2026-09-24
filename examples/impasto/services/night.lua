-- Night light: warmer colours for the evening (SunsetService, in part).
--
-- The original ran hyprsunset and watched it. Here the switch is the
-- `nightLight` setting, and turning it on or off goes through
-- `services/act.lua`, so a dry run only logs it; outside one, hyprsunset
-- is started at `nightTemperature` (or stopped) when it is installed. It is
-- a stub: nothing reads back whether the screen is actually warm.

local settings = require("services.settings")
local act = require("services.act")

local M = {}

M.WARMEST, M.COOLEST = 2500, 6500

local process = nil

function M.available() return act.which("hyprsunset") ~= nil end

function M.on() return settings.nightLight end

local function stop()
  if process then pcall(process.kill, process, "TERM") end
  process = nil
end

local function start()
  stop()
  process = act.spawn("start the night light", "hyprsunset",
    { "-t", tostring(settings.nightTemperature) }, true)
end

--- Turns it on or off, and remembers which.
function M.set(on)
  settings.set("nightLight", on and true or false)
  act.run(on and "turn the night light on" or "turn the night light off", function()
    if not M.available() then return nil, "hyprsunset is not installed" end
    if on then start() else stop() end
    return true
  end)
end

function M.toggle() M.set(not settings.nightLight) end

--- A new temperature; restarts it when it is on.
function M.set_temperature(kelvin)
  settings.set("nightTemperature", kelvin)
  if settings.nightLight then
    act.run("warm the screen to " .. kelvin .. " K", function()
      if not M.available() then return nil, "hyprsunset is not installed" end
      start()
      return true
    end)
  end
end

return M
