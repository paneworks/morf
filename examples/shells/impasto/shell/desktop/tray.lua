-- The card shown while arranging: every module as its real face.
--
-- Port of Tray.qml and the EditTray.qml it is made of. Each module is drawn
-- at the smallest family it offers -- the face it lands with -- packed as a
-- mosaic at three quarters of its size, tallest first, then widest, then by
-- name. Drag a face onto the grid, or click it to place it on the first
-- free cell; drop a widget on the card to remove it. Notes let go against a
-- screen edge are a deck there (the newest note), and the spectrum the bars
-- along it; the edge lights while they are held there
-- (desktop/arrange/decks.lua draws the light).
--
-- Anywhere on the card that is not a face moves it, and so does the grip in
-- its top left corner; the handle in the opposite corner resizes it in
-- whole columns and rows, the faces flowing into the new width and the rest
-- scrolled by the wheel. It opens at its home (the bottom middle) every
-- time; its size lasts for the session.
--
-- The ghost -- the face at full size while dragged -- is drawn on the board,
-- so it can leave the card. The card's rectangle is published as
-- `desk.tray_box` for the widgets' drops.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local controls = require("desktop.arrange.controls")

local C = theme.color
local M = {}

M.PAD = 24          -- wide enough to take hold of the card by its edge
M.FACTOR = 0.75     -- the mosaic's scale
M.START_COLUMNS = 5
M.START_ROWS = 2

-- The card's size in packing units lasts for the session; its place does not.
local columns_signal = morf.signal("impasto.desk.tray.columns", M.START_COLUMNS)
local rows_signal = morf.signal("impasto.desk.tray.rows", -1)   -- -1: never resized
local card_x = morf.signal("impasto.desk.tray.x", -1)
local card_y = morf.signal("impasto.desk.tray.y", -1)
local scrolled = morf.signal("impasto.desk.tray.scrolled", 0)
local pulling = morf.signal("impasto.desk.tray.pulling", "")   -- the module being dragged out
local ghost_x = morf.signal("impasto.desk.tray.ghost.x", 0)
local ghost_y = morf.signal("impasto.desk.tray.ghost.y", 0)
local deck = require("services.deck")
local receiving_edge = deck.receiving
local held = morf.signal("impasto.desk.tray.held", false)       -- card moved or stretched

-- The ghost is rebuilt for each module pulled.
local ghost_model = controls.keyed("tray.ghost", function() return pulling:get() end)

local function smallest(id) return desk.families_for(id)[1] or "4x2" end

-- ---------------------------------------------------------------- packing --

local function unit() return desk.size_for("2x2") end
local function street() return theme.desktop_gutter * M.FACTOR end
local function stride_x() return (unit().width + theme.desktop_gutter) * M.FACTOR end
local function stride_y() return (unit().height + theme.desktop_gutter) * M.FACTOR end
local function span(count, stride) return math.max(0, count * stride - street()) end

