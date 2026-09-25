-- Colours, type, sizes and motion.
--
-- Colours are a Material 3 scheme (lib/material.lua): every role --
-- primary, onSurfaceVariant, surfaceContainerHigh, ... -- is a token of one
-- `morf.theme`, so a binding that reads `theme.color.primary` follows a new
-- wallpaper with no wiring. The scheme comes from the wallpaper (tonal_spot,
-- dark) unless the settings name a source colour.
--
-- Sizes were measured off the reference at 1920x1080: a 10 px frame, a
-- 60 px bar, 25 px corners, 40 px pills.

local morf = require("morf")
local material = require("lib.material")
local config = require("config")

local M = {}

-- ------------------------------------------------------------------ colour --

-- A pink source until the wallpaper's scheme is in: the colours the
-- reference shows before it has a scheme of its own.
M.FALLBACK_SOURCE = "#ffb0ca"

local function initial()
  local source = config.get("theme.source")
  if source == "" or source == "wallpaper" then source = M.FALLBACK_SOURCE end
  return material.scheme(source, { variant = config.get("theme.variant"), mode = config.get("theme.mode") })
end

local roles = {}
for name, value in pairs(initial()) do
  local ok, red = pcall(function() return value.r end)
  if type(value) ~= "string" and ok and type(red) == "number" then roles[name] = value end
end
M.color = morf.theme(roles)

--- Puts every role of scheme `s` in place.
function M.apply(s)
  for name in pairs(roles) do
    if s[name] ~= nil then M.color[name] = s[name] end
  end
end

--- The scheme for `source` (a colour, or "wallpaper" with `wallpaper` the
--- picture's path), applied when it is made.
function M.follow(source, wallpaper)
  local opts = { variant = config.get("theme.variant"), mode = config.get("theme.mode") }
  if source ~= "wallpaper" and source ~= "" then
    M.apply(material.scheme(source, opts))
    return
  end
  if not wallpaper or wallpaper == "" then return end
  material.from_image(wallpaper, opts, function(ok, s)
    if ok then M.apply(s) else morf.log("warn", "caelestia: no scheme from the wallpaper: " .. tostring(s)) end
  end)
end

-- -------------------------------------------------------------------- type --

-- The reference sets its type in Google Sans Flex; Rubik (OFL, shipped in
-- fonts/ beside the configuration, which morf puts on the font path) stands
-- in where that is not installed. `appearance.font_file` (or
-- CAELESTIA_FONT_FILE) names a font file to load for every label instead.
M.font = "Google Sans Flex, Rubik"
M.font_file = (morf.env and morf.env("CAELESTIA_FONT_FILE")) or config.get("appearance.font_file")
if M.font_file ~= "" and not morf.fs.exists(M.font_file) then M.font_file = "" end
M.icon_font = "Material Symbols Rounded"
M.mono = "CaskaydiaCove Nerd Font, JetBrainsMono Nerd Font, monospace"

-- In pixels. The reference sets type in points, which Qt draws at 4/3 of
-- a pixel each on a 96 dpi screen: these are its sizes as they land.
M.size = {
  small = 14, smaller = 15, normal = 16, larger = 17.5, large = 20, extra = 30,
}

-- ------------------------------------------------------------------ sizes --

M.BORDER = 10        -- the frame round the screen
M.BAR = 60           -- the bar, frame included
M.ROUNDING = 25      -- the frame's inner corners, drawers, cards
M.SEAM = 25          -- the fillet where a drawer meets the frame
M.PAD = 16           -- inside a drawer
M.GAP = 12           -- between cards

-- --------------------------------------------------------------- motion --

-- Material 3's curves, as cubic Béziers (the published control points).
M.ease = {
  standard = { x1 = 0.2, y1 = 0, x2 = 0, y2 = 1 },
  standard_decel = { x1 = 0, y1 = 0, x2 = 0, y2 = 1 },
  standard_accel = { x1 = 0.3, y1 = 0, x2 = 1, y2 = 1 },
  emphasized_decel = { x1 = 0.05, y1 = 0.7, x2 = 0.1, y2 = 1 },
  emphasized_accel = { x1 = 0.3, y1 = 0, x2 = 0.8, y2 = 0.15 },
  -- Material 3 Expressive's default spatial curve: out past the end by a
  -- touch and back.
  spatial = { x1 = 0.38, y1 = 1.21, x2 = 0.22, y2 = 1 },
  -- The emphasized curve: a slow start, then most of the way at once, then
  -- a long settle -- two segments (Material's own definition).
  emphasized = { spline = { 0.05, 0, 0.133333, 0.06, 0.166666, 0.4, 0.208333, 0.82, 0.25, 1, 1, 1 } },
}

-- Durations and curves fitted to films of the reference in the sandbox
-- (tools/sandbox/caelestia-motion.steps): a drawer opens on the spatial
-- curve over about 450 ms, overshooting by about 1 % and settling, and
-- closes on the emphasized accelerate curve in about 200 ms.
M.duration = {
  small = 200, normal = 400, large = 600,
  drawer_open = 450, drawer_close = 200,
}

return M
