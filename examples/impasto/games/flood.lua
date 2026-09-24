-- Flood-It: fourteen by fourteen in six colours. The top-left region takes
-- each chosen colour and absorbs the neighbouring cells of that colour,
-- until the board is one colour. The score is the move count, and
-- `finished` fires only on a solved board; there is no losing.
--
-- Port of Flood.qml. Picking the corner's current colour is not a move.
-- Cells hold a colour index, so a palette switch repaints without
-- affecting play; each cell is a node built once, and a move writes the
-- cells it changed. A move spreads out of the corner: every cell waits its
-- distance from it before it turns.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local COLUMNS, ROWS = 14, 14

-- Palettes can map two tokens to the same hex (the accent is the blue in
-- three of them), so the six colours are picked from a longer token list,
-- skipping any too close to one already taken.
local CANDIDATES = {
  "accent", "green", "red", "yellow", "blue", "textMuted", "accentHover",
  "indicatorTimer", "indicatorWarn", "text",
}

local function colour_of(name)
  local c = C[name]
  if type(c) == "function" then return c() end
  return c
end

-- The accent and its hover count as one colour, cyan and blue as two.
local function apart(a, b)
  local dr, dg, db = a.r - b.r, a.g - b.g, a.b - b.b
  return dr * dr + dg * dg + db * db > 0.09
end

--- The six colours, read inside a binding so they follow the palette.
local function colours()
  local picked = {}
  for _, name in ipairs(CANDIDATES) do
    if #picked == 6 then break end
    local candidate = colour_of(name)
    local ok = true
    for _, taken in ipairs(picked) do
      if not apart(taken, candidate) then ok = false break end
    end
    if ok then picked[#picked + 1] = candidate end
  end
  return picked
end

return function(ctx)
  -- Swatches sit under the grid as one more row of cells, with padding all
  -- round.
  local pad = theme.radius_medium
  local cell = math.max(2, math.floor(math.min(
    (ctx.width - 2 * pad) / COLUMNS,
    (ctx.height - 3 * pad) / (ROWS + 1))))

  local id = tostring({})
  local grid = {}
  local signals = {}
  for i = 1, COLUMNS * ROWS do
    grid[i] = 0
    signals[i] = morf.signal("impasto.flood." .. id .. "." .. i, 1)
  end
  -- The corner's colour is the region's colour.
  local current = morf.signal("impasto.flood." .. id .. ".current", 1)

  -- A move spreads out of the corner: every cell waits its distance from
  -- it, sixteen milliseconds a step, before it turns. The original put the
  -- wait in each cell's colour behaviour; a behaviour on a gradient that
  -- is retargeted mid-flight was seen to stop short here, so the ripple is
  -- a timer that turns one diagonal per tick and each cell changes at once.
  local LAST = COLUMNS + ROWS - 2
  local reach = morf.signal("impasto.flood." .. id .. ".reach", LAST + 1)
  local diagonals = {}
  for d = 0, LAST do diagonals[d] = {} end
  for i = 1, COLUMNS * ROWS do
    local d = (i - 1) % COLUMNS + (i - 1) // COLUMNS
    diagonals[d][#diagonals[d] + 1] = i
  end
  local function turn(diagonal)
    for _, i in ipairs(diagonals[diagonal]) do
      if signals[i]:get() ~= grid[i] then signals[i]:set(grid[i]) end
    end
  end
  local function show(at_once)
    current:set(grid[1])
    if at_once then
      for d = 0, LAST do turn(d) end
      reach:set(LAST + 1)
      return
    end
    -- A move while one is still spreading finishes the old one first.
    for d = reach:get(), LAST do turn(d) end
    turn(0)
    reach:set(1)
  end

  local function restart()
    for i = 1, COLUMNS * ROWS do grid[i] = common.random(6) + 1 end
    ctx.score:set(0)
    ctx.over:set(false)
    show(true)
  end

  local function flood(colour)
    if ctx.over:get() or colour == grid[1] then return end
    local from = grid[1]
    -- Flood fill from the corner. Painting before pushing visits each cell
    -- once.
    local stack = { 0 }
    grid[1] = colour
    while #stack > 0 do
      local index = table.remove(stack)
      local x, y = index % COLUMNS, index // COLUMNS
      for _, step in ipairs { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } } do
        local nx, ny = x + step[1], y + step[2]
        if nx >= 0 and ny >= 0 and nx < COLUMNS and ny < ROWS then
          local at = ny * COLUMNS + nx
          if grid[at + 1] == from then
            grid[at + 1] = colour
            stack[#stack + 1] = at
          end
        end
      end
    end
    ctx.score:set(ctx.score:get() + 1)
    show()
    for i = 1, COLUMNS * ROWS do
      if grid[i] ~= colour then return end
    end
    ctx.over:set(true)
    ctx.finished(ctx.score:get())
  end

  local radius = math.max(1, cell * 0.18)
  local squares = common.cells(COLUMNS * ROWS, function(i)
    local column, row = (i - 1) % COLUMNS, (i - 1) // COLUMNS
    local held = signals[i]
    return ui.Rect {
      -- One pixel of background between cells.
      x = column * cell, y = row * cell,
      width = cell - 1, height = cell - 1,
      radius = radius,
      color = "#00000000",
      -- The light on the cell, so the board reads as tiles.
      gradient = function() return common.lit(colours()[held:get()], 0.13, 0.02, 0.13) end,
    }
  end, { x = 0, y = 0 })

  -- Round swatches; the corner's current colour is outlined, and pressing
  -- it does nothing.
  local swatches = {}
  for index = 1, 6 do
    local hovered = morf.signal("impasto.flood." .. id .. ".hover." .. index, false)
    swatches[index] = ui.Rect {
      width = cell, height = cell, radius = cell / 2,
      color = "#00000000",
      gradient = function() return common.lit(colours()[index], 0.18, 0.0, 0.16, 0.6) end,
      border_color = C.text,
      border_width = function() return current:get() == index and 2 or 0 end,
      scale = function() return hovered:get() and 1.12 or 1 end,
      behavior = { scale = theme.behave("fast") },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() flood(index) end,
      },
    }
  end

  local ground_w = cell * COLUMNS + 2 * pad
  local ground_h = cell * (ROWS + 1) + 3 * pad
  local row_w = 6 * cell + 5 * pad
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    -- The ripple: one diagonal a tick until the move has reached the far
    -- corner; it runs on past the solving move, so the last one lands too.
    ui.Timer {
      interval = 16, ["repeat"] = true,
      running = function() return reach:get() <= LAST end,
      on_triggered = function()
        local d = reach:get()
        if d > LAST or not ctx.on_screen() then return end
        turn(d)
        reach:set(d + 1)
      end,
    },
    common.ground {
      x = (ctx.width - ground_w) / 2, y = (ctx.height - ground_h) / 2,
      width = ground_w, height = ground_h,
      ui.Item {
        x = pad, y = pad,
        width = cell * COLUMNS - 1, height = cell * ROWS - 1,
        squares,
      },
      ui.Row {
        x = (ground_w - row_w) / 2, y = ground_h - pad - cell,
        gap = pad,
        table.unpack(swatches),
      },
    },
  }

  local function key(keysym)
    local digit = common.digit(keysym)
    if not digit or digit < 1 or digit > 6 then return false end
    flood(digit)
    return true
  end

  restart()
  return { node = node, key = key, restart = restart }
end
