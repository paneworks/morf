-- The launcher's query, its results and what choosing one does.
--
-- Port of LauncherService.qml and applications.py. The first character
-- picks the mode, so `100` or `5:30` is never misread as a sum or a
-- duration:
--
--     text   applications, ranked by how often and how lately each was
--            launched from here (a count that halves every thirty days)
--     =      a calculator
--     >      the shell itself: every panel, every mode, every setting
--     @      the open windows
--     !      a countdown
--     '      the clipboard history
--
-- The sigils are the defaults above until Settings changes one
-- (`launcherPrefixes`, services/settings.lua); `mode.prefix` always reads
-- the current one.
--
-- The query lives here, not in the panel, because with `launcherFits` the
-- island's height follows the results and must be known before the panel
-- is built.
--
-- This file is also the one place a desktop id is spelled: without
-- `.desktop` (`desktop_id`), for the launch history, the dock's pinned
-- apps and the favourites alike, and the directories applications are read
-- from, flatpak's exports included (`application_paths`), which the dock
-- reads too. An entry marked `Terminal=true` runs in a terminal window of
-- the shell's own (services/terminal.lua).

local settings = require("services.settings")
local island = require("bar.island")
local timer = require("services.timer")
local clipboard = require("services.clipboard")
local workspaces = require("services.workspaces")
local calc = require("services.calc")

local fs = morf.fs
local json = morf.json

local M = {}

M.query = morf.signal("impasto.launcher.query", "")

-- ------------------------------------------------------------------ modes --

-- The prefix-less mode must be last. `prefix` is not stored: it is read
-- from the settings each time (a binding that reads it follows a change).
M.modes = {
  { id = "calculate", label = "Calculate", icon = "󰃬",
    hint = "Work something out", empty = "An expression — 12 * 34, (5 + 5) / 2" },
  { id = "desk", label = "Desk", icon = "󰍜",
    hint = "Open a panel or change a setting", empty = "Nothing on the desk by that name" },
  { id = "windows", label = "Windows", icon = "󰖯",
    hint = "Find an open window", empty = "No window by that name is open" },
  { id = "timer", label = "Timer", icon = "󰔟",
    hint = "Start a countdown", empty = "A duration — 25m, 90s, 1:30" },
  { id = "clipboard", label = "Clipboard", icon = "󰅍",
    hint = "Copy something again", empty = "Nothing copied says that" },
  { id = "apps", label = "Apps", icon = "󰀻",
    hint = "Search applications",
    empty = "No application matches. Type > for what the shell itself can do." },
}
local mode_meta = {
  __index = function(mode, key)
    if key == "prefix" then
      if mode.id == "apps" then return "" end
      return settings.launcher_prefix(mode.id)
    end
  end,
}
for _, mode in ipairs(M.modes) do setmetatable(mode, mode_meta) end

function M.mode(id)
  for _, mode in ipairs(M.modes) do
    if mode.id == id then return mode end
  end
end

--- Whether `sigil` can be `id`'s: one character, not a letter, a digit or
--- a space (which would steal searches), and no other mode's
--- (LauncherSection.qml `accepts`).
function M.accepts_sigil(id, sigil)
  if type(sigil) ~= "string" or utf8.len(sigil) ~= 1 or sigil:match("[%w ]") then return false end
  for _, mode in ipairs(M.modes) do
    if mode.id ~= id and mode.prefix == sigil then return false end
  end
  return true
end

--- A new sigil for a mode, when it is accepted; "" goes back to the
--- default. Returns whether it was taken.
function M.set_sigil(id, sigil)
  if id == "apps" then return false end
  if sigil ~= "" and not M.accepts_sigil(id, sigil) then return false end
  settings.set_launcher_prefix(id, sigil)
  return true
end

