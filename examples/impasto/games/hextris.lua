-- Hextris: a central hexagon with six lanes running in to its sides. Slabs
-- fall down a lane and stack on the side beneath; the arrows rotate the
-- hexagon and its stacks. Three or more touching slabs of one colour, along
-- a side or around a ring, clear, and the outer slabs settle in. A stack
-- nine deep ends the round.
--
-- Ten points per slab cleared, multiplied by the chain length; the fall
-- speeds up with the score.
--
-- Port of Hextris.qml. The original repainted a Canvas thirty times a
-- second. Here every slab a stack can hold is a node built once: all the
-- slabs of one ring are the same shape turned by a sixth, so each is one
-- small SVG (per ring and colour) inside a board-sized item turned to its
-- side, and the whole hexagon turns as one item with a behaviour on its
-- rotation. The falling slab is the only thing the thirty-hertz tick moves.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local SIDES = 6
local RINGS = 8
local SPAWN_AT = 10.5
local DROP_PACE = 0.8
local FLASH_FRAMES = 7
local H = math.sqrt(3) / 2
-- Room around a slab's picture for its stroke and glow.
local M = 4

local function hex(c) return morf.color(c):hex():sub(1, 7) end
local EMPTY = '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>'

--- A slab of side 0 (the top side, before any turns) between radii `r1`
--- and `r2`, as an SVG whose origin is the board's centre shifted by the
--- picture's own corner. `fill` is a colour or "lit"; `glow` is the falling
--- slab's halo.
local function slab_svg(r1, r2, colour, lit, glow)
  local w, h = r2 + 2 * M, (r2 - r1) * H + 2 * M
  -- Board-centred coordinates moved into the picture.
  local ox, oy = r2 / 2 + M, r2 * H + M
  local pts = string.format("%g,%g %g,%g %g,%g %g,%g",
    ox - r1 / 2, oy - r1 * H, ox + r1 / 2, oy - r1 * H,
    ox + r2 / 2, oy - r2 * H, ox - r2 / 2, oy - r2 * H)
  local fill
  local defs = ""
  if lit then
    fill = hex(C.indicator)
  else
    defs = string.format(
      '<defs><linearGradient id="f" gradientUnits="userSpaceOnUse" x1="0" y1="%g" x2="0" y2="%g">'
        .. '<stop offset="0" stop-color="%s"/><stop offset="0.65" stop-color="%s"/>'
        .. '<stop offset="1" stop-color="%s"/></linearGradient></defs>',
      oy - r1 * H, oy - r2 * H,
      hex(common.darker(colour, 1.35)), hex(colour), hex(common.lighter(colour, 1.25)))
    fill = "url(#f)"
  end
  local halo = ""
  if glow then
    halo = string.format('<polygon points="%s" fill="none" stroke="%s" stroke-opacity="0.35" stroke-width="5" stroke-linejoin="round"/>',
      pts, hex(colour))
  end
  return string.format(
    '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %g %g">%s%s'
      .. '<polygon points="%s" fill="%s" stroke="%s" stroke-opacity="0.55" stroke-width="2" stroke-linejoin="round"/></svg>',
    math.ceil(w), math.ceil(h), w, h, defs, halo, pts, fill, hex(C.island)), w, h
end

