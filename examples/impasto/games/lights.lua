-- Lights Out: five by five; a press flips a light and its four neighbours,
-- and the round ends when all 25 are off. The score is the press count
-- (lower is better), and `finished` fires only on a dark board.
--
-- Port of Lights.qml. Each start applies six to ten distinct presses to a
-- dark board, so it is always solvable in a few moves. Every light is a
-- node built once; a press writes the lights that changed.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color
local K = common.K

local SIZE = 5
local COUNT = SIZE * SIZE
local GAP = 12
local PAD = 26

return function(ctx)
  local cell = math.max(1, math.floor(
    (math.min(ctx.width, ctx.height) - 2 * PAD - (SIZE - 1) * GAP) / SIZE))
  local stride = cell + GAP
  local span = SIZE * cell + (SIZE - 1) * GAP

  -- Row-major, true where a light is on; one signal per light so a press
  -- repaints only the five it touched.
  local lit = {}
  for i = 1, COUNT do lit[i] = morf.signal("impasto.lights." .. i .. "." .. tostring(lit), false) end
  local selected = morf.signal("impasto.lights.selected." .. tostring(lit), 13)
  local board = {}

  -- `board` with the cross at `index` (1-based) flipped; edge cells have
  -- fewer neighbours.
  local function flip(cells, index)
    local column, row = (index - 1) % SIZE, (index - 1) // SIZE
    for _, d in ipairs { { 0, 0 }, { -1, 0 }, { 1, 0 }, { 0, -1 }, { 0, 1 } } do
      local x, y = column + d[1], row + d[2]
      if x >= 0 and x < SIZE and y >= 0 and y < SIZE then
        local at = y * SIZE + x + 1
        cells[at] = not cells[at]
      end
    end
  end

  local function dark(cells)
    for i = 1, COUNT do if cells[i] then return false end end
    return true
  end

  local bulbs = {}

  local function show()
    for i = 1, COUNT do
      if lit[i]:get() ~= board[i] then
        lit[i]:set(board[i])
        -- A bulb settles when it is switched.
        if bulbs[i] then common.kick(bulbs[i], "scale", 1.12, 1, 200, "out_back") end
      end
    end
  end

  local function restart()
    -- Distinct presses, since a repeated one undoes itself.
    repeat
      for i = 1, COUNT do board[i] = false end
      local presses = 6 + common.random(5)
      local pressed, n = {}, 0
      while n < presses do
        local at = common.random(COUNT) + 1
        if not pressed[at] then pressed[at] = true n = n + 1 end
      end
      for at in pairs(pressed) do flip(board, at) end
    until not dark(board)
    selected:set((COUNT + 1) // 2)
    ctx.score:set(0)
    ctx.over:set(false)
    show()
  end

  local function press(index)
    if ctx.over:get() then return end
    selected:set(index)
    flip(board, index)
    ctx.score:set(ctx.score:get() + 1)
    show()
    if dark(board) then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
    end
  end

  local lights = {}
  for i = 1, COUNT do
    local on = lit[i]
    local function is_lit() return on:get() end
    -- Glow: two discs behind the light, the wider one fainter, so a lit
    -- bulb throws light on the board around it.
    -- Mixed over the ground as Qt mixes them (see `common.over`).
    local glows = {}
    local function faint() return common.over(ctx.tint(), 0.10) end
    for _, g in ipairs { { at = 2.0, under = nil }, { at = 1.0, under = faint } } do
      local w = cell + GAP * g.at
      glows[#glows + 1] = ui.Rect {
        x = (cell - w) / 2, y = (cell - w) / 2, width = w, height = w, radius = w / 2,
        color = function()
          if g.under then return common.over(ctx.tint(), 0.22, g.under()) end
          return faint()
        end,
        opacity = function() return is_lit() and 1 or 0 end,
        behavior = { opacity = theme.behave("fast") },
      }
    end
    local glass = ui.Rect {
      x = 0, y = 0, width = cell, height = cell, radius = cell / 2,
      color = function() return is_lit() and ctx.tint() or C.islandSurfaceHover end,
      border_color = function()
        return is_lit() and common.lighter(ctx.tint(), 1.4) or C.islandBorder
      end,
      border_width = 1,
      behavior = { color = theme.behave("fast"), border_color = theme.behave("fast") },
      -- Lit, the light sits high in the glass; dark, the glass is hollow.
      ui.Rect {
        x = cell * 0.24, y = cell * 0.24 - cell * 0.14,
        width = cell * 0.52, height = cell * 0.52, radius = cell * 0.26,
        color = C.indicator,
        opacity = function() return is_lit() and 0.4 or 0 end,
        behavior = { opacity = theme.behave("fast") },
      },
      ui.Rect {
        x = cell * 0.19, y = cell * 0.19,
        width = cell * 0.62, height = cell * 0.62, radius = cell * 0.31,
        color = C.island,
        opacity = function() return is_lit() and 0 or 1 end,
        behavior = { opacity = theme.behave("fast") },
      },
    }
    bulbs[i] = glass
    glows[#glows + 1] = glass
    glows[#glows + 1] = ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_clicked = function() press(i) end,
    }
    lights[i] = ui.Item {
      x = PAD + ((i - 1) % SIZE) * stride,
      y = PAD + ((i - 1) // SIZE) * stride,
      width = cell, height = cell,
      table.unpack(glows),
    }
  end

  -- The keyboard ring, after the lights so it draws over them.
  local ring_size = cell + GAP
  local ring = ui.Rect {
    x = function() return PAD + ((selected:get() - 1) % SIZE) * stride - GAP / 2 end,
    y = function() return PAD + ((selected:get() - 1) // SIZE) * stride - GAP / 2 end,
    width = ring_size, height = ring_size, radius = ring_size / 2,
    color = "#00000000",
    border_color = function() return common.alpha(ctx.tint(), 0.45) end,
    border_width = 2,
    behavior = { x = theme.behave("fast"), y = theme.behave("fast") },
  }
  lights[#lights + 1] = ring

  local ground_size = span + 2 * PAD
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    common.ground {
      x = (ctx.width - ground_size) / 2, y = (ctx.height - ground_size) / 2,
      width = ground_size, height = ground_size,
      table.unpack(lights),
    },
  }

  local function key(keysym)
    local s = selected:get() - 1
    local column, row = s % SIZE, s // SIZE
    if keysym == K.SPACE then press(s + 1) return true end
    local dx, dy = common.direction(keysym, false)
    if not dx then return false end
    -- Clamped at the edges rather than wrapping.
    column = math.max(0, math.min(SIZE - 1, column + dx))
    row = math.max(0, math.min(SIZE - 1, row + dy))
    selected:set(row * SIZE + column + 1)
    return true
  end

  restart()
  return { node = node, key = key, restart = restart }
end
