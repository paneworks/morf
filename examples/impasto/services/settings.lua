-- Settings: every preference the shell has, its default here and the
-- user's overrides on disk.
--
-- Port of impasto's SettingsService. The file holds only what differs from
-- the defaults, so a default changed here reaches everyone who never touched
-- it. A read inside a binding tracks that one key; a write saves the file a
-- moment later, once, however many keys a handler changed.

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
  -- Kept in impasto's own settings only: nothing here is pushed to the
  -- compositor, so the user's Hyprland configuration is never written.
  cursorColor = "palette",
  shakeToFind = false,
  keyboardLayouts = "us",
  keyboardSwitch = "",
  keyRepeatRate = 25,
  pointerSensitivity = 0,
  lidPolicy = "system",            -- off | keep | system
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

local function load()
  local text = fs.read(M.path)
  if not text or text == "" then return end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then
    morf.log("warn", "impasto: settings file is not JSON; keeping the defaults")
    return
  end
  for key, value in pairs(plain(decoded)) do
    if M.defaults[key] ~= nil and value ~= json.null then values[key] = value end
  end
end

local function save()
  saving = false
  local out = {}
  for key, value in pairs(values) do
    if not equal(value, M.defaults[key]) then out[key] = value end
  end
  local ok, err = fs.write(M.path, json.encode(out, true))
  if not ok then morf.log("warn", "impasto: could not save settings: " .. tostring(err)) end
end

--- The value of `key`; inside a binding, the binding follows it.
function M.get(key)
  local revision = revisions[key]
  if not revision then error("impasto: unknown setting " .. tostring(key), 2) end
  revision:get()
  return values[key]
end

--- Sets `key`, and saves the file once this handler is done.
function M.set(key, value)
  if M.defaults[key] == nil then error("impasto: unknown setting " .. tostring(key), 2) end
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
  lidPolicy = true, userName = true, userAvatar = true, language = true,
  keyboardLayouts = true, keyboardSwitch = true, weatherPlace = true, githubUser = true,
  doNotDisturb = true, nightLight = true, nightTemperature = true,
  recorderAudio = true, captureShape = true, captureKind = true,
  wallpaper = true, wallpaperDir = true, theme = true, writeAppThemes = true,
}

--- The keys a profile holds, sorted.
function M.profile_keys()
  local out = {}
  for key in pairs(M.defaults) do
    if not M.machine_keys[key] then out[#out + 1] = key end
  end
  table.sort(out)
  return out
end

--- Whether `value` can stand for `key`: the default's type, and a list for
--- the keys whose default is false ("the catalogue's own").
function M.accepts(key, value)
  local default = M.defaults[key]
  if default == nil or value == nil then return false end
  if type(value) == type(default) then return true end
  return default == false and type(value) == "table"
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

return M
