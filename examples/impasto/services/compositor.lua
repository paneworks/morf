-- What the settings push into the compositor: the keyboard (layouts, the
-- switch between them, key repeat), the pointer's sensitivity, the cursor
-- (colour, size, shake to find), the window animation preset, the window
-- shadow and hyprglass. Port of CompositorService.qml, the parts of
-- compositor.py that set options and build the cursor, and the chunk of
-- theme/Motion.qml.
--
-- Hyprland's vocabulary, so it goes through lib/hyprland_config.lua (on top
-- of lib/hyprland.lua) and under any other compositor does nothing:
-- `available()` is false and the settings pages say so. The user's
-- configuration files are never written; the values live in impasto's
-- settings and are pushed on change (debounced, so a slider's drag is one
-- push), once after start, and again after every reload of the
-- compositor's configuration, which drops whatever was set at run time.
-- One screen pushes (services/live.lua). A dry run logs instead
-- (services/act.lua).
--
-- The cursor colour is a hyprcursor theme compiled from one set of SVGs,
-- recoloured: only when `hyprcursor-util` is on PATH and the source is
-- present (`$XDG_STATE_HOME/impasto-morf/cursor-src` with its
-- `manifest.hl`, or upstream's `quickshell/cursor-src`). Without them the
-- size is still pushed on the theme in use, and `cursor_note()` says why
-- the colour is not.

local settings = require("services.settings")
local theme = require("theme")
local act = require("services.act")
local live = require("services.live")

local M = {}

local s = {
  -- "", "unavailable", or the flavour of the Hyprland config ("lua",
  -- "hyprlang"), once known.
  status = morf.signal("impasto.compositor.status", ""),
  shake = morf.signal("impasto.compositor.shake", false),
  glass = morf.signal("impasto.compositor.glass", false),
  cursor_note = morf.signal("impasto.compositor.cursor_note", ""),
}
M.signals = s

-- ------------------------------------------------------------ animations --

-- One bezier and three durations in deciseconds (2.6 = 260 ms): slow for
-- geometry, medium for appearing and disappearing, fast for colour.
M.presets = {
  { id = "macos", label = "Glide", curve = { 0.32, 0.72, 0, 1 },
    slow = 2.6, medium = 1.8, fast = 1.4, windows = "slide", workspaces = "slide" },
  { id = "snappy", label = "Brisk", curve = { 0.22, 1, 0.36, 1 },
    slow = 1.3, medium = 1.0, fast = 0.8, windows = "popin 94%", workspaces = "slide" },
  { id = "smooth", label = "Calm", curve = { 0.25, 0.1, 0.25, 1 },
    slow = 4.2, medium = 3.0, fast = 2.2, windows = "popin 85%", workspaces = "fade" },
  { id = "springy", label = "Bounce", curve = { 0.05, 0.9, 0.1, 1.15 },
    slow = 3.2, medium = 2.0, fast = 1.4, windows = "popin 80%", workspaces = "slide" },
  { id = "off", label = "None" },
}

function M.preset(id)
  for _, entry in ipairs(M.presets) do
    if entry.id == id then return entry end
  end
  return M.presets[1]
end

--- The preset as the library's animation spec (Motion.qml `chunk`).
function M.animation_spec(id)
  local entry = M.preset(id)
  if entry.id == "off" then return { enabled = false } end
  local leaves = {}
  local function leaf(name, speed, style) leaves[#leaves + 1] = { name = name, speed = speed, style = style } end
  -- Parent first, so unnamed leaves inherit the preset.
  leaf("global", entry.slow)
  leaf("windows", entry.slow)
  leaf("windowsIn", entry.slow, entry.windows)
  leaf("windowsOut", entry.medium, entry.windows)
  leaf("workspaces", entry.slow, entry.workspaces)
  leaf("workspacesIn", entry.slow, entry.workspaces)
  leaf("workspacesOut", entry.slow, entry.workspaces)
  leaf("layers", entry.medium)
  leaf("layersIn", entry.medium, "fade")
  leaf("layersOut", entry.fast, "fade")
  leaf("fade", entry.medium)
  -- No fade-in: a half-transparent new window shows the wallpaper through
  -- the gap being opened.
  leaves[#leaves + 1] = { name = "fadeIn", enabled = false }
  leaf("fadeOut", entry.fast)
  leaf("fadeLayersIn", entry.medium)
  leaf("fadeLayersOut", entry.fast)
  leaf("border", entry.fast)
  return { enabled = true, curve = { name = "preset", points = entry.curve }, leaves = leaves }
end

-- ------------------------------------------------------------- the rest --

--- The shadow decoration's options. hyprglass draws through it, so while
--- glass is on it stays enabled but invisible.
function M.shadow_options()
  local drawn = settings.windowShadow
  local alpha = drawn and string.format("%02x", math.floor(theme.shadow.opacity * 255 + 0.5)) or "00"
  return {
    { "decoration:shadow:enabled", "bool", drawn or settings.windowGlass },
    { "decoration:shadow:range", "int", drawn and theme.shadow.range or 0 },
    { "decoration:shadow:render_power", "int", 3 },
    { "decoration:shadow:color", "colour", "rgba(000000" .. alpha .. ")" },
  }
end

--- The keyboard and the pointer, as Hyprland's input options.
function M.input_options()
  local layouts = tostring(settings.keyboardLayouts or ""):gsub("%s+", "")
  if layouts == "" then layouts = "us" end
  return {
    { "input:kb_layout", "str", layouts },
    { "input:kb_options", "str", settings.keyboardSwitch or "" },
    { "input:repeat_rate", "int", settings.keyRepeatRate },
    { "input:sensitivity", "float", settings.pointerSensitivity },
  }
end

local config

--- Whether there is a Hyprland to push to. Inside a binding it follows the
--- status.
function M.available()
  s.status:get()
  return config ~= nil and config ~= false and config.available()
end

function M.status() return s.status:get() end
function M.shake_available() return s.shake:get() end
function M.glass_available() return s.glass:get() end
function M.cursor_note() return s.cursor_note:get() end

local function push(what, build)
  if not M.available() or not live.here() then return end
  act.run("push the " .. what .. " to Hyprland", function()
    config.apply(build, function(ok, replies)
      if not ok then
        morf.log("warn", "impasto: Hyprland did not take the " .. what .. ": "
          .. table.concat(replies or {}, " | "):sub(1, 200))
      end
    end)
    return true
  end)
end

function M.apply_animations()
  push("animation preset", function(how) return config.animation_plan(M.animation_spec(settings.animationPreset), how) end)
end

function M.apply_shadow()
  push("window shadow", function(how) return config.options_plan(M.shadow_options(), how) end)
end

function M.apply_glass()
  push("window glass", function(how)
    if how == "lua" then
      -- `hl.plugin.hyprglass` does not exist until the plugin loads.
      return { { eval = "if hl.plugin.hyprglass ~= nil then hl.plugin.hyprglass.config({ enabled = "
        .. tostring(settings.windowGlass == true) .. " }) end" } }
    end
    if not s.glass:get() then return {} end
    return config.options_plan({ { "plugin:hyprglass:enabled", "bool", settings.windowGlass == true } }, how)
  end)
end

function M.apply_input()
  push("keyboard and pointer", function(how) return config.options_plan(M.input_options(), how) end)
end

function M.apply_shake()
  if not s.shake:get() then return end
  push("shake to find", function(how)
    return config.options_plan({ { "plugin:dynamic_cursors:shake:enabled", "bool", settings.shakeToFind == true } }, how)
  end)
end

-- ----------------------------------------------------------------- cursor --

local fs = morf.fs
local CURSOR_BASE = "impasto-cursor"
-- Bibata's body colours and its white outline; the busy spinner's four
-- colours are not in the set, so they survive.
local CURSOR_BODY = { "#000000", "#ff8300" }
local CURSOR_OUTLINE = "#ffffff"

--- The cursor colour as #rrggbb: "palette" is the accent.
function M.cursor_hex()
  local id = settings.cursorColor
  local colour = id == "palette" and tostring(theme.color.accent()) or tostring(id)
  local hex = colour:match("^#(%x+)$")
  if not hex or (#hex ~= 6 and #hex ~= 8) then return nil end
  return "#" .. hex:sub(-6):lower()
end

local function state_dir() return fs.dir("state") or ((fs.home() or "") .. "/.local/state") end

local function cursor_source()
  for _, candidate in ipairs {
    fs.join(state_dir(), "impasto-morf", "cursor-src"),
    fs.join(state_dir(), "quickshell", "cursor-src"),
  } do
    if fs.is_file(fs.join(candidate, "manifest.hl")) then return candidate end
  end
  return nil
end

local function is_light(hex)
  local r, g, b = tonumber(hex:sub(2, 3), 16), tonumber(hex:sub(4, 5), 16), tonumber(hex:sub(6, 7), 16)
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 > 0.6
end

--- An SVG's body painted `body`, its outline `outline`. Through
--- placeholders, so a white body is not blackened with a flipped outline.
function M.recolour_svg(text, body, outline)
  local function ci(hex)
    return (hex:gsub("%x", function(c)
      if c:match("%a") then return "[" .. c:lower() .. c:upper() .. "]" end
      return c
    end))
  end
  text = text:gsub(ci(CURSOR_OUTLINE), "__OUTLINE__")
  for _, source in ipairs(CURSOR_BODY) do text = text:gsub(ci(source), "__BODY__") end
  return (text:gsub("__BODY__", body):gsub("__OUTLINE__", outline))
end

local serial = 0
local building = false

-- Recolours the source into the cache, compiles it with hyprcursor-util
-- into ~/.local/share/icons/impasto-cursor-<n> (a new name each time: the
-- shake plugin caches by name), sets it, and drops older builds.
local function build_cursor(hex, size, done)
  local source = cursor_source()
  local tool = act.which("hyprcursor-util")
  if not tool or not source then
    s.cursor_note:set(not tool
      and "hyprcursor-util is not installed, so only the size is applied"
      or "No cursor source to colour (cursor-src), so only the size is applied")
    return done(nil)
  end
  if building then return done(nil) end
  building = true
  local icons = fs.join(fs.dir("data") or (fs.home() .. "/.local/share"), "icons")
  local existing = fs.list(icons) or {}
  for _, entry in ipairs(existing) do
    local n = tonumber(tostring(entry.name):match("^" .. CURSOR_BASE .. "%-(%d+)$"))
    if n and n > serial then serial = n end
  end
  serial = serial + 1
  local name = CURSOR_BASE .. "-" .. serial
  local work = fs.join(fs.dir("cache") or (fs.home() .. "/.cache"), "impasto-morf", "cursor-build")
  fs.remove(work, { recursive = true })
  fs.mkdir(work)
  local src = fs.join(work, "src")
  fs.copy(source, src, { recursive = true })
  local outline = is_light(hex) and "#000000" or CURSOR_OUTLINE
  local manifest = fs.join(src, "manifest.hl")
  fs.write(manifest, (tostring(fs.read(manifest) or ""):gsub("\nname%s*=[^\n]*", "\nname = " .. name)
    :gsub("^name%s*=[^\n]*", "name = " .. name)))
  for _, entry in ipairs(fs.list(src, { depth = 8 }) or {}) do
    if not entry.is_dir and tostring(entry.name):match("%.svg$") then
      local text = fs.read(entry.path)
      if text then fs.write(entry.path, M.recolour_svg(text, hex, outline)) end
    end
  end
  fs.mkdir(icons)
  act.collect("compile the cursor", tool, { "--create", src, "--output", icons }, function(_, ok)
    building = false
    local built = fs.join(icons, "theme_" .. name)
    if not ok or not fs.exists(built) then
      s.cursor_note:set("hyprcursor-util could not build the cursor, so only the size is applied")
      return done(nil)
    end
    fs.remove(fs.join(icons, name), { recursive = true })
    fs.rename(built, fs.join(icons, name))
    -- Keep this build and the one before (a nested session may use it).
    for _, entry in ipairs(fs.list(icons) or {}) do
      local n = tonumber(tostring(entry.name):match("^" .. CURSOR_BASE .. "%-(%d+)$"))
      if n and n < serial - 1 then fs.remove(entry.path, { recursive = true }) end
    end
    s.cursor_note:set("")
    done(name)
  end, { mutates = true, timeout_ms = 60000 })
end

local last_theme = nil

function M.apply_cursor()
  if not M.available() or not live.here() then return end
  local hex = M.cursor_hex()
  local size = math.floor(tonumber(settings.cursorSize) or 24)
  local function set(name)
    local function env(key) local v = morf.env(key) return v ~= "" and v or nil end
    name = name or last_theme or env("HYPRCURSOR_THEME") or env("XCURSOR_THEME")
    -- No theme of our own and none named: the one in use is unknown, and
    -- guessing would change it.
    if not name then return end
    last_theme = name
    push("cursor", function() return config.cursor_plan(name, size) end)
  end
  if not hex or act.dry then return set(nil) end
  build_cursor(hex, size, set)
end

-- ------------------------------------------------------------------ start --

function M.start()
  if config ~= nil then return end
  local ok, lib = pcall(require, "lib.hyprland_config")
  config = ok and lib or false
  if not config or not config.available() then
    s.status:set("unavailable")
    return
  end
  config.flavour(function(how) s.status:set(how or "unavailable") end)
  local function plugins()
    config.plugin_available("plugin:dynamic_cursors:shake:enabled", function(on) s.shake:set(on) end)
    config.plugin_available("plugin:hyprglass:enabled", function(on) s.glass:set(on) end)
  end
  plugins()

  -- Each follows its settings; a burst of changes (a slider dragged) is one
  -- push once it rests.
  local seen, timers = {}, {}
  local function follow(name, read, apply, delay)
    morf.effect("impasto.compositor." .. name, function()
      local value = read()
      if seen[name] == nil then seen[name] = value return end
      if seen[name] == value then return end
      seen[name] = value
      if timers[name] then timers[name]:cancel() end
      timers[name] = morf.timer(delay or 120, function() timers[name] = nil apply() end, false)
    end)
  end
  follow("animations", function() return settings.animationPreset end, M.apply_animations)
  follow("shadow", function() return settings.windowShadow end, M.apply_shadow)
  follow("glass", function() return settings.windowGlass end, function()
    M.apply_glass()
    M.apply_shadow()
  end)
  follow("input", function()
    return table.concat({ tostring(settings.keyboardLayouts), tostring(settings.keyboardSwitch),
      tostring(settings.keyRepeatRate), tostring(settings.pointerSensitivity) }, "|")
  end, M.apply_input)
  follow("shake", function() return settings.shakeToFind end, M.apply_shake)
  -- The accent animates between palettes; wait for it to settle rather
  -- than recompile the cursor on every frame.
  follow("cursor", function()
    return tostring(M.cursor_hex()) .. ":" .. tostring(settings.cursorSize)
  end, M.apply_cursor, 450)

  local function all()
    M.apply_animations()
    M.apply_shadow()
    M.apply_glass()
    M.apply_input()
    M.apply_shake()
    M.apply_cursor()
  end
  config.on_reload(function()
    plugins()
    all()
  end)
  -- The config only enables animations and leaves the rest off; the
  -- settings are put on it once the shell is up.
  morf.timer(1500, all, false)
end

return M
