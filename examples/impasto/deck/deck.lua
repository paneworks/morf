-- Notes on the screen edges: tabs that peek out under the pointer.
--
-- Port of Deck.qml. At rest each note on an edge is a thin strip of its
-- paper; hovering slides the tabs out with their titles, the tab under the
-- pointer peeks the whole note beside it, and a click opens the note in the
-- island. A tab dragged along its edge takes another place in the deck;
-- pulled well off the edge and let go, it leaves the deck.
--
-- The original is one full-screen surface with a computed input mask. Here
-- each edge that has a deck gets its own small layer surface, as tall (or
-- wide) as its deck and its peek need, and the surface's input region is
-- morf's usual one -- only where there is a MouseArea, so only the tabs and
-- the peek take the pointer and the desktop under the rest gets its clicks.
-- It never takes the keyboard.
--
-- Hidden under a fullscreen window, and with `deckOnEmpty` on any workspace
-- that has windows (`services/deck.lua`).

local ui = require("morf.ui")
local theme = require("theme")
local notes = require("services.notes")
local service = require("services.deck")
local island = require("bar.island")
local kit = require("components.kit")
local sticky = require("components.sticky")

local C = theme.color
local D = service

local screen = (morf.screens or {})[1] or {}
local SCREEN_W = tonumber(screen.width) or 1920
local SCREEN_H = tonumber(screen.height) or 1080

-- The surfaces sit in the usable area, under the bar's reserved band.
local function board_width() return SCREEN_W end
local function board_height() return SCREEN_H - theme.bar_reserve() end

local SHADOW = 16
-- A side surface's depth: the tabs, the gap and the peek with its shadow.
local THICK = D.tab_depth + D.peek_gap + D.peek_width + SHADOW
local THICK_BOTTOM = D.tab_depth + D.peek_gap + D.peek_height + SHADOW

-- Pointer state, one set for every edge: one pointer, one peek.
local reveal = morf.signal("impasto.deck.reveal", 0)
local hover_count = 0
local retract_generation = 0

local function retract_later()
  retract_generation = retract_generation + 1
  local mine = retract_generation
  -- After a moment, so moving from a tab to its peek is not leaving.
  morf.timer(320, function()
    if mine == retract_generation and hover_count <= 0 and D.dragging:get() == "" then
      D.revealed:set(false)
      D.peeked:set("")
    end
  end, false)
end

local function entered()
  hover_count = hover_count + 1
  retract_generation = retract_generation + 1
  D.revealed:set(true)
end

local function exited()
  hover_count = math.max(0, hover_count - 1)
  if hover_count == 0 then retract_later() end
end

morf.effect("impasto.deck.reveal", function()
  local out = D.revealed:get() or D.peeked:get() ~= "" or D.dragging:get() ~= ""
  reveal:set(out and 1 or 0)
end)

local function open_note(key)
  D.peeked:set("")
  D.revealed:set(false)
  notes.open(key)
  island.open("notes")
end

-- ------------------------------------------------------------------ edge --

