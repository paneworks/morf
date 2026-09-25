-- Theme: colours, sizes, type and motion, as tokens every file reads.
--
-- Port of impasto's Theme.qml. The palette half is live -- it follows the
-- wallpaper, or a fixed palette picked in Settings -- so each palette colour
-- is a signal and a binding that reads `theme.color.accent()` repaints when
-- it changes. The island's own blacks never change: the island is black on
-- every palette, which is the whole point of it.

local settings = require("services.settings")

local theme = {}

-- ------------------------------------------------------------- palette --

local fixed = {
  island = "#000000",
  islandSurface = "#141414",
  islandSurfaceHover = "#1f1f1f",
  islandBorder = "#262626",
  indicator = "#ffffff",
  indicatorDim = "#4d4d4d",
  indicatorGood = "#32d74b",
  indicatorWarn = "#ffd60a",
  indicatorBad = "#ff453a",
  indicatorTimer = "#64d2ff",
  scrim = "#000000bf",
  scrimText = "#ffffff",
  hairline = "#ffffff20",
  paperWash = "#ffffffc4",
  paperInk = "#1c1c1e",
  paperInkMuted = "#1c1c1e8a",
  paperLine = "#1c1c1e26",
}

-- The ones a palette replaces, at the palette impasto starts from.
theme.palette_defaults = {
  background = "#0c0c0c",
  surface = "#141414",
  surfaceHover = "#202020",
  border = "#282828",
  text = "#ffffff",
  textMuted = "#8e8e93",
  accent = "#0a84ff",
  accentHover = "#409cff",
  accentText = "#ffffff",
  red = "#ff453a",
  green = "#32d74b",
  yellow = "#ffd60a",
  blue = "#0a84ff",
}

local palette_signals = {}
for name, value in pairs(theme.palette_defaults) do
  palette_signals[name] = morf.signal("impasto.palette." .. name, value)
end

--- `theme.color.accent()` is the current accent, as a colour value, and a
--- binding that calls it follows the palette. The fixed colours are plain
--- values: `theme.color.island`.
theme.color = setmetatable({}, {
  __index = function(_, name)
    local signal = palette_signals[name]
    if signal then
      return function() return morf.color(signal:get()) end
    end
    local value = fixed[name]
    if value then return morf.color(value) end
    error("impasto: no colour named " .. tostring(name), 2)
  end,
})

--- Replaces palette colours: `{ accent = "#ff8800", ... }`. Missing names
--- keep their value.
function theme.apply_palette(colors)
  for name, value in pairs(colors) do
    local signal = palette_signals[name]
    if signal and value then signal:set(tostring(value)) end
  end
end

--- A palette colour's hex, for code that writes files or compares.
function theme.hex(name)
  return palette_signals[name]:get()
end

theme.github_levels = { "#26292e", "#0e4429", "#006d32", "#26a641", "#39d353" }
theme.fixed_colours = {
  { id = "#000000", label = "Black" }, { id = "#ffffff", label = "White" },
  { id = "#e5484d", label = "Red" }, { id = "#f76b15", label = "Orange" },
  { id = "#f5c518", label = "Yellow" }, { id = "#46a758", label = "Green" },
  { id = "#3b82f6", label = "Blue" }, { id = "#8b5cf6", label = "Purple" },
  { id = "#e93d82", label = "Pink" },
}

-- ---------------------------------------------------------------- sizes --

function theme.capsule_height() return settings.barHeight end
function theme.bar_top_margin() return settings.barMargin end
theme.capsule_spacing = 8
function theme.bar_reserve() return theme.bar_top_margin() + theme.capsule_height() end
function theme.bar_band() return theme.bar_reserve() + 8 end

theme.desktop_cell = 86
theme.desktop_cell_largest = 108
theme.desktop_gutter = 18
theme.desktop_stride = theme.desktop_cell + theme.desktop_gutter
theme.centre_columns = 6
theme.centre_rows = 8
theme.centre_cell_width = 140
theme.centre_cell_height = 64
theme.centre_gutter = 12
theme.centre_stride_x = theme.centre_cell_width + theme.centre_gutter
theme.centre_stride_y = theme.centre_cell_height + theme.centre_gutter
theme.centre_pager_lane = 18
theme.panel_padding = 20
function theme.dock_icon() return settings.dockIconSize end
theme.dock_padding = 8
theme.dock_gap = theme.capsule_spacing
theme.dock_margin = theme.desktop_gutter
theme.dock_dot = 5
theme.dock_dot_lane = 9
function theme.dock_depth() return theme.dock_icon() + theme.dock_dot_lane end
function theme.dock_thickness() return theme.dock_depth() + 2 * theme.dock_padding end
theme.dock_reveal = 4
theme.dock_menu_width = 250
theme.dock_menu_row = 30
theme.dock_menu_padding = 6
theme.dock_lift = 1.125
-- How every surface mixes translucent colours: as Qt does, in sRGB values.
-- In linear light a 55% black capsule is lighter than the original's, and
-- light type on dark antialiases a weight heavier.
theme.blend = "srgb"
theme.desktop_radius = 22
-- How far a translucent widget blurs the wallpaper under it: what the
-- original's Hyprland layer rule did (blur size 6, 2 passes).
theme.desktop_blur = 20
theme.dock_radius = theme.desktop_radius
theme.radius_small = 8
theme.radius_medium = 12
theme.radius_large = 18
theme.radius_pill = 999
theme.picture_corner = 0.24
theme.radius_notch = 6
theme.paper_radius = 10

