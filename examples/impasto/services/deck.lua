-- The note decks on the screen edges: which notes sit on which edge, the
-- geometry of their tabs, and the pointer's state over them.
--
-- Port of DeckService.qml and the deck half of DesktopService.qml. A deck is
-- a row in the `desktopWidgets` setting with an `edge` -- the same list the
-- desktop's widgets live in, so a desktop that is ported later finds the
-- decks where the original kept them:
--
--     { key = "deck-left", id = "notes", edge = "left", notes = { "note-..." }, along = 0 }
--
-- One deck per edge (left, right, bottom); an empty deck is removed. Tabs
-- run top to bottom on the sides and left to right along the bottom.

local settings = require("services.settings")
local notes = require("services.notes")
local theme = require("theme")

local M = {}

M.edges = { "left", "right", "bottom" }
local edge_names = { left = true, right = true, bottom = true }

-- ---------------------------------------------------------------- geometry --
--
-- At rest each note is a sliver on the edge; on hover the tabs come out to
-- `tab_depth` and the hovered note slides out beside its tab.
M.sliver = 5
M.tab_depth = 26
M.tab_length = 112
M.tab_gap = 8
M.peek_width = 250
M.peek_height = 200
M.peek_gap = 6
M.margin = theme.desktop_gutter
-- How far a tab is pulled off its edge before letting go takes it off.
M.reach = 72

function M.strip_length(count)
  return math.max(0, count * (M.tab_length + M.tab_gap) - M.tab_gap)
end

--- Free travel along an edge `length` long: the strip and both margins out.
function M.run_of(count, length)
  return math.max(0, length - M.strip_length(count) - 2 * M.margin)
end

--- `along` is a 0..1 fraction of the run, so a deck keeps its relative
--- place across screen sizes.
function M.start_of(count, along, length)
  return M.margin + math.max(0, math.min(1, along or 0)) * M.run_of(count, length)
end

function M.tab_at(start, index) return start + (index - 1) * (M.tab_length + M.tab_gap) end

--- The slot (1-based) a point along the edge falls in: compared against
--- tab centres, so a tab let go in place keeps its slot.
function M.index_at(position, count, start)
  local slot = math.floor((position - start - M.tab_length / 2) / (M.tab_length + M.tab_gap) + 0.5)
  return math.max(1, math.min(count, slot + 1))
end

-- -------------------------------------------------------------------- rows --

local function rows()
  local list = settings.desktopWidgets
  return type(list) == "table" and list or {}
end

local function is_deck(row)
  return type(row) == "table" and edge_names[row.edge] and row.id ~= "spectrum"
end

--- Live, unarchived note keys on a deck row.
local function deck_notes(row)
  local out = {}
  for _, key in ipairs(row.notes or {}) do
    local note = type(key) == "string" and notes.entry(key)
    if note and not note.archived then out[#out + 1] = key end
  end
  return out
end

--- The decks that have notes: `{ key, edge, along, notes }`. A binding that
--- calls it follows the setting and the notes.
function M.decks()
  local out = {}
  for _, row in ipairs(rows()) do
    if is_deck(row) then
      local keys = deck_notes(row)
      if #keys > 0 then
        out[#out + 1] = { key = row.key, edge = row.edge, along = tonumber(row.along) or 0, notes = keys }
      end
    end
  end
  return out
end

function M.deck_on(edge)
  for _, deck in ipairs(M.decks()) do
    if deck.edge == edge then return deck end
  end
end

--- Where a note is: an edge, "grid" for a desktop widget, or "".
function M.placement_of(key)
  for _, row in ipairs(rows()) do
    if is_deck(row) then
      for _, other in ipairs(row.notes or {}) do
        if other == key then return row.edge end
      end
    elseif type(row) == "table" and row.id == "notes" and row.note == key then
      return "grid"
    end
  end
  return ""
end

-- A copy of the list without this note anywhere; decks left empty go.
local function without(list, key)
  local kept = {}
  for _, row in ipairs(list) do
    if is_deck(row) then
      local left = {}
      for _, other in ipairs(row.notes or {}) do
        if other ~= key then left[#left + 1] = other end
      end
      if #left > 0 then
        local copy = {}
        for k, v in pairs(row) do copy[k] = v end
        copy.notes = left
        kept[#kept + 1] = copy
      end
    elseif not (type(row) == "table" and row.id == "notes" and row.note == key) then
      kept[#kept + 1] = row
    end
  end
  return kept
end

function M.remove_note(key)
  settings.set("desktopWidgets", without(rows(), key))
end

--- Puts a note on an edge at `index` (1-based; nil for the end), joining
--- the deck there or starting one. A deck keeps its key and place.
function M.place_note(key, edge, index)
  if not notes.entry(key) or not edge_names[edge] then return end
  local list = rows()
  local target
  for _, row in ipairs(list) do
    if is_deck(row) and row.edge == edge then target = row end
  end
  local kept = without(list, key)
  if not target then
    kept[#kept + 1] = { key = "deck-" .. edge, id = "notes", edge = edge, notes = { key }, along = 0 }
  else
    local left = {}
    for _, other in ipairs(deck_notes(target)) do
      if other ~= key then left[#left + 1] = other end
    end
    index = math.max(1, math.min(#left + 1, index or (#left + 1)))
    table.insert(left, index, key)
    local copy = {}
    for k, v in pairs(target) do copy[k] = v end
    copy.notes = left
    local placed = false
    for i, row in ipairs(kept) do
      if is_deck(row) and row.edge == edge then kept[i] = copy placed = true end
    end
    if not placed then kept[#kept + 1] = copy end
  end
  settings.set("desktopWidgets", kept)
end

function M.set_along(edge, along)
  local list = {}
  for i, row in ipairs(rows()) do
    if is_deck(row) and row.edge == edge then
      local copy = {}
      for k, v in pairs(row) do copy[k] = v end
      copy.along = math.max(0, math.min(1, along))
      list[i] = copy
    else
      list[i] = row
    end
  end
  settings.set("desktopWidgets", list)
end

-- A note archived or deleted leaves the edges with it.
notes.on_removed[#notes.on_removed + 1] = function(key)
  if M.placement_of(key) ~= "" then M.remove_note(key) end
end

-- ----------------------------------------------------------------- pointer --

M.revealed = morf.signal("impasto.deck.revealed", false)
M.peeked = morf.signal("impasto.deck.peeked", "")
M.dragging = morf.signal("impasto.deck.dragging", "")

-- ----------------------------------------------------------------- showing --
--
-- Hidden under a fullscreen window, as the dock is, and with `deckOnEmpty`
-- on any workspace that has windows. Hyprland is asked only when it is
-- there; anywhere else the decks always show.

local hyprland
local windows_changed = morf.signal("impasto.deck.windows", 0)
local function watch_hyprland()
  if hyprland ~= nil then return hyprland end
  local ok, lib = pcall(require, "lib.hyprland")
  if not ok or not lib.available() then hyprland = false return false end
  hyprland = lib
  if not lib.state.connected then lib.start() end
  for _, name in ipairs { "openwindow", "closewindow", "movewindow", "movewindowv2", "workspace", "workspacev2", "fullscreen" } do
    lib.on(name, function() windows_changed:set(windows_changed:get() + 1) end)
  end
  return lib
end

--- Whether the edges should step aside on this screen now.
function M.away()
  local lib = watch_hyprland()
  if not lib then return false end
  windows_changed:get()
  if lib.state.fullscreen then return true end
  if not settings.deckOnEmpty then return false end
  local id = lib.state.active_workspace.id
  return id and id > 0 and lib.occupied(id) or false
end

return M
