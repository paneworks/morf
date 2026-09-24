-- The bar's catalogue: what each module is, its glyph and figure, whether
-- this machine can show it, and which detail is open.
--
-- Port of ModuleService.qml. A piece on the bar is a module or a button. A
-- module tells you something: it always has a figure, and a click opens
-- its detail in the island. A button does something: a symbol with no
-- figure that opens a panel or runs an action. Detail sizes are declared
-- here because the island has to reach that size before the detail exists.
--
-- The original's switch statements over every module became a table of
-- providers: a module file says what it reads with `modules.define(id, {
-- glyph, value, tint, has, runs, chip, detail, size, limit })`, so a
-- module ported later (weather, timer, stats...) plugs in without an edit
-- here. What is left here is the catalogue and the rules that hold across
-- modules.

local settings = require("services.settings")
local theme = require("theme")

local M = {}

--   bar      whether it can go on the bar
--   desk     false for the one module with no desktop face
--   width    detail size
--   height
M.catalogue = {
  { id = "media", name = "Media", bar = true, width = 380, height = 150 },
  { id = "timer", name = "Timer", bar = true, width = 348, height = 116 },
  { id = "claude", name = "Claude", bar = true, width = 356, height = 150 },
  { id = "battery", name = "Battery", bar = true, width = 320, height = 132 },
  { id = "volume", name = "Volume", bar = true, width = 340, height = 116 },
  { id = "brightness", name = "Brightness", bar = true, width = 340, height = 100 },
  { id = "network", name = "Network", bar = true, width = 356, height = 132 },
  { id = "bluetooth", name = "Bluetooth", bar = true, width = 356, height = 132 },
  { id = "notifications", name = "Notifications", bar = true, desk = false, width = 380, height = 340 },
  { id = "weather", name = "Weather", bar = true, width = 380, height = 150 },
  { id = "github", name = "GitHub", bar = false, width = 380, height = 158 },
  { id = "stats", name = "System", bar = true, width = 380, height = 148 },
  { id = "updates", name = "Updates", bar = true, width = 356, height = 132 },
  { id = "pet", name = "Pet", bar = true, width = 380, height = 172 },
  { id = "games", name = "Games", bar = false, width = 380, height = 150 },
  { id = "calendar", name = "Calendar", bar = true, width = 340, height = 330 },
  { id = "notes", name = "Notes", bar = false, width = 356, height = 150 },
  { id = "tasks", name = "Tasks", bar = false, width = 356, height = 150 },
  { id = "photo", name = "Photo", bar = false, width = 356, height = 150 },
  { id = "spectrum", name = "Spectrum", bar = false, width = 0, height = 0 },
  { id = "clock", name = "Clock", bar = false, width = 150, height = 32 },
}

local by_id = {}
for _, item in ipairs(M.catalogue) do by_id[item.id] = item end

--- The catalogue row; the clock's size follows the settings.
function M.entry(id)
  local item = by_id[id] or M.catalogue[1]
  if item.id == "clock" then
    return { id = "clock", name = "Clock", bar = false,
      width = settings.clockShowsDate and 240 or 150, height = theme.capsule_height() }
  end
  return item
end

-- ------------------------------------------------------------- providers --

M.providers = {}

--- What a module reads and draws. Fields, all optional:
---   glyph()   the symbol          value()  the figure (never empty)
---   tint()    the symbol's colour has()    whether this machine can show it
---   runs()    running, for `when = "running"` and the activities
---   limit     figure width before it elides
---   chip()    the ring face's contents, a node `capsule_height` square
---   detail()  the detail card, a node filling the island
---   size()    the detail's size when it is not the catalogue's
---   chip_mark() the mark in the icon shape, for a module with no font glyph
---             (Claude, the pet); a node `capsule_height * 0.44` square
---   watch(on) a reading that polls: called true while a piece on the bar
---             shows the module, and false when it goes (`M.watch`)
---   mark(), figure()  the two halves of a running activity on the island
function M.define(id, provider)
  local kept = M.providers[id] or {}
  for key, value in pairs(provider) do kept[key] = value end
  M.providers[id] = kept
