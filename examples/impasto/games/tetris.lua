-- Tetris: a 10 x 20 well and seven-bag randomisation. Left and right move,
-- up rotates clockwise with a wall kick, down soft-drops, space hard-drops.
-- Gravity speeds up each level of ten lines.
--
-- Standard scoring: one point per soft-dropped row, two per hard-dropped
-- row, and 100, 300, 500 or 800 for one to four lines times the level. The
-- round ends when a piece can't spawn.
--
-- Port of Tetris.qml. The original painted the well on a Canvas; here each
-- of the two hundred cells is a block built once, holding what it shows (a
-- piece kind, a ghost, or nothing) in a signal, and a move writes only the
-- cells whose picture changed.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local COLUMNS, ROWS = 10, 20
-- Side column width, the gap to it, and the vertical margin. A cell is
-- whatever fits all three; the preview draws at three quarters.
local PANEL, GAP, AIR = 120, 20, 40

-- Indexed from one: I, O, T, S, Z, J and L, each in its rotation box.
local SHAPES = {
  { { 0, 0, 0, 0 }, { 1, 1, 1, 1 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } },
  { { 1, 1 }, { 1, 1 } },
  { { 0, 1, 0 }, { 1, 1, 1 }, { 0, 0, 0 } },
  { { 0, 1, 1 }, { 1, 1, 0 }, { 0, 0, 0 } },
  { { 1, 1, 0 }, { 0, 1, 1 }, { 0, 0, 0 } },
  { { 1, 0, 0 }, { 1, 1, 1 }, { 0, 0, 0 } },
  { { 0, 0, 1 }, { 1, 1, 1 }, { 0, 0, 0 } },
}