--- A hexagon of `radius` around the centre of a `size` square, flat side
--- up, as SVG points.
local function hexagon(size, radius)
  local out = {}
  for corner = 0, SIDES - 1 do
    local at = -2 * math.pi / 3 + corner * math.pi / 3
    out[#out + 1] = string.format("%g,%g", size / 2 + radius * math.cos(at), size / 2 + radius * math.sin(at))
  end
  return table.concat(out, " ")
end

return function(ctx)
  local size = math.max(1, math.floor(math.min(ctx.width, ctx.height)))
  -- Hexagon radius and ring thickness, sized so eight rings plus a falling
  -- slab fit in half the board.
  local core = math.floor(size * 0.09)
  local ring = math.floor(size * 0.035)
  local centre = size / 2
  local id = tostring({})

  local function colours()
    return { ctx.tint(), C.green(), C.yellow(), C.blue() }
  end

  -- One array per side, innermost first, of colour indices.
  local stacks = {}
  local turn = morf.signal("impasto.hextris." .. id .. ".turn", 0)
  local falling = { lane = 0, colour = 1, at = SPAWN_AT }
  local falling_colour = morf.signal("impasto.hextris." .. id .. ".fc", 1)
  local falling_lane = morf.signal("impasto.hextris." .. id .. ".fl", 0)
  local falling_level = morf.signal("impasto.hextris." .. id .. ".flv", 10)
  local dropping = false
  -- Matched slabs stay lit for a few frames so chains read one clear at a
  -- time.
  local flash = {}
  local flash_left = 0
  local chain = 1

  -- What each slab node shows: 0 nothing, a colour index, or -1 for lit.
  local shown = {}
  for side = 0, SIDES - 1 do
    for level = 0, RINGS do
      shown[side * 100 + level] = morf.signal("impasto.hextris." .. id .. ".s" .. side .. "." .. level, 0)
    end
  end

  local function pace() return math.min(0.34, 0.1 + ctx.score:get() / 5000) end

  local function show()
    for side = 0, SIDES - 1 do
      local stack = stacks[side + 1]
      for level = 0, RINGS do
        local key = side * 100 + level
        local value = stack[level + 1] or 0
        if value ~= 0 and flash[key] then value = -1 end
        if shown[key]:get() ~= value then shown[key]:set(value) end
      end
    end
  end

  -- The hexagon side under a lane, after the turns.
  local function side_under(lane) return (lane - turn:get()) % SIDES end

  local falling_node
  local function place_falling()
    local level = math.max(0, math.floor(falling.at))
    falling_level:set(level)
    local r_level = core + level * ring
    local r = core + falling.at * ring
    if falling_node then
      falling_node.translate_y = -(falling.at - level) * ring * H
      falling_node.scale_x = r / r_level
    end
  end

  local function spawn()
    falling.lane = common.random(SIDES)
    falling.colour = common.random(4) + 1
    falling.at = SPAWN_AT
    falling_lane:set(falling.lane)
    falling_colour:set(falling.colour)
    dropping = false
    place_falling()
  end

  -- Every slab touching two or more of its colour, flood-filled from each
  -- unvisited one. Lit slabs act as walls so nothing is counted twice.
  local function matches()
    local seen, found = {}, {}
    for side = 0, SIDES - 1 do
      for level = 0, #stacks[side + 1] - 1 do
        local start = side * 100 + level
        if not seen[start] and not flash[start] then
          local colour = stacks[side + 1][level + 1]
          local group, queue = {}, { { side, level } }
          seen[start] = true
          local head = 1
          while head <= #queue do
            local cell = queue[head]
            head = head + 1
            group[#group + 1] = cell[1] * 100 + cell[2]
            for _, n in ipairs {
              { cell[1], cell[2] - 1 }, { cell[1], cell[2] + 1 },
              { (cell[1] + 1) % SIDES, cell[2] }, { (cell[1] + SIDES - 1) % SIDES, cell[2] },
            } do
              local stack = stacks[n[1] + 1]
              if n[2] >= 0 and n[2] < #stack then
                local key = n[1] * 100 + n[2]
                if not seen[key] and not flash[key] and stack[n[2] + 1] == colour then
                  seen[key] = true
                  queue[#queue + 1] = n
                end
              end
            end
          end
          if #group >= 3 then
            for _, key in ipairs(group) do found[#found + 1] = key end
          end
        end
      end
    end
    return found
  end

  -- Lights and scores matches. Runs after each landing and each settle, so
  -- the multiplier grows through a chain and resets when nothing matches.
  local function resolve()
    local lit = matches()
    if #lit == 0 then
      chain = 1
      return
    end
    ctx.score:set(ctx.score:get() + #lit * 10 * chain)
    chain = chain + 1
    for _, key in ipairs(lit) do flash[key] = true end
    flash_left = FLASH_FRAMES
  end

  -- Lit slabs go and everything outside them slides in.
  local function settle()
    for side = 0, SIDES - 1 do
      local kept = {}
      for level, colour in ipairs(stacks[side + 1]) do
        if not flash[side * 100 + level - 1] then kept[#kept + 1] = colour end
      end
      stacks[side + 1] = kept
    end
    flash = {}
    resolve()
  end

  local function land()
    local side = side_under(falling.lane)
    local stack = stacks[side + 1]
    stack[#stack + 1] = falling.colour
    if #stack > RINGS then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
      return
    end
    resolve()
    spawn()
  end

  local function tick()
    if flash_left > 0 then
      flash_left = flash_left - 1
      if flash_left == 0 then settle() end
    end
    local step = dropping and DROP_PACE or pace()
    local floor = #stacks[side_under(falling.lane) + 1]
    if falling.at - step <= floor then
      land()
    else
      falling.at = falling.at - step
    end
    place_falling()
    show()
  end

  local function restart()
    stacks = { {}, {}, {}, {}, {}, {} }
    turn:set(0)
    flash, flash_left, chain = {}, 0, 1
    ctx.score:set(0)
    ctx.over:set(false)
    spawn()
    show()
  end

  local function rotate(steps)
    if not ctx.over:get() then turn:set(turn:get() + steps) end
  end
  local function drop()
    if not ctx.over:get() then dropping = true end
  end

  -- -------------------------------------------------------------- nodes --

  -- One slab node: a board-sized item turned to its side, holding the
  -- picture of its ring in its colour.
  local function slab_node(side, level, holder)
    local r1, r2 = core + level * ring, core + (level + 1) * ring
    local _, w, h = slab_svg(r1, r2, "#000000", false, false)
    return ui.Item {
      x = 0, y = 0, width = size, height = size,
      rotation = side * 60,
      ui.Image {
        x = centre - r2 / 2 - M, y = centre - r2 * H - M, width = w, height = h,
        visible = function() return holder:get() ~= 0 end,
        source = function()
          local v = holder:get()
          if v == 0 then return EMPTY end
          local svg = slab_svg(r1, r2, v > 0 and colours()[v] or "#000000", v < 0, false)
          return svg
        end,
      },
    }
  end

  local slabs = common.cells(SIDES * (RINGS + 1), function(i)
    local side, level = (i - 1) // (RINGS + 1), (i - 1) % (RINGS + 1)
    return slab_node(side, level, shown[side * 100 + level])
  end, { x = 0, y = 0 })

  -- The fixed hexagons: the stack limit, in the colour that says so, the
  -- core, and a hairline inside it. Symmetric under a sixth of a turn, they
  -- turn back by the whole turns so the core's light stays on top.
  local frame_svg = function()
    return string.format(
      '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d">'
        .. '<defs><linearGradient id="c" gradientUnits="userSpaceOnUse" x1="0" y1="%g" x2="0" y2="%g">'
        .. '<stop offset="0" stop-color="%s"/><stop offset="1" stop-color="%s"/></linearGradient></defs>'
        .. '<polygon points="%s" fill="none" stroke="%s" stroke-width="2"/>'
        .. '<polygon points="%s" fill="url(#c)" stroke="%s" stroke-width="2"/>'
        .. '<polygon points="%s" fill="none" stroke="%s" stroke-width="1"/></svg>',
      size, size, centre - core, centre + core,
      hex(C.islandSurfaceHover), hex(C.island),
      hexagon(size, core + RINGS * ring), common.over(C.red(), 0.3):hex():sub(1, 7),
      hexagon(size, core), common.over(ctx.tint(), 0.5, C.island):hex():sub(1, 7),
      hexagon(size, core * 0.55), common.over("#ffffff", 0.125, C.island):hex():sub(1, 7))
  end
  local frame = ui.Image {
    x = 0, y = 0, width = size, height = size,
    rotation = function() return -turn:get() * 60 end,
    source = frame_svg,
  }

  local spinner = ui.Item {
    x = 0, y = 0, width = size, height = size,
    rotation = function() return turn:get() * 60 end,
    behavior = { rotation = { duration = 260, easing = "out_cubic" } },
    frame,
    slabs,
  }

  -- The falling slab, in a lane that doesn't rotate, with its own light
  -- around it. It is drawn at the ring it is passing and stretched to the
  -- radius it is really at.
  local falling_image = ui.Image {
    x = function()
      local r2 = core + (falling_level:get() + 1) * ring
      return centre - r2 / 2 - M
    end,
    y = function()
      local r2 = core + (falling_level:get() + 1) * ring
      return centre - r2 * H - M
    end,
    width = function()
      local r2 = core + (falling_level:get() + 1) * ring
      return r2 + 2 * M
    end,
    height = ring * H + 2 * M,
    visible = function() return not ctx.over:get() end,
    source = function()
      local level = falling_level:get()
      return (slab_svg(core + level * ring, core + (level + 1) * ring,
        colours()[falling_colour:get()], false, true))
    end,
  }
  falling_node = falling_image
  local lane = ui.Item {
    x = 0, y = 0, width = size, height = size,
    rotation = function() return falling_lane:get() * 60 end,
    falling_image,
  }

  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Timer {
      interval = 33, ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then tick() end end,
    },
    common.ground {
      x = (ctx.width - size) / 2, y = (ctx.height - size) / 2,
      width = size, height = size,
      -- Pointer controls: outer thirds rotate, the middle drops.
      ui.MouseArea {
        anchors = { fill = true },
        on_clicked = function(_, _, lx)
          if lx < size / 3 then rotate(-1)
          elseif lx > 2 * size / 3 then rotate(1)
          else drop() end
        end,
      },
      spinner,
      lane,
    },
  }

  local function key(keysym)
    local l = common.letter(keysym)
    if keysym == common.K.LEFT or l == "a" then rotate(-1)
    elseif keysym == common.K.RIGHT or l == "d" then rotate(1)
    elseif keysym == common.K.DOWN or l == "s" or keysym == common.K.SPACE then drop()
    else return false end
    return true
  end

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function()
      local depths = {}
      for side = 1, SIDES do depths[side] = #stacks[side] end
      return "stacks " .. table.concat(depths, ",") .. " at " .. string.format("%.1f", falling.at)
        .. " lane " .. falling.lane .. " turn " .. turn:get()
    end,
  }
end
