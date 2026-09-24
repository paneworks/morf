-- Target Smash: one target at a time, shrinking to nothing, faster as the
-- score climbs. A hit scores three for the bullseye, two for the middle
-- ring, one for the outer, judged by distance from the centre at the
-- moment of the click. Clicks on empty ground do nothing.
--
-- A target that shrinks away unclicked costs one of three lives. Mouse
-- only.
--
-- Port of Target.qml. The target's scale is the clock, as there: one
-- animation from one to nothing, whose end is the miss. The on-screen
-- radius at a click is read from the time the target has been up, since
-- an animated value is not something Lua can read back.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

-- Full target size, and its centre's margin from the edge: the burst grows
-- it by half again and the ground doesn't clip.
local SIZE = 96
local MARGIN = math.ceil(SIZE * 0.75)
-- Part of the game's pace, so it ignores the shell's motion scale.
local BURST = 220

return function(ctx)
  local id = tostring({})
  local lives = morf.signal("impasto.target." .. id .. ".lives", 3)
  local smashed = morf.signal("impasto.target." .. id .. ".smashed", false)
  local tx, ty = 0, 0
  local clock = morf.elapsed_timer()
  local shrink = nil
  local generation = 0

  -- Target lifetime: 1.6 s at the start, falling to 700 ms by thirty points.
  local lifetime = 1600

  local target, rings, burst, play_burst, pop, play_pop
  local miss

  local function place()
    local span_x = math.max(0, ctx.width - 2 * MARGIN)
    local span_y = math.max(0, ctx.height - 2 * MARGIN)
    tx = MARGIN + math.floor(math.random() * span_x + 0.5)
    ty = MARGIN + math.floor(math.random() * span_y + 0.5)
    target.x, target.y = tx - SIZE / 2, ty - SIZE / 2
    burst.x, burst.y = tx - SIZE * 0.7, ty - SIZE * 0.7
    pop.x, pop.y = tx - 30, ty - SIZE * 0.5
    smashed:set(false)
    rings.scale, rings.opacity = 1, 1
    lifetime = math.max(700, 1600 - ctx.score:get() * 30)
    clock:restart()
    generation = generation + 1
    local mine = generation
    if shrink then shrink:stop() end
    shrink = morf.animation.play {
      on_finished = function(reason)
        if reason == "finished" and mine == generation and ctx.live() then miss() end
      end,
      { node = target, property = "scale", from = 1, to = 0, duration = lifetime, easing = "linear" },
    }
  end

  miss = function()
    if ctx.over:get() then return end
    lives:set(lives:get() - 1)
    if lives:get() == 0 then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
      return
    end
    place()
  end

  local function smash(x, y)
    if ctx.over:get() or smashed:get() then return end
    -- The rings are thirds of the current on-screen radius.
    local scale = math.max(0, 1 - clock:elapsed_ms() / lifetime)
    local radius = scale * SIZE / 2
    local distance = math.sqrt((x - tx) ^ 2 + (y - ty) ^ 2)
    if distance > radius then return end
    local worth = distance <= radius / 3 and 3 or distance <= radius * 2 / 3 and 2 or 1
    ctx.score:set(ctx.score:get() + worth)
    smashed:set(true)
    generation = generation + 1
    local mine = generation
    if shrink then shrink:stop() end
    -- Hold the target where it was hit, then flash the rings out.
    target.scale = scale
    morf.animation.play {
      on_finished = function(reason)
        if reason == "finished" and mine == generation and ctx.live() then place() end
      end,
      { parallel = {
        { node = rings, property = "scale", from = 1, to = 1.5, duration = BURST, easing = "out_cubic" },
        { node = rings, property = "opacity", from = 1, to = 0, duration = BURST, easing = "linear" },
      } },
    }
    play_burst()
    play_pop("+" .. worth)
  end

  local function restart()
    ctx.score:set(0)
    lives:set(3)
    ctx.over:set(false)
    place()
  end

  -- Five rings, the tint and white by turns, with the bullseye in the
  -- colour a bullseye is. All of it whitens on a hit.
  local discs = {}
  for _, ring in ipairs {
    { at = 1.0, paint = "tint" }, { at = 0.78, paint = "pale" }, { at = 0.56, paint = "tint" },
    { at = 0.34, paint = "pale" }, { at = 0.16, paint = "eye" },
  } do
    local w = SIZE * ring.at
    discs[#discs + 1] = ui.Rect {
      x = (SIZE - w) / 2, y = (SIZE - w) / 2, width = w, height = w, radius = w / 2,
      color = function()
        if smashed:get() then return C.indicator end
        if ring.paint == "tint" then return ctx.tint() end
        if ring.paint == "pale" then return C.indicator end
        return C.red()
      end,
      border_color = common.alpha(C.island, 0.35),
      border_width = ring.at == 1 and math.max(1, SIZE * 0.02) or 0,
    }
  end
  -- The light on it, so it reads as a disc rather than a print.
  discs[#discs + 1] = ui.Rect {
    x = SIZE * 0.2, y = SIZE * 0.16, width = SIZE * 0.3, height = SIZE * 0.16,
    radius = SIZE * 0.08, rotation = -28,
    opacity = function() return smashed:get() and 0 or 0.22 end,
    color = C.indicator,
  }
  rings = ui.Item { x = 0, y = 0, width = SIZE, height = SIZE, table.unpack(discs) }
  target = ui.Item { width = SIZE, height = SIZE, rings }

  burst, play_burst = common.burst { tint = ctx.tint, spread = SIZE * 0.7, sparks = 9 }
  pop, play_pop = common.pop { tint = C.indicator, rise = SIZE * 0.4, width = 60 }

  -- The range: a field of dots, drawn once, so the board is a place rather
  -- than an empty rectangle.
  local dots = {}
  for y = 17, ctx.height, 34 do
    for x = 17, ctx.width, 34 do
      dots[#dots + 1] = string.format('<circle cx="%g" cy="%g" r="1.6"/>', x, y)
    end
  end

  -- Three lives, top right; a lost one dims.
  local hearts = {}
  for i = 1, 3 do
    hearts[i] = ui.Rect {
      width = 8, height = 8, radius = 4, color = C.red,
      opacity = function() return i <= lives:get() and 1 or 0.25 end,
      behavior = { opacity = theme.behave("fast") },
    }
  end

  local node = common.ground {
    x = 0, y = 0, width = ctx.width, height = ctx.height,
    ui.Image {
      x = 0, y = 0, width = ctx.width, height = ctx.height,
      source = string.format(
        '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d"><g fill="%s">%s</g></svg>',
        ctx.width, ctx.height, common.over("#ffffff", 0.045):hex(), table.concat(dots)),
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "crosshair",
      on_pressed = function(_, _, lx, ly) smash(lx, ly) end,
    },
    target,
    burst,
    pop,
    ui.Row {
      x = ctx.width - 14 - (3 * 8 + 2 * 6), y = 14,
      gap = 6,
      table.unpack(hearts),
    },
  }

  restart()
  return {
    node = node, restart = restart,
    key = function() return false end,
    debug = function() return "target " .. tx .. "," .. ty .. " lives " .. lives:get() end,
    -- For testing: a click at the target's centre.
    hit = function() smash(tx, ty) end,
  }
end
