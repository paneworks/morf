-- Night light: warmer colours for the evening. Port of SunsetService.qml.
--
-- hyprsunset sets the gamma ramp, which stays out of screenshots and
-- survives a compositor reload. Running means on, and the compositor puts
-- the ramp back when the client goes. The switch is the `nightLight`
-- setting, restored at login by the screen that does the shell's single
-- jobs (services/live.lua); a new temperature is sent over hyprsunset's own
-- socket (`temperature N`, what `hyprctl hyprsunset` sends) rather than a
-- restart, which would flash daylight, coalesced to 120 ms while a slider is
-- dragged.
--
-- One hyprsunset for the whole desk: its pid is kept in the runtime
-- directory, so whichever screen turns it off can end the one another
-- screen started. When it exits on its own (usually another client holding
-- the ramp) the setting goes off to match, and the island says so.
--
-- Starting and stopping go through services/act.lua: a dry run logs them.

local settings = require("services.settings")
local act = require("services.act")
local live = require("services.live")

local M = {}

-- 6500 K is a no-op in hyprsunset, so the cool end stops short of it.
M.WARMEST, M.COOLEST = 2500, 6000

local fs = morf.fs
local DIR = fs.join(morf.env("XDG_RUNTIME_DIR") or "/tmp", "impasto-morf")
local PID_FILE = fs.join(DIR, "hyprsunset.pid")

M.running = morf.signal("impasto.night.running", false)

local handle = nil

function M.available() return act.which("hyprsunset") ~= nil end

function M.on() return settings.nightLight end

function M.icon() return settings.nightLight and "󰃜" or "󰃝" end

function M.detail()
  if not M.available() then return "Needs hyprsunset" end
  return settings.nightLight and (tostring(settings.nightTemperature) .. " K") or "Daylight"
end

local function socket_path()
  local signature = morf.env("HYPRLAND_INSTANCE_SIGNATURE") or ""
  if signature == "" then return nil end
  return fs.join(morf.env("XDG_RUNTIME_DIR") or "/tmp", "hypr", signature, ".hyprsunset.sock")
end

local function recorded_pid()
  return tonumber((fs.read(PID_FILE) or ""):match("^(%d+)"))
end

--- The hyprsunset this shell started, from any screen, if it still runs.
local function alive_pid()
  local pid = recorded_pid()
  if pid and fs.exists("/proc/" .. pid) then return pid end
  return nil
end

local function kelvin()
  local value = tonumber(settings.nightTemperature) or 4500
  return math.max(M.WARMEST, math.min(M.COOLEST, math.floor(value + 0.5)))
end

local function start()
  if alive_pid() then return true end
  fs.mkdir(DIR)
  local mine, spawned
  mine = act.spawn("start the night light", "hyprsunset",
    { "--temperature", tostring(kelvin()) }, true, {
      on_exit = function()
        -- No longer recorded: a screen of this shell stopped it.
        local expected = recorded_pid() ~= spawned
        if handle == mine then handle = nil end
        M.running:set(false)
        if expected then return end
        fs.remove(PID_FILE)
        if settings.nightLight then
          settings.set("nightLight", false)
          pcall(function()
            require("services.osd").request(M.icon(), "Night light stopped", -1)
          end)
        end
      end,
    })
  if not mine then return nil, "hyprsunset did not start" end
  handle = mine
  spawned = mine:pid()
  if spawned then fs.write(PID_FILE, tostring(spawned)) end
  M.running:set(true)
  return true
end

local function stop()
  local pid = alive_pid()
  fs.remove(PID_FILE)
  if handle then
    pcall(handle.kill, handle, "TERM")
    handle = nil
  elseif pid then
    pcall(morf.kill, pid, "TERM")
  end
  M.running:set(false)
  return true
end

local retune_timer = nil
--- The running hyprsunset to the setting's temperature, over its socket.
local function retune()
  if retune_timer then retune_timer:cancel() end
  retune_timer = morf.timer(120, function()
    retune_timer = nil
    local path = socket_path()
    if not path or not alive_pid() then return end
    act.run("warm the screen to " .. kelvin() .. " K", function()
      morf.request_socket(path, "temperature " .. kelvin(), function(_, err)
        if err then morf.log("warn", "impasto: hyprsunset did not answer: " .. tostring(err)) end
      end, { timeout_ms = 2000 })
      return true
    end)
  end, false)
end

--- Turns it on or off, and remembers which.
function M.set(on)
  settings.set("nightLight", on and true or false)
  act.run(on and "turn the night light on" or "turn the night light off", function()
    if not M.available() then return nil, "hyprsunset is not installed" end
    if on then return start() end
    return stop()
  end)
end

function M.toggle()
  if not M.available() then return end
  M.set(not settings.nightLight)
end

--- A new temperature; the running filter follows without a restart.
function M.set_temperature(value)
  settings.set("nightTemperature", math.max(M.WARMEST, math.min(M.COOLEST, math.floor(value + 0.5))))
  if settings.nightLight then retune() end
end

-- At login: the setting is put back by one screen.
morf.timer(1200, function()
  if alive_pid() then M.running:set(true) return end
  if settings.nightLight and M.available() and live.here() then
    act.run("restore the night light", start)
  end
end, false)

return M
