-- Whack-a-Mole: nine holes numbered like a phone keypad (1 top left, 9
-- bottom right). Up to two moles at a time pop up, wait and duck; a click
-- or the hole's digit drops one for a point. Moles stay up for less as the
-- score climbs. Rounds last 45 seconds, shown by the bar at the top.
--
-- A miss (an empty hole's digit or a click on one) costs two seconds, so
-- mashing the digits doesn't pay. Further misses are ignored for a third
-- of a second after one.
--
-- Port of Whack.qml. The mole's body was a Shape path (a dome); here it is
-- an SVG drawn once per tint, over the same rectangles for ears, eyes,
-- snout and paws.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local COLUMNS, ROWS = 3, 3
local HOLES = COLUMNS * ROWS
-- Counted in ticks rather than wall time, so a closed island pauses the
-- round.
local ROUND = 45000
local TICK = 50
local REST = 300
local MAX_UP = 2
local STUN = 300
local PENALTY = 2000

-- The dome of the mole's body, top-lit: light, the tint, then mid.
local function dome_svg(w, h, light, tint, mid)
  local function hex(c) return morf.color(c):hex() end
  return string.format([[<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %f %f">
<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="%s"/><stop offset="0.5" stop-color="%s"/><stop offset="1" stop-color="%s"/>
</linearGradient></defs>
<path fill="url(#g)" d="M %f 0 C %f 0 %f %f %f %f L 0 %f C 0 %f %f 0 %f 0 Z"/></svg>]],
    math.ceil(w), math.ceil(h), w, h, hex(light), hex(tint), hex(mid),
    w * 0.5, w * 0.96, w, h * 0.5, w, h, h, h * 0.5, w * 0.04, w * 0.5)
end

return function(ctx)
  local cell = math.max(1, math.floor(math.min(ctx.width / COLUMNS, ctx.height / ROWS)))
  local id = tostring({})

  local up, until_ = {}, {}
  local raised = {}
  for i = 1, HOLES do
    up[i], until_[i] = false, 0
    raised[i] = morf.signal("impasto.whack." .. id .. ".up." .. i, false)
  end
  local elapsed = morf.signal("impasto.whack." .. id .. ".elapsed", 0)
  local miss_hole = morf.signal("impasto.whack." .. id .. ".miss", -1)
  local miss_left = 0
  local spawn_gap = morf.signal("impasto.whack." .. id .. ".gap", 600)

  -- How long a mole stays up, shrinking with the score.
  local function stay() return math.max(450, 1100 - ctx.score:get() * 20) end

  -- Out of the pit on a spring, back into it flat: the overshoot is what
  -- makes it look alive, and a mole dropping with one would bounce off the
  -- ground. The two curves differ, so each move is played by hand.
  local moles = {}
  local function put(hole, is_up, when)
    local was = up[hole]
    up[hole], until_[hole] = is_up, when
    raised[hole]:set(is_up)
    local mole = moles[hole]
    if mole and was ~= is_up then
      if is_up then
        common.kick(mole, "translate_y", 0, -cell * 0.55, 190, "out_back")
      else
        common.kick(mole, "translate_y", -cell * 0.55, 0, 110, "in_quad")
      end
    end
  end

  local function restart()
    for i = 1, HOLES do put(i, false, 0) end
    elapsed:set(0)
    spawn_gap:set(600)
    ctx.score:set(0)
    miss_hole:set(-1)
    miss_left = 0
    ctx.over:set(false)
  end

  local function spawn()
    local count = 0
    for i = 1, HOLES do if up[i] then count = count + 1 end end
    if count < MAX_UP then
      local free = {}
      local now = elapsed:get()
      for i = 1, HOLES do
        if not up[i] and until_[i] <= now then free[#free + 1] = i end
      end
      if #free > 0 then put(free[common.random(#free) + 1], true, now + stay()) end
    end
    -- Randomised gap before the next mole.
    spawn_gap:set(math.floor(stay() * (0.4 + math.random() * 0.5) + 0.5))
  end

  -- Takes two seconds off the clock without shortening the moles: every
  -- hole's timing shifts with it.
  local function miss(hole)
    if ctx.over:get() or miss_left > 0 then return end
    miss_hole:set(hole)
    miss_left = STUN
    elapsed:set(math.min(ROUND, elapsed:get() + PENALTY))
    for i = 1, HOLES do until_[i] = until_[i] + PENALTY end
  end

  local struck = {}

  local function whack(hole)
    if ctx.over:get() then return end
    if not up[hole] then
      miss(hole)
      return
    end
    ctx.score:set(ctx.score:get() + 1)
    put(hole, false, elapsed:get() + REST)
    if struck[hole] then struck[hole]() end
  end

  local function step()
    local now = elapsed:get() + TICK
    elapsed:set(now)
    if miss_left > 0 then
      miss_left = math.max(0, miss_left - TICK)
      if miss_left == 0 then miss_hole:set(-1) end
    end
    for i = 1, HOLES do
      if up[i] and until_[i] <= now then put(i, false, now + REST) end
    end
    if now >= ROUND then
      for i = 1, HOLES do put(i, false, until_[i]) end
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
    end
  end

  -- ------------------------------------------------------------- holes --

  local mole_w, mole_h = cell * 0.46, cell * 0.56
  -- One hole, built on a budget of its own (see `common.cells`).
  local function build_hole(i)
    local hole = raised[i]
    -- Red while a miss on it is flashing.
    local function earth()
      return miss_hole:get() == i and C.red() or C.islandSurfaceHover
    end
    local function dark() return common.darker(ctx.tint(), 1.7) end
    local function mid() return common.darker(ctx.tint(), 1.25) end
    local function light() return common.lighter(ctx.tint(), 1.3) end

    local parts = {}
    -- Ears, behind the head.
    for _, side in ipairs { -1, 1 } do
      local w = mole_w * 0.30
      parts[#parts + 1] = ui.Rect {
        x = mole_w * 0.5 + side * mole_w * 0.34 - w / 2, y = mole_h * 0.02,
        width = w, height = w, radius = w / 2, color = mid,
      }
    end
    parts[#parts + 1] = ui.Image {
      x = 0, y = 0, width = mole_w, height = mole_h,
      source = function() return dome_svg(mole_w, mole_h, light(), ctx.tint(), mid()) end,
    }
    -- Snout and nose.
    parts[#parts + 1] = ui.Rect {
      x = mole_w * 0.26, y = mole_h * 0.46, width = mole_w * 0.48, height = mole_h * 0.34,
      radius = mole_w * 0.24, opacity = 0.55, color = light,
    }
    local nose = mole_w * 0.16
    parts[#parts + 1] = ui.Rect {
      x = mole_w * 0.5 - nose / 2, y = mole_h * 0.56, width = nose, height = nose * 0.8,
      radius = nose * 0.4, color = dark,
    }
    for _, side in ipairs { -1, 1 } do
      local w, h = mole_w * 0.18, mole_w * 0.22
      parts[#parts + 1] = ui.Rect {
        x = mole_w * 0.5 + side * mole_w * 0.19 - w / 2, y = mole_h * 0.24,
        width = w, height = h, radius = w / 2, color = C.island,
        ui.Rect { x = w * 0.16, y = h * 0.14, width = w * 0.4, height = w * 0.4, radius = w * 0.2, color = C.indicator },
      }
    end
    -- Paws over the lip.
    for _, side in ipairs { -1, 1 } do
      local w, h = mole_w * 0.26, mole_h * 0.16
      parts[#parts + 1] = ui.Rect {
        x = mole_w * 0.5 + side * mole_w * 0.42 - w / 2, y = mole_h * 0.82,
        width = w, height = h, radius = h / 2, color = mid,
      }
    end

    local mole = ui.Item {
      x = (cell - mole_w) / 2, y = cell * 0.74, width = mole_w, height = mole_h,
      table.unpack(parts),
    }
    moles[i] = mole

    local burst, play_burst = common.burst {
      tint = ctx.tint, spread = cell * 0.26,
      x = cell / 2 - cell * 0.26, y = cell * 0.42 - cell * 0.26,
    }
    local pop, play_pop = common.pop {
      tint = ctx.tint, rise = cell * 0.2, width = cell, x = 0, y = cell * 0.28,
    }
    struck[i] = function()
      play_burst()
      play_pop("+1")
    end

    local lip_h = cell * 0.20
    return ui.Item {
      x = ((i - 1) % COLUMNS) * cell, y = ((i - 1) // COLUMNS) * cell,
      width = cell, height = cell,
      -- The earth heaped behind the pit.
      ui.Rect {
        x = cell * 0.10, y = cell * 0.58, width = cell * 0.80, height = cell * 0.30,
        radius = cell * 0.15, color = earth, behavior = { color = theme.behave("fast") },
      },
      -- The pit, dark and the same shape.
      ui.Rect {
        x = cell * 0.16, y = cell * 0.61, width = cell * 0.68, height = cell * 0.22,
        radius = cell * 0.11, color = C.island,
      },
      -- The creature, clipped at the lip so it climbs out of the pit rather
      -- than appearing over it.
      ui.ClipRect {
        x = 0, y = 0, width = cell, height = cell * 0.72, color = "#00000000",
        mole,
      },
      -- The lip of the pit, in front of the creature.
      ui.Rect {
        x = cell * 0.10, y = cell * 0.70, width = cell * 0.80, height = lip_h,
        radius = lip_h / 2, color = earth, behavior = { color = theme.behave("fast") },
        ui.Rect {
          x = 0, y = 0, width = cell * 0.80, height = lip_h * 0.3, radius = lip_h * 0.15,
          opacity = 0.4, color = C.islandBorder,
        },
      },
      burst,
      pop,
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return hole:get() and "pointer" or "default" end,
        on_pressed = function() whack(i) end,
      },
    }
  end
  local holes = common.cells(HOLES, build_hole, { x = 0, y = 0 })

  -- ------------------------------------------------------------ ground --

  local ground_w, ground_h = cell * COLUMNS, cell * ROWS
  local bar_h = math.max(2, math.floor(cell * 0.03 + 0.5))
  local bar_w = ground_w - 2 * theme.radius_medium
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Timer {
      interval = function() return spawn_gap:get() end,
      ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then spawn() end end,
    },
    ui.Timer {
      interval = TICK, ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then step() end end,
    },
    common.ground {
      x = (ctx.width - ground_w) / 2, y = (ctx.height - ground_h) / 2,
      width = ground_w, height = ground_h,
      -- Time left, draining from the right; animated, so a miss's two
      -- seconds visibly drain rather than jump.
      ui.Rect {
        x = theme.radius_medium, y = theme.radius_medium,
        width = bar_w, height = bar_h, radius = bar_h / 2, color = C.islandBorder,
        ui.Rect {
          x = 0, y = 0, height = bar_h, radius = bar_h / 2, color = ctx.tint,
          width = function() return math.max(0.01, bar_w * math.max(0, 1 - elapsed:get() / ROUND)) end,
          behavior = { width = theme.behave("medium") },
        },
      },
      holes,
    },
  }

  local function key(keysym)
    local digit = common.digit(keysym)
    if not digit or digit < 1 then return false end
    whack(digit)
    return true
  end

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function()
      local list = {}
      for i = 1, HOLES do if up[i] then list[#list + 1] = i end end
      return "up " .. table.concat(list, ",") .. " t " .. elapsed:get()
    end,
  }
end