-- Tallest first, then widest, then by name; a 2x2 face is one unit.
local entries = {}
for _, entry in ipairs(desk.catalogue) do
  local shape = desk.family(smallest(entry.id))
  entries[#entries + 1] = { id = entry.id, name = entry.name, cols = shape.cols / 2, rows = shape.rows / 2 }
end
table.sort(entries, function(a, b)
  if a.rows ~= b.rows then return a.rows > b.rows end
  if a.cols ~= b.cols then return a.cols > b.cols end
  return a.name < b.name
end)
local widest, tallest = 1, 1
for _, e in ipairs(entries) do widest = math.max(widest, e.cols) tallest = math.max(tallest, e.rows) end

local pack_cache = {}
-- Each piece at the first free place from the top.
local function pack(columns)
  if pack_cache[columns] then return pack_cache[columns] end
  local taken, out, rows = {}, {}, 0
  for _, e in ipairs(entries) do
    local cols = math.min(e.cols, columns)
    local spot
    local row = 0
    while not spot do
      for col = 0, columns - cols do
        local free = true
        for r = row, row + e.rows - 1 do
          for c = col, col + cols - 1 do
            if taken[r] and taken[r][c] then free = false end
          end
        end
        if free then spot = { col = col, row = row } break end
      end
      row = row + 1
    end
    for r = spot.row, spot.row + e.rows - 1 do
      taken[r] = taken[r] or {}
      for c = spot.col, spot.col + cols - 1 do taken[r][c] = true end
    end
    out[e.id] = { col = spot.col, row = spot.row, cols = cols, rows = e.rows }
    rows = math.max(rows, spot.row + e.rows)
  end
  pack_cache[columns] = { pieces = out, rows = rows }
  return pack_cache[columns]
end

local function columns() return math.max(widest, columns_signal:get()) end
local function layout() return pack(columns()) end
-- The rows it opens with until it is resized: START_ROWS, or one on a
-- board so short that two would take more than two fifths of it (a
-- 1280x720 screen), so the card leaves most of the desk in view.
local function start_rows()
  local two = span(M.START_ROWS, stride_y()) + 2 * M.PAD
  return two <= desk.board().height * 0.4 and M.START_ROWS or 1
end

local function shown_rows()
  local l = layout()
  local asked = rows_signal:get()
  if asked < 0 then asked = start_rows() end
  return math.min(l.rows, math.max(math.min(tallest, l.rows), asked))
end

-- --------------------------------------------------------------------- card --

local function card_width() return span(columns(), stride_x()) + 2 * M.PAD end
local function card_height()
  local board = desk.board()
  return math.min(span(shown_rows(), stride_y()) + 2 * M.PAD, math.max(2 * M.PAD, board.height - 2 * theme.desktop_gutter))
end
local function clamp_x(x)
  local board = desk.board()
  return math.max(theme.desktop_gutter, math.min(board.width - card_width() - theme.desktop_gutter, x))
end
local function clamp_y(y)
  local board = desk.board()
  return math.max(theme.desktop_gutter, math.min(board.height - card_height() - theme.desktop_gutter, y))
end
local function card_left()
  local x = card_x:get()
  if x < 0 then x = (desk.board().width - card_width()) / 2 end
  return clamp_x(x)
end
local function card_top()
  local y = card_y:get()
  if y < 0 then y = desk.board().height - card_height() - theme.desktop_gutter end
  return clamp_y(y)
end
local function viewport_height() return card_height() - 2 * M.PAD end
local function overflow() return math.max(0, span(layout().rows, stride_y()) - viewport_height()) end

--- The card's rectangle on the board, for the widgets' drops.
local function publish()
  desk.tray_box = { x = card_left(), y = card_top(), width = card_width(), height = card_height() }
end

local rail
local function scroll_by(pixels)
  scrolled:set(math.max(0, math.min(overflow(), scrolled:get() + pixels)))
end

-- The bar in the right margin while the mosaic is taller than the card: the
-- thumb is the part in view. Dragged, it scrolls; pressed on the track, the
-- thumb's middle goes there first. It widens under the pointer and turns to
-- the accent while held (EditTray.qml's rail).
local rail_hover = morf.signal("impasto.desk.tray.rail.hover", false)
local scrubbing = morf.signal("impasto.desk.tray.rail.scrub", false)
local function thumb_length()
  local total = span(layout().rows, stride_y())
  return math.max(M.PAD, viewport_height() * viewport_height() / math.max(1, total))
end
local function rail_reach() return viewport_height() - thumb_length() end
local function thumb_top()
  return rail_reach() * math.min(scrolled:get(), overflow()) / math.max(1, overflow())
end
local function scroll_to(top)
  local reach = rail_reach()
  scrolled:set(reach > 0 and math.max(0, math.min(1, top / reach)) * overflow() or 0)
end

rail = function()
  local scrub_from, press_y = 0, 0
  return ui.Item {
    x = function() return card_width() - M.PAD + (M.PAD - 12) / 2 end,
    y = M.PAD, width = 12, height = viewport_height,
    visible = function() return overflow() > 0 end,
    ui.Rect {
      x = function() return (12 - ((rail_hover:get() or scrubbing:get()) and 6 or 4)) / 2 end,
      y = thumb_top, height = thumb_length,
      width = function() return (rail_hover:get() or scrubbing:get()) and 6 or 4 end,
      radius = function() return (rail_hover:get() or scrubbing:get()) and 3 or 2 end,
      color = function()
        if scrubbing:get() then return C.accent() end
        return C.scrimText:alpha(rail_hover:get() and 0.45 or 0.25)
      end,
      behavior = { width = theme.behave("fast"), x = theme.behave("fast") },
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() rail_hover:set(true) end,
      on_exited = function() rail_hover:set(false) end,
      on_pressed = function(_, _, _, y)
        press_y = y or 0
        local top = thumb_top()
        -- On the thumb it is taken where it is; anywhere else its middle
        -- goes to the pointer.
        scrub_from = (press_y >= top and press_y <= top + thumb_length()) and top or (press_y - thumb_length() / 2)
        scrubbing:set(true)
        scroll_to(scrub_from)
      end,
      on_dragged = function(_, _, _, dy) scroll_to(scrub_from + dy) end,
      on_released = function() scrubbing:set(false) end,
      on_wheel = function(_, _, _, py, _, steps) scroll_by(steps ~= 0 and steps * 40 or -py) end,
    },
  }
end

-- ------------------------------------------------------------------- pulling --

local function pointer_on_board(sx, sy)
  local board = desk.board()
  return sx - board.x, sy - board.y
end

-- Where the ghost is headed: an edge for the spectrum, else a cell.
local function aim(px, py)
  local id = pulling:get()
  local family = smallest(id)
  local size = desk.size_for(family)
  ghost_x:set(px - size.width / 2)
  ghost_y:set(py - size.height / 2)
  if desk.over_tray(px, py) then
    desk.set_landing(nil)
    receiving_edge:set("")
    return
  end
  -- A notes piece against an edge is a deck there, and a spectrum the bars
  -- along it if it has none yet; anywhere else, a square.
  if id == "notes" or id == "spectrum" then
    local board = desk.board()
    local edge = deck.edge_at(px, py, board.width, board.height)
    if edge ~= "" and (id == "notes" or desk.spectrum_takes(edge)) then
      desk.set_landing(nil)
      receiving_edge:set(edge)
      return
    end
  end
  receiving_edge:set("")
  local spot = desk.nearest_free(desk.cell_x(ghost_x:get()), desk.cell_y(ghost_y:get()), family, "")
  desk.set_landing(spot, family)
end

local function let_go()
  local id = pulling:get()
  local family = desk.landing_family:get()
  local col, row = desk.landing_col:get(), desk.landing_row:get()
  local edge = receiving_edge:get()
  pulling:set("")
  receiving_edge:set("")
  desk.set_landing(nil)
  if edge ~= "" and id == "notes" then
    desk.add_deck(edge)
  elseif edge ~= "" and id == "spectrum" then
    desk.add_spectrum(edge)
  elseif family ~= "" then
    desk.add(id, col, row)
  end
end

-- ---------------------------------------------------------------------- tile --

local function tile(entry)
  local family = smallest(entry.id)
  local box = desk.size_for(family)
  local press_x, press_y, moved = 0, 0, false
  local function piece() return layout().pieces[entry.id] end
  local shape = desk.family(family)
  local width = span(shape.cols / 2, stride_x())
  local height = span(shape.rows / 2, stride_y())
  local factor = width / box.width
  local face = require("desktop.face").build {
    id = entry.id, key = "", family = family, theme = desk.theme_of(nil),
    ink = desk.ink_for(nil), row = nil, width = box.width, height = box.height,
  }
  local pulled = function() return pulling:get() == entry.id end
  return ui.Item {
    x = function() return piece().col * stride_x() end,
    y = function() return piece().row * stride_y() end,
    width = width, height = height,
    ui.Item {
      width = box.width, height = box.height, scale = factor,
      transform_origin_x = 0, transform_origin_y = 0,
      ui.Rect {
        anchors = { fill = true }, radius = theme.desktop_radius, color = C.island,
        border_color = function() return pulled() and C.accent() or C.islandBorder end,
        border_width = function() return (pulled() and 2 or 1) / factor end,
      },
      face,
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = function() return pulled() and "grabbing" or "grab" end,
      on_pressed = function(sx, sy) press_x, press_y, moved = sx, sy, false end,
      on_dragged = function(_, _, dx, dy)
        if not moved and math.abs(dx) + math.abs(dy) < 4 then return end
        if not moved then
          moved = true
          pulling:set(entry.id)
          desk.select("")
        end
        aim(pointer_on_board(press_x + dx, press_y + dy))
      end,
      on_released = function()
        if moved then
          moved = false
          let_go()
        end
      end,
      on_clicked = function()
        if moved then return end
        if entry.id == "spectrum" then
          if desk.add("spectrum") == "" then desk.add_spectrum("") end
        else
          desk.add(entry.id)
        end
      end,
      on_wheel = function(_, _, _, py, _, steps)
        scroll_by(steps ~= 0 and steps * 40 or -py)
      end,
    },
  }
end

-- --------------------------------------------------------------------- build --

-- Where the card opens: its home at the bottom middle, as the original's,
-- unless that covers widgets and another corner or edge of the board covers
-- fewer. Worked out once as arranging starts, so the card never moves by
-- itself under a widget being dragged.
local function covered(boxes, x, y, w, h)
  local total = 0
  for _, b in ipairs(boxes) do
    local ox = math.min(x + w, b.x + b.width) - math.max(x, b.x)
    local oy = math.min(y + h, b.y + b.height) - math.max(y, b.y)
    if ox > 0 and oy > 0 then total = total + ox * oy end
  end
  return total
end

local function home()
  local board = desk.board()
  local w, h, g = card_width(), card_height(), theme.desktop_gutter
  local left, middle, right = g, (board.width - w) / 2, board.width - w - g
  local top, bottom = g, board.height - h - g
  local boxes = {}
  for _, row in ipairs(desk.squares()) do
    if not desk.left_off(row.key) then boxes[#boxes + 1] = desk.geometry(row.key) end
  end
  local best, best_x, best_y = math.huge, -1, -1
  for _, spot in ipairs {
    { middle, bottom }, { middle, top }, { left, bottom }, { right, bottom },
    { left, top }, { right, top }, { middle, (board.height - h) / 2 },
  } do
    local area = covered(boxes, clamp_x(spot[1]), clamp_y(spot[2]), w, h)
    if area < best then best, best_x, best_y = area, spot[1], spot[2] end
    if area == 0 then break end
  end
  return best_x, best_y
end
M.home = home

--- The card and the ghost, filling the board.
function M.build()
  card_x:set(-1)
  card_y:set(-1)
  do
    local x, y = home()
    card_x:set(clamp_x(x))
    card_y:set(clamp_y(y))
  end
  scrolled:set(0)
  pulling:set("")
  publish()

  local tiles = {}
  for _, entry in ipairs(entries) do tiles[#tiles + 1] = tile(entry) end

  local move_from_x, move_from_y = 0, 0
  local function begin_move()
    move_from_x, move_from_y = card_left(), card_top()
    held:set(true)
  end
  local function move(dx, dy)
    card_x:set(clamp_x(move_from_x + dx))
    card_y:set(clamp_y(move_from_y + dy))
    publish()
  end
  local function end_move()
    held:set(false)
    publish()
  end

  local receiving = function()
    return desk.dragging:get() ~= "" and desk.landing_family:get() == ""
  end

  local stretch_from_x, stretch_from_y = 0, 0
  local stretching = controls.signal("tray.stretching", false)
  local grip_on = controls.signal("tray.grip", false)

  local card = ui.Rect {
    id = "desk-tray",
    x = card_left, y = card_top, width = card_width, height = card_height,
    radius = theme.radius_large, color = C.island,
    border_color = function() return receiving() and C.accent() or C.islandBorder end,
    border_width = function() return receiving() and 2 or 1 end,
    behavior = { border_color = theme.behave("fast") },
    -- Anywhere but a face moves the card.
    ui.MouseArea {
      anchors = { fill = true }, accepted_buttons = { "left", "right" },
      cursor = function() return held:get() and "grabbing" or "move" end,
      on_pressed = begin_move,
      on_dragged = function(_, _, dx, dy) move(dx, dy) end,
      on_released = end_move,
      on_wheel = function(_, _, _, py, _, steps) scroll_by(steps ~= 0 and steps * 40 or -py) end,
    },
    ui.Item {
      x = M.PAD, y = M.PAD, width = function() return card_width() - 2 * M.PAD end,
      height = viewport_height, clip = true,
      ui.Item {
        y = function() return -math.min(scrolled:get(), overflow()) end,
        width = function() return span(columns(), stride_x()) end,
        height = function() return span(layout().rows, stride_y()) end,
        table.unpack(tiles),
      },
      -- A fade at an edge says there is more that way.
      ui.Rect {
        anchors = { left = true, right = true, top = true }, height = M.PAD,
        visible = function() return math.min(scrolled:get(), overflow()) > 0 end,
        gradient = function()
          return { angle = 180, stops = { C.island, C.island:alpha(0) } }
        end,
      },
      ui.Rect {
        anchors = { left = true, right = true, bottom = true }, height = M.PAD,
        visible = function() return math.min(scrolled:get(), overflow()) < overflow() end,
        gradient = function()
          return { angle = 180, stops = { C.island:alpha(0), C.island } }
        end,
      },
    },
    rail(),
    -- The grip: moves the card.
    ui.Rect {
      x = -8, y = -8, width = 24, height = 24, radius = 12, color = C.island,
      border_color = function() return grip_on:get() and C.accent() or C.islandBorder end,
      border_width = function() return grip_on:get() and 2 or 1 end,
      kit.glyph { anchors = { fill = true }, vertical_alignment = "center", glyph = "󰆾", size = 13, color = C.scrimText },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "move",
        on_pressed = function() grip_on:set(true) begin_move() end,
        on_dragged = function(_, _, dx, dy) move(dx, dy) end,
        on_released = function() grip_on:set(false) end_move() end,
      },
    },
    -- The handle: resizes the card in whole columns and rows.
    ui.Rect {
      x = function() return card_width() - 16 end, y = function() return card_height() - 16 end,
      width = 24, height = 24, radius = 12, color = C.island,
      border_color = function() return stretching:get() and C.accent() or C.islandBorder end,
      border_width = function() return stretching:get() and 2 or 1 end,
      ui.Rect { x = 7, y = 15, width = 10, height = 2, radius = 1, color = C.scrimText },
      ui.Rect { x = 15, y = 7, width = 2, height = 10, radius = 1, color = C.scrimText },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "nwse_resize",
        on_pressed = function()
          stretch_from_x, stretch_from_y = card_width(), card_height()
          stretching:set(true)
          held:set(true)
        end,
        on_dragged = function(_, _, dx, dy)
          local board = desk.board()
          local w = stretch_from_x + dx - 2 * M.PAD + street()
          local h = stretch_from_y + dy - 2 * M.PAD + street()
          local cols = math.max(widest, math.floor(w / stride_x() + 0.5))
          local max_cols = math.max(widest, math.floor((board.width - 2 * theme.desktop_gutter - 2 * M.PAD + street()) / stride_x()))
          columns_signal:set(math.min(cols, max_cols))
          rows_signal:set(math.max(1, math.floor(h / stride_y() + 0.5)))
          publish()
        end,
        on_released = function()
          stretching:set(false)
          held:set(false)
          publish()
        end,
      },
    },
  }

  local ghost = ui.Repeater {
    model = ghost_model,
    delegate = function(r)
      local family = smallest(r.id)
      local size = desk.size_for(family)
      return ui.Item {
        x = function() return ghost_x:get() end, y = function() return ghost_y:get() end,
        width = size.width, height = size.height, opacity = 0.9,
        ui.Rect { anchors = { fill = true }, radius = theme.desktop_radius, color = C.island,
          border_color = C.accent, border_width = 2 },
        require("desktop.face").build {
          id = r.id, key = "", family = family, theme = desk.theme_of(nil),
          ink = desk.ink_for(nil), row = nil, width = size.width, height = size.height,
        },
      }
    end,
  }

  return ui.Item {
    anchors = { fill = true },
    card,
    ui.Item { z = 10, anchors = { fill = true }, ghost },
  }
end

return M
