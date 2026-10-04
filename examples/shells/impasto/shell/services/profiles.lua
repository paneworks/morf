-- Profiles: named whole-desk setups, one of them in use.
--
-- Port of ProfileService.qml. The profile in use is the settings file
-- itself, so no other service knows profiles exist; the others wait in
-- `profiles.json` beside it. A profile is every setting that is not about
-- this machine or this person (`settings.machine_keys`) -- the bar, the
-- dock, the desktop widgets and the notes stuck on the edges (both live in
-- `desktopWidgets`), the launcher, the control centre, the look -- plus
-- its wallpaper and palette. Switching stashes the one in use, adopts the
-- other in one handler and puts its picture and palette up. The original
-- also reloaded Hyprland; nothing here touches the compositor.
--
-- Three examples ship in `profiles/` beside this configuration (Moon
-- castle, Fuji, Night bay). Each is offered once and remembered in
-- `offered`, so a deleted one stays deleted; their wallpapers are file
-- names, found in the `wallpaperDir` setting's folder. On a fresh install
-- (no settings file yet) the first one is switched to.
--
-- Import and export use plain JSON files in the original's format:
--
--   impasto    the format (1)
--   name       what it was called
--   wallpaper  the picture, with the home directory written as `~`
--   palette    "adaptive" or a preset id from lib/palette
--   settings   every profile key

local settings = require("services.settings")
local wallpaper = require("services.wallpaper")
local theme_service = require("services.theme")
local palette = require("lib.util.palette")

local fs = morf.fs
local json = morf.json

local M = {}

M.FORMAT = 1
M.NAME_LENGTH = 32
M.path = fs.join(settings.dir, "profiles.json")
M.shipped_dir = fs.join(morf.shell_dir(), "profiles")

-- { id, name, wallpaper, palette, settings }
local list = {}
local active = ""
local offered = {}

M.revision = morf.signal("impasto.profiles.revision", 0)
local function touched() M.revision:set(M.revision:get() + 1) end

local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

local function osd(glyph, text)
  local ok, state = pcall(require, "bar.island_state")
  if ok and state.flash then pcall(state.flash, glyph, text, -1) end
end

-- -------------------------------------------------------------- reading --

--- The profiles, in order. A binding follows every change.
function M.list()
  M.revision:get()
  return list
end

function M.active()
  M.revision:get()
  return active
end

function M.entry(id)
  for _, item in ipairs(list) do if item.id == id then return item end end
  return nil
end

--- What a profile holds now: the live settings for the one in use.
function M.settings_of(id)
  if id == active then return settings.snapshot() end
  local found = M.entry(id)
  return settings.complete(found and found.settings or {})
end

function M.wallpaper_of(id)
  if id == active then return wallpaper.current:get() end
  local found = M.entry(id)
  return found and found.wallpaper or ""
end

function M.palette_of(id)
  if id == active then return theme_service.active_id:get() end
  local found = M.entry(id)
  return found and found.palette or ""
end

--- A palette id this shell knows: "adaptive" or a preset.
function M.known_palette(id)
  if id == "adaptive" then return true end
  for _, preset in ipairs(palette.presets) do if preset.id == id then return true end end
  return false
end

-- ---------------------------------------------------------------- names --

function M.clean_name(name)
  local text = tostring(name or ""):gsub("%s+", " "):match("^%s*(.-)%s*$")
  return text:sub(1, M.NAME_LENGTH)
end

--- "Focus", then "Focus 2", "Focus 3"...
function M.unique_name(wanted, except)
  local base = M.clean_name(wanted)
  if base == "" then base = "Profile" end
  local function taken(name)
    for _, item in ipairs(list) do
      if item.id ~= except and item.name:lower() == name:lower() then return true end
    end
    return false
  end
  if not taken(base) then return base end
  local stem = base:gsub(" %d+$", "")
  local n = 2
  while taken(stem .. " " .. n) do n = n + 1 end
  return stem .. " " .. n
end

local function new_id()
  return string.format("%x%04x", morf.time.now_ms(), math.random(0, 0xffff))
end

-- -------------------------------------------------------------- storage --

local function save()
  local ok, err = fs.write(M.path, json.encode({
    active = active, profiles = list, offered = offered,
  }, true))
  if not ok then morf.log("warn", "impasto: could not save profiles: " .. tostring(err)) end
  touched()
end

-- ------------------------------------------------------------ wallpapers --

local function expand(path)
  if type(path) ~= "string" or path == "" then return "" end
  return fs.expand(path)
end

--- A picture named in a profile: the path when it is there, else a picture
--- of that file name in the wallpaper folder, else the path as given.
function M.resolve_wallpaper(path)
  local full = expand(path)
  if full == "" then return "" end
  if full:find("/", 1, true) and fs.is_file(full) then return full end
  local name = full:match("([^/]+)$") or full
  local candidate = fs.join(wallpaper.dir(), name)
  if fs.is_file(candidate) then return candidate end
  for _, known in ipairs(wallpaper.list()) do
    if known:match("([^/]+)$") == name then return known end
  end
  return full
