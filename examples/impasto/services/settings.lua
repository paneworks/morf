-- Settings: every preference the shell has, its default here and the
-- user's overrides on disk.
--
-- Port of impasto's SettingsService. The file holds only what differs from
-- the defaults, so a default changed here reaches everyone who never touched
-- it. A read inside a binding tracks that one key; a write saves the file a
-- moment later, once, however many keys a handler changed.
--
-- Every screen's runtime holds its own copy, so the file is watched: a
-- change written by another screen (or by hand) is read back, and only the
-- keys that moved re-run what follows them. A value of the wrong type on
-- disk is dropped for the default, and an unknown key is a warning, never an
-- error. On a first start the upstream shell's own settings file
-- ($XDG_STATE_HOME/quickshell/settings.json) is taken in when there is one.

local json = morf.json
local fs = morf.fs

local M = {}

M.defaults = {
  clockFormat = "%H:%M",
  clockShowsDate = false,
  clockShowsSeconds = false,
  barHeight = 32,
  barMargin = 16,
  islandAttached = false,
  barFullWidth = false,
  barSideMargin = 18,
  barEverywhere = true,
  barStyle = "grouped",            -- grouped | spread | capsule
  barLeft = false,                 -- false: the catalogue's default side
  barRight = false,
  islandSummary = true,
  islandActivities = false,
  chipShape = "icon",              -- icon | ring
  chipFigure = "on",               -- on | off | hover
  windowShadow = false,
  windowGlass = false,
  wallpaperTransition = "wipe",
  greeting = "random",
  launcherResults = 9,
  launcherOrder = "recent",
  launcherFits = false,
  clipboardHistory = true,
  clipboardKeep = 200,
  clipboardImages = true,
  clipboardWipeOnLock = false,
  lockBlur = 32,
  lockClock = "stacked",
  userName = "",
  userAvatar = "",
  doNotDisturb = false,
  recorderAudio = false,
  captureShape = "region",
  captureKind = "photo",
  notesHandwriting = true,
  deckOnEmpty = false,
  spectrumOnEmpty = false,
  workspaceCount = 5,
  workspaceMax = 10,
  motionScale = 100,
  motionCurve = "OutCubic",
  animationPreset = "macos",
  fontFamily = "Inter",
  fontMono = "JetBrainsMono Nerd Font Mono",
  notificationTimeout = 5000,
  desktopWidgets = {},
  desktopTheme = "modern",
  desktopStyle = "capsule",
  desktopOpacity = 100,
  centreButtons = false,
  centreBlocks = false,
  centreToggles = false,
  dockEnabled = true,
  dockPinned = {},
  dockEdge = "bottom",
  dockAlignment = "center",
  dockIconSize = 44,
  dockOpacity = 100,
  dockRunning = true,
  dockEverywhere = true,
  dockAutohide = false,
  dockLauncher = true,
  weatherPlace = "",
  githubUser = "",
  petStyle = "creature",
  idleLock = 0,
  idleScreen = 0,
  idleSuspend = 0,
  nightLight = false,
  nightTemperature = 4000,
  cursorSize = 24,
  -- Kept here and pushed to Hyprland at run time (services/compositor.lua),
  -- so the user's Hyprland configuration files are never written.
  cursorColor = "palette",
  shakeToFind = false,
  keyboardLayouts = "us",
  keyboardSwitch = "",
  keyRepeatRate = 25,
  pointerSensitivity = 0,
  lidPolicy = "system",            -- off | keep | system
  -- The screens' arrangement, one per set of connected monitors
  -- (services/displays.lua): set key -> { primary, mirror, monitors =
  -- { [description] = { position, scale, transform, vrr, mode, disabled } } }.
  displays = {},
  -- The profile's keys: bind description -> combination ("SUPER + T", ""
  -- for unbound), written to keys.tsv for Hyprland (services/shortcuts.lua).
  keys = {},
  -- Launcher sigil overrides by mode id; absent means `launcher_prefix_defaults`.
  launcherPrefixes = {},
  wallpaper = "",
  wallpaperDir = "~/.local/share/wallpapers",
  theme = "adaptive",
  -- Off by default: writing other programs' colour files is writing into
  -- the user's dotfiles.
  writeAppThemes = false,
  language = "en",
}

