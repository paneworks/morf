-- The control centre's layout: the row of buttons along the top and the
-- grid of blocks under it. Nothing here draws; the panel reads it.
--
-- Port of ControlsService.qml. The row has the session actions on the left
-- (fixed) and user-chosen buttons ("doors") on the right, each opening
-- another island panel or the settings.
--
-- The grid is `theme.centre_columns` x `theme.centre_rows` cells. Blocks span
-- whole cells, only offer sizes they have a face for, and never overlap: a
-- drop on an occupied cell moves to the nearest free fit, or is cancelled.
-- Toggles are a single block holding its own paged list of tiles.
--
-- Stored lists that are absent mean "default", so catalogue additions reach
-- anyone who never customised the layout.

local settings = require("services.settings")
local theme = require("theme")

local M = {}

-- ------------------------------------------------------------------ doors --

--   id      settings key
--   icon    glyph on the button
--   label   display name
--   detail  subtitle in the launcher's `>` mode; also searched
--   panel   island panel it opens, or "" for the settings window
M.doors = {
  { id = "stats", icon = "󰕬", label = "System statistics", detail = "Processor, memory, disks, the network", panel = "stats" },
  { id = "settings", icon = "󰒓", label = "Settings", detail = "The whole desk, in a window", panel = "" },
  { id = "pet", icon = "󰏩", label = "Pet", detail = "The creature living on the bar", panel = "pet" },
  { id = "games", icon = "󰊗", label = "Games", detail = "The arcade", panel = "games" },
  { id = "notes", icon = "󰎞", label = "Notes", detail = "The deck of sticky notes", panel = "notes" },
  { id = "board", icon = "󰄲", label = "Task board", detail = "To do, doing, done", panel = "board" },
  { id = "overview", icon = "󰕰", label = "Workspace overview", detail = "Every workspace side by side", panel = "overview" },
  { id = "launcher", icon = "󰍉", label = "Launcher", detail = "Where you already are", panel = "launcher" },
  { id = "appearance", icon = "󰏘", label = "Appearance", detail = "The wallpaper and the palette", panel = "appearance" },
  { id = "session", icon = "󰐥", label = "Session menu", detail = "Lock, log out, suspend, restart, off", panel = "session" },
  { id = "keys", icon = "󰌌", label = "Keys", detail = "Every shortcut, on one sheet", panel = "keys" },
  { id = "packages", icon = "󰏗", label = "Packages", detail = "Updates, what is installed, the AUR", panel = "packages" },
}

M.default_buttons = { "stats", "settings", "pet", "games", "notes", "board" }

local function stored(value) return type(value) == "table" end

function M.door(id)
  for _, entry in ipairs(M.doors) do
    if entry.id == id then return entry end
  end
  return nil
end

