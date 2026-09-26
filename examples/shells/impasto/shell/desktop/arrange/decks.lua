-- The edge decks while arranging, drawn on the board.
--
-- Port of Deck.qml's arranging half. The original raised its full-screen
-- deck surface to the overlay while the desk was arranged; here the board
-- is one surface above the windows, so the decks are drawn on it, in its
-- coordinates, and the edge surfaces (deck/deck.lua) step aside meanwhile.
--
-- The tabs stay out. The grip before the first tab slides the deck along its
-- edge; a tab dragged along its edge takes another place, to another edge
-- joins the deck there, onto the grid becomes a note widget (at the cell the
-- landing mark shows) and onto the card leaves the desk. A click on a tab
-- selects the deck, for its inspector. An edge lights along its length
-- while anything that would land on it is held against it.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local D = require("services.deck")
local notes = require("services.notes")
local sticky = require("components.sticky")
local controls = require("desktop.arrange.controls")

local C = theme.color
local M = {}

local function board() return desk.board() end
local function length_of(edge)
  local b = board()
  return edge == "bottom" and b.width or b.height
end

-- Where the carried tab would land: an edge and a 0-based place along it.
local drop_edge = controls.signal("decks.drop.edge", "")
local drop_index = controls.signal("decks.drop.index", -1)
local ghost_x = controls.signal("decks.ghost.x", 0)
local ghost_y = controls.signal("decks.ghost.y", 0)

local function start_of(row)
  local n = #desk.deck_notes(row)
  return D.start_of(n, desk.along_of(row), length_of(row.edge))
end

--- The 0-based place a point along an edge falls in, among `count` tabs:
--- compared against tab centres, so a tab let go in place keeps its place.
local function index_at(position, count, start)
  local slot = math.floor((position - start - D.tab_length / 2) / (D.tab_length + D.tab_gap) + 0.5)
  return math.max(0, math.min(count, slot))
end

-- A tab's box at full depth, `slot` 0-based.
local function tab_box(edge, slot, start)
  local b = board()
  local along = start + slot * (D.tab_length + D.tab_gap)
  if edge == "bottom" then
    return { x = along, y = b.height - D.tab_depth, width = D.tab_length, height = D.tab_depth }
  end
  return { x = edge == "right" and b.width - D.tab_depth or 0, y = along, width = D.tab_depth, height = D.tab_length }
end

-- Just before the first tab, centred on the depth.
local function grip_box(edge, start)
  local b = board()
  local before = start - D.grip / 2 - 4
  if edge == "bottom" then
    return { x = before, y = b.height - D.tab_depth / 2 - D.grip / 2 }
  end
  return { x = edge == "right" and b.width - D.tab_depth / 2 - D.grip / 2 or D.tab_depth / 2 - D.grip / 2, y = before }
end

-- ------------------------------------------------------------------ carrying --

