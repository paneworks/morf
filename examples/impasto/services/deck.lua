-- The note decks on the screen edges: which notes sit on which edge, the
-- geometry of their tabs, and the pointer's state over them.
--
-- Port of DeckService.qml. A deck is a row in the `desktopWidgets` setting
-- with an `edge`, kept by `services/desktop.lua` beside the widgets:
--
--     { key = "notes-2", id = "notes", edge = "left", notes = { "note-..." }, along = 0 }
--
-- One deck per edge (left, right, bottom) and screen; an empty deck is
-- removed. Tabs run top to bottom on the sides and left to right along the
-- bottom. Lengths are along the desk's board (the screen less the bar's and
-- the dock's bands), so a deck never sits under the dock.

local settings = require("services.settings")
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

-- Handle for sliding a deck along its edge while arranging.
M.grip = 24

--- Inverse of `start_of`: the fraction for a strip starting at `start`.
function M.along_at(count, start, length)
  local run = M.run_of(count, length)
  if run <= 0 then return 0 end
  return math.max(0, math.min(1, (start - M.margin) / run))
end

--- The edge a point on the board is against, or "": within `reach` of the
--- left, the right or the bottom. Sides win in the corners.
function M.edge_at(x, y, width, height)
  if x <= M.reach then return "left" end
  if x >= width - M.reach then return "right" end
  if y >= height - M.reach then return "bottom" end
  return ""
end

-- -------------------------------------------------------------------- rows --
--
-- The rows are the desk's (`services/desktop.lua`, DesktopService's deck
-- half), so a drag on the desk and a drop on an edge are one write.

local function desk() return require("services.desktop") end

--- The decks on this screen that have notes: `{ key, edge, along, notes,
--- takes_new }`. A binding that calls it follows the rows and the notes.
function M.decks()
  local out = {}
  for _, row in ipairs(desk().decks()) do
    out[#out + 1] = { key = row.key, edge = row.edge, along = desk().along_of(row),
      notes = desk().deck_notes(row), takes_new = row.takesNew == true }
  end
  return out
end

function M.deck_on(edge)
  for _, deck in ipairs(M.decks()) do
    if deck.edge == edge then return deck end
  end
end

--- Where a note is: an edge, "grid" for a desktop widget, or "".
function M.placement_of(key) return desk().placement_of(key) end
function M.remove_note(key) desk().remove_note(key) end
--- Puts a note on an edge at `index` (1-based; nil for the end).
function M.place_note(key, edge, index) desk().place_note(key, edge, index) end

function M.set_along(edge, along)
  local deck = M.deck_on(edge)
  if deck then desk().set_deck_along(deck.key, along) end
end

-- ----------------------------------------------------------------- pointer --

M.revealed = morf.signal("impasto.deck.revealed", false)
M.peeked = morf.signal("impasto.deck.peeked", "")
-- The note whose tab is carried while arranging, and the deck whose grip is.
M.dragging = morf.signal("impasto.deck.dragging", "")
M.sliding = morf.signal("impasto.deck.sliding", "")
-- The edge that would take what the desk is carrying (a note widget, the
-- card's notes or spectrum, a tab), or "": lit along its length.
M.receiving = morf.signal("impasto.deck.receiving", "")

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