M.dir = fs.join(fs.dir("config") or (fs.home() .. "/.config"), "impasto-morf")
M.path = fs.join(M.dir, "settings.json")

local values = {}
local revisions = {}
local saving = false
-- The text last read from or written to the file: a watch event that finds
-- it unchanged is this runtime's own write.
local last_text = nil

--- Whether there was no settings file when the shell started: a fresh
--- install, which the first shipped profile may take over.
M.fresh = not fs.is_file(M.path)

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

local function equal(a, b)
  if type(a) ~= type(b) then return false end
  if type(a) ~= "table" then return a == b end
  for k, v in pairs(a) do if not equal(v, b[k]) then return false end end
  for k in pairs(b) do if a[k] == nil then return false end end
  return true
end

-- morf.json hands arrays and objects back as tagged tables; the settings
-- keep plain Lua values.
local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

for key, default in pairs(M.defaults) do
  values[key] = copy(default)
  revisions[key] = morf.signal("impasto.settings." .. key, 0)
end

--- Whether `value` can stand for `key`: the default's type (a number
--- finite), and a list for the keys whose default is false ("the
--- catalogue's own").
function M.accepts(key, value)
  local default = M.defaults[key]
  if default == nil or value == nil then return false end
  if type(value) == "number" and (value ~= value or value == math.huge or value == -math.huge) then
    return false
  end
  if type(value) == type(default) then return true end
  return default == false and type(value) == "table"
end

--- What the file says, as key -> value, keeping only known keys of the
--- right type; nil when there is no file or it is not JSON.
local function read_file()
  local text = fs.read(M.path)
  if not text or text == "" then return nil, text end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then
    morf.log("warn", "impasto: settings file is not JSON; keeping what there is")
    return nil, text
  end
  local out = {}
  for key, value in pairs(plain(decoded)) do
    if M.defaults[key] == nil then
      -- An older or newer shell's key: left in the file, not taken.
    elseif value == json.null then
      -- null is "the default".
    elseif M.accepts(key, value) then
      out[key] = value
    else
      morf.log("warn", "impasto: setting " .. key .. " is not a " .. type(M.defaults[key])
        .. "; using the default")
    end
  end
  return out, text
end

-- Values spelled as upstream spells them, or as older versions of this
-- port did, read as the port's own: upstream calls the single capsule
-- "island" (SettingsService.qml barStyles), the port "capsule".
local ALIASES = { barStyle = { island = "capsule" } }
local function canonical(key, value)
  local names = ALIASES[key]
  return names and names[value] or value
end

local function load()
  local stored, text = read_file()
  last_text = text
  if not stored then return end
  for key, value in pairs(stored) do values[key] = canonical(key, value) end
end

local function save()
  saving = false
  local out = {}
  for key, value in pairs(values) do
    if not equal(value, M.defaults[key]) then out[key] = value end
  end
  local text = json.encode(out, true)
  last_text = text
  local ok, err = fs.write(M.path, text)
  if not ok then morf.log("warn", "impasto: could not save settings: " .. tostring(err)) end
end

--- The file changed under this runtime: another screen saved, or a person
--- edited it. Every key takes the file's value (or the default when the file
--- leaves it out), and only those that moved are announced.
local function reread()
  local stored, text = read_file()
  if text == last_text then return end
  last_text = text
  if not stored then return end
  for key, default in pairs(M.defaults) do
    local value = canonical(key, stored[key])
    if value == nil then value = copy(default) end
    if not equal(values[key], value) then
      values[key] = value
      revisions[key]:set(revisions[key]:get() + 1)
    end
  end
end
M.reread = reread

local warned = {}
local function unknown(key)
  if warned[key] then return end
  warned[key] = true
  morf.log("warn", "impasto: unknown setting " .. tostring(key))
end

