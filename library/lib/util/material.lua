-- Material 3 colour schemes, from a colour or from a picture.
--
-- A scheme is five tonal palettes — primary, secondary, tertiary, neutral
-- and neutral variant, plus error — each a hue and a chroma in HCT
-- (`morf.color.tonal_palette`), and a role for every surface and the type
-- on it: a palette and a tone for dark, another tone for light. The
-- variants differ only in how they place the palettes around the source
-- colour's hue and how much chroma each gets.
--
--   local material = require("lib.util.material")
--   local s = material.scheme("#4a7fb5", { variant = "tonal_spot", mode = "dark" })
--   s.primary  s.onPrimary  s.surfaceContainerHigh   -- morf.color values
--   material.hex(s)                                   -- the same, as "#rrggbb"
--
--   material.from_image("~/Pictures/sea.jpg", { mode = "dark" }, function(ok, s)
--     if ok then theme.apply(s) end
--   end)
--
-- The role names are Material's (primaryContainer, onSurfaceVariant, ...),
-- which is what configurations written for M3 shells expect. The terminal
-- set and file writers are lib/palette.lua's; `material.terminal(s)` makes
-- the sixteen colours from a scheme for them.
--
-- The palette rules and the source colour score follow Material Color
-- Utilities (Apache-2.0); no contrast levels beyond the standard one.

local morf = require("morf")

local material = {}

material.VARIANTS = {
  "tonal_spot", "vibrant", "expressive", "neutral", "monochrome",
  "fidelity", "content", "rainbow", "fruit_salad",
}

local function sanitize(degrees)
  degrees = degrees % 360
  if degrees < 0 then degrees = degrees + 360 end
  return degrees
end

-- A hue moved by the rotation of the band the source hue falls in.
local function rotate(hue, hues, rotations)
  for i = 1, #hues - 1 do
    if hue >= hues[i] and hue < hues[i + 1] then
      return sanitize(hue + rotations[i])
    end
  end
  return hue
end

local BANDS = { 0, 41, 61, 101, 131, 181, 251, 301, 360 }
local EXPRESSIVE_BANDS = { 0, 21, 51, 121, 151, 191, 271, 321, 360 }

-- Each variant: hue and chroma of every palette, from the source's.
local RULES = {
  tonal_spot = function(h)
    return { h, 36 }, { h, 16 }, { sanitize(h + 60), 24 }, { h, 6 }, { h, 8 }
  end,
  vibrant = function(h)
    return { h, 200 },
      { rotate(h, BANDS, { 18, 15, 10, 12, 15, 18, 15, 12 }), 24 },
      { rotate(h, BANDS, { 35, 30, 20, 25, 30, 35, 30, 25 }), 32 },
      { h, 10 }, { h, 12 }
  end,
  expressive = function(h)
    return { sanitize(h + 240), 40 },
      { rotate(h, EXPRESSIVE_BANDS, { 45, 95, 45, 20, 45, 90, 45, 45 }), 24 },
      { rotate(h, EXPRESSIVE_BANDS, { 120, 120, 20, 45, 20, 15, 20, 120 }), 32 },
      { sanitize(h + 15), 8 }, { sanitize(h + 15), 12 }
  end,
  neutral = function(h)
    return { h, 12 }, { h, 8 }, { sanitize(h + 60), 16 }, { h, 2 }, { h, 2 }
  end,
  monochrome = function(h)
    return { h, 0 }, { h, 0 }, { h, 0 }, { h, 0 }, { h, 0 }
  end,
  -- The source's own chroma: the scheme looks like the colour it came from.
  fidelity = function(h, c)
    return { h, c }, { h, math.max(c - 32, c * 0.5) },
      { sanitize(h + 60), math.max(c * 0.6, 24) }, { h, c / 8 }, { h, c / 8 + 4 }
  end,
  content = function(h, c)
    return { h, c }, { h, math.max(c - 32, c * 0.5) },
      { sanitize(h + 60), math.max(c * 0.6, 24) }, { h, c / 8 }, { h, c / 8 + 4 }
  end,
  rainbow = function(h)
    return { h, 48 }, { h, 16 }, { sanitize(h + 60), 24 }, { h, 0 }, { h, 0 }
  end,
  fruit_salad = function(h)
    return { sanitize(h - 50), 48 }, { sanitize(h - 50), 36 }, { h, 36 }, { h, 10 }, { h, 16 }
  end,
}

