-- A frame-stall rehearsal: a box that never rests on its spring, beside a
-- handler that is slow on purpose.
--
-- The box swings between the two ends of a strip, its spring retargeted
-- every `--swing` ms (default 700, 0 for only when asked) so it is always
-- in motion. A timer every `--interval` ms (default 16) burns `--budget`
-- Lua instructions (default 0: no stall). A handler may run 1M of them
-- (`MORF_LIMITS` handler), so the budget is split over as many timers as
-- it needs, all due together: one firing is one stall of the whole budget.
-- What each frame's time went on is the frame accounting's to say
-- (`MORF_WAKE_LOG=1`, `morf check --frames`, the `@frames` hook,
-- `test.frames()`).
--
--     morf examples/demos/motion/frame-stall.lua -- --budget 4000000 --interval 100
--     morf check examples/demos/motion/frame-stall.lua --after 3000 --frames -- --budget 400000
--     morf ipc call stall 4000000 100   -- a new budget and interval; `stall 0` stops
--     morf ipc call swing               -- the other end, now
--
-- The environment names the same three for a run whose arguments are not
-- its own (the sandbox): `STALL_BUDGET`, `STALL_INTERVAL`, `STALL_SWING`.

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "frame-stall"
morf.surface.anchors = { bottom = true, left = true, right = true }
morf.surface.height = 160
morf.surface.layer = "top"
morf.surface.exclusive_zone = 0
morf.surface.keyboard_focus = "none"

local SIZE = 120
-- Iterations of the burning loop one timer runs: two instructions each,
-- with room under a handler's million.
local CHUNK = 400000

local function setting(name, env, default)
  local given = morf.options[name] or morf.env(env)
  return tonumber(given) or default
end

local budget = setting("budget", "STALL_BUDGET", 0)
local interval = setting("interval", "STALL_INTERVAL", 16)
local swing_every = setting("swing", "STALL_SWING", 700)

local screen = morf.screens[1]
local NEAR, FAR = 40, ((screen and screen.width) or 1280) - SIZE - 40

local box = ui.Rect {
  id = "box",
  width = SIZE,
  height = SIZE,
  y = 20,
  radius = 24,
  color = "#c9a7ff",
  translate_x = NEAR,
  behavior = { translate_x = { kind = "spring", stiffness = 90, damping = 11 } },
}
ui.Rect {
  id = "strip",
  anchors = { fill = true },
  color = "#1c1b22",
  box,
}

local target = NEAR
local function swing()
  target = target == NEAR and FAR or NEAR
  box.translate_x = target
  return target
end

local fired = 0
local function burn(iterations)
  local sum = 0
  for i = 1, iterations do sum = sum + i end
  fired = fired + 1
  return sum
end

local timers = {}
local function stall(new_budget, new_interval)
  for _, timer in ipairs(timers) do timer:cancel() end
  timers = {}
  budget = math.max(0, math.floor(new_budget))
  interval = math.max(1, new_interval or interval)
  local iterations = budget // 2
  while iterations > 0 do
    local chunk = math.min(CHUNK, iterations)
    iterations = iterations - chunk
    timers[#timers + 1] = morf.timer(interval, function() burn(chunk) end, true)
  end
  return budget, interval, #timers
end

morf.ipc.stall = function(new_budget, new_interval)
  return stall(tonumber(new_budget) or 0, tonumber(new_interval))
end
morf.ipc.burn = function(iterations) return burn(tonumber(iterations) or CHUNK) end
morf.ipc.swing = swing
morf.ipc.x = function() return box.translate_x end
morf.ipc.target = function() return target end
morf.ipc.fired = function() return fired end

stall(budget, interval)
if swing_every > 0 then morf.timer(swing_every, swing, true) end
