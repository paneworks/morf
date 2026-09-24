-- Space Blaster: a ship at the bottom and a formation of twenty invaders
-- that sweeps sideways and steps down at each edge, speeding up as it
-- thins. At most three shots in flight, three lives, and a second of
-- invulnerability after a hit.
--
-- Ten points per invader, twenty for the front row; each wave is faster.
-- The round ends when the lives run out or the formation reaches the
-- ship's row.
--
-- Port of Space.qml. The original repainted a Canvas at 30 fps. Here the
-- sky is one picture, each invader is two small pictures (its two walking
-- frames) in one formation item that the tick moves, and shots, bombs and
-- blasts come from small pools. The tick writes positions, never builds.
--
-- Left and right were held keys there: the tick read them, and a release
-- cleared them. morf hands a configuration presses (and the keyboard's own
-- repeats) but no releases, so here a press holds its direction for a
-- moment and every repeat extends it: a tap nudges the ship, a held key
-- flies it.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local MARGIN = 16
local COLUMNS, ROWS = 5, 4
local FORMATION_TOP = 64
local DROP_STEP = 18
local SHIP_SPEED = 7
local SHOT_LENGTH = 10
local SHOT_SPEED = 11
local BOMB_SPEED = 5
local SHOTS_IN_FLIGHT = 3
local BOMBS_IN_FLIGHT = 3
local BLAST_LIFE = 9
local BLASTS = 6
-- How long a press holds its direction before a repeat must renew it: the
-- first long enough to be a nudge, the rest just longer than the gap
-- between repeats.
local TAP_HOLD = 300
local REPEAT_HOLD = 110

-- Eleven cells across and eight down, two frames each: the row decides
-- which creature and what it is worth.
local SHAPES = {
  squid = { {
    "....###....", "...#####...", "..#######..", "..##.#.##..",
    "..#######..", "...#.#.#...", "..#.#.#.#..", "...#...#...",
  }, {
    "....###....", "...#####...", "..#######..", "..##.#.##..",
    "..#######..", "...#.#.#...", "..#.....#..", "...#...#...",
  } },
  crab = { {
    "..#.....#..", "...#...#...", "..#######..", ".##.###.##.",
    "###########", "#.#######.#", "#.#.....#.#", "...##.##...",
  }, {
    "..#.....#..", "#..#...#..#", "#.#######.#", "###.###.###",
    "###########", ".#########.", "..#.....#..", ".#.......#.",
  } },
  octopus = { {
    "...#####...", "..#######..", ".#########.", "###..#..###",
    "###########", "..###.###..", ".##.....##.", "...##.##...",
  }, {
    "...#####...", "..#######..", ".#########.", "###..#..###",
    "###########", "...#.#.#...", "..#.#.#.#..", ".##.....##.",
  } },
}

-- The front row is worth double and wears the colour that says so.
local function breed_of(row)
  if row == ROWS - 1 then return "octopus" end
  return row == 0 and "squid" or "crab"
end
local function colour_of(row)
  if row == ROWS - 1 then return C.yellow() end
  return row == 0 and C.green() or C.red()
end

local function hex(c) return morf.color(c):hex():sub(1, 7) end

--- One walking frame of one creature, as runs of cells.
local function invader_svg(shape, size, colour)
  local unit = size / #shape[1]
  local top = (size - unit * #shape) / 2
  local rects = {}
  for row, cells in ipairs(shape) do
    local at = 1
    while at <= #cells do
      if cells:sub(at, at) ~= "#" then
        at = at + 1
      else
        local run = 1
        while at + run <= #cells and cells:sub(at + run, at + run) == "#" do run = run + 1 end
        rects[#rects + 1] = string.format('<rect x="%g" y="%g" width="%g" height="%g"/>',
          (at - 1) * unit, top + (row - 1) * unit, unit * run + 0.5, unit + 0.5)
        at = at + run
      end
    end
  end
  return string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d"><g fill="%s">%s</g></svg>',
    size, size, hex(colour), table.concat(rects))
end