local function aim(px, py)
  ghost_x:set(px - 36)
  ghost_y:set(py - 36)
  if desk.over_tray(px, py) then
    drop_edge:set("") drop_index:set(-1)
    D.receiving:set("")
    desk.set_landing(nil)
    return
  end
  local b = board()
  local edge = D.edge_at(px, py, b.width, b.height)
  if edge ~= "" then
    local row = desk.deck_on(edge)
    local index = 0
    if row then
      index = index_at(edge == "bottom" and px or py, #desk.deck_notes(row), start_of(row))
    end
    drop_edge:set(edge)
    drop_index:set(index)
    D.receiving:set(edge)
    desk.set_landing(nil)
    return
  end
  drop_edge:set("") drop_index:set(-1)
  D.receiving:set("")
  local face = desk.size_for("2x2")
  local spot = desk.nearest_free(desk.cell_x(px - face.width / 2), desk.cell_y(py - face.height / 2), "2x2", "")
  desk.set_landing(spot, "2x2")
end

-- On an edge, it takes its place there; on the grid, the nearest free cell;
-- on the card, it leaves the desk. With no free cell it stays where it was.
local function drop(key, px, py)
  local edge, index = drop_edge:get(), drop_index:get()
  D.dragging:set("")
  D.receiving:set("")
  drop_edge:set("") drop_index:set(-1)
  desk.set_landing(nil)
  if desk.over_tray(px, py) then
    desk.remove_note(key)
    return
  end
  if edge ~= "" then
    desk.place_note(key, edge, index + 1)
    return
  end
  local face = desk.size_for("2x2")
  desk.note_to_grid(key, desk.cell_x(px - face.width / 2), desk.cell_y(py - face.height / 2))
end

-- --------------------------------------------------------------------- deck --

local function deck(r)
  local key = r.key
  local function row() return desk.entry_of(key) end
  local function edge() local d = row() return d and d.edge or "right" end
  local function keys() return desk.deck_notes(row()) end
  local function start() local d = row() return d and start_of(d) or 0 end
  local selected = function() return desk.selected:get() == key end

  -- Keyed by note, so a tab dragged along its deck keeps its node.
  local model = morf.list_model({})
  local holder = ui.Item { anchors = { fill = true }, visible = function() return #keys() > 0 end }
  morf.effect("impasto.desk.decks.tabs", function()
    local list = {}
    for _, note in ipairs(keys()) do list[#list + 1] = { key = note } end
    model:replace(list, "key")
  end, { owner = holder })

  local function tab(t)
    local note_key = t.key
    local note = function() return notes.entry(note_key) end
    local held = function() return D.dragging:get() == note_key end
    local function index()
      for i, k in ipairs(keys()) do if k == note_key then return i - 1 end end
      return 0
    end
    -- Its own place, or one along while the carried tab is let go before it
    -- on this edge.
    local function shown()
      local at = index()
      local carried = D.dragging:get()
      if carried == "" or held() or drop_edge:get() ~= edge() then return at end
      local from = -1
      for i, k in ipairs(keys()) do if k == carried then from = i - 1 end end
      if from >= 0 and from < at then at = at - 1 end
      if drop_index:get() <= at then at = at + 1 end
      return at
    end
    local function box() return tab_box(edge(), shown(), start()) end
    local press_x, press_y, moved = 0, 0, false
    local R = theme.radius_small
    return ui.Rect {
      x = function() return box().x end, y = function() return box().y end,
      width = function() return box().width end, height = function() return box().height end,
      z = function() return held() and 0 or 1 end,
      opacity = function() return held() and 0.3 or 1 end,
      top_left_radius = function() return edge() == "left" and 0 or R end,
      bottom_left_radius = function() return (edge() == "left" or edge() == "bottom") and 0 or R end,
      top_right_radius = function() return edge() == "right" and 0 or R end,
      bottom_right_radius = function() return (edge() == "right" or edge() == "bottom") and 0 or R end,
      color = function() local n = note() return notes.paper_of(n and n.tint or "yellow") end,
      border_color = C.accent,
      border_width = function() return selected() and 2 or 0 end,
      behavior = { x = theme.behave("fast"), y = theme.behave("fast") },
      kit.text {
        anchors = { center_in = true },
        width = D.tab_length - 16, elide = "right", horizontal_alignment = "center",
        rotation = function() return edge() == "right" and 90 or (edge() == "left" and -90 or 0) end,
        text = function() local n = note() return n and notes.title_of(n):upper() or "" end,
        size = theme.size.label, weight = 600, letter_spacing = 1, color = C.paperInk,
      },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return held() and "grabbing" or "grab" end,
        on_pressed = function(sx, sy) press_x, press_y, moved = sx, sy, false end,
        on_dragged = function(_, _, dx, dy)
          if not moved and math.abs(dx) + math.abs(dy) < 4 then return end
          local b = board()
          if not moved then
            moved = true
            D.peeked:set("")
            D.dragging:set(note_key)
            desk.select("")
          end
          aim(press_x + dx - b.x, press_y + dy - b.y)
        end,
        on_released = function(sx, sy)
          if not moved then return end
          moved = false
          local b = board()
          drop(note_key, sx - b.x, sy - b.y)
        end,
        on_clicked = function()
          if moved then return end
          desk.select(selected() and "" or key)
        end,
      },
    }
  end

  -- The grip slides the whole deck along its edge.
  local sliding = function() return D.sliding:get() == key end
  local slide_from = 0
  local grip = ui.Rect {
    x = function() return grip_box(edge(), start()).x end,
    y = function() return grip_box(edge(), start()).y end,
    width = D.grip, height = D.grip, radius = D.grip / 2, z = 2,
    color = C.island,
    border_color = function() return sliding() and C.accent() or C.islandBorder end,
    border_width = function() return sliding() and 2 or 1 end,
    kit.glyph {
      anchors = { fill = true }, vertical_alignment = "center", size = 12, color = C.scrimText,
      glyph = function() return edge() == "bottom" and "󰡏" or "󰡎" end,
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = function() return edge() == "bottom" and "ew_resize" or "ns_resize" end,
      on_pressed = function() slide_from = start() end,
      on_dragged = function(_, _, dx, dy)
        if not sliding() then
          desk.select("")
          D.sliding:set(key)
        end
        local d = row()
        if not d then return end
        local s = slide_from + (edge() == "bottom" and dx or dy)
        desk.set_deck_along(key, D.along_at(#keys(), s, length_of(edge())))
      end,
      on_released = function() if sliding() then D.sliding:set("") end end,
    },
  }

  ui.reparent(ui.Repeater { model = model, delegate = tab }, holder)
  ui.reparent(grip, holder)
  return holder
end

-- ---------------------------------------------------------------------- lit --

-- An edge lit along its length while something that would land there is
-- held against it.
local function edge_light(edge)
  local lit = function() return D.receiving:get() == edge end
  return ui.Rect {
    x = function() return edge == "right" and board().width - 3 or 0 end,
    y = function() return edge == "bottom" and board().height - 3 or 0 end,
    width = function() return edge == "bottom" and board().width or 3 end,
    height = function() return edge == "bottom" and 3 or board().height end,
    color = C.accent,
    opacity = function() return lit() and 1 or 0 end,
    behavior = { opacity = theme.behave("fast") },
  }
end

--- The decks, the edge lights and the carried tab, filling the board.
function M.build()
  D.dragging:set("")
  D.sliding:set("")
  D.receiving:set("")
  local ghost_note = function() return notes.entry(D.dragging:get()) end
  return ui.Item {
    anchors = { fill = true },
    ui.Repeater { model = desk.deck_keys, delegate = deck },
    edge_light("left"), edge_light("right"), edge_light("bottom"),
    ui.Item {
      z = 10, width = 72, height = 72,
      x = function() return ghost_x:get() end, y = function() return ghost_y:get() end,
      visible = function() return D.dragging:get() ~= "" end,
      opacity = 0.94,
      sticky.build {
        note = ghost_note, width = 72, height = 72,
        padding = 8, title_size = theme.size.label, body_size = 11, show_age = false,
        shadow_color = "#00000080", shadow_blur = 12, shadow_offset_y = 3,
      },
    },
  }
end

return M