-- Every role: its palette, its tone in dark, its tone in light.
local ROLES = {
  { "primary", "p", 80, 40 }, { "onPrimary", "p", 20, 100 },
  { "primaryContainer", "p", 30, 90 }, { "onPrimaryContainer", "p", 90, 10 },
  { "inversePrimary", "p", 40, 80 }, { "surfaceTint", "p", 80, 40 },
  { "primaryFixed", "p", 90, 90 }, { "primaryFixedDim", "p", 80, 80 },
  { "onPrimaryFixed", "p", 10, 10 }, { "onPrimaryFixedVariant", "p", 30, 30 },

  { "secondary", "s", 80, 40 }, { "onSecondary", "s", 20, 100 },
  { "secondaryContainer", "s", 30, 90 }, { "onSecondaryContainer", "s", 90, 10 },
  { "secondaryFixed", "s", 90, 90 }, { "secondaryFixedDim", "s", 80, 80 },
  { "onSecondaryFixed", "s", 10, 10 }, { "onSecondaryFixedVariant", "s", 30, 30 },

  { "tertiary", "t", 80, 40 }, { "onTertiary", "t", 20, 100 },
  { "tertiaryContainer", "t", 30, 90 }, { "onTertiaryContainer", "t", 90, 10 },
  { "tertiaryFixed", "t", 90, 90 }, { "tertiaryFixedDim", "t", 80, 80 },
  { "onTertiaryFixed", "t", 10, 10 }, { "onTertiaryFixedVariant", "t", 30, 30 },

  { "error", "e", 80, 40 }, { "onError", "e", 20, 100 },
  { "errorContainer", "e", 30, 90 }, { "onErrorContainer", "e", 90, 10 },

  { "background", "n", 6, 98 }, { "onBackground", "n", 90, 10 },
  { "surface", "n", 6, 98 }, { "onSurface", "n", 90, 10 },
  { "surfaceDim", "n", 6, 87 }, { "surfaceBright", "n", 24, 98 },
  { "surfaceContainerLowest", "n", 4, 100 }, { "surfaceContainerLow", "n", 10, 96 },
  { "surfaceContainer", "n", 12, 94 }, { "surfaceContainerHigh", "n", 17, 92 },
  { "surfaceContainerHighest", "n", 22, 90 },
  { "inverseSurface", "n", 90, 20 }, { "inverseOnSurface", "n", 20, 95 },
  { "shadow", "n", 0, 0 }, { "scrim", "n", 0, 0 },

  { "surfaceVariant", "v", 30, 90 }, { "onSurfaceVariant", "v", 80, 30 },
  { "outline", "v", 60, 50 }, { "outlineVariant", "v", 30, 80 },
}

material.ROLES = {}
for i, role in ipairs(ROLES) do material.ROLES[i] = role[1] end

local function key_colour(source)
  if type(source) == "number" then return source, 48 end
  local h, c = morf.color(source):hct()
  return h, c
end

--- The palettes of a scheme: `{ primary, secondary, tertiary, neutral,
--- neutral_variant, error }`, each a `morf.color.tonal_palette`.
function material.palettes(source, variant)
  variant = variant or "tonal_spot"
  local rule = RULES[variant]
  if not rule then error("material: unknown variant " .. tostring(variant), 2) end
  local h, c = key_colour(source)
  local p, s, t, n, v = rule(h, c)
  local palette = morf.color.tonal_palette
  return {
    primary = palette(p[1], p[2]),
    secondary = palette(s[1], s[2]),
    tertiary = palette(t[1], t[2]),
    neutral = palette(n[1], n[2]),
    neutral_variant = palette(v[1], v[2]),
    error = palette(25, 84),
  }
end

--- A scheme: every role as a `morf.color`, plus `mode`, `variant`,
--- `source` and `palettes`. `source` is a colour (anything `morf.color`
--- takes) or a hue in degrees. Options: `variant` ("tonal_spot"), `mode`
--- ("dark" or "light").
function material.scheme(source, opts)
  opts = opts or {}
  local mode = opts.mode or "dark"
  if mode ~= "dark" and mode ~= "light" then
    error("material: mode must be \"dark\" or \"light\"", 2)
  end
  local palettes = material.palettes(source, opts.variant)
  local by_key = {
    p = palettes.primary, s = palettes.secondary, t = palettes.tertiary,
    n = palettes.neutral, v = palettes.neutral_variant, e = palettes.error,
  }
  local scheme = {
    mode = mode,
    variant = opts.variant or "tonal_spot",
    source = type(source) == "number" and morf.color.hct(source, 48, 50) or morf.color(source),
    palettes = palettes,
  }
  local dark = mode == "dark"
  for _, role in ipairs(ROLES) do
    scheme[role[1]] = by_key[role[2]](dark and role[3] or role[4])
  end
  return scheme
end

--- The roles of a scheme as "#rrggbb" strings, for writing out.
function material.hex(scheme)
  local out = {}
  for _, name in ipairs(material.ROLES) do out[name] = scheme[name]:hex() end
  return out
end

-- ------------------------------------------------------------ source colour

-- How the colours of a picture are ranked for the one a scheme is made
-- from: a colour counts for its own share and its neighbours' within 15
-- degrees of hue, and for its chroma; greys and near-absent colours do not
-- count; the chosen ones are at least `min_hue_distance` apart.
local TARGET_CHROMA = 48
local CUTOFF_CHROMA = 5
local CUTOFF_SHARE = 0.01

