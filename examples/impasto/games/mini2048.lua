-- 2048 on a four by four board. Tiles slide as far as they can; equal
-- neighbours merge once per move (2 2 2 2 becomes 4 4, not 8), and a move
-- that changes nothing doesn't count. Each real move adds a 2, or a 4 one
-- time in ten. The round ends when the board is full with no merges left;
-- reaching 2048 doesn't end it. The score is the sum of all merges.
--
-- Port of Mini2048.qml: sixteen tiles built once, since nothing moves
-- between key presses; a tile that takes a value swells and settles.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local SIDE = 4
local GAP = 10

-- Tint up to 32, accent for 64, yellow for 128-512, red for 1024 and on.
local function fill(value, tint)
  local y, r = C.yellow(), C.red()
  if value == 0 then return C.islandSurfaceHover end
  if value == 2 then return common.alpha(tint, 0.25) end
  if value == 4 then return common.alpha(tint, 0.4) end
  if value == 8 then return common.alpha(tint, 0.6) end
  if value == 16 then return common.alpha(tint, 0.8) end
  if value == 32 then return tint end
  if value == 64 then return C.accent() end
  if value == 128 then return common.alpha(y, 0.7) end
  if value == 256 then return common.alpha(y, 0.85) end
  if value == 512 then return y end
  if value == 1024 then return common.alpha(r, 0.85) end
  return r
end

-- White text on dark tiles, black on bright ones, judged by the tile's
-- luminance over the background rather than its value.
local function ink(value, tint)
  local c = morf.color(fill(value, tint))
  local luma = (0.299 * c.r + 0.587 * c.g + 0.114 * c.b) * c.a
  return luma > 0.5 and C.island or C.text()
end

return function(ctx)
  local cell = math.max(1, math.floor((math.min(ctx.width, ctx.height) - (SIDE + 1) * GAP) / SIDE))
  local id = tostring({})
  local grid, values = {}, {}
  for i = 1, SIDE * SIDE do
    grid[i] = 0
    values[i] = morf.signal("impasto.2048." .. id .. "." .. i, 0)
  end
  local tiles = {}

  -- A 2 (or a 4, one time in ten) on a random free cell.
  local function spawn(cells)
    local free = {}
    for i = 1, SIDE * SIDE do if cells[i] == 0 then free[#free + 1] = i end end
    if #free == 0 then return end
    cells[free[common.random(#free) + 1]] = math.random() < 0.1 and 4 or 2
  end

  -- One line, read from the wall the tiles move towards: packed, merged
  -- once outward from the wall, then padded with empties.
  local function slide(line)
    local packed = {}
    for _, v in ipairs(line) do if v ~= 0 then packed[#packed + 1] = v end end
    local out, gained = {}, 0
    local i = 1
    while i <= #packed do
      if i + 1 <= #packed and packed[i] == packed[i + 1] then
        out[#out + 1] = packed[i] * 2
        gained = gained + packed[i] * 2
        i = i + 2
      else
        out[#out + 1] = packed[i]
        i = i + 1
      end
    end
    while #out < SIDE do out[#out + 1] = 0 end
    return out, gained
  end

  -- No free cell and no equal neighbours in either direction.
  local function stuck(cells)
    for y = 0, SIDE - 1 do
      for x = 0, SIDE - 1 do
        local v = cells[y * SIDE + x + 1]
        if v == 0 then return false end
        if x + 1 < SIDE and cells[y * SIDE + x + 2] == v then return false end
        if y + 1 < SIDE and cells[(y + 1) * SIDE + x + 1] == v then return false end
      end
    end
    return true
  end

  local function show()
    for i = 1, SIDE * SIDE do
      if values[i]:get() ~= grid[i] then
        values[i]:set(grid[i])
        if grid[i] ~= 0 and tiles[i] then common.kick(tiles[i], "scale", 1.16, 1, 170, "out_back") end
      end
    end
  end

  local function restart()
    for i = 1, SIDE * SIDE do grid[i] = 0 end
    spawn(grid)
    spawn(grid)
    ctx.score:set(0)
    ctx.over:set(false)
    show()
  end

  local function move(dx, dy)
    if ctx.over:get() then return end
    local gained, changed = 0, false
    for lane = 0, SIDE - 1 do
      -- This line's cells, nearest the wall first.
      local indices, line = {}, {}
      for step = 0, SIDE - 1 do
        local near, far = step, SIDE - 1 - step
        local x = dx ~= 0 and (dx > 0 and far or near) or lane
        local y = dy ~= 0 and (dy > 0 and far or near) or lane
        indices[#indices + 1] = y * SIDE + x + 1
        line[#line + 1] = grid[y * SIDE + x + 1]
      end
      local out, got = slide(line)
      gained = gained + got
      for step, index in ipairs(indices) do
        if grid[index] ~= out[step] then
          grid[index] = out[step]
          changed = true
        end
      end
    end
    if not changed then return end
    ctx.score:set(ctx.score:get() + gained)
    spawn(grid)
    show()
    if stuck(grid) then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
    end
  end

  for i = 1, SIDE * SIDE do
    local held = values[i]
    local function value() return held:get() end
    tiles[i] = ui.Rect {
      x = GAP + ((i - 1) % SIDE) * (cell + GAP),
      y = GAP + ((i - 1) // SIDE) * (cell + GAP),
      width = cell, height = cell,
      radius = theme.radius_small,
      -- Mixed over the ground as Qt mixes it (see `common.over`), with
      -- the light on the face, so a tile is a tile and not a patch of
      -- colour. The face fades to a new value on the fast curve, as
      -- Mini2048.qml's ColorAnimation, while the swell carries the beat.
      color = "#00000000",
      gradient = function()
        local c = morf.color(fill(value(), ctx.tint()))
        local face = common.over(c, c.a)
        if value() == 0 then return common.lit(face, 0, 0, 0, 0.45) end
        return common.lit(face, 0.16, 0.03, 0.14, 0.45)
      end,
      behavior = { gradient = theme.behave("fast") },
      border_color = function() return value() == 0 and "#00000000" or "#00000040" end,
      border_width = 1,
      ui.Text {
        x = 0, y = 0, width = cell, height = cell,
        horizontal_alignment = "center", vertical_alignment = "center",
        visible = function() return value() ~= 0 end,
        text = function() return tostring(value()) end,
        color = function() return ink(value(), ctx.tint()) end,
        font_family = function() return theme.font_mono() end,
        font_weight = 600,
        -- Three digits fit at full size; four shrink.
        font_size = function()
          local v = value()
          return math.floor(cell * (v < 100 and 0.4 or v < 1000 and 0.32 or 0.26) + 0.5)
        end,
      },
    }
  end

  local side = SIDE * cell + (SIDE + 1) * GAP
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    common.ground {
      x = (ctx.width - side) / 2, y = (ctx.height - side) / 2,
      width = side, height = side,
      table.unpack(tiles),
    },
  }

  local function key(keysym)
    local dx, dy = common.direction(keysym, true)
    if not dx then return false end
    move(dx, dy)
    return true
  end

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function() return "grid " .. table.concat(grid, ",") end,
  }
end
