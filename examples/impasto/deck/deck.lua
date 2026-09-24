-- Notes on the screen edges: tabs that peek out under the pointer.
--
-- Port of Deck.qml, at rest. Each note on an edge is a thin strip of its
-- paper; hovering slides the tabs out with their titles, the tab under the
-- pointer peeks the whole note beside it, a click opens the note in the
-- island, and a right click offers Open and Edit (the desk's menu; Edit
-- arranges the desk with the deck selected). A tab is never dragged at
-- rest: moving a note, or a deck, is arranging's (desktop/arrange/decks.lua
-- draws the decks on the board then, and these surfaces step aside).
--
-- The original is one full-screen surface with a computed input mask. Here
-- each edge that has a deck gets its own small layer surface, as tall (or
-- wide) as its deck and its peek need, and the surface's input region is
-- morf's usual one -- only where there is a MouseArea, so only the tabs and
-- the peek take the pointer and the desktop under the rest gets its clicks.
-- It never takes the keyboard. The surfaces sit on the desk's board, inset
-- from the bar's band and the dock's, so a deck never lies over the dock.
--
-- Hidden under a fullscreen window, and with `deckOnEmpty` on any workspace
-- that has windows (`services/deck.lua`).

local ui = require("morf.ui")
local theme = require("theme")
local notes = require("services.notes")
local service = require("services.deck")
local island = require("bar.island")
local desk = require("services.desktop")
local kit = require("components.kit")
local sticky = require("components.sticky")

local C = theme.color
local D = service

local screen = (morf.screens or {})[1] or {}
local SCREEN_W = tonumber(screen.width) or 1920
local SCREEN_H = tonumber(screen.height) or 1080

-- The surfaces sit on the desk's board: under the bar's band, clear of
-- the dock's.
local function board_width() return desk.board().width end
local function board_height() return desk.board().height end

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
    if mine == retract_generation and hover_count <= 0 then
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
  local out = D.revealed:get() or D.peeked:get() ~= ""
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

  local function slot_of(key)
    local d = the_deck()
    for i, other in ipairs(d and d.notes or {}) do if other == key then return i end end
    return 1
  end

  -- The surface's top left on the board, for the menu's place.
  local function origin()
    if edge == "right" then return board_width() - THICK, 0 end
    if edge == "bottom" then return 0, board_height() - THICK_BOTTOM end
    return 0, 0
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
    local along = function() return D.tab_at(start(), slot_of(key)) end
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
      behavior = {
        x = theme.behave("medium"), y = theme.behave("medium"),
        width = theme.behave("medium"), height = theme.behave("medium"),
        color = theme.behave("fast"),
      },
      title,
      -- A press opens the note, a right click the menu. Never a drag: a
      -- note leaves its edge only while the desk is arranged, or from the
      -- notes panel.
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        accepted_buttons = { "left", "right" },
        on_entered = function()
          entered()
          D.peeked:set(key)
        end,
        on_exited = exited,
        on_clicked = function(sx, sy, lx, ly, button)
          if button == "right" then
            local d = the_deck()
            if not d then return end
            D.peeked:set("")
            local ox, oy = origin()
            local tx, ty = 0, 0
            if edge == "bottom" then tx = D.tab_at(start(), slot_of(key)) else ty = D.tab_at(start(), slot_of(key)) end
            if edge == "right" then tx = THICK - depth() elseif edge == "bottom" then ty = THICK_BOTTOM - depth() end
            desk.open_menu(d.key, ox + tx + (lx or 0), oy + ty + (ly or 0), key)
            return
          end
          open_note(key)
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
    return key ~= "" and key == peek_key:get() and not desk.menu_open:get()
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

  local root = ui.Item {
    width = function() return vertical and THICK or extent() end,
    height = function() return vertical and extent() or THICK_BOTTOM end,
    ui.Repeater { model = model, delegate = tab },
    peek,
  }

  local anchors = edge == "left" and { left = true, top = true }
    or edge == "right" and { right = true, top = true }
    or { left = true, bottom = true }
  local window = morf.window.layer {
    namespace = "impasto-deck",
    layer = "top",
    keyboard_focus = "none",
    -- Placed on the board by margins, not pushed by the other zones.
    exclusive_zone = -1,
    anchors = anchors,
    width = vertical and THICK or 1,
    height = vertical and 1 or THICK_BOTTOM,
    visible = false,
    root = root,
  }

  local open = false
  morf.effect("impasto.deck." .. edge .. ".window", function()
    -- While arranging the board draws the decks (desktop/arrange/decks.lua).
    local want = the_deck() ~= nil and not service.away() and not desk.editing:get()
    local size = extent()
    if vertical then window:size(THICK, size) else window:size(size, depth_box) end
    local b = desk.board()
    local margins = {
      margin_top = vertical and b.y or 0,
      margin_left = edge ~= "right" and b.x or 0,
      margin_right = edge == "right" and (desk.screen_width - b.x - b.width) or 0,
      margin_bottom = edge == "bottom" and (desk.screen_height - b.y - b.height) or 0,
    }
    for name, value in pairs(margins) do
      if window[name] ~= value then window[name] = value end
    end
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