--- The best source colours among swatches (`{ color =, fraction = }`, as
--- `morf.image.palette` gives them), best first. Options: `count` (4),
--- `min_hue_distance` (15), `fallback` ("#4285f4", used when nothing has
--- colour enough).
function material.score(swatches, opts)
  opts = opts or {}
  local wanted = opts.count or 4
  local fallback = morf.color(opts.fallback or "#4285f4")
  local share_by_hue = {}
  for i = 0, 359 do share_by_hue[i] = 0 end
  local entries, total = {}, 0
  for _, swatch in ipairs(swatches) do
    local colour = morf.color(swatch.color or swatch[1])
    local fraction = swatch.fraction or swatch[2] or 0
    local h, c, t = colour:hct()
    entries[#entries + 1] = { colour = colour, h = h, c = c, t = t, fraction = fraction }
    total = total + fraction
  end
  if total <= 0 then return { fallback } end
  for _, e in ipairs(entries) do
    local bin = math.floor(e.h + 0.5) % 360
    share_by_hue[bin] = share_by_hue[bin] + e.fraction / total
  end
  local scored = {}
  for _, e in ipairs(entries) do
    local bin = math.floor(e.h + 0.5) % 360
    local share = 0
    for d = -14, 15 do share = share + share_by_hue[(bin + d) % 360] end
    if e.c >= CUTOFF_CHROMA and share > CUTOFF_SHARE then
      local chroma_weight = e.c < TARGET_CHROMA and 0.1 or 0.3
      e.score = share * 100 * 0.7 + (e.c - TARGET_CHROMA) * chroma_weight
      scored[#scored + 1] = e
    end
  end
  table.sort(scored, function(a, b) return a.score > b.score end)
  local function distance(a, b)
    local d = math.abs(a - b) % 360
    return math.min(d, 360 - d)
  end
  -- The widest spread that still gives enough colours, down to 15 degrees.
  local chosen = {}
  for spread = 90, opts.min_hue_distance or 15, -1 do
    chosen = {}
    for _, e in ipairs(scored) do
      local apart = true
      for _, c in ipairs(chosen) do
        if distance(e.h, c.h) < spread then apart = false break end
      end
      if apart then chosen[#chosen + 1] = e end
      if #chosen >= wanted then break end
    end
    if #chosen >= wanted then break end
  end
  if #chosen == 0 then return { fallback } end
  local out = {}
  for i, e in ipairs(chosen) do out[i] = e.colour end
  return out
end

--- A scheme from a picture: its colours quantised by the engine, the best
--- one as the source. Answers `on_done(true, scheme)` or `on_done(false,
--- message)` on a later turn. Options: those of `scheme` and `score`, and
--- `swatches` (how many colours to quantise into, 32).
function material.from_image(path, opts, on_done)
  opts = opts or {}
  local queued, why = morf.image.palette(path, opts.swatches or 32, function(ok, entries)
    if not ok then return on_done(false, entries) end
    local done, scheme = pcall(function()
      local source = material.score(entries, opts)[1]
      local s = material.scheme(source, opts)
      s.swatches = entries
      return s
    end)
    on_done(done, scheme)
  end)
  if not queued then on_done(false, why or "the image queue is full") end
end

-- --------------------------------------------------------------- terminal

-- The terminal's eight hues: black, red, green, yellow, blue, magenta,
-- cyan, white, by hue in HCT.
local TERMINAL_HUES = { nil, 25, 145, 95, 265, 330, 200, nil }

--- Sixteen terminal colours and the usual extras from a scheme, in
--- lib/palette.lua's `terminal` shape: `color0`..`color15`, `foreground`,
--- `background`, `cursor`, `cursor_text`, `selection`,
--- `selection_foreground`. The hues are the usual ones, tinted a little
--- towards the scheme's primary, with the scheme's chroma.
function material.terminal(scheme)
  local dark = scheme.mode == "dark"
  local ph, pc = scheme.primary:hct()
  local chroma = math.max(24, math.min(pc, 60))
  local term = {}
  for i = 0, 7 do
    local hue = TERMINAL_HUES[i + 1]
    local normal, bright
    if hue == nil then
      local neutral = scheme.palettes.neutral
      if i == 0 then
        normal, bright = neutral(dark and 10 or 90), neutral(dark and 35 or 70)
      else
        normal, bright = neutral(dark and 80 or 25), neutral(dark and 95 or 5)
      end
    else
      -- A little of the primary's hue, as far as a quarter of the way.
      local d = sanitize(ph - hue)
      if d > 180 then d = d - 360 end
      local h = sanitize(hue + d * 0.15)
      normal = morf.color.hct(h, chroma, dark and 70 or 40)
      bright = morf.color.hct(h, chroma, dark and 82 or 30)
    end
    term["color" .. i] = normal
    term["color" .. (i + 8)] = bright
  end
  term.foreground = scheme.onSurface
  term.background = scheme.surface
  term.cursor = scheme.primary
  term.cursor_text = scheme.onPrimary
  term.selection = scheme.secondaryContainer
  term.selection_foreground = scheme.onSecondaryContainer
  return term
end

return material