function M.mode_for(query)
  query = query or ""
  for _, mode in ipairs(M.modes) do
    if mode.prefix ~= "" and query:sub(1, #mode.prefix) == mode.prefix then return mode end
  end
  return M.modes[#M.modes]
end

function M.term_for(query)
  query = query or ""
  local mode = M.mode_for(query)
  return (query:sub(#mode.prefix + 1):match("^%s*(.-)%s*$"))
end

--- What the empty list says in the current mode.
function M.empty_text()
  local mode = M.mode_for(M.query:get())
  if mode.id == "clipboard" then
    if not settings.clipboardHistory then return "The clipboard history is off" end
    if clipboard.count() == 0 then return "Nothing has been copied yet" end
  end
  if mode.id == "windows" and not workspaces.available() then
    return "No compositor to ask: the window list needs Hyprland"
  end
  return mode.empty
end

-- ----------------------------------------------------------- applications --

local index = nil          -- the desktop entries object
local applications = {}    -- plain rows, sorted by name
-- Revisions count in plain Lua: reading a signal to bump it would make an
-- effect that bumps it depend on itself.
M.apps_revision = morf.signal("impasto.launcher.apps", 0)
local apps_counter = 0

--- A desktop id as this shell spells it: the file name without
--- `.desktop` (upstream kept the file name; both are read).
function M.desktop_id(id)
  return (tostring(id or ""):match("^%s*(.-)%s*$"):gsub("%.desktop$", ""))
end

--- Where applications are read from, in XDG precedence (a user's copy
--- shadows the system's), with flatpak's exported entries last.
function M.application_paths()
  local env = morf.env or os.getenv
  local home = fs.home()
  local paths, seen = {}, {}
  local function add(path)
    if type(path) == "string" and path ~= "" and not seen[path] then
      seen[path] = true
      paths[#paths + 1] = path
    end
  end
  -- XDG precedence, so a user override shadows the system copy.
  add((env("XDG_DATA_HOME") or (home .. "/.local/share")) .. "/applications")
  for directory in (env("XDG_DATA_DIRS") or "/usr/local/share:/usr/share"):gmatch("[^:]+") do
    add(directory:gsub("/$", "") .. "/applications")
  end
  add("/usr/local/share/applications")
  add("/usr/share/applications")
  add("/var/lib/flatpak/exports/share/applications")
  add(home .. "/.local/share/flatpak/exports/share/applications")
  return paths
end

local function rebuild_applications()
  local list, seen = {}, {}
  local ok, all = pcall(function() return index:applications() end)
  if not ok or type(all) ~= "table" then return end
  for _, entry in ipairs(all) do
    local name = (entry.name or ""):match("^%s*(.-)%s*$")
    if not entry.no_display and name ~= "" and not seen[entry.id] then
      seen[entry.id] = true
      local keywords = table.concat({ name, table.concat(entry.keywords or {}, " "), entry.generic_name or "" }, " ")
      local subtitle = (entry.comment or ""):match("^%s*(.-)%s*$")
      if subtitle == "" then subtitle = entry.generic_name ~= "" and entry.generic_name or "Application" end
      list[#list + 1] = {
        kind = "app", id = M.desktop_id(entry.id), name = name, lower = name:lower(),
        subtitle = subtitle, keywords = keywords:lower(), icon = entry.icon or "",
        wmclass = entry.startup_class or "",
        -- `Terminal=true`: its argv runs in a terminal window instead.
        terminal = entry.run_in_terminal == true,
        command = entry.command, cwd = entry.working_directory,
      }
    end
  end
  table.sort(list, function(a, b) return a.lower < b.lower end)
  applications = list
  apps_counter = apps_counter + 1
  M.apps_revision:set(apps_counter)
end

--- Reads the application index again (the launcher does on open, since
--- scanning is cheaper than watching every XDG directory).
function M.refresh()
  if not index then
    local ok, made = pcall(morf.desktop_entries, M.application_paths())
    if not ok then
      morf.log("warn", "impasto: cannot read the applications: " .. tostring(made))
      return
    end
    index = made
    rebuild_applications()
    return
  end
  local ok, changed = pcall(index.refresh, index)
  if ok and changed then rebuild_applications() end
end

function M.applications()
  M.apps_revision:get()
  return applications
end

-- Read once when the shell starts, so the first search does not wait on
-- the disk; again each time the launcher opens.
M.refresh()

-- ---------------------------------------------------------------- history --

-- Desktop id -> { count, last }; decay is applied on read.
M.history_path = fs.join(settings.dir, "launcher-history.json")
M.HALF_LIFE = 30 * 24 * 3600 * 1000
local launches = {}
M.history_revision = morf.signal("impasto.launcher.history", 0)
local history_counter = 0

-- The file as last read or written, so a watch event for this runtime's
-- own write is not a change.
local history_text = nil

local function read_history()
  local text = fs.read(M.history_path)
  history_text = text
  local out = {}
  if text and text ~= "" then
    local ok, decoded = pcall(json.decode, text)
    if ok and type(decoded) == "table" and type(decoded.launches) == "table" then
      for id, row in pairs(decoded.launches) do
        if type(id) == "string" and type(row) == "table" then
          -- A file name from the QML shell and a bare id are one app.
          local key = M.desktop_id(id)
          local count, last = tonumber(row.count) or 0, tonumber(row.last) or 0
          local held = out[key]
          if held then
            out[key] = { count = held.count + count, last = math.max(held.last, last) }
          else
            out[key] = { count = count, last = last }
          end
        end
      end
    end
  end
  return out
end

local function history_changed()
  history_counter = history_counter + 1
  M.history_revision:set(history_counter)
end

launches = read_history()

-- Every screen launches and records: each follows the file, so one screen's
-- launch is counted by all and none writes over another's.
require("services.watch").file(M.history_path, function()
  local text = fs.read(M.history_path)
  if text == history_text then return end
  launches = read_history()
  history_changed()
end)

function M.score_of(id)
  local row = launches[M.desktop_id(id)]
  if not row or not row.count or row.count == 0 then return 0 end
  local age = morf.time.now_ms() - (row.last or 0)
  return row.count * 0.5 ^ (age / M.HALF_LIFE)
end

function M.record(id)
  id = M.desktop_id(id)
  if id == "" then return end
  -- What another screen wrote since, first.
  launches = read_history()
  launches[id] = { count = M.score_of(id) + 1, last = morf.time.now_ms() }
  history_changed()
  local text = json.encode({ launches = launches }, true)
  history_text = text
  local ok, err = fs.write(M.history_path, text)
  if not ok then morf.log("warn", "impasto: could not save the launch history: " .. tostring(err)) end
end

-- The dock's pinned apps get a bonus of one launch.
local function favourites()
  local pinned = settings.dockPinned
  local out = {}
  if type(pinned) == "table" then
    for position, id in ipairs(pinned) do
      local key = M.desktop_id(id)
      if out[key] == nil then out[key] = position end
    end
  end
  return out
end
M.favourites = favourites

--- Every application matching `term`, ordered by:
---
---   1. the name starts with the term
---   2. weight: decayed launches plus the favourite bonus
---   3. dock order, between two favourites of equal weight
---   4. the name
---
--- In alphabetical order only 1 and 4 apply.
function M.rank(term)
  M.history_revision:get()
  local found = {}
  for _, app in ipairs(M.applications()) do
    if term == "" or app.lower:find(term, 1, true) or app.keywords:find(term, 1, true) then
      found[#found + 1] = app
    end
  end
  local by_use = settings.launcherOrder ~= "alphabetical"
  local kept = favourites()
  local weight = {}
  for _, app in ipairs(found) do
    weight[app.id] = M.score_of(app.id) + (kept[app.id] and 1 or 0)
  end
  table.sort(found, function(a, b)
    if term ~= "" then
      local lead_a = a.lower:sub(1, #term) == term
      local lead_b = b.lower:sub(1, #term) == term
      if lead_a ~= lead_b then return lead_a end
    end
    if by_use then
      local wa, wb = weight[a.id], weight[b.id]
      if math.abs(wa - wb) > 0.0001 then return wa > wb end
      local ka, kb = kept[a.id], kept[b.id]
      if ka and kb and ka ~= kb then return ka < kb end
    end
    if a.lower ~= b.lower then return a.lower < b.lower end
    return a.id < b.id
  end)
  return found
end

-- ------------------------------------------------------------------- desk --

-- The panels the shell can open, as the control centre's doors name them.
-- Only those registered in the island are offered.
M.doors = {
  { id = "controls", icon = "󰒓", label = "Control centre", detail = "Toggles, sliders and the doors" },
  { id = "overview", icon = "󰕰", label = "Workspace overview", detail = "Every workspace side by side" },
  { id = "stats", icon = "󰕬", label = "System statistics", detail = "Processor, memory, disks, the network" },
  { id = "settings", icon = "󰒓", label = "Settings", detail = "The whole desk, in a window" },
  { id = "pet", icon = "󰏩", label = "Pet", detail = "The creature living on the bar" },
  { id = "games", icon = "󰊗", label = "Games", detail = "The arcade" },
  { id = "notes", icon = "󰎞", label = "Notes", detail = "The deck of sticky notes" },
  { id = "board", icon = "󰄲", label = "Task board", detail = "To do, doing, done" },
  { id = "appearance", icon = "󰏘", label = "Appearance", detail = "The wallpaper and the palette" },
  { id = "session", icon = "󰐥", label = "Session menu", detail = "Lock, log out, suspend, restart, off" },
  { id = "keys", icon = "󰌌", label = "Keys", detail = "Every shortcut, on one sheet" },
  { id = "packages", icon = "󰏗", label = "Packages", detail = "Updates, what is installed, the AUR" },
  { id = "wifi", icon = "󰤨", label = "Wi-Fi", detail = "Networks nearby" },
  { id = "bluetooth", icon = "󰂯", label = "Bluetooth", detail = "Devices nearby" },
}

-- "clockShowsDate" -> "Clock shows date".
local function words(key)
  local spaced = key:gsub("[._-]", " "):gsub("(%l)(%u)", "%1 %2"):gsub("(%a)(%d)", "%1 %2"):lower()
  return spaced:sub(1, 1):upper() .. spaced:sub(2)
end
M.words = words

local LISTS = { barLeft = true, barRight = true }

local function describe_value(value)
  if value == false then return "Off" end
  if value == true then return "On" end
  if type(value) == "table" then
    local count = 0
    for _ in pairs(value) do count = count + 1 end
    return count == 0 and "None" or (count .. (count == 1 and " item" or " items"))
  end
  local text = tostring(value)
  if text == "" then return "Not set" end
  return text
end

--- The `>` list: the panels, the other modes, then every setting. A
--- setting that is on or off flips in place; the others say their value.
function M.desk(term)
  local lead, rest = {}, {}
  local function offer(entry)
    if term == "" then rest[#rest + 1] = entry return end
    local name = entry.name:lower()
    if name:sub(1, #term) == term then
      lead[#lead + 1] = entry
    elseif name:find(term, 1, true) or entry.subtitle:lower():find(term, 1, true)
        or (entry.search and entry.search:find(term, 1, true)) then
      rest[#rest + 1] = entry
    end
  end

  local named = {}
  for _, door in ipairs(M.doors) do
    named[door.id] = true
    if door.id == "settings" then
      -- A window rather than a panel: settings/window.lua.
      offer { kind = "settings", id = door.id, icon = door.icon, name = door.label,
        subtitle = door.detail }
    elseif island.panels[door.id] then
      offer { kind = "panel", id = door.id, icon = door.icon, name = door.label,
        subtitle = door.detail, panel = door.id }
    end
  end
  -- Panels registered since, under their own names.
  local others = {}
  for id in pairs(island.panels) do
    if not named[id] and id ~= "launcher" then others[#others + 1] = id end
  end
  table.sort(others)
  for _, id in ipairs(others) do
    offer { kind = "panel", id = id, icon = "󰕮", name = words(id), subtitle = "Panel", panel = id }
  end

  for _, mode in ipairs(M.modes) do
    if mode.id ~= "desk" and mode.prefix ~= ""
        and not (mode.id == "clipboard" and not settings.clipboardHistory) then
      offer { kind = "mode", id = mode.id, icon = mode.icon, name = mode.label,
        subtitle = mode.hint, sigil = mode.prefix }
    end
  end

  -- The control centre's one-shot tiles (Capture, Annotate, Read text,
  -- Colour, Record, Clear clipboard) that can run here now.
  local ok_controls, controls = pcall(require, "services.controls")
  if ok_controls and type(controls.tile_catalogue) == "table" then
    for _, tile in ipairs(controls.tile_catalogue) do
      local ok_a, available = pcall(tile.available)
      if tile.closes and ok_a and available then
        local ok_i, glyph = pcall(tile.icon)
        local ok_d, detail = pcall(tile.detail)
        offer { kind = "action", id = tile.key, icon = ok_i and glyph or "󰐊",
          name = tile.label, subtitle = ok_d and detail or "" }
      end
    end
  end

  local keys = {}
  for key in pairs(settings.defaults) do keys[#keys + 1] = key end
  table.sort(keys)
  for _, key in ipairs(keys) do
    local value = settings.get(key)
    -- `barLeft` and `barRight` are false for "the default side" and a list
    -- otherwise; only a real on/off setting flips.
    local flips = type(settings.defaults[key]) == "boolean" and not LISTS[key]
    offer { kind = "setting", id = key, icon = flips and (value and "󰔡" or "󰨙") or "󰒓",
      name = words(key), search = key:lower(),
      subtitle = "Setting  ·  " .. ((LISTS[key] and value == false) and "The default pieces" or describe_value(value))
        .. (flips and "  ·  Enter switches it" or ""),
      flips = flips }
  end

  for _, entry in ipairs(rest) do lead[#lead + 1] = entry end
  return lead
end

-- ------------------------------------------------------------ other modes --

function M.calculate(expression)
  if expression == "" then return {} end
  local value = calc.evaluate(expression)
  if value == nil then return {} end
  return { {
    kind = "calculation", id = "", icon = "󰃬", name = calc.format(value),
    subtitle = expression .. "  ·  Enter copies it",
  } }
end

function M.timer(text)
  local ms = timer.parse(text)
  if ms <= 0 then return {} end
  return { {
    kind = "timer", id = "", icon = "󰔟", name = "Start a " .. timer.spell(ms) .. " timer",
    subtitle = "Counts down on the island", milliseconds = ms,
  } }
end

function M.windows(term, limit)
  local found = {}
  for _, client in ipairs(workspaces.clients()) do
    if #found >= limit then break end
    local title, class = (client.title or ""):lower(), (client.class or ""):lower()
    if term == "" or title:find(term, 1, true) or class:find(term, 1, true) then
      found[#found + 1] = {
        kind = "window", id = client.address, icon = "󰖯", app_icon = client.class,
        name = client.title ~= "" and client.title or client.class,
        subtitle = client.class .. "  ·  workspace " .. tostring(client.workspace),
      }
    end
  end
  return found
end

function M.clipboard(term, limit)
  local found = {}
  if not settings.clipboardHistory then return found end
  for _, entry in ipairs(clipboard.search(term, limit)) do
    found[#found + 1] = {
      kind = "clip", id = entry.key, icon = "󰅍",
      name = clipboard.title(entry), subtitle = clipboard.describe(entry),
      picture = entry.kind == "image" and entry.file or "",
    }
  end
  return found
end

-- ---------------------------------------------------------------- results --

function M.max_results() return math.max(1, math.floor(tonumber(settings.launcherResults) or 9)) end

--- The results for a query; a binding that calls it follows everything the
--- mode reads.
function M.search(query)
  local mode = M.mode_for(query)
  local raw = M.term_for(query)
  local term = raw:lower()
  local limit = M.max_results()
  if mode.id == "calculate" then return M.calculate(raw) end
  if mode.id == "desk" then return M.desk(term) end
  if mode.id == "windows" then return M.windows(term, limit) end
  if mode.id == "timer" then return M.timer(raw) end
  if mode.id == "clipboard" then return M.clipboard(term, limit) end
  local ranked = M.rank(term)
  local out = {}
  for position = 1, math.min(limit, #ranked) do out[position] = ranked[position] end
  return out
end

-- The results of the current query, computed once per change and read by
-- the panel and the island's size.
local current = {}
M.count = morf.signal("impasto.launcher.count", 0)
M.results_revision = morf.signal("impasto.launcher.results", 0)
local results_counter = 0
M.open = morf.signal("impasto.launcher.open", false)

morf.effect("impasto.launcher.search", function()
  -- Nothing is searched while the launcher is shut, except to size it: the
  -- island reads the height a frame before the panel exists.
  if not M.open:get() and M.query:get() == "" and not settings.launcherFits then return end
  current = M.search(M.query:get())
  M.count:set(#current)
  results_counter = results_counter + 1
  M.results_revision:set(results_counter)
end)

function M.results()
  M.results_revision:get()
  return current
end

-- --------------------------------------------------------------- the size --

M.field_height = 34
M.row_height = 48
M.row_spacing = 2
M.gap = 12
M.chrome_height = M.field_height + 2 * M.gap + 1
M.panel_width = 560

function M.height_for(rows)
  local padding = 20
  return M.chrome_height + rows * M.row_height + (rows - 1) * M.row_spacing + 2 * padding
end

--- Fixed, with a scrolling list, or (`launcherFits`) sized to the results
--- up to `launcherResults` rows.
function M.panel_height()
  if settings.launcherFits then
    return M.height_for(math.max(1, math.min(M.count:get(), M.max_results())))
  end
  return 520
end

-- ------------------------------------------------------------- activation --

local function launch(id)
  if not index then M.refresh() end
  if not index then return end
  local ok, err = pcall(index.launch, index, id)
  if not ok then morf.log("warn", "impasto: could not launch " .. id .. ": " .. tostring(err)) end
end

local function row_of(id)
  for _, app in ipairs(applications) do
    if app.id == id then return app end
  end
end

--- Starts an application by id (with or without `.desktop`): a terminal
--- program in a terminal window of the shell's, anything else as the
--- desktop entry says. Shared with the dock.
function M.launch(id)
  id = M.desktop_id(id)
  if not index then M.refresh() end
  local app = row_of(id)
  if app and app.terminal and type(app.command) == "table" and #app.command > 0 then
    -- Starting a program changes nothing on the machine, so a dry run
    -- starts it as the desktop entry would.
    local cwd = app.cwd ~= "" and app.cwd or nil
    local term, why = require("services.terminal").run(app.command, { title = app.name, cwd = cwd })
    if not term then morf.log("warn", "impasto: no terminal for " .. app.name .. ": " .. tostring(why)) end
    return
  end
  launch(id)
end

--- Runs an entry. Returns "close" when the launcher should close, "keep"
--- when it stays open (a mode switched, a setting flipped).
function M.activate(entry)
  if not entry then return "keep" end
  local kind = entry.kind
  if kind == "app" then
    -- Recorded here, not in `launch`, so the dock's launches do not count
    -- towards the ranking.
    M.record(entry.id)
    M.launch(entry.id)
  elseif kind == "panel" then
    island.open(entry.panel)
    return "panel"
  elseif kind == "mode" then
    M.query:set(entry.sigil)
    return "keep"
  elseif kind == "setting" then
    if entry.flips then
      settings.set(entry.id, not settings.get(entry.id))
      return "keep"
    end
    -- Anything else is changed in the settings window, on its page.
    local ok, panel = pcall(require, "settings.panel")
    local section, part = "", ""
    if ok then section, part = panel.section_for_key(entry.id) end
    require("settings.window").open(section, part)
    return "close"
  elseif kind == "settings" then
    require("settings.window").open()
    return "close"
  elseif kind == "action" then
    -- The island closes first; the tile waits for it where it has to
    -- (a capture must not photograph the launcher).
    local ok, controls = pcall(require, "services.controls")
    local tile = ok and controls.tile_of(entry.id)
    if tile then
      morf.timer(1, function() controls.activate(tile) end, false)
    end
  elseif kind == "timer" then
    timer.start(entry.milliseconds, "")
  elseif kind == "window" then
    workspaces.focus_window(entry.id)
  elseif kind == "calculation" then
    pcall(morf.clipboard.set, entry.name)
  elseif kind == "clip" then
    clipboard.copy(entry.id)
  end
  return "close"
end

--- Opens the launcher, optionally in a mode: `launcher.open_with("'")`.
function M.open_with(query)
  M.query:set(query or "")
  island.open("launcher")
end

--- The clipboard key (shell.qml's `clipboard` shortcut): pressed again on
--- the clipboard mode it closes, pressed on another mode it switches.
function M.toggle_clipboard()
  local sigil = M.mode("clipboard").prefix
  local query = M.query:get()
  if island.state.open_panel() == "launcher" and query:sub(1, #sigil) == sigil then
    island.close()
    return "closed"
  end
  M.open_with(sigil)
  return "open"
end

return M