end

local function call(id, field, ...)
  local provider = M.providers[id]
  local fn = provider and provider[field]
  if type(fn) == "function" then return fn(...) end
  return nil
end

-- ---------------------------------------------------------------- buttons --

-- Every door of the control centre can go on the bar, and so can the
-- control centre itself and the capture surface. Alone in a capsule, a
-- button is drawn as a circle.
M.button_ids = { "launcher", "overview", "controls", "capture",
  "appearance", "notes", "board", "games", "keys", "packages", "settings", "session" }

-- A reading that polls keeps polling while a piece on the bar shows it, in
-- either shape (ModuleService.watch). The chip calls this as it is built
-- and as it goes; the module's own `watch(on)` subscribes and releases.
function M.watch(id, on)
  local provider = M.providers[id]
  if provider and type(provider.watch) == "function" then provider.watch(on and true or false) end
end

-- Buttons cannot see the island, so they ask here; the bar's island answers.
M.panel_requests = morf.signal("impasto.modules.panel_request", "")
M.settings_requests = morf.signal("impasto.modules.settings_request", 0)

function M.toggle_panel(panel)
  require("bar.island_state").toggle(panel)
end

--- The settings window is not an island panel; whoever owns it listens.
function M.request_settings()
  M.settings_requests:set(M.settings_requests:get() + 1)
end

function M.buttons()
  local out = {}
  local controls = require("services.controls")
  for _, id in ipairs(M.button_ids) do
    if id == "controls" then
      out[id] = { name = "Control centre", glyph = "󰨚", panel = "controls" }
    elseif id == "capture" then
      out[id] = { name = "Capture", glyph = "󰹑", panel = "capture" }
    else
      local door = controls.door(id)
      if door then
        out[id] = door.panel ~= ""
          and { name = door.label, glyph = door.icon, panel = door.panel }
          or { name = door.label, glyph = door.icon, action = M.request_settings }
      end
    end
  end
  return out
end

function M.is_button(id) return M.buttons()[id] ~= nil end

--- Whether an id from a saved layout is still a piece.
function M.placeable(id)
  if id == "workspaces" or id == "split" or M.is_button(id) then return true end
  local item = by_id[id]
  return item ~= nil and item.bar
end

--- The open panel, for a button that stays lit while its panel is open.
function M.shown_panel() return require("bar.island_state").open_panel() end

-- ------------------------------------------------------------ chip shape --

-- Modules whose reading fills from empty to full have a ring face. A state
-- or a count has nothing to fill, so it keeps its symbol in either shape.
M.ringed = { media = true, timer = true, claude = true, battery = true, volume = true,
  brightness = true, stats = true, pet = true }

--- A piece's own shape when it has one, the bar's when it does not.
function M.shape_of(id, own)
  local chosen = (own and own ~= "") and own or settings.chipShape
  return (chosen == "ring" and M.ringed[id]) and "ring" or "icon"
end

function M.figure_of(own)
  return (own and own ~= "") and own or settings.chipFigure
end

-- ------------------------------------------------------ glyph and figure --

-- Symbols of modules nobody has defined yet, so a chip still has a mark.
local fallback_glyphs = {
  weather = "󰖐", updates = "󰏖", media = "󰎇", timer = "󰔛", stats = "󰍛",
  calendar = "󰃭", notifications = "󰂚",
}

function M.glyph_of(id)
  return call(id, "glyph") or fallback_glyphs[id] or ""
end

--- Never empty, so "always show the figure" applies to every module.
function M.value_of(id)
  return call(id, "value") or ""
end

