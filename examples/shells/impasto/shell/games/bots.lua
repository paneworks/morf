-- Bot Bash: robots march down five lanes, faster as the score climbs. A
-- click in a lane, or its digit, bashes the lowest robot in it: one point,
-- two if it had reached the bottom third. Three robots reaching the floor
-- end the round.
--
-- A bash on an empty lane also costs a life, so mashing the digits doesn't
-- work. Further misses are ignored for a third of a second after one, so a
-- double tap counts once.
--
-- Port of Bots.qml. The original repainted one Canvas at 30 fps; here the
-- lanes are drawn once and each robot on the board is a node from a small
-- pool, built once, which the 30 Hz step only moves, sways and squashes.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local LANES = 5
local RATE = 30
local STUN = math.floor(RATE * 0.3 + 0.5)
-- More robots than can ever be on the board at once: the fastest crossing
-- takes nearly two seconds and the fastest spawn is 380 ms.
local POOL = 14

return function(ctx)
  local cell = math.max(1, math.floor(ctx.width / LANES))
  local drop = math.max(1, math.floor(ctx.height))
  -- A robot's body size. Every `y` is a fraction of the drop: 0 at the top,
  -- 1 at the floor.
  local body = math.max(4, math.floor(cell * 0.5 + 0.5))
  local half = body / 2 / drop
  local id = tostring({})

  local robots = {}
  local lives = morf.signal("impasto.bots." .. id .. ".lives", 3)
  local ticks = 0
  local miss_lane, miss_left = -1, 0
  local miss_signal = morf.signal("impasto.bots." .. id .. ".miss", 0)

  -- Fractions of the drop per second, rising with the score and capped so
  -- a crossing still takes nearly two seconds.
  local function speed() return math.min(0.55, 0.16 + ctx.score:get() * 0.006) end

  -- ------------------------------------------------------------ drawing --

  local lit = function() return common.lighter(ctx.tint(), 1.35) end
  local shade = function() return common.darker(ctx.tint(), 1.5) end
  local corner = math.max(2, math.floor(body * 0.22 + 0.5))

  -- The robot's own box: its feet at `FEET` down it, room above for the
  -- antenna and below for a lifted track. Squashing scales it about the
  -- feet.
  local BOX_W, BOX_H, FEET = body * 1.4, body * 1.6, body * 1.4
  local cx = BOX_W / 2
  local top = FEET - body

  local nodes = {}
  local function robot_node(i)
    local tracks = {}
    for k, side in ipairs { -1, 1 } do
      tracks[k] = ui.Rect {
        x = cx + side * body * 0.3 - body * 0.19, y = FEET - body * 0.1,
        width = body * 0.38, height = body * 0.2, radius = body * 0.1,
        color = shade,
      }
    end
    local antenna = ui.Item {
      x = 0, y = 0, width = BOX_W, height = top,
      ui.Rect {
        x = cx - math.max(1, math.floor(body * 0.06 + 0.5)) / 2, y = top - body * 0.2,
        width = math.max(1, math.floor(body * 0.06 + 0.5)), height = body * 0.2, color = shade,
      },
      ui.Rect {
        x = cx - body * 0.08, y = top - body * 0.32, width = body * 0.16, height = body * 0.16,
        radius = body * 0.08, color = lit,
      },
    }
    local node = ui.Item {
      x = 0, y = 0, width = BOX_W, height = BOX_H,
      transform_origin_y = FEET / BOX_H,
      visible = false,
      tracks[1], tracks[2],
      -- Arms.
      ui.Rect { x = cx - body * 0.49 - body * 0.07, y = top + body * 0.3, width = body * 0.14,
        height = body * 0.42, radius = body * 0.07, color = shade },
      ui.Rect { x = cx + body * 0.49 - body * 0.07, y = top + body * 0.3, width = body * 0.14,
        height = body * 0.42, radius = body * 0.07, color = shade },
      antenna,
      -- The body, lit from above.
      ui.Rect {
        x = cx - body / 2, y = top, width = body, height = body, radius = corner,
        color = "#00000000",
        gradient = function()
          return { angle = 180, stops = { { lit(), 0 }, { ctx.tint(), 0.55 }, { shade(), 1 } } }
        end,
      },
      -- The visor, with an eye at each end of it.
      ui.Rect {
        x = cx - body * 0.34, y = top + body * 0.22, width = body * 0.68, height = body * 0.34,
        radius = body * 0.17, color = common.alpha(C.island, 0.9),
      },
      ui.Rect { x = cx - body * 0.17 - body * 0.075, y = top + body * 0.39 - body * 0.075,
        width = body * 0.15, height = body * 0.15, radius = body * 0.075, color = lit },
      ui.Rect { x = cx + body * 0.17 - body * 0.075, y = top + body * 0.39 - body * 0.075,
        width = body * 0.15, height = body * 0.15, radius = body * 0.075, color = lit },
      -- The light on the top edge.
      ui.Rect { x = cx - body * 0.28, y = top + body * 0.08, width = body * 0.4, height = body * 0.08,
        radius = body * 0.04, color = common.alpha(C.indicator, 0.22) },
    }
    nodes[i] = { node = node, tracks = tracks, antenna = antenna }
    return node
  end

  -- Writes every robot to its node; nodes past the last robot hide.
  local function draw()
    for i = 1, POOL do
      local n = nodes[i]
      local robot = robots[i]
      if n then
        if not robot then
          if n.shown then n.node.visible = false n.shown = false end
        else
          local alive = 1 - robot.squash
          local sway = math.sin(ticks / 4 + robot.lane * 1.3) * body * 0.05 * alive
          local feet = (robot.y + half) * drop
          local node = n.node
          if not n.shown then node.visible = true n.shown = true end
          -- Moved by its transform, so a step re-lays nothing.
          node.translate_x = robot.lane * cell + cell / 2 - BOX_W / 2 + sway
          node.translate_y = feet - FEET
          node.opacity = alive
          node.scale_x = 1 + robot.squash * 0.6
          node.scale_y = 1 - robot.squash * 0.85
          n.antenna.visible = robot.squash == 0
          -- Tracks, one lifted as it walks.
          local step = math.sin(ticks / 3 + robot.lane) * body * 0.06 * alive
          n.tracks[1].translate_y = -step
          n.tracks[2].translate_y = step
        end
      end
    end
  end

  -- -------------------------------------------------------------- rules --

  local hits = {}

  -- Only into a lane whose last robot has cleared the top; none if every
  -- lane is taken.
  local function spawn()
    if #robots >= POOL then return end
    local clear = {}
    for lane = 0, LANES - 1 do
      local free = true
      for _, robot in ipairs(robots) do
        if robot.lane == lane and robot.y < half * 3 then free = false break end
      end
      if free then clear[#clear + 1] = lane end
    end
    if #clear == 0 then return end
    robots[#robots + 1] = { lane = clear[common.random(#clear) + 1], y = -half, squash = 0 }
  end

  local function lose(count)
    lives:set(math.max(0, lives:get() - count))
    if lives:get() == 0 then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
    end
  end

  local function miss(lane)
    if ctx.over:get() or miss_left > 0 then return end
    miss_lane, miss_left = lane, STUN
    miss_signal:set(lane * 100 + miss_left)
    lose(1)
  end

  -- Bashes the lowest standing robot in the lane.
  local function bash_lane(lane)
    if ctx.over:get() then return end
    local lowest
    for _, robot in ipairs(robots) do
      if robot.lane == lane and robot.squash == 0 and (not lowest or robot.y > lowest.y) then
        lowest = robot
      end
    end
    if not lowest then
      miss(lane)
      return
    end
    local worth = lowest.y > 2 / 3 and 2 or 1
    ctx.score:set(ctx.score:get() + worth)
    lowest.squash = 0.001
    local h = hits[lane + 1]
    if h then h(lowest.y, worth) end
    draw()
  end

  local function step()
    local dt = 1 / RATE
    local kept, lost = {}, 0
    for _, robot in ipairs(robots) do
      if robot.squash > 0 then
        robot.squash = robot.squash + dt / 0.16
        if robot.squash < 1 then kept[#kept + 1] = robot end
      else
        robot.y = robot.y + speed() * dt
        if robot.y + half >= 1 then lost = lost + 1 else kept[#kept + 1] = robot end
      end
    end
    robots = kept
    ticks = ticks + 1
    if miss_left > 0 then
      miss_left = miss_left - 1
      miss_signal:set(miss_lane * 100 + miss_left)
    end
    if lost > 0 then lose(lost) end
    draw()
  end

  local function restart()
    robots = {}
    lives:set(3)
    ctx.score:set(0)
    ticks = 0
    miss_lane, miss_left = -1, 0
    miss_signal:set(0)
    ctx.over:set(false)
    draw()
  end

  -- -------------------------------------------------------------- lanes --

  local width = cell * LANES
  local floor = math.floor(cell * 0.26 + 0.5)
  local under = {}
  -- Lanes: every other one lifted off the ground, a floor they are
  -- marching at, and the digit that bashes each. Translucent colours are
  -- mixed over the ground as Qt mixes them (see `common.over`).
  for lane = 0, LANES - 1 do
    if lane % 2 == 1 then
      under[#under + 1] = ui.Rect {
        x = lane * cell, y = 0, width = cell, height = drop,
        color = function() return common.over(C.indicator, 0.02) end,
      }
    end
  end
  -- A miss flashes its lane red, fading over the stun.
  under[#under + 1] = ui.Rect {
    y = 0, width = cell, height = drop,
    x = function() return (miss_signal:get() // 100) * cell end,
    visible = function() return miss_signal:get() % 100 > 0 end,
    color = function()
      local left = miss_signal:get() % 100
      return common.over(C.red(), 0.28 * left / STUN)
    end,
  }
  for lane = 1, LANES - 1 do
    under[#under + 1] = ui.Rect {
      x = lane * cell, y = 0, width = 1, height = drop - floor,
      color = function() return common.over("#ffffff", 0x20 / 255) end,
    }
  end
  under[#under + 1] = ui.Rect {
    x = 0, y = drop - floor, width = width, height = floor,
    color = function() return common.over(C.indicator, 0.05) end,
  }
  under[#under + 1] = ui.Rect {
    x = 0, y = drop - floor, width = width, height = math.max(1, cell * 0.012),
    color = function()
      return common.over(C.red(), lives:get() < 3 and 0.5 or 0.25, common.over(C.indicator, 0.05))
    end,
  }
  for lane = 0, LANES - 1 do
    under[#under + 1] = ui.Text {
      x = lane * cell, y = drop - floor, width = cell, height = floor,
      horizontal_alignment = "center", vertical_alignment = "center",
      text = tostring(lane + 1),
      font_family = function() return theme.font_mono() end,
      font_size = theme.size.small,
      color = function() return common.over(C.textMuted(), 0.65, common.over(C.indicator, 0.05)) end,
    }
  end
  -- Lives, top right, dimming from the right.
  local dot = math.max(2, math.floor(cell * 0.04 + 0.5))
  for life = 0, 2 do
    under[#under + 1] = ui.Rect {
      x = width - theme.radius_medium - dot - (2 - life) * dot * 3 - dot,
      y = theme.radius_medium, width = dot * 2, height = dot * 2, radius = dot,
      color = function()
        if life >= lives:get() then return common.over(C.red(), 0.22) end
        return C.red()
      end,
    }
  end

  -- One burst and one figure per lane, thrown where the robot was.
  local effects = {}
  for lane = 0, LANES - 1 do
    local burst, play_burst = common.burst { tint = ctx.tint, spread = body * 0.7 }
    local pop, play_pop = common.pop { tint = ctx.tint, rise = body * 0.6, width = cell }
    burst.x = lane * cell + cell / 2 - body * 0.7
    pop.x = lane * cell
    hits[lane + 1] = function(y, worth)
      burst.y = (y + half) * drop - body / 2 - body * 0.7
      pop.y = burst.y + body * 0.7 - body * 0.4 - 8
      play_burst()
      play_pop("+" .. worth)
    end
    effects[#effects + 1] = burst
    effects[#effects + 1] = pop
  end

  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Timer {
      interval = math.floor(1000 / RATE + 0.5), ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then step() end end,
    },
    -- Spawn interval shrinks in steps of five points.
    ui.Timer {
      interval = function() return math.max(380, 1100 - (ctx.score:get() // 5) * 50) end,
      ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then spawn() end end,
    },
    common.ground {
      x = (ctx.width - width) / 2, y = (ctx.height - drop) / 2,
      width = width, height = drop,
      ui.Item { x = 0, y = 0, width = width, height = drop, table.unpack(under) },
      -- Clipped to the ground, which a robot walks in from above.
      ui.ClipRect {
        x = 0, y = 0, width = width, height = drop, radius = theme.radius_medium,
        color = "#00000000",
        common.cells(POOL, robot_node, { x = 0, y = 0 }),
      },
      ui.Item { x = 0, y = 0, width = width, height = drop, table.unpack(effects) },
      ui.MouseArea {
        anchors = { fill = true },
        on_pressed = function(_, _, lx)
          bash_lane(math.max(0, math.min(LANES - 1, math.floor(lx / cell))))
        end,
      },
    },
  }

  local function key(keysym)
    local digit = common.digit(keysym)
    if not digit or digit < 1 or digit > LANES then return false end
    bash_lane(digit - 1)
    return true
  end

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function() return "robots " .. #robots .. " lives " .. lives:get() .. " ticks " .. ticks end,
  }
end