--- The value of `key`; inside a binding, the binding follows it.
function M.get(key)
  local revision = revisions[key]
  if not revision then
    unknown(key)
    return nil
  end
  revision:get()
  return values[key]
end

--- The value of `key` without tracking it: for code building a view that
--- must not be built again when this key moves.
function M.peek(key) return values[key] end

--- Sets `key`, and saves the file once this handler is done.
function M.set(key, value)
  if M.defaults[key] == nil then
    unknown(key)
    return
  end
  value = canonical(key, value)
  if not M.accepts(key, value) then
    morf.log("warn", "impasto: setting " .. key .. " is not a " .. type(M.defaults[key]))
    return
  end
  if equal(values[key], value) then return end
  values[key] = copy(value)
  revisions[key]:set(revisions[key]:get() + 1)
  if not saving then
    saving = true
    morf.timer(250, save, false)
  end
end

--- Back to the default.
function M.reset(key) M.set(key, copy(M.defaults[key])) end

-- ------------------------------------------------------------- profiles --

-- What belongs to this machine and this person rather than to a look: a
-- profile never carries these, and switching one leaves them alone.
M.machine_keys = {
  lidPolicy = true, displays = true, userName = true, userAvatar = true, language = true,
  keyboardLayouts = true, keyboardSwitch = true, weatherPlace = true, githubUser = true,
  doNotDisturb = true, nightLight = true, nightTemperature = true,
  recorderAudio = true, captureShape = true, captureKind = true,
  wallpaper = true, wallpaperDir = true, theme = true, writeAppThemes = true,
}

-- ------------------------------------------------------ launcher sigils --

-- The first character that selects each launcher mode (SettingsService's
-- launcherPrefixDefaults). Overrides are kept by mode id.
M.launcher_prefix_defaults = {
  calculate = "=", desk = ">", windows = "@", timer = "!", clipboard = "'",
}

--- The sigil for a mode: the override when it is one character, else the
--- default. Inside a binding it follows the setting.
function M.launcher_prefix(id)
  local kept = M.get("launcherPrefixes") or {}
  local chosen = kept[id]
  if type(chosen) == "string" and utf8.len(chosen) == 1 then return chosen end
  return M.launcher_prefix_defaults[id] or ""
end

--- A new sigil for a mode; "" or the default drops the override.
function M.set_launcher_prefix(id, sigil)
  local next = copy(values.launcherPrefixes or {})
  if sigil == nil or sigil == "" or sigil == M.launcher_prefix_defaults[id] then
    next[id] = nil
  else
    next[id] = sigil
  end
  M.set("launcherPrefixes", next)
end

-- ------------------------------------------------- the upstream format --

-- The QML shell writes a few values differently: Qt's time formats rather
-- than strftime, "island" for the one-capsule bar, null for "the
-- catalogue's own", and a font list rather than one family.
local QT_TO_STRFTIME = { { "HH", "%H" }, { "hh", "%I" }, { "H", "%H" }, { "h", "%I" },
  { "mm", "%M" }, { "ss", "%S" }, { "AP", "%p" }, { "ap", "%P" } }

