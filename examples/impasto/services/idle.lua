-- Idle: lock, screen off and suspend after a while untouched.
--
-- Port of IdleService.qml, in place of hypridle. The compositor says when
-- the session has been idle for a timeout (ext-idle-notify, through
-- `morf.idle.subscribe`), honouring inhibitors, so a video player holds all
-- three off. The three timeouts are minutes in Settings, zero for never.
--
-- Each action is its own subscription at its own timeout. An effect follows
-- the three settings and trades a subscription for a new one when its
-- minutes change, and morf hands the compositor the new timeout at once.

local settings = require("services.settings")
local lock = require("services.lock")
local session = require("services.session")

local M = {}

local MINUTE = 60 * 1000

local screen_off = false
local armed = {}

--- Turns this screen off or on, once each way. morf asks the compositor
--- (wlr-output-power-management) rather than dispatching Hyprland's dpms,
--- which toggles blindly; every screen's shell turns off its own.
function M.screen(on)
  if on == not screen_off then return end
  screen_off = not on
  morf.output_power.set(on and "on" or "off")
end

local actions = {
  -- `lock()` does nothing when already locked.
  idleLock = { on_idle = function() lock.lock() end },
  -- Waking matters: the screen comes back on the first touch.
  idleScreen = { on_idle = function() M.screen(false) end, on_wake = function() M.screen(true) end },
  -- The session action locks and waits for the lock before suspending. Once,
  -- not once a screen.
  idleSuspend = { on_idle = function() if session.leader() then session.run("suspend") end end },
}

local function arm(key, minutes)
  local current = armed[key]
  if current and current.minutes == minutes then return end
  if current then current.subscription:cancel() end
  armed[key] = nil
  if type(minutes) ~= "number" or minutes <= 0 then return end
  local action = actions[key]
  armed[key] = {
    minutes = minutes,
    subscription = morf.idle.subscribe(math.floor(minutes * MINUTE), function(idle)
      morf.log("info", "impasto: idle " .. key .. (idle and " fired" or " woke"))
      if idle then action.on_idle()
      elseif action.on_wake then action.on_wake() end
    end, false),
  }
end

local started = false
function M.start()
  if started then return end
  started = true
  morf.effect("impasto.idle.timeouts", function()
    for key in pairs(actions) do arm(key, settings.get(key)) end
  end)
end

return M