return function(ctx)
  local bw, bh = math.floor(ctx.width), math.floor(ctx.height)
  -- Designed at 520 x 600: a column is a tenth of the width, and
  -- everything else derives from it.
  local stride = math.max(1, math.floor(bw / 10))
  local row_stride = math.floor(stride * 0.85)
  local size = math.floor(stride * 0.58)
  local ship_w = math.floor(stride * 0.7)
  local ship_h = math.floor(ship_w * 0.6)
  local ship_top = bh - 40 - ship_h
  local lane = math.max(1, bw - 2 * MARGIN - ship_w)
  local id = tostring({})

  local invaders = {}          -- by c * 10 + r: alive
  local count = 0
  local form_x, form_y, form_dir = MARGIN, FORMATION_TOP, 1
  local shots, bombs, blasts = {}, {}, 0
  local ship_at = 0.5
  local lives = morf.signal("impasto.space." .. id .. ".lives", 3)
  local wave = 0
  local shield = 0
  local ticks = 0
  local frame = morf.signal("impasto.space." .. id .. ".frame", 0)
  local alive = {}
  for c = 0, COLUMNS - 1 do
    for r = 0, ROWS - 1 do alive[c * 10 + r] = morf.signal("impasto.space." .. id .. ".a" .. c .. r, true) end
  end
  local held = { left = 0, right = 0 }
  local clock = morf.elapsed_timer()

  local function ship_x() return MARGIN + ship_w / 2 + ship_at * lane end

  -- Formation speed per tick: a base that rises with the wave, multiplied
  -- as the formation thins.
  local function pace()
    local thinned = 1 - count / (COLUMNS * ROWS)
    return math.min(10, (1.8 + 0.5 * wave) * (1 + 2 * thinned))
  end

  -- -------------------------------------------------------------- nodes --

  local formation, ship, flame, shot_nodes, bomb_nodes, blast_play = nil, nil, nil, {}, {}, {}

  local function spawn_wave()
    count = 0
    for c = 0, COLUMNS - 1 do
      for r = 0, ROWS - 1 do
        invaders[c * 10 + r] = true
        alive[c * 10 + r]:set(true)
        count = count + 1
      end
    end
    form_x, form_y, form_dir = MARGIN, FORMATION_TOP, 1
    shots, bombs = {}, {}
  end

  local function draw()
    formation.translate_x, formation.translate_y = form_x, form_y
    frame:set((ticks // 9) % 2)
    local sx = ship_x()
    ship.translate_x = sx - ship_w / 2
    -- It skips frames while invulnerable.
    ship.visible = (shield // 3) % 2 == 0
    flame.scale_y = (0.3 + 0.22 * math.abs(math.sin(ticks / 2))) / 0.52
    for i = 1, SHOTS_IN_FLIGHT do
      local s, n = shots[i], shot_nodes[i]
      n.visible = s ~= nil
      if s then n.translate_x, n.translate_y = s.x, s.y end
    end
    for i = 1, BOMBS_IN_FLIGHT do
      local b, n = bombs[i], bomb_nodes[i]
      n.visible = b ~= nil
      if b then n.translate_x, n.translate_y = b.x, b.y end
    end
  end

  -- ------------------------------------------------------------- rules --

  local function fire()
    if ctx.over:get() or #shots >= SHOTS_IN_FLIGHT then return end
    shots[#shots + 1] = { x = ship_x(), y = ship_top - SHOT_LENGTH }
  end

  -- Bombs drop from the lowest invader of a random column, so none fires
  -- through its own front row.
  local function drop_bomb()
    local standing = {}
    for key in pairs(invaders) do standing[#standing + 1] = key end
    if #standing == 0 then return end
    local pick = standing[common.random(#standing) + 1]
    local c = pick // 10
    local lowest = -1
    for r = 0, ROWS - 1 do if invaders[c * 10 + r] then lowest = r end end
    bombs[#bombs + 1] = {
      x = form_x + c * stride + size / 2,
      y = form_y + lowest * row_stride + size,
    }
  end

  local function finish()
    ctx.over:set(true)
    ctx.finished(ctx.score:get())
  end

  local function step()
    ticks = ticks + 1
    -- Ship.
    local now = clock:elapsed_ms()
    local heading = (held.right > now and 1 or 0) - (held.left > now and 1 or 0)
    ship_at = math.max(0, math.min(1, ship_at + heading * SHIP_SPEED / lane))
    if shield > 0 then shield = shield - 1 end

    -- Our shots, and hits.
    local flying = {}
    for _, shot in ipairs(shots) do
      local y = shot.y - SHOT_SPEED
      if y + SHOT_LENGTH >= 0 then
        local hit
        for key in pairs(invaders) do
          local ix, iy = form_x + (key // 10) * stride, form_y + (key % 10) * row_stride
          if shot.x >= ix and shot.x <= ix + size and y <= iy + size and y + SHOT_LENGTH >= iy then
            hit = key
            break
          end
        end
        if hit then
          local r = hit % 10
          ctx.score:set(ctx.score:get() + (r == ROWS - 1 and 20 or 10))
          invaders[hit] = nil
          alive[hit]:set(false)
          count = count - 1
          blasts = blasts % BLASTS + 1
          blast_play[blasts](form_x + (hit // 10) * stride + size / 2, form_y + r * row_stride + size / 2, r)
        else
          flying[#flying + 1] = { x = shot.x, y = y }
        end
      end
    end
    shots = flying
    if count == 0 then
      wave = wave + 1
      spawn_wave()
      draw()
      return
    end

    -- Formation: sideways, stepping down when its own edge (not the
    -- grid's) reaches a side.
    local min_c, max_c, max_r = COLUMNS, -1, -1
    for key in pairs(invaders) do
      min_c, max_c = math.min(min_c, key // 10), math.max(max_c, key // 10)
      max_r = math.max(max_r, key % 10)
    end
    local x, y = form_x + form_dir * pace(), form_y
    local left_edge = x + min_c * stride
    local right_edge = x + max_c * stride + size
    if left_edge < MARGIN or right_edge > bw - MARGIN then
      x = left_edge < MARGIN and MARGIN - min_c * stride or bw - MARGIN - size - max_c * stride
      y = y + DROP_STEP
      form_dir = -form_dir
    end
    form_x, form_y = x, y
    if y + max_r * row_stride + size >= ship_top then
      draw()
      finish()
      return
    end

    -- Bombs, and hits. The shield makes the ship unhittable.
    local falling, struck = {}, false
    local sx = ship_x()
    for _, bomb in ipairs(bombs) do
      local by = bomb.y + BOMB_SPEED
      if by <= bh then
        if shield == 0 and not struck and math.abs(bomb.x - sx) <= ship_w / 2
          and by + SHOT_LENGTH >= ship_top and by <= ship_top + ship_h then
          struck = true
        else
          falling[#falling + 1] = { x = bomb.x, y = by }
        end
      end
    end
    bombs = falling
    if struck then
      lives:set(lives:get() - 1)
      shield = 30
      if lives:get() == 0 then
        draw()
        finish()
        return
      end
    end
    if #bombs < BOMBS_IN_FLIGHT and math.random() < 0.02 + 0.006 * wave then drop_bomb() end
    draw()
  end

  local function restart()
    ctx.score:set(0)
    lives:set(3)
    wave, shield, ship_at, ticks = 0, 0, 0.5, 0
    held.left, held.right = 0, 0
    ctx.over:set(false)
    spawn_wave()
    draw()
  end

  -- --------------------------------------------------------------- sky --

  -- Stars, each at its own strength (mixed over the ground as Qt mixes it,
  -- see `common.over`), and one in seven breathing.
  local stars, twinkles = {}, {}
  for index = 0, 59 do
    local sx, sy = math.random() * bw, math.random() * bh
    local r = 0.7 + math.random() * 1.4
    local lit = 0.12 + math.random() * 0.4
    if index % 7 == 0 then
      local low, high = math.max(0.05, lit - 0.18), lit + 0.18
      twinkles[#twinkles + 1] = ui.Rect {
        x = sx - r, y = sy - r, width = 2 * r, height = 2 * r, radius = r,
        color = common.over(C.indicator, high),
        opacity = low / high,
        behavior = { opacity = { duration = math.floor(9 * 33 * math.pi), easing = "in_out_sine",
          loops = "forever", ping_pong = true } },
      }
    else
      stars[#stars + 1] = string.format('<circle cx="%g" cy="%g" r="%g" fill="%s"/>',
        sx, sy, r, hex(common.over(C.indicator, lit)))
    end
  end
  local sky = ui.Image {
    x = 0, y = 0, width = bw, height = bh,
    source = string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d">%s</svg>',
      bw, bh, table.concat(stars)),
  }

  -- --------------------------------------------------------- formation --

  -- Both frames march on the same clock, so the wall steps together.
  -- Built a picture at a time (see `common.cells`).
  local pictures = common.cells(COLUMNS * ROWS * 2, function(i)
    local f = (i - 1) % 2
    local c, r = ((i - 1) // 2) // ROWS, ((i - 1) // 2) % ROWS
    local key = c * 10 + r
    return ui.Image {
      x = c * stride, y = r * row_stride, width = size, height = size,
      visible = function() return alive[key]:get() and frame:get() == f end,
      source = function() return invader_svg(SHAPES[breed_of(r)][f + 1], size, colour_of(r)) end,
    }
  end, { x = 0, y = 0 })
  formation = ui.Item { x = 0, y = 0, width = bw, height = bh, pictures }

  -- What is left of the ones that are gone: a ring and its pieces, thrown
  -- out of where they stood.
  local blast_nodes = {}
  for i = 1, BLASTS do
    local row = morf.signal("impasto.space." .. id .. ".blast" .. i, 0)
    local node, play = common.burst {
      tint = function() return colour_of(row:get()) end,
      spread = size * 0.85, sparks = 6, span = BLAST_LIFE * 33,
    }
    blast_nodes[i] = node
    blast_play[i] = function(x, y, r)
      row:set(r)
      node.x, node.y = x - size * 0.85, y - size * 0.85
      play()
    end
  end

  -- Shots up, bombs down, each in its own light.
  local function bolt(colour)
    return ui.Item {
      x = 0, y = 0, width = 5, height = SHOT_LENGTH + 4, visible = false,
      ui.Rect { x = -2.5, y = -2, width = 5, height = SHOT_LENGTH + 4,
        color = function() return common.over(colour(), 0.25) end },
      ui.Rect { x = -1, y = 0, width = 2, height = SHOT_LENGTH, color = colour },
    }
  end
  for i = 1, SHOTS_IN_FLIGHT do shot_nodes[i] = bolt(ctx.tint) end
  for i = 1, BOMBS_IN_FLIGHT do bomb_nodes[i] = bolt(C.red) end

  -- The ship: a hull, a cockpit and an engine that flickers.
  local sw, sh = ship_w, ship_h
  local hull = function()
    local pts = string.format("%g,0 %g,%g %g,%g %g,%g 0,%g 0,%g %g,%g",
      sw / 2, sw / 2 + sw * 0.22, sh * 0.62, sw, sh * 0.72, sw, sh, sh, sh * 0.72, sw / 2 - sw * 0.22, sh * 0.62)
    return string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d">'
      .. '<defs><linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="%s"/>'
      .. '<stop offset="1" stop-color="%s"/></linearGradient></defs><polygon points="%s" fill="url(#h)"/></svg>',
      sw, sh, hex(common.lighter(ctx.tint(), 1.4)), hex(common.darker(ctx.tint(), 1.35)), pts)
  end
  local flame_h = sh * 0.52
  flame = ui.Image {
    x = sw / 2 - sw * 0.12, y = sh, width = sw * 0.24, height = flame_h,
    transform_origin_y = 0,
    source = function()
      return string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %g %g" preserveAspectRatio="none">'
        .. '<polygon points="0,0 %g,%g %g,0" fill="%s"/></svg>',
        math.ceil(sw * 0.24), math.ceil(flame_h), sw * 0.24, flame_h, sw * 0.12, flame_h, sw * 0.24,
        hex(common.over(C.yellow(), 0.75)))
    end,
  }
  ship = ui.Item {
    x = 0, y = ship_top, width = sw, height = sh,
    flame,
    ui.Image { x = 0, y = 0, width = sw, height = sh, source = hull },
    ui.Rect { x = sw / 2 - sw * 0.09, y = sh * 0.6 - sw * 0.09, width = sw * 0.18, height = sw * 0.18,
      radius = sw * 0.09, color = common.alpha(C.indicator, 0.85) },
  }

  -- Lives, top right, dimming as they go.
  local dots = {}
  for index = 0, 2 do
    dots[#dots + 1] = ui.Rect {
      x = bw - MARGIN - 10 - index * 16, y = MARGIN, width = 10, height = 10, radius = 5,
      color = function()
        if index < lives:get() then return C.red() end
        return common.over(C.red(), 0.25)
      end,
    }
  end

  local layers = { sky }
  for _, t in ipairs(twinkles) do layers[#layers + 1] = t end
  layers[#layers + 1] = formation
  for _, n in ipairs(blast_nodes) do layers[#layers + 1] = n end
  for _, n in ipairs(shot_nodes) do layers[#layers + 1] = n end
  for _, n in ipairs(bomb_nodes) do layers[#layers + 1] = n end
  layers[#layers + 1] = ship
  for _, n in ipairs(dots) do layers[#layers + 1] = n end

  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Timer {
      interval = 33, ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then step() end end,
    },
    common.ground {
      x = (ctx.width - bw) / 2, y = (ctx.height - bh) / 2, width = bw, height = bh,
      ui.ClipRect {
        x = 0, y = 0, width = bw, height = bh, radius = theme.radius_medium, color = "#00000000",
        table.unpack(layers),
      },
    },
  }

  -- A press holds its direction for a moment; the key's own repeats keep
  -- it held (see the top of the file). The other direction lets go.
  local function hold(side, other)
    local now = clock:elapsed_ms()
    local repeating = held[side] > now
    held[side] = now + (repeating and REPEAT_HOLD or TAP_HOLD)
    held[other] = 0
  end

  local function key(keysym)
    local l = common.letter(keysym)
    if keysym == common.K.LEFT or l == "a" then hold("left", "right")
    elseif keysym == common.K.RIGHT or l == "d" then hold("right", "left")
    elseif keysym == common.K.SPACE then fire()
    else return false end
    return true
  end

  restart()
  -- Written after construction, so the breathing has somewhere to go.
  for _, t in ipairs(twinkles) do t.opacity = 1 end
  return {
    node = node, key = key, restart = restart,
    debug = function()
      return "invaders " .. count .. " wave " .. wave .. " lives " .. lives:get()
        .. " ship " .. string.format("%.2f", ship_at) .. " form " .. math.floor(form_x) .. "," .. math.floor(form_y)
    end,
  }
end