--- "HH:mm" -> "%H:%M", "hh:mm AP" -> "%I:%M %p".
function M.qt_time_to_strftime(format)
  local out, i = {}, 1
  while i <= #format do
    local matched = false
    for _, pair in ipairs(QT_TO_STRFTIME) do
      if format:sub(i, i + #pair[1] - 1) == pair[1] then
        out[#out + 1] = pair[2]
        i = i + #pair[1]
        matched = true
        break
      end
    end
    if not matched then
      local c = format:sub(i, i)
      out[#out + 1] = c == "%" and "%%" or c
      i = i + 1
    end
  end
  return table.concat(out)
end

--- "%H:%M" -> "HH:mm", the other way.
function M.strftime_to_qt(format)
  local map = { H = "HH", I = "hh", M = "mm", S = "ss", p = "AP", P = "ap", ["%"] = "%" }
  return (format:gsub("%%(.)", function(c) return map[c] or ("%" .. c) end))
end

-- Keys whose "catalogue's own" is null upstream and false here.
local NULL_DEFAULTS = {
  barLeft = true, barRight = true, islandActivities = true,
  centreButtons = true, centreBlocks = true, centreToggles = true,
}

--- Upstream values in this port's words. Unknown keys are left for the
--- caller to drop.
function M.from_upstream(given)
  local out = copy(given or {})
  if out.barStyle == "island" then out.barStyle = "capsule" end
  if type(out.clockFormat) == "string" and not out.clockFormat:find("%%") then
    out.clockFormat = M.qt_time_to_strftime(out.clockFormat)
  end
  if type(out.fontFamily) == "string" then out.fontFamily = out.fontFamily:match("^%s*([^,]+)") or out.fontFamily end
  if type(out.fontMono) == "string" then out.fontMono = out.fontMono:match("^%s*([^,]+)") or out.fontMono end
  for key in pairs(NULL_DEFAULTS) do
    if out[key] == json.null then out[key] = false end
  end
  return out
end

--- This port's values in the upstream shell's words, for an export it can
--- read.
function M.to_upstream(given)
  local out = copy(given or {})
  if out.barStyle == "capsule" then out.barStyle = "island" end
  if type(out.clockFormat) == "string" then out.clockFormat = M.strftime_to_qt(out.clockFormat) end
  for key in pairs(NULL_DEFAULTS) do
    if out[key] == false then out[key] = json.null end
  end
  return out
end

--- On a first start, the upstream shell's settings, where it left them.
--- Returns how many keys were taken.
local function import_upstream()
  local state = fs.dir("state") or ((fs.home() or "") .. "/.local/state")
  local path = fs.join(state, "quickshell", "settings.json")
  local text = fs.read(path)
  if not text or text == "" then return 0 end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then return 0 end
  local taken = 0
  for key, value in pairs(M.from_upstream(plain(decoded))) do
    if M.accepts(key, value) and not equal(value, M.defaults[key]) then
      values[key] = value
      taken = taken + 1
    end
  end
  if taken > 0 then
    morf.log("info", "impasto: took " .. taken .. " settings from " .. path)
    save()
  end
  return taken
end

--- The keys a profile holds, sorted.
function M.profile_keys()
  local out = {}
  for key in pairs(M.defaults) do
    if not M.machine_keys[key] then out[#out + 1] = key end
  end
  table.sort(out)
  return out
end

--- A whole profile out of `given`: every profile key, the given value where
--- it is accepted, the default elsewhere. Also returns how many given keys
--- were left out.
function M.complete(given)
  given = type(given) == "table" and given or {}
  local out, skipped = {}, 0
  for key, value in pairs(given) do
    if M.defaults[key] == nil or M.machine_keys[key] or not M.accepts(key, value) then
      skipped = skipped + 1
    end
  end
  for _, key in ipairs(M.profile_keys()) do
    local value = given[key]
    out[key] = copy(M.accepts(key, value) and value or M.defaults[key])
  end
  return out, skipped
end

--- The profile keys as they are now, as plain data.
function M.snapshot()
  local out = {}
  for _, key in ipairs(M.profile_keys()) do out[key] = copy(values[key]) end
  return out
end

--- Takes on a whole profile in one handler, so the file is written once.
function M.adopt(profile)
  local whole = M.complete(profile)
  for key, value in pairs(whole) do M.set(key, value) end
end

--- Every profile key back to its default.
function M.reset_all()
  for _, key in ipairs(M.profile_keys()) do M.reset(key) end
end

--- `settings.barHeight` reads like a field, and tracks like `get`.
setmetatable(M, {
  __index = function(_, key)
    if revisions[key] then return M.get(key) end
    return nil
  end,
})

load()
if M.fresh then
  local ok, taken = pcall(import_upstream)
  if ok and taken > 0 then M.imported = taken end
end

-- The file, watched: another screen's save, or a hand edit, is read back
-- (services/watch.lua). A change while this runtime's own save is pending
-- waits for that save, which then wins.
fs.mkdir(M.dir)
require("services.watch").file(M.path, function()
  if not saving then reread() end
end)

return M