--- Door ids on the row, left to right. Unknown ids are skipped.
function M.buttons()
  local kept = settings.centreButtons
  local list = stored(kept) and kept or M.default_buttons
  local out = {}
  for _, id in ipairs(list) do
    if M.door(id) then out[#out + 1] = id end
  end
  return out
end

function M.shown_doors()
  local out = {}
  for _, id in ipairs(M.buttons()) do out[#out + 1] = M.door(id) end
  return out
end

function M.shows_door(id)
  for _, each in ipairs(M.buttons()) do if each == id then return true end end
  return false
end

--- A copy of `list` with `id` moved by `delta` places.
function M.moved(list, id, delta)
  local next = {}
  local at
  for index, each in ipairs(list) do
    next[index] = each
    if each == id then at = index end
  end
  if not at then return next end
  local to = at + delta
  if to < 1 or to > #next then return next end
  table.remove(next, at)
  table.insert(next, to, id)
  return next
end

function M.set_door(id, on)
  local list = {}
  for _, each in ipairs(M.buttons()) do if each ~= id then list[#list + 1] = each end end
  if on then list[#list + 1] = id end
  settings.set("centreButtons", list)
end

function M.move_door(id, delta) settings.set("centreButtons", M.moved(M.buttons(), id, delta)) end

-- ------------------------------------------------------------------ tiles --

-- One toggle tile, bound live to its service: every field but `key`,
-- `label`, `panel` and `closes` is a function a binding calls. `closes`
-- marks one-shot actions that need the panel closed first.
local function lazy(name)
  return function() return require(name) end
end
local network, bluetooth, audio, system, notify, osd =
  lazy("services.network"), lazy("services.bluetooth"), lazy("services.audio"),
  lazy("services.system"), lazy("services.notifications"), lazy("services.osd")

--- Airplane mode is Wi-Fi and Bluetooth both off; neither service owns it.
function M.airborne()
  return not network().radio_on() and not bluetooth().enabled()
end

local yes = function() return true end
local no = function() return false end

M.tile_catalogue = {
  { key = "wifi", label = "Wi-Fi", panel = "wifi",
    icon = function() return network().icon() end,
    detail = function() return network().connection_name() end,
    active = function() return network().radio_on() end,
    available = function() return network().available() end,
    expandable = yes,
    action = function() network().toggle_wifi() end },
  { key = "bluetooth", label = "Bluetooth", panel = "bluetooth",
    icon = function() return bluetooth().icon() end,
    detail = function() return bluetooth().summary() end,
    active = function() return bluetooth().enabled() end,
    available = function() return bluetooth().available() end,
    expandable = function() return bluetooth().available() end,
    -- Toggling the radio swaps the audio sink; no volume OSD for that.
    action = function() osd().suppress_audio() bluetooth().toggle() end },
  { key = "power", label = "Power",
    icon = function() return "󰓅" end,
    detail = function()
      local profile = system().power_profile()
      return profile ~= "" and profile or "Unknown"
    end,
    active = function() return system().performance_mode() end,
    available = function() return system().ready() end,
    action = function() system().toggle("power-profile") end },
  { key = "focus", label = "Focus",
    icon = function() return settings.doNotDisturb and "󰂛" or "󰂚" end,
    detail = function() return settings.doNotDisturb and "Silenced" or "Notifying" end,
    active = function() return settings.doNotDisturb end,
    action = function() notify().toggle_dnd() end },
  { key = "microphone", label = "Microphone",
    icon = function() return audio().source_icon() end,
    detail = function() return audio().source_muted() and "Muted" or "Live" end,
    active = function() return not audio().source_muted() end,
    available = function() return audio().source_ready() end,
    action = function() audio().toggle_source_mute() end },
  { key = "airplane", label = "Airplane",
    icon = function() return M.airborne() and "󰀝" or "󰀞" end,
    detail = function() return M.airborne() and "Radios off" or "Radios on" end,
    active = function() return M.airborne() end,
    action = function()
      local turn_on = M.airborne()
      osd().suppress_audio()
      network().set_wifi(turn_on)
      if bluetooth().available() and bluetooth().enabled() ~= turn_on then bluetooth().toggle() end
    end },
  { key = "nightlight", label = "Night light",
    icon = function() return "󰖔" end,
    detail = function() return settings.nightLight and (settings.nightTemperature .. " K") or "Off" end,
    active = function() return settings.nightLight end,
    available = no },
  { key = "output", label = "Output",
    icon = function() return audio().icon() end,
    detail = function() return audio().muted() and "Muted" or (audio().volume() .. "%") end,
    active = function() return not audio().muted() end,
    available = function() return audio().ready() end,
    action = function() audio().toggle_mute() end },
  { key = "dock", label = "Dock",
    icon = function() return "󱂠" end,
    detail = function() return settings.dockEnabled and "Shown" or "Hidden" end,
    active = function() return settings.dockEnabled end,
    action = function() settings.set("dockEnabled", not settings.dockEnabled) end },
  { key = "notch", label = "Notch",
    icon = function() return "󰌢" end,
    detail = function() return settings.islandAttached and "Attached" or "Floating" end,
    active = function() return settings.islandAttached end,
    action = function() settings.set("islandAttached", not settings.islandAttached) end },
  { key = "shadow", label = "Shadows",
    icon = function() return "󰘷" end,
    detail = function() return settings.windowShadow and "On the windows" or "Off" end,
    active = function() return settings.windowShadow end,
    action = function() settings.set("windowShadow", not settings.windowShadow) end },
  { key = "screenshot", label = "Capture", closes = true,
    icon = function() return "󰹑" end, detail = function() return "Photo or video" end,
    available = no },
  { key = "annotate", label = "Annotate", closes = true,
    icon = function() return "󰏫" end, detail = function() return "A region, in satty" end,
    available = no },
  { key = "text", label = "Read text", closes = true,
    icon = function() return "󱄽" end, detail = function() return "A region, to the clipboard" end,
    available = no },
  { key = "picker", label = "Colour", closes = true,
    icon = function() return "󰈋" end, detail = function() return "A pixel" end,
    available = no },
  { key = "record", label = "Record", closes = true,
    icon = function() return "󰕧" end, detail = function() return "The screen" end,
    available = no },
  { key = "clearClipboard", label = "Clear clipboard", closes = true,
    icon = function() return "󰅍" end, detail = function() return "Nothing kept" end,
    available = no },
}

local tiles_by_key = {}
for _, tile in ipairs(M.tile_catalogue) do
  tile.icon = tile.icon or function() return "" end
  tile.detail = tile.detail or function() return "" end
  tile.active = tile.active or no
  tile.available = tile.available or yes
  tile.expandable = tile.expandable or no
  tile.panel = tile.panel or ""
  tiles_by_key[tile.key] = tile
end

--- Lets the service behind a tile (capture, the picker, the recorder, the
--- night light, the clipboard) wire it once it is ported: its fields
--- replace the placeholder's.
function M.define_tile(key, fields)
  local tile = tiles_by_key[key]
  if not tile then
    tile = { key = key, label = key, panel = "", icon = function() return "" end,
      detail = function() return "" end, active = no, available = yes, expandable = no }
    M.tile_catalogue[#M.tile_catalogue + 1] = tile
    tiles_by_key[key] = tile
  end
  for name, value in pairs(fields) do tile[name] = value end
end

M.editing = morf.signal("impasto.controls.editing", false)

--- Inert while arranging or when its service is unavailable.
function M.activate(tile)
  if M.editing:get() or not tile.available() then return end
  if tile.action then tile.action() end
end

M.default_toggles = { "wifi", "bluetooth", "power", "focus", "microphone", "airplane" }

function M.tile_of(key) return tiles_by_key[key] end

local function known_tiles(list)
  local out = {}
  for _, key in ipairs(list) do if tiles_by_key[key] then out[#out + 1] = key end end
  return out
end

--- Fallback tiles for a toggles block without its own list.
function M.toggle_keys()
  local kept = settings.centreToggles
  return known_tiles(stored(kept) and kept or M.default_toggles)
end

-- ----------------------------------------------------------------- blocks --

--   id     settings key, and what the block face draws
--   sizes  columns x rows it has a face for, smallest first
-- Cells are wider than tall, so 1x2 and 2x4 are the square sizes.
M.catalogue = {
  { id = "toggles", name = "Toggles", icon = "󰨚", sizes = { "2x2", "2x3", "2x4", "3x2", "3x3", "4x2", "4x3" } },
  { id = "volume", name = "Volume", icon = "󰕾", sizes = { "2x1", "3x1", "4x1" } },
  { id = "brightness", name = "Brightness", icon = "󰃠", sizes = { "2x1", "3x1", "4x1" } },
  { id = "appearance", name = "Appearance", icon = "󰏘", sizes = { "2x2", "2x3", "2x4", "3x3", "4x2" } },
  { id = "media", name = "Media", icon = "󰝚", sizes = { "2x2", "2x3", "3x2", "4x2" } },
  { id = "weather", name = "Weather", icon = "󰖐", sizes = { "2x1", "2x2", "2x3", "4x2" } },
  { id = "calendar", name = "Calendar", icon = "󰃭", sizes = { "2x3", "2x4", "3x4" } },
  { id = "notifications", name = "Notifications", icon = "󰂚", sizes = { "2x4", "2x6", "2x8", "3x8" } },
  { id = "impasto", name = "impasto", icon = "󰏘", sizes = { "1x2", "2x2", "2x4" } },
  { id = "pet", name = "Pet", icon = "󰏩", sizes = { "2x2", "2x3" } },
  { id = "clock", name = "Clock", icon = "󰥔", sizes = { "1x2", "2x2", "2x4" } },
  { id = "games", name = "Games", icon = "󰊗", sizes = { "2x1", "2x2" } },
  { id = "notes", name = "Notes", icon = "󰎞", sizes = { "1x2", "2x2", "2x4" } },
  { id = "tasks", name = "Tasks", icon = "󰄲", sizes = { "1x2", "2x2", "2x3", "2x4" } },
}

-- Toggles, sliders and appearance on the left; media, weather and calendar
-- in the middle; notifications on the right.
M.default_blocks = {
  { id = "toggles", col = 0, row = 0, size = "2x3" },
  { id = "volume", col = 0, row = 3, size = "2x1" },
  { id = "brightness", col = 0, row = 4, size = "2x1" },
  { id = "appearance", col = 0, row = 5, size = "2x3" },
  { id = "media", col = 2, row = 0, size = "2x2" },
  { id = "weather", col = 2, row = 2, size = "2x3" },
  { id = "calendar", col = 2, row = 5, size = "2x3" },
  { id = "notifications", col = 4, row = 0, size = "2x8" },
}

function M.entry(id)
  for _, item in ipairs(M.catalogue) do if item.id == id then return item end end
  return nil
end

function M.sizes_for(id)
  local item = M.entry(id)
  return item and item.sizes or { "2x2" }
end

function M.offers(id, size)
  for _, each in ipairs(M.sizes_for(id)) do if each == size then return true end end
  return false
end

--- "2x3" -> 2, 3.
function M.parse(size)
  local cols, rows = tostring(size or ""):match("^(%d+)x(%d+)$")
  if not cols then return 2, 2 end
  return tonumber(cols), tonumber(rows)
end

--- A size the block does not offer falls back to its smallest.
function M.size_of(block)
  if block and M.offers(block.id, block.size) then return block.size end
  return M.sizes_for(block and block.id or "")[1]
end

function M.label(size)
  local cols, rows = M.parse(size)
  return cols .. "×" .. rows
end

-- ------------------------------------------------------------------ board --

-- Computed from the grid rather than measured, so the island can size
-- itself before the panel exists.
M.columns = theme.centre_columns
M.rows = theme.centre_rows
M.board_width = M.columns * theme.centre_cell_width + (M.columns - 1) * theme.centre_gutter
M.board_height = M.rows * theme.centre_cell_height + (M.rows - 1) * theme.centre_gutter
M.row_height = 28
M.row_gap = 14
M.panel_width = M.board_width + 2 * theme.panel_padding
M.panel_height = M.board_height + M.row_height + M.row_gap + 2 * theme.panel_padding

function M.offset_x(col) return col * theme.centre_stride_x end
function M.offset_y(row) return row * theme.centre_stride_y end

function M.pixels(size)
  local cols, rows = M.parse(size)
  return cols * theme.centre_cell_width + (cols - 1) * theme.centre_gutter,
    rows * theme.centre_cell_height + (rows - 1) * theme.centre_gutter
end

--- A stored row as a rectangle, clamped onto the board.
function M.geometry(block)
  local size = M.size_of(block)
  local cols, rows = M.parse(size)
  local w, h = M.pixels(size)
  local last_col = math.max(0, M.columns - cols)
  local last_row = math.max(0, M.rows - rows)
  return {
    x = M.offset_x(math.max(0, math.min(last_col, block.col or 0))),
    y = M.offset_y(math.max(0, math.min(last_row, block.row or 0))),
    width = w, height = h,
  }
end

-- ----------------------------------------------------------------- layout --

-- Rows are keyed by `key`, not by block id, so a block can appear twice.
local function normalise(list)
  local rows = {}
  for _, kept in ipairs(list or {}) do
    if type(kept) == "table" and M.entry(kept.id) then
      local row = {}
      for k, v in pairs(kept) do row[k] = v end
      row.key = row.key or row.id
      rows[#rows + 1] = row
    end
  end
  return rows
end

local function read()
  local kept = settings.get("centreBlocks")
  return normalise(stored(kept) and kept or M.default_blocks)
end

local blocks = {}
local revision = 0
M.revision = morf.signal("impasto.controls.revision", 0)
-- Counted here rather than read back, so the effect below that bumps it
-- does not also follow it.
local function bump()
  revision = revision + 1
  M.revision:set(revision)
end
-- The keys, as a list model the panel's Repeater follows: it changes only
-- when a block is added or removed, so moving one does not rebuild it.
M.keys = morf.list_model({})

local function sync_keys()
  local rows = {}
  for _, block in ipairs(blocks) do rows[#rows + 1] = { key = block.key } end
  M.keys:replace(rows, "key")
end

local saving = false
local function write(next)
  blocks = next
  bump()
  sync_keys()
  if not saving then
    saving = true
    morf.timer(120, function()
      saving = false
      settings.set("centreBlocks", blocks)
    end, false)
  end
end

--- The rows; a binding that calls this follows every change.
function M.blocks()
  M.revision:get()
  return blocks
end

function M.entry_of(key)
  M.revision:get()
  for _, block in ipairs(blocks) do if block.key == key then return block end end
  return nil
end

function M.count_of(id)
  local n = 0
  for _, block in ipairs(M.blocks()) do if block.id == id then n = n + 1 end end
  return n
end

function M.placed(id) return M.count_of(id) > 0 end

-- Settings written elsewhere (Settings, a reset) reach the grid, except the
-- echo of this file's own debounced write.
morf.effect("impasto.controls.read", function()
  local fresh = read()
  if saving then return end
  blocks = fresh
  bump()
  sync_keys()
end)

--- Tiles for one toggles block, from its own `toggles` field.
function M.toggle_keys_of(key)
  local block = M.entry_of(key)
  local own = block and block.toggles
  return stored(own) and known_tiles(own) or M.toggle_keys()
end

function M.tiles_of(key)
  local out = {}
  for _, tile in ipairs(M.toggle_keys_of(key)) do out[#out + 1] = tiles_by_key[tile] end
  return out
end

function M.shows_tile_in(key, tile)
  for _, each in ipairs(M.toggle_keys_of(key)) do if each == tile then return true end end
  return false
end

-- -------------------------------------------------------------- collisions --

function M.overlaps(col, row, size, except)
  local cols, rows = M.parse(size)
  for _, other in ipairs(blocks) do
    if other.key ~= except then
      local their_cols, their_rows = M.parse(M.size_of(other))
      local oc, orow = other.col or 0, other.row or 0
      if col < oc + their_cols and oc < col + cols and row < orow + their_rows and orow < row + rows then
        return true
      end
    end
  end
  return false
end

function M.on_board(col, row, size)
  local cols, rows = M.parse(size)
  return col >= 0 and row >= 0 and col + cols <= M.columns and row + rows <= M.rows
end

function M.free(col, row, size, except)
  return M.on_board(col, row, size) and not M.overlaps(col, row, size, except)
end

--- Nearest free cell by squared distance, or nil if the shape fits nowhere.
function M.nearest_free(col, row, size, except)
  if M.free(col, row, size, except) then return { col = col, row = row } end
  local best, best_distance = nil, math.huge
  for c = 0, M.columns - 1 do
    for r = 0, M.rows - 1 do
      if M.free(c, r, size, except) then
        local distance = (c - col) ^ 2 + (r - row) ^ 2
        if distance < best_distance then best, best_distance = { col = c, row = r }, distance end
      end
    end
  end
  return best
end

function M.first_free(size, except)
  for r = 0, M.rows - 1 do
    for c = 0, M.columns - 1 do
      if M.free(c, r, size, except) then return { col = c, row = r } end
    end
  end
  return nil
end

function M.cell_x(position) return math.floor(position / theme.centre_stride_x + 0.5) end
function M.cell_y(position) return math.floor(position / theme.centre_stride_y + 0.5) end

-- ---------------------------------------------------------------- writing --

local function copy_with(block, changes)
  local out = {}
  for k, v in pairs(block) do out[k] = v end
  for k, v in pairs(changes) do out[k] = v end
  return out
end

function M.update(key, changes)
  local next = {}
  for index, block in ipairs(blocks) do
    next[index] = block.key == key and copy_with(block, changes) or block
  end
  write(next)
end

function M.toggle_tile_in(key, tile)
  local list, next, removed = M.toggle_keys_of(key), {}, false
  for _, each in ipairs(list) do
    if each == tile then removed = true else next[#next + 1] = each end
  end
  if not removed then next[#next + 1] = tile end
  M.update(key, { toggles = next })
end

function M.move_tile_in(key, tile, delta)
  M.update(key, { toggles = M.moved(M.toggle_keys_of(key), tile, delta) })
end

--- "<id>-<n>" with the first free n.
function M.new_key(id)
  local n = 1
  while M.entry_of(id .. "-" .. n) do n = n + 1 end
  return id .. "-" .. n
end

--- Adds a block at its smallest size, at or near a cell, else at the first
--- free one. Returns the key, or "" if there is no room.
function M.add(id, col, row)
  if not M.entry(id) then return "" end
  local size = M.sizes_for(id)[1]
  local spot = (col and col >= 0) and M.nearest_free(col, row, size, "") or M.first_free(size, "")
  if not spot then return "" end
  local key = M.new_key(id)
  local next = {}
  for index, block in ipairs(blocks) do next[index] = block end
  next[#next + 1] = { key = key, id = id, col = spot.col, row = spot.row, size = size }
  write(next)
  return key
end

M.selected = morf.signal("impasto.controls.selected", "")
M.dragging = morf.signal("impasto.controls.dragging", "")
-- Drop preview: a cell and a size while something is dragged.
M.landing_col = morf.signal("impasto.controls.landing.col", -1)
M.landing_row = morf.signal("impasto.controls.landing.row", -1)
M.landing_size = morf.signal("impasto.controls.landing.size", "")

function M.set_landing(spot, size)
  M.landing_col:set(spot and spot.col or -1)
  M.landing_row:set(spot and spot.row or -1)
  M.landing_size:set(spot and size or "")
end

function M.remove(key)
  local next = {}
  for _, block in ipairs(blocks) do if block.key ~= key then next[#next + 1] = block end end
  write(next)
  if M.selected:get() == key then M.selected:set("") end
end

--- Drop: the target cell or the nearest free fit; otherwise unchanged.
function M.place(key, col, row)
  local block = M.entry_of(key)
  if not block then return end
  local spot = M.nearest_free(col, row, M.size_of(block), key)
  if spot then M.update(key, { col = spot.col, row = spot.row }) end
end

--- Resizes in place, moving as little as possible.
function M.set_size(key, size)
  local block = M.entry_of(key)
  if not block or not M.offers(block.id, size) or M.size_of(block) == size then return end
  local spot = M.nearest_free(block.col or 0, block.row or 0, size, key)
  if spot then M.update(key, { size = size, col = spot.col, row = spot.row }) end
end

--- The wheel while arranging: the next size in `delta`'s direction that fits.
function M.cycle_size(key, delta)
  local block = M.entry_of(key)
  if not block then return end
  local sizes = M.sizes_for(block.id)
  local at = 1
  for index, size in ipairs(sizes) do if size == M.size_of(block) then at = index end end
  for step = 1, #sizes - 1 do
    local next = sizes[((at - 1 + delta * step) % #sizes) + 1]
    if M.nearest_free(block.col or 0, block.row or 0, next, key) then
      M.set_size(key, next)
      return
    end
  end
end

-- The resize handle: the smallest offered footprint containing the pointer
-- pulled back by `handle_inset`, else the nearest corner.
M.handle_inset = 0.35

function M.size_nearest(id, cols, rows)
  local offered = M.sizes_for(id)
  local x, y = cols - M.handle_inset, rows - M.handle_inset
  local best, best_area = "", math.huge
  for _, size in ipairs(offered) do
    local c, r = M.parse(size)
    if x <= c and y <= r and c * r < best_area then best, best_area = size, c * r end
  end
  if best ~= "" then return best end
  local best_distance = math.huge
  for _, size in ipairs(offered) do
    local c, r = M.parse(size)
    local distance = (c - cols) ^ 2 + (r - rows) ^ 2
    if distance < best_distance then best, best_distance = size, distance end
  end
  return best
end

--- Back to the shipped layout.
function M.restore() settings.reset("centreBlocks") end

-- -------------------------------------------------------------- arranging --

-- Deliberately not kept across sessions.
function M.edit(on)
  M.editing:set(on and true or false)
  M.selected:set("")
  if not on then
    M.dragging:set("")
    M.set_landing(nil)
  end
end

return M
