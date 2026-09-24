-- The desk: which modules are on the wallpaper, where, and in which shape.
--
-- Port of DesktopService.qml. A widget is a module placed on the grid in one
-- of four families -- 2x2, 4x2, 4x4 and 8x2 cells -- and each family has a
-- face of its own; a module offers only the families it has faces for.
-- Widgets never overlap: a drop on an occupied cell lands on the nearest
-- free fit, or is refused.
--
-- Rows are keyed by `key` ("clock-2"), not by module, so one module can be
-- placed twice. A row carries `id`, `col`, `row`, `family`, and optionally
-- `theme`, `style`, `opacity` (each defaulting to the desk's setting),
-- `picture`/`caption` for a photo, `note` for a note, and the spectrum's look
-- (`look`, `fill`, `color`, `color2`, `bar`, `gap`, `lows`, `peaks`). A row
-- with an `edge` is a spectrum along that edge of the screen instead of a
-- square, or a deck of notes there (the decks section below).
--
-- The rows are kept here and written back to `desktopWidgets` on a short
-- debounce, so a drag is one write. Every screen runs the configuration
-- once, so this process's board is its own screen's: a row with a `screen`
-- naming another output is left to that output's process.
--
-- Nothing here builds nodes. The board reads `M.keys` (a list model keyed
-- by row key) and each widget asks `M.face_id(key)` which face to build;
-- both are brought up to date by `sync`, which runs on the next tick after
-- any change, outside whatever effect or handler caused it.

local settings = require("services.settings")
local theme = require("theme")

local M = {}

local C = theme.color

-- -------------------------------------------------------------- families --

M.families = {
  { id = "2x2", cols = 2, rows = 2, label = "Small" },
  { id = "4x2", cols = 4, rows = 2, label = "Wide" },
  { id = "4x4", cols = 4, rows = 4, label = "Large" },
  { id = "8x2", cols = 8, rows = 2, label = "Band" },
}
local family_by_id = {}
for _, f in ipairs(M.families) do family_by_id[f.id] = f end

function M.family(id) return family_by_id[id] or family_by_id["4x2"] end

M.themes = { { id = "modern", label = "Modern" }, { id = "analogue", label = "Analogue" } }

-- Families each theme has a face for, per module. Every module has 4x2.
M.faces = {
  modern = {
    media = { "2x2", "4x2", "4x4" }, timer = { "2x2", "4x2" },
    claude = { "2x2", "4x2", "4x4" }, battery = { "2x2", "4x2" },
    volume = { "2x2", "4x2" }, brightness = { "2x2", "4x2" },
    network = { "2x2", "4x2" }, bluetooth = { "2x2", "4x2" },
    weather = { "2x2", "4x2", "4x4", "8x2" }, stats = { "2x2", "4x2", "4x4" },
    github = { "2x2", "4x2", "8x2" },
    updates = { "2x2", "4x2" }, pet = { "2x2", "4x2" },
    games = { "2x2", "4x2" }, calendar = { "2x2", "4x2", "4x4" },
    notes = { "2x2", "4x2", "4x4", "8x2" }, tasks = { "2x2", "4x2", "4x4" },
    clock = { "2x2", "4x2", "8x2" }, photo = { "2x2", "4x2", "4x4", "8x2" },
    spectrum = { "4x2", "8x2", "4x4" },
  },
  analogue = {
    media = { "2x2", "4x2", "4x4" }, timer = { "2x2", "4x2" },
    claude = { "2x2", "4x2" }, battery = { "2x2", "4x2" },
    volume = { "2x2", "4x2" }, brightness = { "2x2", "4x2" },
    network = { "2x2", "4x2" }, bluetooth = { "2x2", "4x2" },
    weather = { "2x2", "4x2", "4x4", "8x2" }, stats = { "2x2", "4x2", "4x4" },
    github = { "2x2", "4x2", "8x2" },
    updates = { "2x2", "4x2" }, pet = { "2x2", "4x2" },
    games = { "2x2", "4x2" }, calendar = { "2x2", "4x2", "4x4" },
    notes = { "2x2", "4x2", "4x4", "8x2" }, tasks = { "2x2", "4x2", "4x4" },
    clock = { "2x2", "4x2", "4x4", "8x2" }, photo = { "2x2", "4x2", "4x4", "8x2" },
    spectrum = { "4x2", "8x2", "4x4" },
  },
}

-- The modules the card offers, in its order: every module that has a face
-- on the desk. `glyph` is the tile's mark.
M.catalogue = {
  { id = "clock", name = "Clock", glyph = "󰥔" },
  { id = "calendar", name = "Calendar", glyph = "󰃭" },
  { id = "weather", name = "Weather", glyph = "󰖐" },
  { id = "media", name = "Media", glyph = "󰎇" },
  { id = "pet", name = "Pet", glyph = "󰩃" },
  { id = "stats", name = "System", glyph = "󰻠" },
  { id = "battery", name = "Battery", glyph = "󰁹" },
  { id = "github", name = "GitHub", glyph = "󰊤" },
  { id = "claude", name = "Claude", glyph = "󰚩" },
  { id = "notes", name = "Notes", glyph = "󰎞" },
  { id = "tasks", name = "Tasks", glyph = "󰄲" },
  { id = "photo", name = "Photo", glyph = "󰋩" },
  { id = "timer", name = "Timer", glyph = "󱎫" },
  { id = "volume", name = "Volume", glyph = "󰕾" },
  { id = "brightness", name = "Brightness", glyph = "󰃠" },
  { id = "network", name = "Network", glyph = "󰤨" },
  { id = "bluetooth", name = "Bluetooth", glyph = "󰂯" },
  { id = "updates", name = "Updates", glyph = "󰏖" },
  { id = "games", name = "Games", glyph = "󰊗" },
  { id = "spectrum", name = "Spectrum", glyph = "󰺢" },
}
local catalogue_by_id = {}
for _, entry in ipairs(M.catalogue) do catalogue_by_id[entry.id] = entry end
function M.entry(id) return catalogue_by_id[id] end

-- ------------------------------------------------------------------- rows --

local rows = {}              -- the rows, in order
local revision = morf.signal("impasto.desk.revision", 0)
local function bump() revision:set(revision:get() + 1) end

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

-- Legacy rows: a missing key is the module id; `bare` is a style; `ink` is gone.
local function normalise(list)
  local out = {}
  for _, kept in ipairs(type(list) == "table" and list or {}) do
    if type(kept) == "table" and kept.id then
      local row = copy(kept)
      if not row.key then row.key = row.id end
      if row.bare == true and not row.style then row.style = "bare" end
      row.bare, row.ink = nil, nil
      out[#out + 1] = row
    end
  end
  return out
end

--- Every row. A binding that calls it follows every change.
function M.rows()
  revision:get()
  return rows
end

function M.revision() return revision:get() end

function M.entry_of(key)
  revision:get()
  for _, row in ipairs(rows) do
    if row.key == key then return row end
  end
  return nil
end

function M.is_edge(row) return row ~= nil and type(row.edge) == "string" and row.edge ~= "" end
function M.is_spectrum(row) return M.is_edge(row) and row.id == "spectrum" end

-- This process draws one screen: rows naming another are that screen's.
local screen = (morf.screens or {})[1] or {}
M.screen_name = tostring(screen.name or "")
local function here(row)
  local on = row.screen
  if type(on) ~= "string" or on == "" then return true end
  return on == M.screen_name or on == tostring(screen.description or "")
end

--- The squares on this board.
function M.squares()
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do
    if not M.is_edge(row) and here(row) then out[#out + 1] = row end
  end
  return out
end

--- The spectra along this screen's edges.
function M.spectra()
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do
    if M.is_spectrum(row) and here(row) then out[#out + 1] = row end
  end
  return out
end

function M.count_of(id)
  local n = 0
  for _, row in ipairs(M.rows()) do if row.id == id then n = n + 1 end end
  return n
end

-- ----------------------------------------------------------------- themes --

function M.theme_of(row)
  local own = row and row.theme
  if own == "modern" or own == "analogue" then return own end
  return settings.desktopTheme == "analogue" and "analogue" or "modern"
end

function M.families_for(id, theme_id)
  local table_ = M.faces[theme_id or settings.desktopTheme] or M.faces.modern
  return table_[id] or { "4x2" }
end

function M.offers(id, family_id, theme_id)
  for _, f in ipairs(M.families_for(id, theme_id)) do
    if f == family_id then return true end
  end
  return false
end

-- Without a face for `family_id`, the largest offered family that fits
-- inside it, so a widget never grows into a neighbour.
function M.drawn_family(id, family_id, theme_id)
  if M.offers(id, family_id, theme_id) then return family_id end
  local shape = M.family(family_id)
  local best, best_area = "2x2", 0
  for _, offered in ipairs(M.families_for(id, theme_id)) do
    local c = M.family(offered)
    local area = c.cols * c.rows
    if c.cols <= shape.cols and c.rows <= shape.rows and area > best_area then
      best, best_area = offered, area
    end
  end
  return best
end

function M.family_of(row)
  if not row or not row.family then return "4x2" end
  return M.drawn_family(row.id, row.family, M.theme_of(row))
end

-- ------------------------------------------------------------------ board --

local SCREEN_W = tonumber(screen.width) or 1920
local SCREEN_H = tonumber(screen.height) or 1080
M.screen_width, M.screen_height = SCREEN_W, SCREEN_H

-- What the bar and the dock keep clear. The dock's band is kept wherever the
-- dock could be, not where it happens to be painting, so nothing moves when
-- a window goes fullscreen.
function M.insets()
  local out = { top = theme.bar_reserve(), left = 0, right = 0, bottom = 0 }
  local ok, dock = pcall(require, "services.dock")
  if ok and dock then
    local zone = dock.zone()
    out[dock.edge()] = zone
  end
  return out
end

function M.board()
  local i = M.insets()
  return { x = i.left, y = i.top, width = SCREEN_W - i.left - i.right, height = SCREEN_H - i.top - i.bottom }
end

local function round(v) return math.floor(v + 0.5) end

-- The grid on a board: the stride between squares, how many fit each way and
-- where the first starts. The margin is the same on all four sides, which
-- takes a stride that splits the difference between the board's sides into
-- whole squares: the smallest such square no smaller than the cell, and on
-- each side the count whose margin is nearest one street. A board too
-- nearly square keeps the plain stride, centred.
function M.grid_for(width, height)
  local gutter = theme.desktop_gutter
  if width <= 0 or height <= 0 then
    return { stride = theme.desktop_stride, columns = 8, rows = 6, origin_x = gutter, origin_y = gutter }
  end
  local shortest = math.min(width, height)
  local difference = math.abs(width - height)
  local apart = math.floor(difference / theme.desktop_stride)
  local even = apart > 0 and difference / apart - gutter <= theme.desktop_cell_largest
  local stride = even and difference / apart or theme.desktop_stride
  local across = math.max(1, round((shortest - gutter) / stride))
  if across > 1 and across * stride > shortest then across = across - 1 end
  local function count(length)
    if even then return across + round((length - shortest) / stride) end
    return math.max(1, math.floor(length / stride))
  end
  local columns, rows_ = count(width), count(height)
  return {
    stride = stride, columns = columns, rows = rows_,
    origin_x = (width + gutter - columns * stride) / 2,
    origin_y = (height + gutter - rows_ * stride) / 2,
  }
end

function M.grid()
  local b = M.board()
  return M.grid_for(b.width, b.height)
end

function M.offset_x(col) local g = M.grid() return round(g.origin_x + col * g.stride) end
function M.offset_y(row) local g = M.grid() return round(g.origin_y + row * g.stride) end
function M.span(count) return round(count * M.grid().stride - theme.desktop_gutter) end
function M.cell_x(x) local g = M.grid() return round((x - g.origin_x) / g.stride) end
function M.cell_y(y) local g = M.grid() return round((y - g.origin_y) / g.stride) end

--- A family's size in pixels on this board.
function M.size_for(family_id)
  local shape = M.family(family_id)
  return { width = M.span(shape.cols), height = M.span(shape.rows) }
end

--- A cell box on the board: `{ x, y, width, height }`.
function M.box(col, row, family_id)
  local shape = M.family(family_id)
  local x, y = M.offset_x(col), M.offset_y(row)
  return {
    x = x, y = y,
    width = M.offset_x(col + shape.cols) - theme.desktop_gutter - x,
    height = M.offset_y(row + shape.rows) - theme.desktop_gutter - y,
  }
end

-- ------------------------------------------------------------- collisions --

local function clamped(row, shape, grid)
  return {
    col = math.max(0, math.min(grid.columns - shape.cols, row.col or 0)),
    row = math.max(0, math.min(grid.rows - shape.rows, row.row or 0)),
  }
end

local function clashes(taken, col, row, shape)
  for _, o in ipairs(taken) do
    if col < o.col + o.cols and o.col < col + shape.cols and row < o.row + o.rows and o.row < row + shape.rows then
      return true
    end
  end
  return false
end

-- Where each square is on this board: its own cell kept inside the board,
-- or the nearest free one when another holds it. The farthest right and
-- down go first, so on a smaller board those against the edge keep it.
-- Never written back, so a layout made on a larger screen stays intact.
local spots_cache, spots_at = nil, nil
local function spots()
  local grid = M.grid()
  local stamp = revision:get() .. ":" .. grid.columns .. "x" .. grid.rows .. ":" .. settings.desktopTheme
  if spots_at == stamp then return spots_cache end
  local list = {}
  for _, row in ipairs(M.squares()) do
    local shape = M.family(M.family_of(row))
    list[#list + 1] = { row = row, shape = shape, right = (row.col or 0) + shape.cols, bottom = (row.row or 0) + shape.rows }
  end
  table.sort(list, function(a, b)
    if a.right ~= b.right then return a.right > b.right end
    if a.bottom ~= b.bottom then return a.bottom > b.bottom end
    return a.row.key < b.row.key
  end)
  local out, taken = {}, {}
  for _, item in ipairs(list) do
    local shape = item.shape
    local home = clamped(item.row, shape, grid)
    local spot = home
    if clashes(taken, home.col, home.row, shape) then
      local nearest = math.huge
      for col = 0, grid.columns - shape.cols do
        for r = 0, grid.rows - shape.rows do
          local d = (col - home.col) ^ 2 + (r - home.row) ^ 2
          if d < nearest and not clashes(taken, col, r, shape) then
            nearest = d
            spot = { col = col, row = r }
          end
        end
      end
    end
    taken[#taken + 1] = { col = spot.col, row = spot.row, cols = shape.cols, rows = shape.rows }
    out[item.row.key] = spot
  end
  spots_cache, spots_at = out, stamp
  return out
end

function M.spot_of(row)
  return spots()[row.key] or clamped(row, M.family(M.family_of(row)), M.grid())
end

--- The widget's box on the board, or nil when it is gone.
function M.geometry(key)
  local row = M.entry_of(key)
  if not row then return nil end
  local spot = M.spot_of(row)
  return M.box(spot.col, spot.row, M.family_of(row))
end

local function overlaps(col, row, family_id, except)
  local shape = M.family(family_id)
  for _, other in ipairs(M.squares()) do
    if other.key ~= except then
      local theirs = M.family(M.family_of(other))
      local spot = M.spot_of(other)
      if col < spot.col + theirs.cols and spot.col < col + shape.cols
        and row < spot.row + theirs.rows and spot.row < row + shape.rows then
        return true
      end
    end
  end
  return false
end

function M.on_board(col, row, family_id)
  local shape, grid = M.family(family_id), M.grid()
  return col >= 0 and row >= 0 and col + shape.cols <= grid.columns and row + shape.rows <= grid.rows
end

function M.free(col, row, family_id, except)
  return M.on_board(col, row, family_id) and not overlaps(col, row, family_id, except)
end

--- The nearest free cell by squared distance, or nil when the shape fits nowhere.
function M.nearest_free(col, row, family_id, except)
  if M.free(col, row, family_id, except) then return { col = col, row = row } end
  local grid = M.grid()
  local best, best_d = nil, math.huge
  for c = 0, grid.columns - 1 do
    for r = 0, grid.rows - 1 do
      local d = (c - col) ^ 2 + (r - row) ^ 2
      if d < best_d and M.free(c, r, family_id, except) then
        best, best_d = { col = c, row = r }, d
      end
    end
  end
  return best
end

function M.first_free(family_id, except)
  local grid = M.grid()
  for r = 0, grid.rows - 1 do
    for c = 0, grid.columns - 1 do
      if M.free(c, r, family_id, except) then return { col = c, row = r } end
    end
  end
  return nil
end

-- ---------------------------------------------------------------- writing --

M.keys = morf.list_model({})          -- the squares on this board, by key
M.spectrum_keys = morf.list_model({}) -- the spectra along its edges
M.deck_keys = morf.list_model({})     -- the note decks on its edges
M.face_models = {}                    -- key -> the one-row face models of its widgets

local writing = false     -- a write is waiting to be saved
local sync_queued = false

--- Which face a widget shows: module, drawn family and theme. A widget
--- rebuilds its face when this changes, and only then.
function M.face_id(row)
  if not row then return "" end
  local size = M.size_for(M.family_of(row))
  return row.id .. "|" .. M.family_of(row) .. "|" .. M.theme_of(row) .. "|" .. size.width .. "x" .. size.height
end

local function sync()
  sync_queued = false
  local keys, edges, decks = {}, {}, {}
  for _, row in ipairs(rows) do
    if here(row) then
      if M.is_spectrum(row) then edges[#edges + 1] = { key = row.key }
      elseif M.is_edge(row) then decks[#decks + 1] = { key = row.key }
      else keys[#keys + 1] = { key = row.key } end
    end
  end
  M.keys:replace(keys, "key")
  M.spectrum_keys:replace(edges, "key")
  M.deck_keys:replace(decks, "key")
  for key, models in pairs(M.face_models) do
    local row
    for _, r in ipairs(rows) do if r.key == key then row = r end end
    if row then
      for _, model in ipairs(models) do model:replace({ { id = M.face_id(row) } }, "id") end
    else
      M.face_models[key] = nil
    end
  end
end

--- A one-row model naming the face `key` shows, kept current by `sync`: a
--- Repeater over it rebuilds the face when the family or theme changes.
function M.face_model(key)
  local model = morf.list_model({ { id = M.face_id(M.entry_of(key)) } })
  local list = M.face_models[key] or {}
  list[#list + 1] = model
  M.face_models[key] = list
  return model
end

--- Brings the board's models up to date on the next tick.
function M.queue_sync()
  if sync_queued then return end
  sync_queued = true
  morf.timer(1, sync, false)
end

local function save()
  writing = false
  settings.set("desktopWidgets", copy(rows))
end

local function write(next_rows)
  rows = next_rows
  bump()
  if not writing then
    writing = true
    morf.timer(120, save, false)
  end
  M.queue_sync()
end

--- Changes fields of one row; a false value removes the field, so the row
--- inherits the desk's default again.
function M.update(key, changes)
  local out = {}
  for _, row in ipairs(rows) do
    if row.key == key then
      local next_row = copy(row)
      for field, value in pairs(changes) do
        if value == false then next_row[field] = nil else next_row[field] = value end
      end
      out[#out + 1] = next_row
    else
      out[#out + 1] = row
    end
  end
  write(out)
end

function M.new_key(id)
  local n = 1
  while true do
    local key = id .. "-" .. n
    local taken = false
    for _, row in ipairs(rows) do if row.key == key then taken = true end end
    if not taken then return key end
    n = n + 1
  end
end

local function screen_field()
  local list = morf.screens or {}
  if #list <= 1 then return nil end
  return M.screen_name ~= "" and M.screen_name or nil
end

--- Adds a widget at its smallest family, at or near a cell, else at the
--- first free one. Returns the new key, or "" when there is no room.
function M.add(id, col, row, fields)
  if not catalogue_by_id[id] then return "" end
  local family_id = M.families_for(id)[1] or "4x2"
  local spot = col and col >= 0 and M.nearest_free(col, row, family_id, "") or (not col and M.first_free(family_id, ""))
  if not spot then return "" end
  local key = M.new_key(id)
  local made = copy(fields or {})
  made.key, made.id, made.col, made.row, made.family = key, id, spot.col, spot.row, family_id
  made.screen = screen_field()
  local out = copy(rows)
  out[#out + 1] = made
  write(out)
  return key
end

function M.remove(key)
  if M.selected:get() == key then M.selected:set("") end
  local out = {}
  for _, row in ipairs(rows) do if row.key ~= key then out[#out + 1] = row end end
  write(out)
end

--- A drop: the cell, or the nearest free fit; otherwise nothing moves.
function M.place(key, col, row)
  local widget = M.entry_of(key)
  if not widget then return end
  local spot = M.nearest_free(col, row, M.family_of(widget), key)
  if not spot then return end
  M.update(key, { col = spot.col, row = spot.row })
end

--- Re-placed from its current cell, so a resized widget moves as little as
--- possible.
function M.set_family(key, family_id)
  local widget = M.entry_of(key)
  if not widget or not M.offers(widget.id, family_id, M.theme_of(widget)) then return end
  if M.family_of(widget) == family_id then return end
  local at = M.spot_of(widget)
  local spot = M.nearest_free(at.col, at.row, family_id, key)
  if not spot then return end
  M.update(key, { family = family_id, col = spot.col, row = spot.row })
end

--- The wheel while arranging: the next family in `delta`'s direction that fits.
function M.cycle_family(key, delta)
  local widget = M.entry_of(key)
  if not widget then return end
  local families = M.families_for(widget.id, M.theme_of(widget))
  local at = 1
  for i, f in ipairs(families) do if f == M.family_of(widget) then at = i end end
  local spot = M.spot_of(widget)
  for step = 1, #families - 1 do
    local next_family = families[((at - 1 + delta * step) % #families) + 1]
    if M.nearest_free(spot.col, spot.row, next_family, key) then
      M.set_family(key, next_family)
      return
    end
  end
end

-- The family the resize handle picks for a pull of `cols` by `rows` cells:
-- the pointer is pulled back by a third of a cell and the smallest family
-- containing it wins; beyond every footprint, the nearest corner.
M.handle_inset = 0.35
function M.family_nearest(id, cols, rows_, theme_id)
  local offered = M.families_for(id, theme_id)
  local x, y = cols - M.handle_inset, rows_ - M.handle_inset
  local best, best_area = "", math.huge
  for _, f in ipairs(offered) do
    local s = M.family(f)
    if x <= s.cols and y <= s.rows and s.cols * s.rows < best_area then best, best_area = f, s.cols * s.rows end
  end
  if best ~= "" then return best end
  local best_d = math.huge
  for _, f in ipairs(offered) do
    local s = M.family(f)
    local d = (s.cols - cols) ^ 2 + (s.rows - rows_) ^ 2
    if d < best_d then best, best_d = f, d end
  end
  return best
end

--- A row's own theme; false goes back to the desk's. Only ever shrinks the
--- family, so nothing needs placing again.
function M.set_theme(key, theme_id)
  M.update(key, { theme = theme_id or false })
  M.conform()
end

function M.conform()
  local changed = false
  local out = {}
  for _, row in ipairs(rows) do
    local kept = row.family or "4x2"
    if not M.is_edge(row) then
      local drawn = M.drawn_family(row.id, kept, M.theme_of(row))
      if drawn ~= kept then
        row = copy(row)
        row.family = drawn
        changed = true
      end
    end
    out[#out + 1] = row
  end
  if changed then write(out) end
end

-- ------------------------------------------------------------- appearance --

M.styles = {
  { id = "capsule", label = "Capsule" }, { id = "accent", label = "Accent" },
  { id = "outline", label = "Outline" }, { id = "bare", label = "No capsule" },
}

function M.style_of(row)
  -- Notes draw their own paper, photos are their picture and the spectrum
  -- is its bars: always bare.
  if row and (row.id == "notes" or row.id == "photo" or row.id == "spectrum") then return "bare" end
  local own = row and row.style
  for _, s in ipairs(M.styles) do if s.id == own then return own end end
  return settings.desktopStyle
end

function M.opacity_of(row)
  local own = row and row.opacity
  if type(own) == "number" then return own end
  return settings.desktopOpacity
end

function M.set_style(key, style) M.update(key, { style = style or false }) end
function M.set_opacity(key, value) M.update(key, { opacity = type(value) == "number" and value or false }) end

--- The colours a face draws in, resolved through its row's style: a table of
--- functions, each returning a colour, so a binding that calls one follows
--- the palette and the row. `row` is a function returning the row (or nil,
--- for the card's tiles). The accent style inverts the capsule.
function M.ink_for(row)
  local function accent_style()
    return M.style_of(row and row() or nil) == "accent"
  end
  local ink = {}
  local plain = {
    ground = function() return C.island end,
    border = function() return C.islandBorder end,
    text = C.text, muted = C.textMuted, accent = C.accent, accentText = C.accentText,
    raised = function() return C.islandSurfaceHover end,
    dim = function() return C.indicatorDim end,
  }
  local on_accent = {
    ground = C.accent,
    border = function() return morf.color("transparent") end,
    text = C.accentText,
    muted = function() return C.accentText():alpha(0.7) end,
    accent = C.accentText, accentText = C.accent,
    raised = function() return C.accentText():alpha(0.18) end,
    dim = function() return C.accentText():alpha(0.3) end,
  }
  for name, fn in pairs(plain) do
    local other = on_accent[name]
    ink[name] = function()
      if accent_style() then return other() end
      return fn()
    end
  end
  ink.red = C.red
  return ink
end

-- ---------------------------------------------------------------- spectrum --

M.spectrum_looks = {
  { id = "rounded", label = "Rounded columns" }, { id = "square", label = "Square columns" },
  { id = "segments", label = "Segments" }, { id = "dots", label = "Dots" }, { id = "wave", label = "Wave" },
}
M.spectrum_fills = {
  { id = "fade", label = "Fading to the tip" }, { id = "solid", label = "Solid" }, { id = "blend", label = "Two colours" },
}
M.spectrum_ranges = {
  reach = { from = 60, to = 400 }, bar = { from = 2, to = 24 },
  gap = { from = 1, to = 16 }, opacity = { from = 20, to = 100 },
}

local function colour_name(value, fallback)
  if value == "palette" then return value end
  for _, entry in ipairs(theme.fixed_colours) do if entry.id == value then return value end end
  return fallback
end

function M.spectrum_colour(name)
  if name == "palette" then return C.accent() end
  return morf.color(name)
end

--- A spectrum row's look, every field in range. Colours are read live, so
--- call it inside a binding.
function M.spectrum_of(row)
  local own = row or {}
  local function pick(value, list, fallback)
    for _, e in ipairs(list) do if (e.id or e) == value then return value end end
    return fallback
  end
  local function within(field, fallback)
    local r, v = M.spectrum_ranges[field], own[field]
    if type(v) == "number" then return math.max(r.from, math.min(r.to, round(v))) end
    return fallback
  end
  local c1 = colour_name(own.color, "palette")
  local c2 = colour_name(own.color2, "#ffffff")
  return {
    look = pick(own.look, M.spectrum_looks, "rounded"),
    fill = pick(own.fill, M.spectrum_fills, "fade"),
    color_name = c1, color2_name = c2,
    color = M.spectrum_colour(c1), color2 = M.spectrum_colour(c2),
    reach = within("reach", theme.spectrum.reach),
    bar = within("bar", theme.spectrum.bar),
    gap = within("gap", theme.spectrum.gap),
    lows = pick(own.lows, { "corners", "along" }, "corners"),
    peaks = own.peaks == true,
    opacity = within("opacity", 100),
  }
end

M.edges = { "left", "right", "bottom" }

function M.spectrum_on(edge)
  for _, row in ipairs(M.spectra()) do if row.edge == edge then return row end end
  return nil
end

function M.spectrum_takes(edge)
  local ok = false
  for _, e in ipairs(M.edges) do if e == edge then ok = true end end
  return ok and M.spectrum_on(edge) == nil
end

function M.free_spectrum_edge()
  for _, edge in ipairs { "bottom", "left", "right" } do
    if M.spectrum_on(edge) == nil then return edge end
  end
  return ""
end

--- Bars along a whole edge: the bottom from corner to corner, a side from
--- the top of the screen down to the bottom's bars, so the two never cross.
--- In screen coordinates.
function M.spectrum_box(row)
  local reach = M.spectrum_of(row).reach
  if row.edge == "bottom" then
    return { x = 0, y = SCREEN_H - reach, width = SCREEN_W, height = reach }
  end
  local bottom = M.spectrum_on("bottom")
  local height = bottom and SCREEN_H - M.spectrum_of(bottom).reach or SCREEN_H
  return { x = row.edge == "right" and SCREEN_W - reach or 0, y = 0, width = reach, height = height }
end

function M.add_spectrum(edge)
  local on = edge ~= nil and edge ~= "" and edge or M.free_spectrum_edge()
  if not M.spectrum_takes(on) then return "" end
  local made = { key = M.new_key("spectrum"), id = "spectrum", edge = on, screen = screen_field() }
  local out = copy(rows)
  out[#out + 1] = made
  write(out)
  return made.key
end

function M.set_spectrum(key, changes)
  local row = M.entry_of(key)
  if row and row.id == "spectrum" then M.update(key, changes) end
end

--- A square spectrum to an edge, keeping its key and look.
function M.spectrum_to_edge(key, edge)
  local widget = M.entry_of(key)
  if not widget or widget.id ~= "spectrum" or M.is_edge(widget) or not M.spectrum_takes(edge) then return end
  if M.selected:get() == key then M.selected:set("") end
  M.update(key, { edge = edge, col = false, row = false, family = false, theme = false, style = false, opacity = false })
end

--- An edge's spectrum back to the grid, at a cell or the first free one.
function M.spectrum_to_grid(key, col, row)
  local widget = M.entry_of(key)
  if not M.is_spectrum(widget) then return false end
  local family_id = M.families_for("spectrum")[1] or "4x2"
  local spot = col and M.nearest_free(col, row, family_id, "") or M.first_free(family_id, "")
  if not spot then return false end
  if M.selected:get() == key then M.selected:set("") end
  M.update(key, { edge = false, reach = false, opacity = false, col = spot.col, row = spot.row, family = family_id })
  return true
end

--- Gone under a fullscreen window, and with `spectrumOnEmpty` on any
--- workspace that has windows.
function M.spectrum_away()
  local ok, dock = pcall(require, "services.dock")
  if ok and dock and dock.signals and dock.signals.covered and dock.signals.covered:get() then return true end
  if not settings.spectrumOnEmpty then return false end
  local okh, hypr = pcall(require, "lib.hyprland")
  if not okh or not hypr.available or not hypr.available() then return false end
  local ws = hypr.state.active_workspace.id
  return ws and ws > 0 and hypr.occupied(ws) or false
end

-- ------------------------------------------------------------------- decks --
--
-- Notes on the screen edges: a row with an `edge` that is not a spectrum,
-- `{ key, id = "notes", edge, notes = { note keys }, along, takesNew,
-- screen }`. One deck per edge and screen; an empty deck is removed. Every
-- move first takes the note from wherever it was. The geometry and the
-- pointer's state over the tabs are `services/deck.lua`'s.

local function notes_service()
  local ok, notes = pcall(require, "services.notes")
  return ok and notes or nil
end

function M.is_deck(row) return M.is_edge(row) and row.id ~= "spectrum" end

--- Live, unarchived note keys on a deck row.
function M.deck_notes(row)
  local notes = notes_service()
  local out = {}
  for _, key in ipairs(row and type(row.notes) == "table" and row.notes or {}) do
    local note = notes and type(key) == "string" and notes.entry(key)
    if note and not note.archived then out[#out + 1] = key end
  end
  return out
end

--- The decks on this screen that have notes to show.
function M.decks()
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do
    if M.is_deck(row) and here(row) and #M.deck_notes(row) > 0 then out[#out + 1] = row end
  end
  return out
end

function M.deck_on(edge)
  for _, row in ipairs(M.decks()) do if row.edge == edge then return row end end
  return nil
end

--- Position along the edge, as a 0..1 fraction of the free run.
function M.along_of(row)
  local own = row and row.along
  return type(own) == "number" and math.max(0, math.min(1, own)) or 0
end

function M.set_deck_along(key, along)
  if M.is_deck(M.entry_of(key)) then M.update(key, { along = math.max(0, math.min(1, along)) }) end
end

--- Where a note is: "grid", an edge, or "" for nowhere.
function M.placement_of(note_key)
  revision:get()
  for _, row in ipairs(rows) do
    if not M.is_edge(row) and row.id == "notes" and row.note == note_key then return "grid" end
  end
  for _, row in ipairs(rows) do
    if M.is_deck(row) then
      for _, key in ipairs(M.deck_notes(row)) do
        if key == note_key then return row.edge end
      end
    end
  end
  return ""
end

-- `list` without any widget or deck entry for this note; decks left empty
-- are dropped.
local function without_note(list, note_key)
  local kept = {}
  for _, row in ipairs(list) do
    if not M.is_deck(row) then
      if not (row.id == "notes" and row.note == note_key) then kept[#kept + 1] = row end
    else
      local left = {}
      for _, key in ipairs(M.deck_notes(row)) do if key ~= note_key then left[#left + 1] = key end end
      if #left > 0 then
        local next_row = copy(row)
        next_row.notes = left
        kept[#kept + 1] = next_row
      end
    end
  end
  return kept
end

local function keep_selection(list)
  local selected = M.selected:get()
  if selected == "" then return end
  for _, row in ipairs(list) do if row.key == selected then return end end
  M.selected:set("")
end

function M.remove_note(note_key)
  local kept = without_note(rows, note_key)
  keep_selection(kept)
  write(kept)
end

--- Puts a note on an edge at `index` (1-based; nil for the end), joining the
--- deck there or starting one. A deck keeps its key and place even when this
--- was its only note, so a tab being dragged is not destroyed.
function M.place_note(note_key, edge, index)
  local notes = notes_service()
  if not notes or not notes.entry(note_key) then return end
  local ok = false
  for _, e in ipairs(M.edges) do if e == edge then ok = true end end
  if not ok then return end
  local target
  for _, row in ipairs(rows) do
    if M.is_deck(row) and here(row) and row.edge == edge then target = row end
  end
  if not target then
    local made = { key = M.new_key("notes"), id = "notes", edge = edge, notes = { note_key }, along = 0,
      screen = screen_field() }
    local kept = without_note(rows, note_key)
    kept[#kept + 1] = made
    write(kept)
    return
  end
  local left = {}
  for _, key in ipairs(M.deck_notes(target)) do if key ~= note_key then left[#left + 1] = key end end
  index = math.max(1, math.min(#left + 1, index or (#left + 1)))
  table.insert(left, index, note_key)
  local others = {}
  for _, row in ipairs(rows) do if row.key ~= target.key then others[#others + 1] = row end end
  local kept = without_note(others, note_key)
  local next_row = copy(target)
  next_row.notes = left
  kept[#kept + 1] = next_row
  write(kept)
end

--- A note to the nearest free 2x2 at or near a cell. False when there is
--- no room.
function M.note_to_grid(note_key, col, row)
  local notes = notes_service()
  if not notes or not notes.entry(note_key) then return false end
  local spot = M.nearest_free(col or 0, row or 0, "2x2", "")
  if not spot then return false end
  local made = { key = M.new_key("notes"), id = "notes", col = spot.col, row = spot.row, family = "2x2",
    note = note_key, screen = screen_field() }
  local kept = without_note(rows, note_key)
  kept[#kept + 1] = made
  write(kept)
  return true
end

--- A notes widget dragged to an edge: its note joins the deck there.
function M.note_to_edge(key, edge)
  local widget = M.entry_of(key)
  if not widget then return end
  local note = require("desktop.sources").notes.note_for(widget)
  if not note then return end
  if M.selected:get() == key then M.selected:set("") end
  M.place_note(note.key, edge)
end

--- The card's notes dropped on an edge: the newest note goes there.
function M.add_deck(edge)
  local notes = notes_service()
  local note = notes and notes.newest()
  if note then M.place_note(note.key, edge) end
end

--- A deck to another edge, merging into the deck already there.
function M.set_deck_edge(key, edge)
  local deck = M.entry_of(key)
  local ok = false
  for _, e in ipairs(M.edges) do if e == edge then ok = true end end
  if not M.is_deck(deck) or not ok or deck.edge == edge then return end
  local other = M.deck_on(edge)
  if not other then
    M.update(key, { edge = edge })
    return
  end
  local merged = M.deck_notes(other)
  local have = {}
  for _, n in ipairs(merged) do have[n] = true end
  for _, n in ipairs(M.deck_notes(deck)) do if not have[n] then merged[#merged + 1] = n end end
  if M.selected:get() == key then M.selected:set(other.key) end
  local out = {}
  for _, row in ipairs(rows) do
    if row.key == other.key then
      local next_row = copy(row)
      next_row.notes = merged
      if deck.takesNew == true then next_row.takesNew = true end
      out[#out + 1] = next_row
    elseif row.key ~= key then
      out[#out + 1] = row
    end
  end
  write(out)
end

--- The one deck new notes land on: `takesNew` on its row, cleared from
--- every other deck when set.
function M.set_takes_new(key, on)
  if not M.is_deck(M.entry_of(key)) then return end
  local out = {}
  for _, row in ipairs(rows) do
    if M.is_deck(row) then
      local next_row = copy(row)
      next_row.takesNew = (on and row.key == key) and true or nil
      out[#out + 1] = next_row
    else
      out[#out + 1] = row
    end
  end
  write(out)
end

function M.takes_new(row) return row ~= nil and row.takesNew == true end

--- A note just written lands on the deck that takes new notes, if any.
function M.note_added(note_key)
  for _, row in ipairs(M.decks()) do
    if row.takesNew == true then M.place_note(note_key, row.edge) return end
  end
end

--- A note ticked on or off a deck in its inspector.
function M.toggle_deck_note(key, note_key)
  local deck = M.entry_of(key)
  if not M.is_deck(deck) then return end
  local list = M.deck_notes(deck)
  for _, n in ipairs(list) do
    if n == note_key then
      local left = {}
      for _, other in ipairs(list) do if other ~= note_key then left[#left + 1] = other end end
      if #left == 0 then M.remove(key) else M.update(key, { notes = left }) end
      return
    end
  end
  M.place_note(note_key, deck.edge)
end

-- A note archived or deleted leaves the desk; a new one lands on the deck
-- that takes new notes.
do
  local notes = notes_service()
  if notes then
    notes.on_removed[#notes.on_removed + 1] = function(key)
      if M.placement_of(key) ~= "" then M.remove_note(key) end
    end
    notes.on_added[#notes.on_added + 1] = function(key) M.note_added(key) end
  end
end

-- ---------------------------------------------------------------- pictures --

M.picture_types = { png = true, jpg = true, jpeg = true, webp = true, bmp = true }

--- Takes a path or a file:// URL; anything that is not a picture is refused.
function M.set_picture(key, url)
  local path = tostring(url or "")
  if path:sub(1, 7) == "file://" then
    path = path:sub(8):gsub("%%(%x%x)", function(h) return string.char(tonumber(h, 16)) end)
  end
  local ext = path:match("%.([^./]+)$")
  if not ext or not M.picture_types[ext:lower()] then return false end
  M.update(key, { picture = path })
  return true
end

function M.picture_of(row)
  return row and type(row.picture) == "string" and row.picture or ""
end

--- A row's picture in a window of the shell's own (the original handed it
--- to imv): fitted, the wheel zooms, a drag pans, Escape closes.
function M.open_picture(row)
  local path = M.picture_of(row)
  if path == "" then return end
  local caption = row and type(row.caption) == "string" and row.caption ~= "" and row.caption or nil
  require("desktop.viewer").open(path, caption or path:match("([^/]+)$"))
end

-- ----------------------------------------------------------------- editing --

-- Arranging: not kept across sessions.
M.editing = morf.signal("impasto.desk.editing", false)
M.selected = morf.signal("impasto.desk.selected", "")   -- the inspector's widget
M.dragging = morf.signal("impasto.desk.dragging", "")   -- the widget in hand
M.picking = morf.signal("impasto.desk.picking", "")     -- the photo whose picker is open
M.typing = morf.signal("impasto.desk.typing", false)    -- a caption is being typed
-- The drop preview: a cell and family, or an empty family for none.
M.landing_col = morf.signal("impasto.desk.landing.col", 0)
M.landing_row = morf.signal("impasto.desk.landing.row", 0)
M.landing_family = morf.signal("impasto.desk.landing.family", "")
-- The card's rectangle on the board while arranging; a widget dropped on it
-- is removed. Written by the tray.
M.tray_box = nil
-- The right-click menu: open, on which key ("" for the background), where.
M.menu_open = morf.signal("impasto.desk.menu.open", false)
M.menu_key = morf.signal("impasto.desk.menu.key", "")
M.menu_x = morf.signal("impasto.desk.menu.x", 0)
M.menu_y = morf.signal("impasto.desk.menu.y", 0)
-- The note whose deck tab the menu was opened on, "" for a widget's menu.
M.menu_note = morf.signal("impasto.desk.menu.note", "")

function M.set_landing(spot, family_id)
  if not spot then M.landing_family:set("") return end
  M.landing_col:set(spot.col)
  M.landing_row:set(spot.row)
  M.landing_family:set(family_id)
end

function M.over_tray(x, y)
  local box = M.tray_box
  return box ~= nil and x >= box.x and x <= box.x + box.width and y >= box.y and y <= box.y + box.height
end

-- None or one row: the arranging board is built while it has one.
M.board_model = morf.list_model({})

function M.edit(on)
  local was = M.editing:get()
  M.editing:set(on and true or false)
  if was ~= (on and true or false) then
    morf.timer(1, function()
      M.board_model:replace(M.editing:get() and { { id = "board" } } or {}, "id")
    end, false)
  end
  M.menu_open:set(false)
  M.typing:set(false)
  M.picking:set("")
  if not on then
    M.dragging:set("")
    M.selected:set("")
    M.set_landing(nil)
  end
end

function M.select(key)
  M.typing:set(false)
  if M.picking:get() ~= key then M.picking:set("") end
  M.selected:set(key or "")
end

function M.open_menu(key, x, y, note)
  M.menu_key:set(key or "")
  M.menu_note:set(note or "")
  M.menu_x:set(x)
  M.menu_y:set(y)
  if M.on_menu_open then M.on_menu_open() end
  M.menu_open:set(true)
end

function M.close_menu() M.menu_open:set(false) end

-- Arranging lasts while the screen stays as it was: another workspace, a
-- window opening or going fullscreen ends it, as in the original.
do
  local ok, hypr = pcall(require, "lib.hyprland")
  if ok and hypr and hypr.on then
    for _, event in ipairs { "workspacev2", "activespecialv2", "openwindow" } do
      hypr.on(event, function() if M.editing:get() then M.edit(false) end end)
    end
    hypr.on("fullscreen", function(on) if on and M.editing:get() then M.edit(false) end end)
  end
end

-- ------------------------------------------------------------------- start --

rows = normalise(settings.desktopWidgets)

-- External changes (another process, a reset, a hand edit) are read back;
-- our own writes echo with the same value.
morf.effect("impasto.desk.settings", function()
  local list = settings.desktopWidgets
  if writing then return end
  local fresh = normalise(list)
  local same = #fresh == #rows
  if same then
    for i, row in ipairs(fresh) do
      local a, b = morf.json.encode(row), morf.json.encode(rows[i])
      if a ~= b then same = false break end
    end
  end
  if not same then
    rows = fresh
    bump()
    M.queue_sync()
  end
end)

-- A theme change redraws every widget, and shrinks any whose family the
-- new theme has no face for.
local seen_theme = settings.desktopTheme
morf.effect("impasto.desk.theme", function()
  local now = settings.desktopTheme
  if now == seen_theme then return end
  seen_theme = now
  morf.timer(1, function() M.conform() M.queue_sync() end, false)
end)

-- The board changing size (the dock's band) re-lays every face.
local seen_stride = nil
morf.effect("impasto.desk.board", function()
  local g = M.grid()
  local stamp = g.stride .. ":" .. g.columns .. ":" .. g.rows
  if stamp == seen_stride then return end
  seen_stride = stamp
  M.queue_sync()
end)

sync()

return M