theme.spectrum = { bar = 10, gap = 6, reach = 170, floor = 3, base = 0.95, tip = 0.25, curve = 0.55 }
theme.shadow = { range = 14, opacity = 0.5, spread = 4, bar_scale = 0.6 }

-- ----------------------------------------------------------------- type --

local installed = {}
for _, family in ipairs(morf.font_families and morf.font_families() or {}) do
  installed[family] = true
end

--- The first family of a comma-separated stack that is installed, or the
--- last name in it.
function theme.font_of(stack)
  local last
  for name in tostring(stack):gmatch("[^,]+") do
    local family = name:match("^%s*(.-)%s*$")
    last = family
    if family ~= "" and installed[family] then return family end
  end
  return last or "sans-serif"
end

function theme.font() return theme.font_of(settings.fontFamily .. ", Inter, Cantarell, sans-serif") end
function theme.font_mono() return theme.font_of(settings.fontMono .. ", monospace") end
function theme.font_display()
  local family = theme.font()
  if family == "Inter" then return theme.font_of("Inter Display, Inter") end
  return family
end
-- Grape Nuts ships with the shell (fonts/, OFL), as it does with impasto.
-- morf puts a `fonts` folder beside the configuration on the font path, so
-- it is normally installed as far as the shell can tell; when it is not (the
-- shell run from elsewhere), a Text that names it also names the file, and
-- the engine loads that file once before shaping.
theme.hand_file = morf.fs.join(morf.shell_dir(), "fonts", "GrapeNuts-Regular.ttf")
local hand_bundled = not installed["Grape Nuts"] and morf.fs.is_file(theme.hand_file)
if hand_bundled then installed["Grape Nuts"] = true end

function theme.font_signature() return theme.font_of("Grape Nuts, Georgia, " .. theme.font()) end
function theme.font_hand()
  if settings.notesHandwriting then return theme.font_signature() end
  return theme.font()
end
--- The file behind `font_hand()`, for a node's `font_source`; "" when the
--- hand is the UI face.
function theme.font_hand_source()
  if settings.notesHandwriting and hand_bundled then return theme.hand_file end
  return ""
end

theme.size = {
  label = 10, small = 11, regular = 13, medium = 14, large = 16,
  widget = 30, display = 68,
}

-- -------------------------------------------------------------- motion --

-- The curves Settings offers, as cubic Beziers so they read the same here
-- as in Qt: OutCubic, OutQuint, OutBack, Linear.
theme.curves = {
  OutCubic = { x1 = 0.33, y1 = 1, x2 = 0.68, y2 = 1 },
  OutQuint = { x1 = 0.22, y1 = 1, x2 = 0.36, y2 = 1 },
  OutBack = { x1 = 0.34, y1 = 1.56, x2 = 0.64, y2 = 1 },
  Linear = "linear",
}

function theme.motion() return settings.motionScale / 100 end
function theme.easing()
  return theme.curves[settings.motionCurve] or theme.curves.OutCubic
end
function theme.duration_fast() return math.floor(140 * theme.motion() + 0.5) end
function theme.duration_medium() return math.floor(200 * theme.motion() + 0.5) end
function theme.duration_morph() return math.floor(380 * theme.motion() + 0.5) end
function theme.duration_island_gone() return theme.duration_morph() + 40 end

--- A behavior entry at one of the three speeds: `theme.behave("morph")`.
--- Zero motion is a snap, not a zero-length tween the engine refuses.
function theme.behave(speed)
  local ms = speed == "fast" and theme.duration_fast()
    or speed == "medium" and theme.duration_medium()
    or theme.duration_morph()
  return { duration = math.max(1, ms), easing = theme.easing() }
end

return theme
