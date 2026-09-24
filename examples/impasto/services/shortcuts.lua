-- Keys: every key the compositor has bound, the key sheet, and rebinding.
--
-- Port of ShortcutService.qml. Binds come from Hyprland (`lib.hyprland`'s
-- `binds()`, what `hyprctl binds` prints); a bind's description reads
-- "Group · Action", and it is the one field that survives into that list,
-- so it is the key everywhere: the key sheet folds binds sharing a group,
-- modifiers and an action up to a trailing number or direction into one
-- row ("Workspace 1…9"), and the profile's `keys` setting maps a
-- description to a combination ("SUPER + T", "" for unbound).
--
-- Rebinding never edits the user's Hyprland configuration. The profile's
-- keys are written to `$XDG_STATE_HOME/impasto-morf/keys.tsv`
-- (description, a tab, the combination; upstream's format) and Hyprland is
-- reloaded; the user's configuration reads the file through one line
-- (`M.source_line`), after which a bind whose description is in the file
-- takes the combination kept there:
--
--   Lua config (hyprland.lua, 0.56+), near the top, before any bind:
--     pcall(dofile, (os.getenv("XDG_STATE_HOME") or (os.getenv("HOME") .. "/.local/state")) .. "/impasto-morf/keys.lua")
--   hyprlang (hyprland.conf), at the end, after every bind:
--     source = ~/.local/state/impasto-morf/keys.conf
--
-- keys.lua (written here too) wraps `hl.bind`; keys.conf unbinds each
-- changed bind and binds it again on its new keys, with the dispatcher
-- Hyprland reported. Without either line the keys are kept and shown, and
-- the page says the compositor does not read them yet.

local M = {}

local ok_lib, hyprland = pcall(require, "lib.hyprland")
if not ok_lib then hyprland = nil end
local ok_config, config = pcall(require, "lib.hyprland_config")
if not ok_config then config = nil end
local settings = require("services.settings")
local act = require("services.act")
local live = require("services.live")

local s = {
  revision = morf.signal("impasto.keys.revision", 0),
  -- "", "reading", "ok", or why there are no binds.
  status = morf.signal("impasto.keys.status", ""),
}
M.signals = s

local binds = {}

M.sheet_width = 620
M.sheet_height = 600
M.field_height = 36
M.row_height = 36
M.heading_height = 32

-- Unlisted groups follow, in the order they first appear.
M.order = { "Shell", "Applications", "Windows", "Workspaces", "Session", "Utilities", "Media" }

local MODIFIERS = { { bit = 64, cap = "Super" }, { bit = 4, cap = "Ctrl" }, { bit = 8, cap = "Alt" }, { bit = 1, cap = "Shift" } }

-- Keycap labels where the xkb name will not do. Arrows come from the icon
-- font: the interface face lacks them.
local CAPS = {
  left = "󰁍", right = "󰁔", up = "󰁝", down = "󰁅",
  TAB = "Tab", Tab = "Tab", Return = "Enter", space = "Space", Escape = "Esc",
  comma = ",", period = ".", slash = "/", minus = "-", equal = "=",
  ["mouse:272"] = "Left button", ["mouse:273"] = "Right button",
  mouse_down = "Wheel down", mouse_up = "Wheel up",
  XF86AudioRaiseVolume = "Vol +", XF86AudioLowerVolume = "Vol −",
  XF86AudioMute = "Mute", XF86AudioMicMute = "Mic mute",
  XF86MonBrightnessUp = "Bright +", XF86MonBrightnessDown = "Bright −",
  XF86AudioPlay = "Play", XF86AudioNext = "Next", XF86AudioPrev = "Previous",
}

function M.cap_of(key)
  if CAPS[key] then return CAPS[key] end
  -- Hyprland keeps the key as the config spelled it: SPACE, Return, return.
  if CAPS[key:lower()] then return CAPS[key:lower()] end
  if key == "RETURN" then return "Enter" end
  if key:sub(1, 4) == "XF86" then return key:sub(5) end
  return key
end

local function rank(name, first_seen)
  for index, each in ipairs(M.order) do if each == name then return index end end
  return 99 + first_seen
end

local sheet = {}

local function rebuild()
  local groups, order = {}, {}
  for _, bind in ipairs(binds) do
    local key = tostring(bind.key or "")
    if key ~= "" and key:sub(1, 7) ~= "switch:" then
      local text = tostring(bind.description or "")
      local category, action = text:match("^(.-) · (.+)$")
      if not category then
        category = "Other"
        action = text ~= "" and text
          or (tostring(bind.dispatcher or "") .. " " .. tostring(bind.arg or "")):match("^%s*(.-)%s*$")
        if action == "" then action = key end
      end
      local base, tail = action:match("^(.*) (%d+)$")
      local kind = base and "digits" or ""
      if not base then
        for _, word in ipairs { "left", "right", "up", "down" } do
          local b = action:match("^(.*) " .. word .. "$")
          if b then base, kind = b, "words" break end
        end
      end
      base = base or action
      if not groups[category] then
        groups[category] = {}
        order[#order + 1] = category
      end
      local rows = groups[category]
      local same
      if kind ~= "" then
        for _, row in ipairs(rows) do
          if row.base == base and row.kind == kind and row.modmask == (tonumber(bind.modmask) or 0) then same = row break end
        end
      end
      if same then
        same.keys[#same.keys + 1] = key
      else
        rows[#rows + 1] = { base = base, action = action, kind = kind, modmask = tonumber(bind.modmask) or 0, keys = { key } }
      end
      local _ = tail
    end
  end
  local seen = {}
  for index, name in ipairs(order) do seen[name] = index end
  table.sort(order, function(a, b) return rank(a, seen[a]) < rank(b, seen[b]) end)

  sheet = {}
  for _, category in ipairs(order) do
    local out = {}
    for _, row in ipairs(groups[category]) do
      local caps = {}
      for _, modifier in ipairs(MODIFIERS) do
        if row.modmask & modifier.bit ~= 0 then caps[#caps + 1] = modifier.cap end
      end
      local many = #row.keys > 1
      if many and row.kind == "digits" then
        caps[#caps + 1] = row.keys[1] .. "…" .. row.keys[#row.keys]
      else
        for _, key in ipairs(row.keys) do caps[#caps + 1] = M.cap_of(key) end
      end
      out[#out + 1] = { action = many and row.base or row.action, caps = caps, keys = row.keys }
    end
    sheet[#sheet + 1] = { name = category, rows = out }
  end
  s.revision:set(s.revision:get() + 1)
end

function M.sheet() s.revision:get() return sheet end

-- ------------------------------------------------------------ catalogue --

--- The shell's own shortcuts, in settings order: `name` the verb (`morf
--- ipc call <name>`), `label`, and the `description` keybinds carry, which
--- must match exactly.
M.catalogue = {
  { name = "launcher", label = "Launcher", description = "Shell · Open the launcher" },
  { name = "controls", label = "Control centre", description = "Shell · Open the control centre" },
  { name = "overview", label = "Workspace overview", description = "Shell · Open the workspace overview" },
  { name = "settings", label = "Settings", description = "Shell · Open settings" },
  { name = "appearance", label = "Appearance", description = "Shell · Open appearance" },
  { name = "palette", label = "Palette", description = "Shell · Open the palette" },
  { name = "stats", label = "System statistics", description = "Shell · Open system statistics" },
  { name = "session", label = "Session menu", description = "Session · Session menu" },
  { name = "lock", label = "Lock the screen", description = "Session · Lock the screen" },
  { name = "pet", label = "Pet", description = "Shell · Open the pet" },
  { name = "games", label = "Games", description = "Shell · Open the games" },
  { name = "notes", label = "Notes", description = "Shell · Open the notes" },
  { name = "board", label = "Task board", description = "Shell · Open the task board" },
  { name = "keys", label = "Keys", description = "Shell · Show every key" },
  { name = "packages", label = "Packages", description = "Shell · Open the packages" },
  { name = "clipboard", label = "Clipboard history", description = "Shell · Open the clipboard history" },
  { name = "picker", label = "Colour picker", description = "Shell · Pick a colour off the screen" },
  { name = "capture", label = "Capture", description = "Shell · Open the capture surface" },
  { name = "captureRegion", label = "Capture a region", description = "Shell · Capture a region" },
  { name = "captureWindow", label = "Capture a window", description = "Shell · Capture a window" },
  { name = "captureScreen", label = "Capture the screen", description = "Shell · Capture the whole screen" },
  { name = "captureEdit", label = "Capture and annotate", description = "Shell · Capture a region and annotate it" },
  { name = "captureText", label = "Read a region", description = "Shell · Read a region as text" },
  { name = "record", label = "Record the screen", description = "Shell · Start or stop recording the screen" },
}

local mine = {}
for _, entry in ipairs(M.catalogue) do mine[entry.description] = entry end
function M.mine(description) return mine[description] ~= nil end

-- ---------------------------------------------------------------- table --

M.MODIFIERS = { { bit = 64, name = "SUPER" }, { bit = 4, name = "CTRL" }, { bit = 8, name = "ALT" }, { bit = 1, name = "SHIFT" } }

--- A bind as `hl.bind` spells it: "SUPER + SHIFT + T".
function M.spell(bind)
  local parts = {}
  local mask = tonumber(bind.modmask) or 0
  for _, m in ipairs(M.MODIFIERS) do
    if mask & m.bit ~= 0 then parts[#parts + 1] = m.name end
  end
  local key = tostring(bind.key or "")
  parts[#parts + 1] = key ~= "" and key or ("code:" .. tostring(bind.keycode or 0))
  return table.concat(parts, " + ")
end

--- Every bind with the profile's combination: the compositor's in its
--- order, then the shell's own left unbound, then any other the profile
--- names (Hyprland does not list unbound binds). Switches are left out.
--- The profile's value wins, so a row changes before the reload lands.
--- `{ { description, combination, bound } }`, `bound` Hyprland's.
function M.table()
  s.revision:get()
  local kept = settings.keys or {}
  local rows, seen = {}, {}
  local function add(description, bound)
    if description == "" or seen[description] then return end
    seen[description] = true
    local own = kept[description]
    rows[#rows + 1] = { description = description, bound = bound,
      combination = type(own) == "string" and own or bound }
  end
  for _, bind in ipairs(binds) do
    if not tostring(bind.key or ""):match("^switch:") then add(tostring(bind.description or ""), M.spell(bind)) end
  end
  for _, entry in ipairs(M.catalogue) do add(entry.description, "") end
  local extra = {}
  for description in pairs(kept) do extra[#extra + 1] = description end
  table.sort(extra)
  for _, description in ipairs(extra) do add(description, "") end
  return rows
end

--- The combination on `description`, "" when unbound or not yet read.
function M.current(description)
  for _, row in ipairs(M.table()) do
    if row.description == description then return row.combination end
  end
  return ""
end

--- Mouse binds are shown, not edited: the editor records keys.
function M.fixed(description) return M.current(description):find("mouse", 1, true) ~= nil end

--- The action another bind on `combination` does ("" for none): Hyprland
--- takes duplicate binds and fires both, so this warns first.
function M.clash(combination, exclude)
  local wanted = tostring(combination or ""):upper():gsub("%s+", "")
  if wanted == "" then return "" end
  for _, row in ipairs(M.table()) do
    if row.description ~= exclude and row.combination ~= ""
      and row.combination:upper():gsub("%s+", "") == wanted then
      return row.description:match("^.- · (.+)$") or row.description
    end
  end
  return ""
end

--- Whether rebinding can happen here: Hyprland is running and its binds
--- have been read.
function M.can_rebind()
  s.revision:get()
  return hyprland ~= nil and hyprland.available() and #binds > 0
end

--- The config language of the running Hyprland ("lua", "hyprlang"), or nil.
function M.flavour()
  s.revision:get()
  return config and config.known_flavour() or nil
end

--- Whether Hyprland evidently reads the keys file: the profile keeps keys,
--- and every bind Hyprland lists is on the keys kept for it.
function M.sourced()
  s.revision:get()
  local kept = settings.keys or {}
  local any = false
  for _, bind in ipairs(binds) do
    local wanted = kept[tostring(bind.description or "")]
    if type(wanted) == "string" and wanted ~= "" then
      any = true
      if wanted:upper():gsub("%s+", "") ~= M.spell(bind):upper():gsub("%s+", "") then return false end
    end
  end
  return any
end

--- Puts `combination` on `description` in the profile's keys. The whole
--- table is written, since a profile built from defaults has none stored
--- yet. Refused until the bind list has been read.
function M.rebind(description, combination)
  combination = tostring(combination or "")
  if combination == "" or not M.can_rebind() then return false end
  if combination:find("[\t\r\n]") then return false end
  local rows = M.table()
  local known = false
  for _, row in ipairs(rows) do if row.description == description then known = true end end
  if not known then return false end
  local next = {}
  for _, row in ipairs(rows) do
    next[row.description] = row.description == description and combination or row.combination
  end
  settings.set("keys", next)
  return true
end

-- ------------------------------------------------------------- key names --

-- keysyms to xkb names, written out because the punctuation names cannot
-- be derived. Modifiers are "" (a held Shift does not end the capture).
local KEYSYMS = {
  [0x20] = "space", [0xff09] = "TAB", [0xff0d] = "Return", [0xff8d] = "Return",
  [0xff1b] = "Escape", [0xff08] = "BackSpace", [0xffff] = "Delete", [0xff63] = "Insert",
  [0xff50] = "Home", [0xff57] = "End", [0xff55] = "Prior", [0xff56] = "Next",
  [0xff51] = "Left", [0xff53] = "Right", [0xff52] = "Up", [0xff54] = "Down",
  [0xff61] = "Print", [0x2c] = "comma", [0x2e] = "period", [0x2f] = "slash",
  [0x5c] = "backslash", [0x2d] = "minus", [0x3d] = "equal", [0x3b] = "semicolon",
  [0x27] = "apostrophe", [0x60] = "grave", [0x5b] = "bracketleft", [0x5d] = "bracketright",
}

--- The xkb name Hyprland binds a keysym by, or "" (modifiers, and keys
--- with no plain name).
function M.key_name(keysym)
  keysym = tonumber(keysym) or 0
  if keysym >= 0x61 and keysym <= 0x7a then return string.char(keysym - 32) end
  if keysym >= 0x41 and keysym <= 0x5a then return string.char(keysym) end
  if keysym >= 0x30 and keysym <= 0x39 then return string.char(keysym) end
  if keysym >= 0xffbe and keysym <= 0xffc9 then return "F" .. (keysym - 0xffbe + 1) end
  return KEYSYMS[keysym] or ""
end

--- morf's held modifiers ("ctrl+shift") in bind order and spelling.
function M.modifiers_of(text)
  local held = {}
  for word in tostring(text or ""):gmatch("[^+]+") do held[word:lower()] = true end
  local out = {}
  if held.super or held.logo then out[#out + 1] = "SUPER" end
  if held.ctrl then out[#out + 1] = "CTRL" end
  if held.alt then out[#out + 1] = "ALT" end
  if held.shift then out[#out + 1] = "SHIFT" end
  return out
end

-- ------------------------------------------------------------- the files --

local fs = morf.fs
M.dir = fs.join(fs.dir("state") or ((fs.home() or "") .. "/.local/state"), "impasto-morf")
M.tsv_path = fs.join(M.dir, "keys.tsv")
M.lua_path = fs.join(M.dir, "keys.lua")
M.conf_path = fs.join(M.dir, "keys.conf")
M.source_line = {
  lua = 'pcall(dofile, (os.getenv("XDG_STATE_HOME") or (os.getenv("HOME") .. "/.local/state")) .. "/impasto-morf/keys.lua")',
  hyprlang = "source = " .. M.conf_path,
}

local HEADER = "# The keys of the profile in use: a bind's description, a tab, and its\n"
  .. "# combination, empty for none. Written by impasto from Settings, Keys,\n"
  .. "# and read by keys.lua (a Lua config) or keys.conf (hyprlang); an edit\n"
  .. "# here is overwritten.\n"

local function flat(text) return (tostring(text):gsub("[\t\r\n]+", " "):match("^%s*(.-)%s*$")) end

--- keys.tsv's text for the profile's keys; "" when it has none.
function M.tsv_text(kept)
  kept = kept or settings.peek("keys") or {}
  local names = {}
  for description, combination in pairs(kept) do
    if type(combination) == "string" then names[#names + 1] = description end
  end
  if #names == 0 then return "" end
  table.sort(names)
  local lines = {}
  for _, description in ipairs(names) do lines[#lines + 1] = flat(description) .. "\t" .. flat(kept[description]) end
  return HEADER .. table.concat(lines, "\n") .. "\n"
end

--- keys.lua: wraps `hl.bind` so a bind whose description keys.tsv names
--- takes the combination kept there ("" leaves it unbound).
function M.lua_text()
  return table.concat({
    "-- Written by impasto (services/shortcuts.lua); read by one line in",
    "-- hyprland.lua before any bind. A bind whose description is in keys.tsv",
    "-- takes the combination kept there, and none when it is empty.",
    "local keys = {}",
    string.format("local file = io.open(%q)", M.tsv_path),
    "if file then",
    "  for line in file:lines() do",
    '    local description, combination = line:match("^([^#\t][^\t]*)\t(.*)$")',
    "    if description then keys[description] = combination end",
    "  end",
    "  file:close()",
    "end",
    "if type(hl) == \"table\" and type(hl.bind) == \"function\" then",
    "  local bind = rawget(hl, \"__impasto_bind\") or hl.bind",
    "  hl.__impasto_bind = bind",
    "  hl.bind = function(combination, action, options)",
    "    local wanted = nil",
    "    if type(options) == \"table\" and options.description then wanted = keys[options.description] end",
    "    if wanted == nil then return bind(combination, action, options) end",
    "    if wanted == \"\" then return end",
    "    return bind(wanted, action, options)",
    "  end",
    "end",
    "",
  }, "\n")
end

-- "SUPER + SHIFT + T" as hyprlang's "SUPER SHIFT, T".
local function conf_combination(combination)
  local parts = {}
  for part in tostring(combination):gmatch("[^+]+") do parts[#parts + 1] = part:match("^%s*(.-)%s*$") end
  local key = table.remove(parts)
  return table.concat(parts, " "), key or ""
end

--- keys.conf for a hyprlang config: each bind the profile moves is unbound
--- from where the configuration puts it and bound again on the kept keys,
--- with the dispatcher Hyprland reported. Once the file is sourced
--- Hyprland lists a moved bind on its new keys (or not at all), so where
--- each bind came from is kept in the file itself, on `# from` lines, and
--- read back from `previous` (the file's text) the next time. Lua binds
--- (`__lua`) cannot be re-created this way and are left out.
function M.conf_text(kept, previous)
  kept = kept or settings.peek("keys") or {}
  local origins, order = {}, {}
  for line in tostring(previous or ""):gmatch("[^\n]+") do
    local description, combination, dispatcher, arg = line:match("^# from\t([^\t]*)\t([^\t]*)\t([^\t]*)\t(.*)$")
    if description and not origins[description] then
      origins[description] = { combination = combination, dispatcher = dispatcher, arg = arg }
      order[#order + 1] = description
    end
  end
  for _, bind in ipairs(binds) do
    local description = tostring(bind.description or "")
    local dispatcher = tostring(bind.dispatcher or "")
    if description ~= "" and not origins[description] and dispatcher ~= "" and dispatcher ~= "__lua"
      and not tostring(bind.key or ""):match("^switch:") then
      origins[description] = { combination = M.spell(bind), dispatcher = dispatcher, arg = flat(bind.arg or "") }
      order[#order + 1] = description
    end
  end
  local lines = { "# Written by impasto (services/shortcuts.lua); sourced at the end of",
    "# hyprland.conf. Moves the binds the profile's keys change." }
  for _, description in ipairs(order) do
    local origin = origins[description]
    local wanted = kept[description]
    if type(wanted) == "string" and wanted ~= origin.combination and not description:find("[,\t]") then
      lines[#lines + 1] = table.concat({ "# from", description, origin.combination, origin.dispatcher, origin.arg }, "\t")
      local mods, key = conf_combination(origin.combination)
      lines[#lines + 1] = "unbind = " .. mods .. ", " .. key
      if wanted ~= "" then
        local new_mods, new_key = conf_combination(wanted)
        lines[#lines + 1] = "bindd = " .. new_mods .. ", " .. new_key .. ", " .. description .. ", "
          .. origin.dispatcher .. (origin.arg ~= "" and (", " .. origin.arg) or ",")
      end
    end
  end
  return table.concat(lines, "\n") .. "\n"
end

-- Written only once the file on disk is known and it differs, so a normal
-- start writes and reloads nothing.
local function write_keys()
  if not hyprland or not hyprland.available() or not live.here() then return end
  local text = M.tsv_text()
  local on_disk = fs.read(M.tsv_path) or ""
  if on_disk == text then return end
  act.run("write the profile's keys and reload Hyprland", function()
    fs.mkdir(M.dir)
    fs.write(M.tsv_path, text)
    fs.write(M.lua_path, M.lua_text())
    fs.write(M.conf_path, M.conf_text(nil, fs.read(M.conf_path)))
    if config then config.reload() end
    return true
  end)
end
M.write_keys = write_keys
function M.status() return s.status:get() end

function M.count()
  local n = 0
  for _, group in ipairs(M.sheet()) do n = n + #group.rows end
  return n
end

-- What a term can start: the words of the action and its group, the caps
-- and the xkb names behind them.
local function words_of(group, row)
  local text = (group.name .. " " .. row.action .. " " .. table.concat(row.caps, " ") .. " "
    .. table.concat(row.keys, " ")):lower()
  local out = {}
  for word in text:gmatch("[%w]+") do out[#out + 1] = word end
  for _, key in ipairs(row.keys) do out[#out + 1] = key:lower() end
  return out
end

--- The sheet as one list, headings between groups, keeping the rows every
--- term of `query` begins a word of. Terms split on spaces and on `+`.
function M.find(query)
  local terms = {}
  for term in tostring(query or ""):lower():gmatch("[^%s+]+") do terms[#terms + 1] = term end
  local out = {}
  for _, group in ipairs(M.sheet()) do
    local kept = {}
    for _, row in ipairs(group.rows) do
      local words = words_of(group, row)
      local all = true
      for _, term in ipairs(terms) do
        local any = false
        for _, word in ipairs(words) do
          if word:sub(1, #term) == term then any = true break end
        end
        if not any then all = false break end
      end
      if all then kept[#kept + 1] = row end
    end
    if #kept > 0 then
      out[#out + 1] = { heading = true, name = group.name }
      for _, row in ipairs(kept) do out[#out + 1] = { heading = false, action = row.action, caps = row.caps } end
    end
  end
  return out
end

local started = false
--- Follows the profile's keys into keys.tsv, and reads the binds again 400
--- ms after every reload (the list is stale right after one).
function M.start()
  if started then return end
  started = true
  if not hyprland or not hyprland.available() then return end
  local seen = nil
  morf.effect("impasto.keys.file", function()
    local text = M.tsv_text(settings.keys or {})
    if seen == nil then
      seen = text
      -- Once the settings and the bind list are in.
      morf.timer(2000, write_keys, false)
      return
    end
    if text == seen then return end
    seen = text
    morf.timer(1, write_keys, false)
  end)
  hyprland.on("configreloaded", function() morf.timer(400, M.load, false) end)
  M.load()
end

--- Reads the binds again, so a rebind made since the last opening shows.
function M.load()
  if not hyprland or not hyprland.available or not hyprland.available() then
    if #binds == 0 then s.status:set("Hyprland is not running, so there is no key list to read.") end
    return
  end
  if #binds == 0 then s.status:set("reading") end
  hyprland.binds(function(list, err)
    if type(list) ~= "table" then
      s.status:set("The key list did not come back: " .. tostring(err))
      return
    end
    binds = list
    s.status:set("ok")
    rebuild()
  end)
end

--- A test bench's binds, as Hyprland would print them.
function M.sample()
  local b = {}
  local function add(modmask, key, description)
    b[#b + 1] = { modmask = modmask, key = key, description = description }
  end
  add(64, "SPACE", "Shell · Open the launcher")
  add(64, "A", "Shell · Open the control centre")
  add(64, "TAB", "Shell · Open the workspace overview")
  add(64, "K", "Shell · Show every key")
  add(65, "S", "Shell · Capture a region")
  add(64, "Print", "Shell · Capture the whole screen")
  add(65, "R", "Shell · Start or stop recording the screen")
  add(65, "C", "Shell · Pick a colour off the screen")
  add(64, "Return", "Applications · Terminal")
  add(64, "B", "Applications · Browser")
  add(64, "E", "Applications · Files")
  add(64, "Q", "Windows · Close")
  add(64, "F", "Windows · Fullscreen")
  add(64, "V", "Windows · Float")
  for _, d in ipairs { "left", "right", "up", "down" } do add(64, d, "Windows · Focus " .. d) end
  for _, d in ipairs { "left", "right", "up", "down" } do add(65, d, "Windows · Move " .. d) end
  for i = 1, 9 do add(64, tostring(i), "Workspaces · Workspace " .. i) end
  for i = 1, 9 do add(65, tostring(i), "Workspaces · Send to workspace " .. i) end
  add(64, "mouse_down", "Workspaces · Next")
  add(64, "L", "Session · Lock the screen")
  add(69, "Delete", "Session · Session menu")
  add(0, "XF86AudioRaiseVolume", "Media · Louder")
  add(0, "XF86AudioLowerVolume", "Media · Quieter")
  add(0, "XF86AudioMute", "Media · Mute")
  add(0, "XF86AudioPlay", "Media · Play or pause")
  add(0, "switch:Lid Switch", "Session · Lid")
  binds = b
  s.status:set("ok")
  rebuild()
end

return M