local function build_edge(edge)
  local vertical = edge ~= "bottom"
  local drop_slot = morf.signal("impasto.deck." .. edge .. ".drop", 0)
  local pulled_off = morf.signal("impasto.deck." .. edge .. ".off", false)
  local ghost_at = morf.signal("impasto.deck." .. edge .. ".ghost", 0)

  local function the_deck() return D.deck_on(edge) end
  local function count() local d = the_deck() return d and #d.notes or 0 end
  local function length() return vertical and board_height() or board_width() end
  local function start()
    local d = the_deck()
    return D.start_of(count(), d and d.along or 0, length())
  end

  -- Where the peek for the tab in slot `index` sits along the edge.
  local function peek_along(index)
    local size = vertical and D.peek_height or D.peek_width
    local centre = vertical and 0 or (D.tab_length / 2 - D.peek_width / 2)
    return math.max(D.margin, math.min(length() - D.margin - size, D.tab_at(start(), index) + centre))
  end

  -- The surface runs from the board's start to past the last thing it
  -- draws: the strip, or the lowest peek.
  local function extent()
    local n = count()
    if n == 0 then return 1 end
    local far = start() + D.strip_length(n)
    for i = 1, n do
      far = math.max(far, peek_along(i) + (vertical and D.peek_height or D.peek_width))
    end
    return math.min(length(), math.ceil(far + SHADOW))
  end
  local depth_box = vertical and THICK or THICK_BOTTOM

  local function depth()
    return D.sliver + (D.tab_depth - D.sliver) * reveal:get()
  end

  -- A slot's slot while a tab of this deck is dragged: the others step
  -- aside for where it would land.
  local function shown_slot(key, index)
    local dragging = D.dragging:get()
    if dragging == "" or dragging == key or pulled_off:get() then return index end
    local d = the_deck()
    local from = 0
    for i, other in ipairs(d and d.notes or {}) do if other == dragging then from = i end end
    if from == 0 then return index end
    local at = index
    if from < index then at = at - 1 end
    if drop_slot:get() <= at then at = at + 1 end
    return at
  end

  local function slot_of(key)
    local d = the_deck()
    for i, other in ipairs(d and d.notes or {}) do if other == key then return i end end
    return 1
  end

  -- Rounded on the screen side, square against the edge.
  local R = theme.radius_small
  local corners = {
    left = { 0, R, R, 0 }, right = { R, 0, 0, R }, bottom = { R, R, 0, 0 },
  }
  corners = corners[edge]

  local model = morf.list_model({})
  morf.effect("impasto.deck." .. edge .. ".tabs", function()
    local rows = {}
    local d = the_deck()
    for _, key in ipairs(d and d.notes or {}) do rows[#rows + 1] = { key = key } end
    model:replace(rows, "key")
  end)

  local function tab(row)
    local key = row.key
    local note = function() return notes.entry(key) end
    local along = function() return D.tab_at(start(), shown_slot(key, slot_of(key))) end
    local title = kit.text {
      anchors = { center_in = true },
      width = D.tab_length - 16, elide = "right", horizontal_alignment = "center",
      rotation = edge == "right" and 90 or (edge == "left" and -90 or 0),
      text = function() local n = note() return n and notes.title_of(n):upper() or "" end,
      size = theme.size.label, weight = 600, letter_spacing = 1,
      color = C.paperInk,
      opacity = function() return reveal:get() end,
      behavior = { opacity = theme.behave("medium") },
    }
    local position = {}
    if edge == "left" then
      position.x = 0
      position.y = along
      position.width = depth
      position.height = D.tab_length
    elseif edge == "right" then
      position.x = function() return THICK - depth() end
      position.y = along
      position.width = depth
      position.height = D.tab_length
    else
      position.x = along
      position.y = function() return THICK_BOTTOM - depth() end
      position.width = D.tab_length
      position.height = depth
    end
    return ui.Rect {
      x = position.x, y = position.y, width = position.width, height = position.height,
      top_left_radius = corners[1], top_right_radius = corners[2],
      bottom_right_radius = corners[3], bottom_left_radius = corners[4],
      color = function() local n = note() return notes.paper_of(n and n.tint or "yellow") end,
      opacity = function() return D.dragging:get() == key and 0.3 or 1 end,
      behavior = {
        x = theme.behave("medium"), y = theme.behave("medium"),
        width = theme.behave("medium"), height = theme.behave("medium"),
        color = theme.behave("fast"),
      },
      title,
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return D.dragging:get() == key and "grabbing" or "pointer" end,
        on_entered = function()
          entered()
          if D.dragging:get() == "" then D.peeked:set(key) end
        end,
        on_exited = exited,
        on_clicked = function(button)
          if button ~= "right" and D.dragging:get() == "" then open_note(key) end
        end,
        on_drag_started = function(sx, sy)
          D.peeked:set("")
          D.dragging:set(key)
          drop_slot:set(slot_of(key))
          ghost_at:set(vertical and sy or sx)
        end,
        on_dragged = function(sx, sy)
          local along_edge = vertical and sy or sx
          local across = vertical and (edge == "left" and sx or THICK - sx) or (THICK_BOTTOM - sy)
          ghost_at:set(along_edge)
          pulled_off:set(across > D.tab_depth + D.reach)
          drop_slot:set(D.index_at(along_edge, count(), start()))
        end,
        on_drag_finished = function()
          local off, slot = pulled_off:get(), drop_slot:get()
          D.dragging:set("")
          pulled_off:set(false)
          if off then service.remove_note(key) else service.place_note(key, edge, slot) end
          retract_later()
        end,
      },
    }
  end

  -- The hovered note, slid out beside its tab. Which note and where are
  -- held through the fade-out: bound straight to the hovered tab they would
  -- reset as the pointer leaves, and the paper would fade from a corner.
  local peek_key = morf.signal("impasto.deck." .. edge .. ".peek", "")
  morf.effect("impasto.deck." .. edge .. ".peek", function()
    local key = D.peeked:get()
    if key ~= "" and service.placement_of(key) == edge then peek_key:set(key) end
  end)
  local showing = function()
    local key = D.peeked:get()
    return key ~= "" and key == peek_key:get() and D.dragging:get() == ""
  end
  local peek_along_now = function()
    local key = peek_key:get()
    return key ~= "" and peek_along(slot_of(key)) or 0
  end
  local peek_pos = {}
  if edge == "left" then
    peek_pos.x = D.tab_depth + D.peek_gap
    peek_pos.y = peek_along_now
  elseif edge == "right" then
    peek_pos.x = THICK - D.tab_depth - D.peek_gap - D.peek_width
    peek_pos.y = peek_along_now
  else
    peek_pos.x = peek_along_now
    peek_pos.y = THICK_BOTTOM - D.tab_depth - D.peek_gap - D.peek_height
  end
  local peek = ui.Item {
    x = peek_pos.x, y = peek_pos.y,
    width = D.peek_width, height = D.peek_height,
    visible = function() return peek_key:get() ~= "" end,
    opacity = function() return showing() and 1 or 0 end,
    enter = { opacity = 0 },
    behavior = { opacity = theme.behave("fast") },
    sticky.build {
      note = function() return notes.entry(peek_key:get()) end,
      width = D.peek_width, height = D.peek_height,
      padding = 14, title_size = theme.size.regular, body_size = 17,
      shadow_color = "#00000080", shadow_blur = 14, shadow_offset_y = 3,
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      enabled = showing,
      on_entered = entered,
      on_exited = exited,
      on_clicked = function() open_note(peek_key:get()) end,
    },
  }

  -- The dragged tab, a small square of its paper under the pointer.
  local ghost_note = function() return notes.entry(D.dragging:get()) end
  local ghost = ui.Item {
    z = 10, width = 72, height = 72,
    visible = function() return D.dragging:get() ~= "" and service.placement_of(D.dragging:get()) == edge end,
    opacity = function() return pulled_off:get() and 0.6 or 0.94 end,
    x = function()
      if vertical then return edge == "left" and D.tab_depth + 8 or THICK - D.tab_depth - 80 end
      return ghost_at:get() - 36
    end,
    y = function()
      if vertical then return ghost_at:get() - 36 end
      return THICK_BOTTOM - D.tab_depth - 80
    end,
    sticky.build {
      note = ghost_note, width = 72, height = 72,
      padding = 8, title_size = theme.size.label, body_size = 11, show_age = false,
      shadow_color = "#00000080", shadow_blur = 12, shadow_offset_y = 3,
    },
  }

  local root = ui.Item {
    width = function() return vertical and THICK or extent() end,
    height = function() return vertical and extent() or THICK_BOTTOM end,
    ui.Repeater { model = model, delegate = tab },
    peek,
    ghost,
  }

  local anchors = edge == "left" and { left = true, top = true }
    or edge == "right" and { right = true, top = true }
    or { left = true, bottom = true }
  local window = morf.window.layer {
    namespace = "impasto-deck",
    layer = "top",
    keyboard_focus = "none",
    anchors = anchors,
    width = vertical and THICK or 1,
    height = vertical and 1 or THICK_BOTTOM,
    visible = false,
    root = root,
  }

  local open = false
  morf.effect("impasto.deck." .. edge .. ".window", function()
    local want = the_deck() ~= nil and not service.away()
    local size = extent()
    if vertical then window:size(THICK, size) else window:size(size, depth_box) end
    if want and not open then window:open() open = true
    elseif not want and open then window:close() open = false end
  end)
  return window
end

local deck = { windows = {} }
for _, edge in ipairs(D.edges) do deck.windows[edge] = build_edge(edge) end

-- Test verbs: `morf ipc call deck.peek <key>` slides a note out as a hover
-- would; `deck.reveal` brings the tabs out; `deck.rest` puts them back.
morf.ipc["deck.peek"] = function(key)
  local placed = key and service.placement_of(key) or ""
  if placed == "" then
    for _, d in ipairs(service.decks()) do key = d.notes[1] break end
  end
  if not key then return "no deck" end
  D.revealed:set(true)
  D.peeked:set(key)
  return key
end
morf.ipc["deck.reveal"] = function() D.revealed:set(true) return "ok" end
morf.ipc["deck.rest"] = function() D.revealed:set(false) D.peeked:set("") return "ok" end
morf.ipc["deck.place"] = function(key, edge)
  if not key then return "usage: deck.place <note> [left|right|bottom]" end
  service.place_note(key, edge or "left")
  return service.placement_of(key)
end

return deck