end

local function tilde(path)
  local home = fs.home() or ""
  if home ~= "" and path:sub(1, #home + 1) == home .. "/" then return "~" .. path:sub(#home + 1) end
  return path
end

-- ------------------------------------------------------------- documents --

--- The QML impasto's values, in this port's words (services/settings.lua).
local function translate(given)
  return settings.from_upstream(given)
end

--- A document out of a file's text, or nil with why.
function M.parse(text)
  local ok, document = pcall(json.decode, text or "")
  if not ok or type(document) ~= "table" then return nil, "not JSON" end
  document = plain(document)
  if type(document.impasto) ~= "number" or type(document.settings) ~= "table" then
    return nil, "not an impasto profile"
  end
  if document.impasto > M.FORMAT then return nil, "made by a newer impasto" end
  return document
end

--- A new entry out of a document, named by it or by `fallback`.
local function from_document(document, fallback)
  local pal = type(document.palette) == "string" and M.known_palette(document.palette)
    and document.palette or ""
  local values, skipped = settings.complete(translate(document.settings))
  return {
    id = new_id(),
    name = M.unique_name(type(document.name) == "string" and document.name or fallback, ""),
    wallpaper = M.resolve_wallpaper(document.wallpaper),
    palette = pal,
    settings = values,
  }, skipped
end

-- --------------------------------------------------------------- actions --

--- A new profile at the defaults, with the picture and palette up now.
function M.create()
  local made = {
    id = new_id(), name = M.unique_name("New profile", ""),
    wallpaper = wallpaper.current:get(), palette = theme_service.active_id:get(),
    settings = settings.complete({}),
  }
  list[#list + 1] = made
  save()
  return made.id
end

--- A copy, straight after the one it was copied from.
function M.duplicate(id)
  local from = M.entry(id)
  if not from then return "" end
  local made = {
    id = new_id(), name = M.unique_name(from.name, ""),
    wallpaper = M.wallpaper_of(id), palette = M.palette_of(id), settings = M.settings_of(id),
  }
  for index, item in ipairs(list) do
    if item.id == id then table.insert(list, index + 1, made) break end
  end
  save()
  return made.id
end

function M.rename(id, name)
  local clean = M.clean_name(name)
  local found = M.entry(id)
  if clean == "" or not found then return end
  found.name = M.unique_name(clean, id)
  save()
end

--- Any but the one in use.
function M.remove(id)
  if id == active then return end
  for index, item in ipairs(list) do
    if item.id == id then table.remove(list, index) break end
  end
  save()
end

--- Saves the one in use into the list.
local function stash()
  local current = M.entry(active)
  if not current then return end
  current.settings = settings.snapshot()
  current.wallpaper = wallpaper.current:get()
  current.palette = theme_service.active_id:get()
end

function M.switch_to(id)
  local target = M.entry(id)
  if not target or id == active then return end
  stash()
  settings.adopt(target.settings)
  active = id
  save()
  local pal = target.palette or ""
  if pal ~= "" and pal ~= theme_service.active_id:get() and M.known_palette(pal) then
    theme_service.set_theme(pal)
  end
  local picture = M.resolve_wallpaper(target.wallpaper)
  if picture ~= "" and picture ~= wallpaper.current:get() and fs.is_file(picture) then
    wallpaper.apply(picture)
  end
  osd("󰀉", target.name)
end

--- Writes one profile to `path` (".json" added when missing).
function M.export_to(id, path)
  local found = M.entry(id)
  if not found or not path or path == "" then return false, "nothing to export" end
  local file = fs.expand(path)
  if not file:lower():match("%.json$") then file = file .. ".json" end
  local document = {
    impasto = M.FORMAT, name = found.name,
    wallpaper = tilde(M.wallpaper_of(id)), palette = M.palette_of(id),
    -- In the QML shell's words, so either shell reads the file.
    settings = settings.to_upstream(M.settings_of(id)),
  }
  local ok, err = fs.write(file, json.encode(document, true) .. "\n")
  if ok then osd("󰈝", "Exported " .. found.name) else osd("󰀦", "Could not write that file") end
  return ok, err or file
end

--- Adds the profile in the file at `path`. Returns its id, or nil and why.
function M.import_from(path)
  local file = fs.expand(path or "")
  local text = file ~= "" and fs.read(file) or nil
  if not text then
    osd("󰀦", "Could not read that file")
    return nil, "could not read " .. file
  end
  local document, why = M.parse(text)
  if not document then
    osd("󰀦", "Not an impasto profile")
    return nil, why
  end
  local fallback = (file:match("([^/]+)$") or "Profile"):gsub("%.[Jj][Ss][Oo][Nn]$", "")
  local made, skipped = from_document(document, fallback)
  list[#list + 1] = made
  save()
  osd("󰋺", skipped > 0 and ("Imported " .. made.name .. " · " .. skipped .. " left out")
    or ("Imported " .. made.name))
  return made.id
end

--- The summary a row shows: "Grouped bar · dock at the bottom · 3 widgets".
function M.summary(id)
  local values = M.settings_of(id)
  local bars = { grouped = "Grouped bar", spread = "Spread bar", capsule = "One island" }
  local docks = { bottom = "dock at the bottom", left = "dock on the left", right = "dock on the right" }
  local parts = { bars[values.barStyle] or bars.grouped }
  parts[#parts + 1] = values.dockEnabled and (docks[values.dockEdge] or docks.bottom) or "no dock"
  local widgets = 0
  for _, row in ipairs(values.desktopWidgets or {}) do
    if type(row) == "table" and not row.edge then widgets = widgets + 1 end
  end
  parts[#parts + 1] = widgets == 0 and "nothing on the desk"
    or (widgets .. (widgets == 1 and " widget" or " widgets"))
  local pal = M.palette_of(id)
  if pal ~= "" and pal ~= "adaptive" then
    for _, preset in ipairs(palette.presets) do
      if preset.id == pal then parts[#parts + 1] = preset.name end
    end
  end
  return table.concat(parts, " · ")
end

-- -------------------------------------------------------------- shipped --

local function offer_shipped(placeholder)
  local names = {}
  for _, entry in ipairs(fs.list(M.shipped_dir) or {}) do
    if entry.is_file and entry.extension == "json" then names[#names + 1] = entry.name end
  end
  table.sort(names)
  local seen = {}
  for _, name in ipairs(offered) do seen[name] = true end
  local made = {}
  for _, name in ipairs(names) do
    local base = name:gsub("%.json$", "")
    if not seen[base] then
      local document = M.parse(fs.read(fs.join(M.shipped_dir, name)))
      if document then
        local entry = from_document(document, base)
        list[#list + 1] = entry
        made[#made + 1] = entry
      end
      offered[#offered + 1] = base
    end
  end
  if #made == 0 then return false end
  -- A fresh install trades the empty placeholder for the first example; on
  -- an existing setup the placeholder is the person's own and stays.
  if placeholder and placeholder == active and settings.fresh then
    M.switch_to(made[1].id)
    for index, item in ipairs(list) do
      if item.id == placeholder then table.remove(list, index) break end
    end
  end
  return true
end

local function arrive()
  local text = fs.read(M.path)
  local stored = {}
  if text and text ~= "" then
    local ok, decoded = pcall(json.decode, text)
    if ok and type(decoded) == "table" then stored = plain(decoded) end
  end
  local seen = {}
  for _, item in ipairs(type(stored.profiles) == "table" and stored.profiles or {}) do
    if type(item) == "table" and type(item.id) == "string" and item.id ~= "" and not seen[item.id] then
      seen[item.id] = true
      list[#list + 1] = {
        id = item.id,
        name = M.clean_name(item.name) ~= "" and M.clean_name(item.name) or "Profile",
        wallpaper = type(item.wallpaper) == "string" and item.wallpaper or "",
        palette = type(item.palette) == "string" and item.palette or "",
        settings = type(item.settings) == "table" and item.settings or {},
      }
    end
  end
  active = type(stored.active) == "string" and stored.active or ""
  offered = type(stored.offered) == "table" and stored.offered or {}
  local placeholder
  local changed = false
  if not seen[active] then
    local made = { id = new_id(), name = M.unique_name("Default", ""),
      wallpaper = wallpaper.current:get(), palette = theme_service.active_id:get(), settings = {} }
    table.insert(list, 1, made)
    active = made.id
    placeholder = made.id
    changed = true
  end
  if offer_shipped(placeholder) then changed = true end
  if changed then save() else touched() end
end

arrive()

--- `morf ipc call profile [name]` switches to the profile of that name (any
--- case) and answers with the one in use.
morf.ipc.profile = function(name)
  if name and name ~= "" then
    for _, item in ipairs(list) do
      if item.name:lower() == name:lower() then M.switch_to(item.id) break end
    end
  end
  local current = M.entry(active)
  return current and current.name or ""
end

local function by_name(name)
  for _, item in ipairs(list) do
    if item.name:lower() == tostring(name or ""):lower() then return item end
  end
  return nil
end

--- TESTING ONLY: `profile_export <name> <path>` and `profile_import
--- <path>`, what the menu's Export and the Import button do, without a
--- pointer to press them.
morf.ipc.profile_export = function(name, path)
  local found = by_name(name)
  if not found then return "no profile " .. tostring(name) end
  local ok, where = M.export_to(found.id, path)
  return ok and tostring(where) or ("not written: " .. tostring(where))
end
morf.ipc.profile_import = function(path)
  local id, why = M.import_from(path)
  if not id then return "not imported: " .. tostring(why) end
  return M.entry(id).name
end

return M
