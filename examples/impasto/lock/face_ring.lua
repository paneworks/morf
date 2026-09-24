-- The face ring: the scan round the padlock while the camera looks.
--
-- Port of FaceRing.qml. A ring of 48 ticks; a bright head circles it once a
-- second with a tail fading behind it, which reads as looking rather than as
-- waiting. A match lights the whole ring; the colour is the island's.
--
-- The original moved the head along fixed ticks, recomputing each tick's
-- brightness every frame. Here the brightness is laid down once, head at the
-- top, and the ring itself turns: the same picture, with the motion run by
-- the engine instead of 48 bindings a frame.

local ui = require("morf.ui")
local theme = require("theme")

local COUNT = 48
local TICK = 7
-- The share of a turn the tail covers behind the head.
local TAIL = 0.42

--- `diameter`, and functions `scanning()`, `closed()`, `hue()`, `shown()`.
return function(values)
  local diameter = values.diameter or 72
  local size = diameter + 2 * TICK
  local scanning, closed, hue, shown = values.scanning, values.closed, values.hue, values.shown

  local spokes = {}
  for index = 0, COUNT - 1 do
    -- Turns behind the head, 0 at the head itself.
    local behind = ((COUNT - index) % COUNT) / COUNT
    local glow = math.max(0.2, 1 - behind / TAIL)
    spokes[#spokes + 1] = ui.Item {
      anchors = { fill = true },
      rotation = index * 360 / COUNT,
      ui.Rect {
        x = size / 2 - 1.2, y = 0,
        width = 2.4, height = TICK, radius = 1.2,
        color = hue,
        opacity = function()
          if closed() then return 1 end
          return scanning() and glow or 0.2
        end,
        behavior = { opacity = theme.behave("morph"), color = theme.behave("fast") },
      },
    }
  end

  local wheel = ui.Item {
    anchors = { fill = true },
    table.unpack(spokes),
  }

  -- One turn a second while looking; stopped where it is on a match, so
  -- the ring closes from wherever the head had got to.
  local turning
  morf.effect("impasto.lock.face_ring.turn", function()
    local on = scanning() and not closed()
    if on and not turning then
      turning = morf.animation.play {
        loops = "forever",
        { node = wheel, property = "rotation", from = 0, to = 360, duration = 1000, easing = "linear" },
      }
    elseif not on and turning then
      turning:stop()
      turning = nil
    end
  end)

  return ui.Item {
    x = values.x, y = values.y, anchors = values.anchors,
    width = size, height = size,
    -- The ticks grow out from the ring as it appears.
    opacity = function() return shown() and 1 or 0 end,
    scale = function() return shown() and 1 or 0.85 end,
    behavior = {
      opacity = { duration = math.max(1, theme.duration_morph()), easing = "out_cubic" },
      scale = { duration = math.max(1, theme.duration_morph()), easing = "out_cubic" },
    },
    wheel,
  }
end
