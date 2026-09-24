-- One spectrum row along the whole of its screen edge.
--
-- Port of EdgeSpectrum.qml. The bottom runs corner to corner, a side from
-- the top of the screen down to the bottom's bars (`desk.spectrum_box`). It
-- is part of the desk, so it is under the windows: an empty workspace shows
-- all of it, and beside a window only its foot shows, in the margin the
-- window leaves. Gone, and deaf, while it is away (a fullscreen window, or
-- `spectrumOnEmpty` on a busy workspace).
--
-- At rest it takes no input. While arranging the whole strip is its handle:
-- a click opens its inspector; dragging carries a card of it, and letting
-- go on another free edge moves it there, on the grid makes it a widget
-- (at the cell the landing mark shows), and on the card of modules takes it
-- off. `build(key, arranging)` returns a node in screen coordinates.

local ui = require("morf.ui")
local theme = require("theme")
local desk = require("services.desktop")
local bars = require("desktop.spectrum_bars")

local C = theme.color
local M = {}

local count = 0

-- The edge under a point on the screen, "" for none: DeckService's reach
-- from the board's left, right and bottom.
local function edge_at(x, y)
  local board = desk.board()
  return require("services.deck").edge_at(x - board.x, y - board.y, board.width, board.height)
end

function M.build(key, arranging)
  count = count + 1
  local id = "impasto.desk.edge." .. count
  local function row() return desk.entry_of(key) end
  local function box()
    local r = row()
    if not r or not desk.is_spectrum(r) then return { x = 0, y = 0, width = 1, height = 1 } end
    return desk.spectrum_box(r)
  end
  local function looks() return desk.spectrum_of(row()) end
  local function listening() return arranging or not desk.spectrum_away() end

  -- The bars are built for the box the row has now; the strip rebuilds them
  -- when the edge or the reach changes (a new box needs a new shader size).
  local shape = morf.list_model({})
  local function shape_id()
    local b = box()
    local r = row()
    return (r and r.edge or "") .. ":" .. b.width .. "x" .. b.height
  end
  local current = shape_id()
  shape:replace({ { id = current } }, "id")

  local held = morf.signal(id .. ".held", false)
  local children = {
    ui.Repeater {
      model = shape,
      delegate = function()
        local b = box()
        local r = row()
        return bars.build {
          x = 0, y = 0, width = b.width, height = b.height,
          edge = r and r.edge or "bottom", looks = looks, listening = listening,
        }
      end,
    },
    -- Keeps the bars' shape current: a changed box swaps the one row.
    ui.Item {
      width = 0, height = 0, visible = false,
      implicit_width = function()
        local want = shape_id()
        if want ~= current then
          current = want
          morf.timer(1, function() shape:replace({ { id = current } }, "id") end, false)
        end
        return 0
      end,
    },
  }

  if arranging then
    local selected = function() return desk.selected:get() == key end
    -- While arranging, the strip's ground, so it can be found in silence.
    table.insert(children, 1, ui.Rect {
      anchors = { fill = true }, radius = theme.radius_small,
      color = function() return C.accent():alpha(selected() and 0.16 or 0.08) end,
      border_color = function() return selected() and C.accent() or C.hairline end,
      border_width = function() return selected() and 2 or 1 end,
      opacity = function() return held:get() and 0.4 or 1 end,
    })

    -- The card in the hand, as the tray draws it: a 4x2 capsule following
    -- the pointer. Shown only while carried.
    local card_size = desk.size_for("4x2")
    local hand_x = morf.signal(id .. ".hand.x", 0)
    local hand_y = morf.signal(id .. ".hand.y", 0)
    local press_x, press_y, moved = 0, 0, false
    local function strip_origin() local b = box() return b.x, b.y end

    local function aim(px, py)
      -- The card's top left, centred on the pointer, in screen coordinates.
      hand_x:set(px - card_size.width / 2)
      hand_y:set(py - card_size.height / 2)
      local board = desk.board()
      local D = require("services.deck")
      if desk.over_tray(px - board.x, py - board.y) then
        D.receiving:set("")
        desk.set_landing(nil)
        return
      end
      -- Another free edge lights while the strip is held against it.
      local edge = edge_at(px, py)
      if edge ~= "" then
        D.receiving:set(desk.spectrum_takes(edge) and edge or "")
        desk.set_landing(nil)
        return
      end
      D.receiving:set("")
      local family = desk.families_for("spectrum")[1] or "4x2"
      local spot = desk.nearest_free(
        desk.cell_x(px - card_size.width / 2 - board.x),
        desk.cell_y(py - card_size.height / 2 - board.y), family, "")
      desk.set_landing(spot, family)
    end

    local function drop(px, py)
      held:set(false)
      desk.dragging:set("")
      require("services.deck").receiving:set("")
      local family = desk.landing_family:get()
      local col, row_ = desk.landing_col:get(), desk.landing_row:get()
      desk.set_landing(nil)
      local board = desk.board()
      if desk.over_tray(px - board.x, py - board.y) then
        desk.remove(key)
        return
      end
      local edge = edge_at(px, py)
      if edge ~= "" then
        if desk.spectrum_takes(edge) then desk.update(key, { edge = edge }) end
        return
      end
      if family ~= "" then desk.spectrum_to_grid(key, col, row_) end
    end

    children[#children + 1] = ui.MouseArea {
      anchors = { fill = true },
      cursor = function() return held:get() and "grabbing" or "grab" end,
      on_pressed = function(sx, sy)
        press_x, press_y, moved = sx, sy, false
      end,
      on_dragged = function(_, _, dx, dy)
        if not moved and math.abs(dx) + math.abs(dy) < 4 then return end
        if not moved then
          moved = true
          held:set(true)
          desk.dragging:set(key)
          desk.select("")
        end
        aim(press_x + dx, press_y + dy)
      end,
      on_released = function(sx, sy)
        if moved then
          moved = false
          drop(sx, sy)
        end
      end,
      on_clicked = function()
        if moved then return end
        desk.select(selected() and "" or key)
      end,
    }

    -- The card, placed in screen coordinates by undoing the strip's own.
    children[#children + 1] = ui.Rect {
      z = 10,
      visible = function() return held:get() end,
      x = function() local ox = strip_origin() return hand_x:get() - ox end,
      y = function() local _, oy = strip_origin() return hand_y:get() - oy end,
      width = card_size.width, height = card_size.height,
      radius = theme.desktop_radius, opacity = 0.9,
      color = C.island, border_color = C.accent, border_width = 2,
      bars.build {
        x = theme.desktop_gutter, y = theme.desktop_gutter,
        width = card_size.width - 2 * theme.desktop_gutter,
        height = card_size.height - 2 * theme.desktop_gutter,
        edge = "bottom", looks = looks, sample = true,
      },
    }
  end

  local out = {
    x = function() return box().x end,
    y = function() return box().y end,
    width = function() return box().width end,
    height = function() return box().height end,
    visible = function() return row() ~= nil and listening() end,
  }
  for _, child in ipairs(children) do out[#out + 1] = child end
  return ui.Item(out)
end

return M