--- Maximum width of a text figure before it elides.
function M.figure_limit(id)
  local provider = M.providers[id]
  if provider and provider.limit then return provider.limit end
  if id == "media" then return 150 end
  if id == "network" or id == "bluetooth" then return 110 end
  return 0
end

--- Warnings use the fixed indicator hues; everything else is text colour.
function M.tint_of(id)
  return call(id, "tint") or theme.color.text()
end

-- ------------------------------------------------------------- visibility --

M.runners = { timer = true, media = true }

function M.runs(id) return call(id, "runs") == true end

function M.shows(id, when)
  if when == "running" and M.runners[id] then return M.runs(id) end
  return id == "media" or M.has(id)
end

-- ------------------------------------------------------------- activities --

--- Whether a countdown or music is shown beside the time.
function M.beside(id)
  local kept = settings.islandActivities
  local list = type(kept) == "table" and kept or { "timer", "media" }
  for _, each in ipairs(list) do if each == id then return true end end
  return false
end

--- Up to two running activities beside the time, most urgent first:
--- recording, countdown, music.
function M.activities()
  local list = {}
  if M.runs("recorder") then list[#list + 1] = "recorder" end
  if M.runs("timer") and M.beside("timer") then list[#list + 1] = "timer" end
  if M.runs("media") and M.beside("media") then list[#list + 1] = "media" end
  while #list > 2 do table.remove(list) end
  return list
end

--- The time alone keeps the catalogue width; with activities it shrinks to
--- fit and each side gets a slot.
function M.clock_core()
  if settings.clockShowsDate then return 150 end
  return settings.clockShowsSeconds and 88 or 72
end
function M.activity_side() return #M.activities() > 1 and 92 or 64 end
function M.rest_width()
  if #M.activities() == 0 then return M.entry("clock").width end
  return M.clock_core() + 2 * M.activity_side()
end

-- The glance the island opens under a resting pointer.
M.summary_width = 384
function M.summary_height()
  local ok, media = pcall(require, "services.media")
  return (ok and media.available()) and 168 or 116
end

-- ------------------------------------------------------------ open detail --

-- One detail at a time, always shown by the island's "module" panel.
M.open_id = morf.signal("impasto.modules.open", "")

function M.close()
  M.open_id:set("")
end

--- The catalogue size, except network and Bluetooth, which open the control
--- centre's lists; an empty notification list, which is short; and
--- brightness, which grows a row for every other screen it can dim.
function M.open_size(id)
  if id == "network" or id == "bluetooth" then return 420, 500 end
  local own = call(id, "size")
  if own then return own[1] or own.width, own[2] or own.height end
  local item = M.entry(id)
  return item.width, item.height
end

--- Opens a module's detail, or closes it when it is the one open.
function M.activate(id)
  local island_state = require("bar.island_state")
  if M.open_id:get() == id and island_state.open_panel() == "module" then
    island_state.close()
    M.close()
    return
  end
  if not M.has(id) then return end
  M.open_id:set(id)
  island_state.open("module")
end

--- A module asking for a panel, e.g. the calendar opening the board.
function M.request_panel(panel)
  require("bar.island_state").open(panel)
end

-- The island left the detail (Escape, a click outside, another panel), or
-- the module went away.
morf.effect("impasto.modules.follow", function()
  local island_state = require("bar.island_state")
  local panel = island_state.open_panel()
  local id = M.open_id:get()
  if id ~= "" and panel ~= "module" then
    M.open_id:set("")
  elseif id ~= "" and not M.has(id) then
    M.open_id:set("")
    island_state.close()
  end
end)

-- ---------------------------------------------------------- availability --

-- Always there, whatever the machine.
local always = { clock = true, calendar = true, workspaces = true, notifications = true,
  network = true, photo = true }

--- Whether this machine can show the module at all. Not a preference;
--- placement is the layout's job.
function M.has(id)
  if M.is_button(id) then return true end
  local provided = call(id, "has")
  if provided ~= nil then return provided == true end
  return always[id] == true
end

return M
