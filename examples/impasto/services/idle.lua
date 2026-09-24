-- Idle: lock, screen off and suspend after a while untouched.
--
-- Port of IdleService.qml, in place of hypridle. The compositor says when
-- the session has been idle for a timeout (ext-idle-notify, through
-- `morf.idle.subscribe`), honouring inhibitors, so a video player holds all
-- three off. The three timeouts are minutes in Settings, zero for never.
--
-- morf hands the compositor its timeouts when the shell starts, so a timeout
-- changed later in Settings would wait for a restart. Instead one timeout is
-- asked for, a minute, and the minutes after it are counted here while the
-- session stays idle: the settings are read at the moment each would fire,
-- so a change applies at once.

local settings = require("services.settings")
local lock = require("services.lock")
local session = require("services.session")

local M = {}

local MINUTE = 60 * 1000
local CHECK = 5000

local idle_since
local counting
local done = {}
local screen_off = false

--- Turns this screen off or on, once each way. morf asks the compositor
--- (wlr-output-power-management) rather than dispatching Hyprland's dpms,
--- which toggles blindly; every screen's shell turns off its own.
function M.screen(on)
  if on == not screen_off then return end
  screen_off = not on
  morf.output_power.set(on and "on" or "off")
end

local function due(key, minutes, elapsed)
  if minutes <= 0 or done[key] then return false end
  if elapsed >= minutes * MINUTE then
    done[key] = true
    return true
  end
  return false
end

local function check(since)
  -- The compositor says "idle" a minute after the last touch.
  local elapsed = MINUTE + (morf.time.now_ms() - since)
  -- `lock()` does nothing when already locked.
  if due("lock", settings.idleLock, elapsed) then lock.lock() end
  if due("screen", settings.idleScreen, elapsed) then M.screen(false) end
  -- The session action locks and waits for the lock before suspending. Once,
  -- not once a screen.
  if due("suspend", settings.idleSuspend, elapsed) and session.leader() then session.run("suspend") end
end

local function on_idle(idle)
  if idle then
    if counting then return end
    done = {}
    idle_since = morf.time.now_ms()
    local since = idle_since
    check(since)
    counting = morf.timer(CHECK, function() check(since) end, true)
  else
    if counting then counting:cancel() counting = nil end
    idle_since = nil
    -- Waking matters: the screen comes back on the first touch.
    M.screen(true)
  end
end

local started = false
function M.start()
  if started then return end
  started = true
  morf.idle.subscribe(MINUTE, on_idle, false)
end

return M
