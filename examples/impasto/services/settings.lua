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
  wallpaper = "",
  wallpaperDir = "~/.local/share/wallpapers",
  theme = "adaptive",
  language = "en",
}

M.dir = fs.join(fs.dir("config") or (fs.home() .. "/.config"), "impasto-morf")
M.path = fs.join(M.dir, "settings.json")

local values = {}
local revisions = {}
local saving = false

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
  local ok, err = fs.write(M.path, json.encode(out, { pretty = true }))
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

--- `settings.barHeight` reads like a field, and tracks like `get`.
setmetatable(M, {
  __index = function(_, key)
    if revisions[key] then return M.get(key) end
    return nil
  end,
})

load()

return M
