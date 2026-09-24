-- What the settings push into Hyprland: the window animation preset, the
-- window shadow and hyprglass. Port of the parts of CompositorService.qml
-- and theme/Motion.qml that change the compositor at run time.
--
-- Hyprland's vocabulary, so it goes through lib/hyprland.lua and does
-- nothing under any other compositor. Each is one `eval` of a Lua chunk,
-- as upstream's `hyprctl eval`, so no window animates on a mix of old and
-- new curves; pushed on change, and again after every config reload (which
-- drops what was pushed at run time). One screen pushes (services/live.lua).
-- A dry run logs the chunks instead (services/act.lua).

local settings = require("services.settings")
local theme = require("theme")
local act = require("services.act")
local live = require("services.live")

local M = {}

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

--- The preset as one chunk (Motion.qml `chunk`).
function M.animation_chunk(id)
  local entry = M.preset(id)
  if entry.id == "off" then return "hl.config({ animations = { enabled = false } })" end
  local c = entry.curve
  local lines = {
    "hl.config({ animations = { enabled = true } })",
    string.format('hl.curve("preset", { type = "bezier", points = { {%s, %s}, {%s, %s} } })',
      c[1], c[2], c[3], c[4]),
  }
  local function leaf(name, speed, style)
    local tail = style and (', style = "' .. style .. '"') or ""
    lines[#lines + 1] = string.format(
      'hl.animation({ leaf = "%s", enabled = true, speed = %s, bezier = "preset"%s })', name, speed, tail)
  end
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
  lines[#lines + 1] = 'hl.animation({ leaf = "fadeIn", enabled = false })'
  leaf("fadeOut", entry.fast)
  leaf("fadeLayersIn", entry.medium)
  leaf("fadeLayersOut", entry.fast)
  leaf("border", entry.fast)
  return table.concat(lines, " ")
end

--- The shadow decoration. hyprglass draws through it, so while glass is on
--- it stays enabled but invisible.
function M.shadow_chunk()
  local drawn = settings.windowShadow
  local alpha = drawn and string.format("%02x", math.floor(theme.shadow.opacity * 255 + 0.5)) or "00"
  return string.format(
    'hl.config({ decoration = { shadow = { enabled = %s, range = %d, render_power = 3, color = "rgba(000000%s)" } } })',
    tostring(drawn or settings.windowGlass), drawn and theme.shadow.range or 0, alpha)
end

function M.glass_chunk()
  return "if hl.plugin.hyprglass ~= nil then hl.plugin.hyprglass.config({ enabled = "
    .. tostring(settings.windowGlass == true) .. " }) end"
end

local hyprland
local function push(what, chunk)
  if not hyprland or not hyprland.available() or not live.here() then return end
  act.run("push the " .. what .. " to Hyprland", function()
    hyprland.eval(chunk, function(_, err)
      if err then morf.log("warn", "impasto: Hyprland did not take the " .. what .. ": " .. tostring(err)) end
    end)
    return true
  end)
end

function M.apply_animations() push("animation preset", M.animation_chunk(settings.animationPreset)) end
function M.apply_shadow() push("window shadow", M.shadow_chunk()) end
function M.apply_glass() push("window glass", M.glass_chunk()) end

function M.start()
  if hyprland ~= nil then return end
  local ok, lib = pcall(require, "lib.hyprland")
  hyprland = ok and lib or false
  if not hyprland or not hyprland.available() then return end
  local seen = {}
  local function follow(name, read, apply)
    morf.effect("impasto.compositor." .. name, function()
      local value = read()
      if seen[name] == nil then seen[name] = value return end
      if seen[name] == value then return end
      seen[name] = value
      apply()
    end)
  end
  follow("animations", function() return settings.animationPreset end, M.apply_animations)
  follow("shadow", function() return settings.windowShadow end, M.apply_shadow)
  follow("glass", function() return settings.windowGlass end, function()
    M.apply_glass()
    M.apply_shadow()
  end)
  local function all()
    M.apply_animations()
    M.apply_shadow()
    M.apply_glass()
  end
  hyprland.on("configreloaded", all)
  -- The config only enables animations and leaves the rest off; the
  -- settings are put on it once the shell is up.
  morf.timer(1500, all, false)
end

return M