return function(ctx)
  local cell = math.max(1, math.floor(math.min(
    (ctx.width - PANEL - GAP) / COLUMNS, (ctx.height - 2 * AIR) / ROWS)))
  local mini = math.max(1, math.floor(cell * 0.75))
  local id = tostring({})

  -- One colour per piece, in the same order: the T uses the frame's tint,
  -- the rest palette hues, so a palette switch repaints the stack.
  local function paint(kind)
    if kind == 1 then return C.blue() end
    if kind == 2 then return C.yellow() end
    if kind == 3 then return ctx.tint() end
    if kind == 4 then return C.green() end
    if kind == 5 then return C.red() end
    if kind == 6 then return C.accentHover() end
    return C.textMuted()
  end

  -- One block: inset and rounded so pieces read as pieces, lit from above
  -- with the light on the top edge and the shade under it -- a slab rather
  -- than a square of colour. `shown` holds a kind, minus a kind for a
  -- ghost, or 0.
  local function block(x, y, size, shown, at_x, at_y)
    local inset = math.max(1, math.floor(size * 0.1 + 0.5))
    local side = size - 2 * inset
    local corner = size * 0.2
    local function kind() return math.abs(shown:get()) end
    local function faint() return shown:get() < 0 end
    return ui.Rect {
      x = at_x and function() return at_x:get() + inset end or x + inset,
      y = at_y and function() return at_y:get() + inset end or y + inset,
      width = side, height = side,
      radius = corner,
      visible = function() return shown:get() ~= 0 end,
      color = "#00000000",
      gradient = function()
        local k = kind()
        if k == 0 then return {} end
        local p = paint(k)
        -- The ghost is faint over the empty well, mixed as Qt mixes it.
        local function at(c) return faint() and common.over(c, 0.22) or c end
        return { angle = 180, stops = {
          at(common.lighter(p, 1.25)), at(p), at(common.darker(p, 1.3)),
        } }
      end,
      border_width = function() return faint() and 0 or math.max(1, size * 0.04) end,
      border_color = function()
        local k = kind()
        if k == 0 then return "#00000000" end
        return common.alpha(common.darker(paint(k), 1.5), 0.8)
      end,
      ui.Rect {
        x = side * 0.16, y = side * 0.12, width = side * 0.5, height = side * 0.14,
        radius = side * 0.07,
        visible = function() return not faint() end,
        color = common.alpha(C.indicator, 0.22),
      },
    }
  end

  -- ------------------------------------------------------------- state --

  local board = {}          -- 200 cells, row-major from the top: kind or 0
  local piece = nil         -- { kind, cells, x, y }
  local next_kind = morf.signal("impasto.tetris." .. id .. ".next", 0)
  local lines = morf.signal("impasto.tetris." .. id .. ".lines", 0)
  local bag = {}

  local function level() return 1 + lines:get() // 10 end

  -- Seven-bag: all seven shuffled and dealt until empty, so a shape never
  -- goes missing for more than twelve pieces.
  local function deal()
    if #bag == 0 then
      bag = { 1, 2, 3, 4, 5, 6, 7 }
      for i = #bag, 2, -1 do
        local other = common.random(i) + 1
        bag[i], bag[other] = bag[other], bag[i]
      end
    end
    return table.remove(bag, 1)
  end

  -- A clockwise quarter turn within the piece's box.
  local function rotated(cells)
    local size = #cells
    local out = {}
    for row = 1, size do
      out[row] = {}
      for column = 1, size do out[row][column] = cells[size - column + 1][row] end
    end
    return out
  end

  -- Whether a piece fits there. Rows above the well are allowed (pieces
  -- spawn there); everything else outside is a wall.
  local function fits(cells, x, y)
    for row = 1, #cells do
      for column = 1, #cells do
        if cells[row][column] == 1 then
          local ax, ay = x + column - 1, y + row - 1
          if ax < 0 or ax >= COLUMNS or ay >= ROWS then return false end
          if ay >= 0 and board[ay * COLUMNS + ax + 1] ~= 0 then return false end
        end
      end
    end
    return true
  end

  -- Resting row, for the ghost and the hard drop.
  local function landing()
    local y = piece.y
    while fits(piece.cells, piece.x, y + 1) do y = y + 1 end
    return y
  end

  -- ------------------------------------------------------------ picture --

  local shown, preview = {}, {}
  for i = 1, COLUMNS * ROWS do shown[i] = morf.signal("impasto.tetris." .. id .. ".c" .. i, 0) end
  for i = 1, 16 do preview[i] = morf.signal("impasto.tetris." .. id .. ".p" .. i, 0) end
  local picture = {}

  -- The stack, then the ghost, then the piece, clipped to the well's rows.
  local function draw()
    for i = 1, COLUMNS * ROWS do picture[i] = board[i] end
    if piece then
      local rest = landing()
      local function stamp(y, value)
        for row = 1, #piece.cells do
          for column = 1, #piece.cells do
            if piece.cells[row][column] == 1 then
              local ax, ay = piece.x + column - 1, y + row - 1
              if ay >= 0 and ay < ROWS then picture[ay * COLUMNS + ax + 1] = value end
            end
          end
        end
      end
      if rest > piece.y then stamp(rest, -piece.kind) end
      stamp(piece.y, piece.kind)
    end
    for i = 1, COLUMNS * ROWS do
      if shown[i]:get() ~= picture[i] then shown[i]:set(picture[i]) end
    end
  end

  -- Centred on its occupied cells rather than its box, so I and O sit as
  -- squarely as T. Where each preview block sits is a signal, since the
  -- blocks are built a moment after the board.
  local px, py = {}, {}
  for i = 1, 16 do
    px[i] = morf.signal("impasto.tetris." .. id .. ".px" .. i, 0)
    py[i] = morf.signal("impasto.tetris." .. id .. ".py" .. i, 0)
  end
  local function draw_preview()
    local kind = next_kind:get()
    local cells = SHAPES[kind]
    for i = 1, 16 do preview[i]:set(0) end
    if not cells then return end
    local left, top, right, bottom = 99, 99, -1, -1
    for row = 1, #cells do
      for column = 1, #cells do
        if cells[row][column] == 1 then
          left, right = math.min(left, column), math.max(right, column)
          top, bottom = math.min(top, row), math.max(bottom, row)
        end
      end
    end
    local ox = math.floor((PANEL - (right - left + 1) * mini) / 2 + 0.5)
    local oy = math.floor((PANEL - (bottom - top + 1) * mini) / 2 + 0.5)
    local n = 0
    for row = 1, #cells do
      for column = 1, #cells do
        if cells[row][column] == 1 then
          n = n + 1
          px[n]:set(ox + (column - left) * mini)
          py[n]:set(oy + (row - top) * mini)
          preview[n]:set(kind)
        end
      end
    end
  end

  -- ------------------------------------------------------------- rules --

  local flash, pop, play_pop

  local lock

  -- Spawns the next piece centred at the top and deals the one after. No
  -- room ends the round.
  local function spawn()
    local kind = next_kind:get()
    next_kind:set(deal())
    draw_preview()
    local cells = SHAPES[kind]
    local top = 0
    for row = 1, #cells do
      local filled = false
      for column = 1, #cells do if cells[row][column] == 1 then filled = true end end
      if filled then top = row - 1 break end
    end
    local x = (COLUMNS - #cells) // 2
    piece = { kind = kind, cells = cells, x = x, y = -top }
    if not fits(cells, x, -top) then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
    end
  end

  -- Moves down one row if possible, otherwise locks. Returns which, since a
  -- soft drop only scores rows actually fallen.
  local function fall()
    if not fits(piece.cells, piece.x, piece.y + 1) then
      lock()
      return false
    end
    piece.y = piece.y + 1
    return true
  end

  -- Locks the piece, clears full rows and settles the rest. A piece
  -- locking partly above the well ends the round.
  lock = function()
    local above = false
    for row = 1, #piece.cells do
      for column = 1, #piece.cells do
        if piece.cells[row][column] == 1 then
          local ax, ay = piece.x + column - 1, piece.y + row - 1
          if ay >= 0 then board[ay * COLUMNS + ax + 1] = piece.kind else above = true end
        end
      end
    end
    local kept = {}
    for y = 0, ROWS - 1 do
      local full = true
      for x = 1, COLUMNS do
        if board[y * COLUMNS + x] == 0 then full = false break end
      end
      if not full then
        local line = {}
        for x = 1, COLUMNS do line[x] = board[y * COLUMNS + x] end
        kept[#kept + 1] = line
      end
    end
    local cleared = ROWS - #kept
    for y = 0, ROWS - 1 do
      local line = kept[y - cleared + 1]
      for x = 1, COLUMNS do board[y * COLUMNS + x] = line and line[x] or 0 end
    end
    if cleared > 0 then
      local paid = ({ 100, 300, 500, 800 })[cleared] * level()
      ctx.score:set(ctx.score:get() + paid)
      lines:set(lines:get() + cleared)
      -- A sweep flashes the well and says what it paid, over the stack.
      common.kick(flash, "opacity", 0.35, 0, 260, "out_cubic")
      play_pop("+" .. paid)
    end
    if above then
      piece = nil
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
      return
    end
    spawn()
  end

  local function restart()
    for i = 1, COLUMNS * ROWS do board[i] = 0 end
    bag = {}
    lines:set(0)
    ctx.score:set(0)
    ctx.over:set(false)
    next_kind:set(deal())
    spawn()
    draw()
  end

  -- --------------------------------------------------------------- nodes --

  local well_w, well_h = cell * COLUMNS, cell * ROWS
  local well = {}
  -- The well's own rows and columns, so the empty part of it is a grid and
  -- not a hole.
  local line_colour = function() return common.over("#ffffff", 0.035) end
  for column = 1, COLUMNS - 1 do
    well[#well + 1] = ui.Rect { x = column * cell, y = 0, width = 1, height = well_h, color = line_colour }
  end
  for row = 1, ROWS - 1 do
    well[#well + 1] = ui.Rect { x = 0, y = row * cell, width = well_w, height = 1, color = line_colour }
  end
  well[#well + 1] = common.cells(COLUMNS * ROWS, function(i)
    return block(((i - 1) % COLUMNS) * cell, ((i - 1) // COLUMNS) * cell, cell, shown[i])
  end, { x = 0, y = 0 })
  flash = ui.Rect {
    x = 0, y = 0, width = well_w, height = well_h, z = 1,
    radius = theme.radius_medium, color = C.indicator, opacity = 0,
  }
  well[#well + 1] = flash
  pop, play_pop = common.pop { tint = C.indicator, rise = cell * 1.6, width = well_w, x = 0, y = well_h * 0.4 }
  pop.z = 1
  well[#well + 1] = pop

  local minis = common.cells(16, function(i) return block(0, 0, mini, preview[i], px[i], py[i]) end,
    { x = 0, y = 0 })

  -- A muted mono caption over a plain number.
  local function reading(caption, value)
    return ui.Column {
      gap = 2,
      ui.Text {
        text = caption, color = C.textMuted,
        font_family = function() return theme.font_mono() end, font_size = theme.size.label,
      },
      ui.Text {
        text = value, color = C.text,
        font_family = function() return theme.font_mono() end, font_size = theme.size.large,
      },
    }
  end

  local total_w = well_w + GAP + PANEL
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    -- Gravity: 800 ms per row, 70 ms less each level, never under 90.
    ui.Timer {
      interval = function() return math.max(90, 800 - 70 * (level() - 1)) end,
      running = function() return not ctx.over:get() end,
      ["repeat"] = true,
      on_triggered = function()
        if not ctx.live() or not piece then return end
        fall()
        draw()
      end,
    },
    -- Well on the left, side column on the right, centred together.
    ui.Item {
      x = (ctx.width - total_w) / 2, y = (ctx.height - well_h) / 2,
      width = total_w, height = well_h,
      common.ground {
        x = 0, y = 0, width = well_w, height = well_h,
        table.unpack(well),
      },
      ui.Column {
        x = well_w + GAP, y = 0, width = PANEL, gap = GAP,
        common.ground { width = PANEL, height = PANEL, minis },
        reading("level", function() return tostring(level()) end),
        reading("lines", function() return tostring(lines:get()) end),
      },
    },
  }

  local function key(keysym)
    -- After game over nothing moves and keys aren't taken.
    if ctx.over:get() or not piece then return false end
    local l = common.letter(keysym)
    if keysym == common.K.LEFT or l == "a" then
      if fits(piece.cells, piece.x - 1, piece.y) then piece.x = piece.x - 1 end
    elseif keysym == common.K.RIGHT or l == "d" then
      if fits(piece.cells, piece.x + 1, piece.y) then piece.x = piece.x + 1 end
    elseif keysym == common.K.UP or l == "w" then
      -- Clockwise, trying in place and then one step off either wall.
      local cells = rotated(piece.cells)
      for _, kick in ipairs { 0, -1, 1 } do
        if fits(cells, piece.x + kick, piece.y) then
          piece.cells, piece.x = cells, piece.x + kick
          break
        end
      end
    elseif keysym == common.K.DOWN or l == "s" then
      if fall() then ctx.score:set(ctx.score:get() + 1) end
    elseif keysym == common.K.SPACE then
      local y = landing()
      ctx.score:set(ctx.score:get() + 2 * (y - piece.y))
      piece.y = y
      lock()
    else
      return false
    end
    draw()
    return true
  end

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function() return "level " .. level() .. " lines " .. lines:get() .. " score " .. ctx.score:get() end,
  }
end
