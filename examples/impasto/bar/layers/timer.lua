-- The countdown on the island, and `morf ipc call timer`.
--
-- A running countdown is one of the island's activities (IslandRest.qml's
-- timer segment): the rest layer draws its ring and its figure beside the
-- clock, in the timer's hues (blue, then yellow in the last two minutes,
-- red in the last thirty seconds), and only when `islandActivities` lists
-- it -- the island keeps its rest width and the clock stays centred. What
-- the timer is on the bar and in its detail is bar/modules/timer.lua's.
--
-- This file makes sure the activity has what the rest layer asks of it
-- (`runs`), should the module not have defined it, and answers the verb.

local modules = require("services.modules")
local timer = require("services.timer")

local provider = modules.providers.timer or {}
if type(provider.runs) ~= "function" then
  modules.define("timer", { runs = function() return timer.running() end })
end

-- `morf ipc call timer [5m | pause | resume | toggle | cancel]`.
morf.ipc.timer = function(arg)
  arg = arg or ""
  if arg == "pause" or arg == "resume" or arg == "toggle" then timer.toggle()
  elseif arg == "cancel" then timer.cancel()
  elseif arg ~= "" then
    local ms = timer.parse(arg)
    if ms <= 0 then return "not a duration: " .. arg end
    timer.start(ms, "")
  end
  if not timer.running() then return "idle" end
  return timer.display() .. (timer.paused() and " (held)" or "")
end
