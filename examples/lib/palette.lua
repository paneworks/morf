-- A desk's colours from its painting.
--
-- The wallpaper is quantised by the engine (`morf.image.palette`), and
-- everything else is decided here, in Lua: which of the picture's colours
-- becomes the accent, the grounds and type tinted from it, the four
-- semantic colours pulled a little towards the picture's hue family, and a
-- sixteen-colour terminal set, each one moved in lightness until it reads
-- against what it sits on. Then it is written out, in whatever file format
-- the rest of the desk reads: kitty, foot, alacritty, btop, cava, GTK,
-- pywal, JSON, Lua, or a template of your own.
--
-- The token names are impasto's (github.com/andreumassanet/impasto), whose
-- theme manager this ports: background, surface, surfaceHover, border,
-- text, textMuted, accent, accentHover, accentText, red, green, yellow,
-- blue. A palette is a plain table of those as `morf.color` values, plus
-- `terminal` (color0..color15, foreground, background, cursor,
-- cursor_text, selection, selection_foreground) and a few descriptive
-- fields (`mode`, `source`, `picked`, `swatches`).
--
--   local palette = require("lib.palette")
--   palette.from_image("~/Pictures/sea.jpg", { mode = "dark" }, function(ok, p)
--     if not ok then return morf.log.warn(p) end
--     palette.write.kitty(p, morf.cache_path("colors/kitty.conf"))
--     theme.accent = p.accent
--   end)
--
-- Every writer only writes; none of them signals or restarts anything.
-- `palette.reload_hints` says what each program would need, for a
-- configuration that wants to do it itself.

local morf = require("morf")

local palette = {}

-- Bumped whenever the derivation changes, so a cached palette from an
-- older rule is not served for a newer one.
palette.VERSION = 1

palette.TOKENS = {
  "background", "surface", "surfaceHover", "border",
  "text", "textMuted",
  "accent", "accentHover", "accentText",
  "red", "green", "yellow", "blue",
}

palette.TERMINAL = {
  "color0", "color1", "color2", "color3", "color4", "color5", "color6", "color7",
  "color8", "color9", "color10", "color11", "color12", "color13", "color14", "color15",
  "foreground", "background", "cursor", "cursor_text", "selection", "selection_foreground",
}

-- The contrast floors every derived palette keeps (WCAG 2 ratios). A
-- pair is `{ foreground, background, minimum }`; names starting with
-- `terminal.` are looked up in the terminal set.
palette.FLOORS = {
  { "text", "background", 7 },
  { "text", "surface", 4.5 },
  { "text", "surfaceHover", 4.5 },
  { "textMuted", "background", 4.5 },
  { "textMuted", "surface", 4.5 },
  { "accentText", "accent", 4.5 },
  { "accentText", "accentHover", 4.5 },
  { "accent", "background", 3 },
  { "red", "background", 4.5 },
  { "green", "background", 4.5 },
  { "yellow", "background", 4.5 },
  { "blue", "background", 4.5 },
  { "terminal.foreground", "terminal.background", 7 },
  { "terminal.selection_foreground", "terminal.selection", 4.5 },
}
for slot = 1, 6 do
  palette.FLOORS[#palette.FLOORS + 1] = { "terminal.color" .. slot, "terminal.background", 4.5 }
  palette.FLOORS[#palette.FLOORS + 1] = { "terminal.color" .. (slot + 8), "terminal.background", 7 }
end
palette.FLOORS[#palette.FLOORS + 1] = { "terminal.color8", "terminal.background", 3 }

-- ── colour arithmetic ─────────────────────────────────────────────────────
--
-- Colours are worked on as OkLCh "inks", `{ l, c, h }`, and only become
-- `morf.color` values at the end. The one piece of maths done here rather
-- than by the engine is the OkLab to linear-sRGB step, because every
-- lightness search asks "what luminance is this?" dozens of times and a
-- handler has a fuel budget.

local RAD = math.pi / 180
local ROOM = 1e-4
-- Searched colours aim this far above a floor, so rounding to 8-bit hex
-- afterwards cannot drop them below it.
local MARGIN = 0.08

local function clamp(value, low, high)
  if value < low then return low end
  if value > high then return high end
  return value
end

-- OkLCh to linear sRGB, with the hue already turned into its cosine and
-- sine: a search changes lightness a dozen times and hue never.
local function linear(l, c, ca, sa)
  local a, b = c * ca, c * sa
  local l_ = l + 0.3963377774 * a + 0.2158037573 * b
  local m_ = l - 0.1055613458 * a - 0.0638541728 * b
  local s_ = l - 0.0894841775 * a - 1.2914855480 * b
  local L, M, S = l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_
  return 4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S,
    -1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S,
    -0.0041960863 * L - 0.7034186147 * M + 1.7076147010 * S
end

local HIGH = 1 + ROOM

local function inside(l, c, ca, sa)
  local r, g, b = linear(l, c, ca, sa)
  return r >= -ROOM and r <= HIGH and g >= -ROOM and g <= HIGH and b >= -ROOM and b <= HIGH
end

-- The most chroma at or below `c` that sRGB can show at this lightness and
-- hue: out-of-gamut colours give up saturation, never hue or lightness.
local function fit(l, c, ca, sa)
  if c <= 0 or inside(l, c, ca, sa) then return c end
  local low, high = 0, c
  for _ = 1, 6 do
    local middle = (low + high) / 2
    if inside(l, middle, ca, sa) then low = middle else high = middle end
  end
  return low
end

-- WCAG relative luminance of a colour as sRGB would clip it: what a
-- lightness search steers by.
local function clipped(l, c, ca, sa)
  if c <= 0 then return l * l * l end
  local r, g, b = linear(l, c, ca, sa)
  return 0.2126 * clamp(r, 0, 1) + 0.7152 * clamp(g, 0, 1) + 0.0722 * clamp(b, 0, 1)
end

-- WCAG relative luminance of the colour as it will be painted, chroma
-- fitted to the gamut.
-- The fit here is coarser than `fit`'s (a sixteenth of the chroma, always
-- on the in-gamut side); it only has to steer, and MARGIN covers it.
local function luminance(l, c, ca, sa)
  if c <= 0 then return l * l * l end
  local r, g, b = linear(l, c, ca, sa)
  if r < -ROOM or r > HIGH or g < -ROOM or g > HIGH or b < -ROOM or b > HIGH then
    local low, high = 0, c
    for _ = 1, 4 do
      local middle = (low + high) / 2
      if inside(l, middle, ca, sa) then low = middle else high = middle end
    end
    r, g, b = linear(l, low, ca, sa)
  end
  return 0.2126 * clamp(r, 0, 1) + 0.7152 * clamp(g, 0, 1) + 0.0722 * clamp(b, 0, 1)
end

local function ratio(one, two)
  if one < two then one, two = two, one end
  return (one + 0.05) / (two + 0.05)
end

local function ink(l, c, h)
  h = h or 0
  return { l = l, c = c, h = h, ca = math.cos(h * RAD), sa = math.sin(h * RAD) }
end

local function ink_of(color)
  local k = morf.color(color):oklch()
  local h = k.h
  if h ~= h or k.c < 0.002 then return ink(k.l, 0, 0) end
  return ink(k.l, k.c, h)
end

local function y_of(value) return luminance(value.l, value.c, value.ca, value.sa) end

local function paint(value)
  local l = clamp(value.l, 0, 1)
  return morf.color.oklch(l, fit(l, value.c, value.ca, value.sa), value.h)
end

-- Which way a colour moves to get off this ground: up on a dark one, down
-- on a light one. 0.179 is where black and white read equally (impasto's
-- CONTRAST_PIVOT).
local function away(ground_y)
  return ground_y > 0.179 and -1 or 1
end

-- `value` moved in lightness, away from the ground, until it reaches
-- `minimum` contrast on it. Hue and chroma stay (chroma only where the
-- gamut allows). impasto walks HSL lightness in 1% steps; this halves the
-- interval instead, measuring the colour as it will really be painted.
local function lift(value, minimum, ground)
  local ground_y = type(ground) == "number" and ground or y_of(ground)
  local target = minimum + MARGIN
  local c, ca, sa = value.c, value.ca, value.sa
  if ratio(luminance(value.l, c, ca, sa), ground_y) >= target then return value end
  local up = away(ground_y) > 0
  local extreme = up and 1 or 0
  if ratio(extreme, ground_y) < target then return ink(extreme, 0, value.h) end
  local fails, passes = value.l, extreme
  for _ = 1, 7 do
    local middle = (fails + passes) / 2
    if ratio(clipped(middle, c, ca, sa), ground_y) >= target then
      passes = middle
    else
      fails = middle
    end
  end
  -- Steered by the clipped colour, which is cheap; checked as painted,
  -- which is not, and nudged on in the rare case the gamut cost contrast.
  local l = passes
  for _ = 1, 20 do
    if ratio(luminance(l, c, ca, sa), ground_y) >= target then return ink(l, c, value.h) end
    l = up and math.min(1, l + 0.01) or math.max(0, l - 0.01)
  end
  return ink(extreme, 0, value.h)
end

-- The bright half of an ANSI pair: a fixed step further from the ground,
-- then the higher floor (impasto's `brighten`; lifting alone leaves pale
-- palettes with identical normal and bright slots).
local function brighten(value, ground)
  local ground_y = type(ground) == "number" and ground or y_of(ground)
  local stepped = ink(clamp(value.l + 0.08 * away(ground_y), 0, 1), value.c, value.h)
  return lift(stepped, 7, ground_y)
end

local function mix_ink(a, b, t)
  -- In OkLab, so a tint towards a hue does not swing through others.
  local aa, ab = a.c * math.cos(a.h * RAD), a.c * math.sin(a.h * RAD)
  local ba, bb = b.c * math.cos(b.h * RAD), b.c * math.sin(b.h * RAD)
  local l = a.l + (b.l - a.l) * t
  local x, y = aa + (ba - aa) * t, ab + (bb - ab) * t
  local c = math.sqrt(x * x + y * y)
  if c < 1e-6 then return ink(l, 0, a.h) end
  local h = math.atan(y, x) / RAD
  if h < 0 then h = h + 360 end
  return ink(l, c, h)
end

-- Signed shortest turn from hue `from` to hue `to`, in degrees.
local function turn(from, to)
  local d = (to - from) % 360
  if d > 180 then d = d - 360 end
  return d
end

-- ── choosing the accent ───────────────────────────────────────────────────

-- impasto's fallback when a picture has no colour worth the name.
palette.NEUTRAL_ACCENT = "#89b4fa"

local function describe(entry, total)
  local color = morf.color(entry.color or entry[1])
  local rgb = color:rgb()
  local high = math.max(rgb.r, rgb.g, rgb.b)
  local low = math.min(rgb.r, rgb.g, rgb.b)
  local k = color:oklch()
  local chroma, hue = k.c, k.h
  if hue ~= hue or chroma < 0.002 then chroma, hue = 0, nil end
  return {
    color = color,
    hex = color:hex(),
    l = k.l,
    c = chroma,
    h = hue,
    -- impasto's two measures: HSV saturation and a cheap luma.
    sat = high == 0 and 0 or (high - low) / high,
    luma = 0.2126 * rgb.r + 0.7152 * rgb.g + 0.0722 * rgb.b,
    fraction = entry.fraction or entry[2] or 1 / total,
  }
end

-- impasto picks a saturated mid-tone: among swatches with saturation over
-- 0.2 and luma between 0.2 and 0.85, the highest `2 * sat + (1 - |luma -
-- 0.55|)`. Two terms are added here: OkLCh chroma (HSV saturation calls a
-- near-black navy fully saturated; chroma does not) and the square root of
-- the swatch's share of the picture, so a speck of neon does not beat the
-- sky. With nothing vibrant, the most saturated colour over 0.1 wins; with
-- nothing at all, the neutral accent.
local function pick(swatches, opts)
  local weight = opts.population_weight or 1
  local floor = opts.min_population or 0.004
  local best, best_score
  for _, s in ipairs(swatches) do
    if s.sat > 0.2 and s.luma > 0.2 and s.luma < 0.85 and s.fraction >= floor then
      local score = s.sat * 2 + (1 - math.abs(s.luma - 0.55)) + s.c * 3
        + weight * math.sqrt(s.fraction)
      if not best or score > best_score then best, best_score = s, score end
    end
  end
  if best then return best, "vibrant" end
  for _, s in ipairs(swatches) do
    if s.sat > 0.1 and s.c >= 0.02 and s.fraction >= floor then
      local score = s.sat + s.c * 3 + 0.25 * weight * math.sqrt(s.fraction)
      if not best or score > best_score then best, best_score = s, score end
    end
  end
  if best then return best, "colorful" end
  return describe({ color = opts.fallback_accent or palette.NEUTRAL_ACCENT, fraction = 0 }, 1), "neutral"
end

-- ── the palette ───────────────────────────────────────────────────────────

-- Where the semantic colours start, as OkLCh hues, with the chroma and the
-- lightness they take on a dark and on a light ground.
local SEMANTIC = {
  red = { h = 25, c = 0.15, dark = 0.72, light = 0.52 },
  green = { h = 145, c = 0.14, dark = 0.78, light = 0.55 },
  yellow = { h = 92, c = 0.13, dark = 0.86, light = 0.60 },
  blue = { h = 255, c = 0.13, dark = 0.74, light = 0.52 },
  magenta = { h = 330, c = 0.14, dark = 0.74, light = 0.52 },
  cyan = { h = 200, c = 0.11, dark = 0.80, light = 0.55 },
}

local function semantic(name, dark, accent, vividness, opts)
  local base = SEMANTIC[name]
  local h = base.h
  if accent.c >= 0.02 then
    -- Pulled a quarter of the way towards the accent's hue, never more than
    -- twelve degrees: a red that leans the picture's way and is still red.
    local most = opts.hue_shift or 12
    h = (h + clamp(turn(h, accent.h) * (opts.hue_pull or 0.25), -most, most)) % 360
  end
  -- A muted picture gets muted signals, a vivid one full ones; never so
  -- grey that red and green stop being told apart.
  local c = base.c * clamp(vividness / 0.1, 0.5, 1)
  return ink(dark and base.dark or base.light, c, h)
end

local function build_terminal(t, opts)
  -- `t` holds inks for the tokens (and optionally magenta/cyan).
  local ground = t.background
  local ground_y = y_of(ground)
  local dark = away(ground_y) > 0
  local function on(value, minimum) return lift(value, minimum or 4.5, ground_y) end
  local magenta = t.magenta
  if not magenta then
    -- A palette that was handed to us has no magenta: rotate its red to
    -- where magenta sits, carrying whatever lean the red already has.
    local lean = clamp(turn(SEMANTIC.red.h, t.red.h), -20, 20)
    magenta = ink(t.red.l, math.max(t.red.c, 0.08), (SEMANTIC.magenta.h + lean) % 360)
  end
  local cyan
  if opts.cyan_is_accent == false then
    cyan = t.cyan
    if not cyan then
      local lean = clamp(turn(SEMANTIC.blue.h, t.blue.h), -20, 20)
      cyan = ink(t.blue.l, math.max(t.blue.c, 0.06), (SEMANTIC.cyan.h + lean) % 360)
    end
  else
    -- impasto's decision: cyan carries the accent, because it is the one
    -- ANSI hue with no conventional meaning.
    cyan = t.accent
  end
  local hues = { t.red, t.green, t.yellow, t.blue, magenta, cyan }
  local out = {}
  for slot, value in ipairs(hues) do
    out["color" .. slot] = on(value)
    out["color" .. (slot + 8)] = brighten(value, ground_y)
  end
  if dark then
    out.color0 = t.surfaceHover
    out.color7 = on(t.textMuted)
    out.color8 = lift(mix_ink(t.textMuted, ground, 0.35), 3, ground_y)
    out.color15 = on(t.text, 7)
  else
    -- On a light ground "black" is the ink and "white" the paper.
    out.color0 = on(t.text, 7)
    out.color7 = t.surfaceHover
    out.color8 = lift(t.textMuted, 3, ground_y)
    out.color15 = t.border
  end
  out.foreground = on(t.text, 7)
  out.background = ground
  out.cursor = out.foreground
  out.cursor_text = ground
  out.selection = t.accent
  out.selection_foreground = t.accentText
  return out
end

local function finish(inks, terminal, extra)
  -- Many slots are the same ink as a token (the ground, the selection):
  -- each ink is painted once.
  local painted = {}
  local function once(value)
    local colour = painted[value]
    if not colour then
      colour = paint(value)
      painted[value] = colour
    end
    return colour
  end
  local p = {}
  for _, name in ipairs(palette.TOKENS) do p[name] = once(inks[name]) end
  p.terminal = {}
  for _, name in ipairs(palette.TERMINAL) do p.terminal[name] = once(terminal[name]) end
  for key, value in pairs(extra or {}) do p[key] = value end
  return p
end

--- The palette for a set of swatches: a list of `{ color = , fraction = }`
--- (what `morf.image.palette` hands back) or `{ color, fraction }` pairs.
--- Synchronous and deterministic: the same swatches and options give the
--- same palette.
---
--- Options: `mode` ("dark", the default, "light", or "auto" from the
--- picture's own lightness), `accent` (a colour to use instead of picking
--- one), `fallback_accent`, `population_weight` (1), `min_population`
--- (0.004), `hue_pull` (0.25), `hue_shift` (12 degrees), `cyan_is_accent`
--- (true).
-- The derivation is two halves, so `from_image` can run them in two turns
-- of the loop: the tokens, then the terminal set and the painting.
local function derive_tokens(swatches, opts)
  local list = {}
  for _, entry in ipairs(swatches) do list[#list + 1] = describe(entry, #swatches) end
  table.sort(list, function(a, b)
    if a.fraction ~= b.fraction then return a.fraction > b.fraction end
    return a.hex < b.hex
  end)
  local vividness, lightness, total = 0, 0, 0
  for _, s in ipairs(list) do
    vividness = vividness + s.fraction * s.c
    lightness = lightness + s.fraction * s.l
    total = total + s.fraction
  end
  if total > 0 then
    vividness, lightness = vividness / total, lightness / total
  end

  local picked, why
  if opts.accent then
    picked, why = describe({ color = opts.accent, fraction = 1 }, 1), "given"
    vividness = math.max(vividness, picked.c)
  else
    picked, why = pick(list, opts)
  end
  if why == "neutral" or why == "given" and #list == 0 then vividness = math.max(vividness, picked.c) end

  local mode = opts.mode or "dark"
  if mode == "auto" then mode = lightness > 0.62 and "light" or "dark" end
  local dark = mode ~= "light"

  local hue = picked.h or 0
  local chromatic = picked.h ~= nil and picked.c >= 0.02
  -- Grounds are near-black (or near-white) tinted towards the accent, as
  -- impasto's are; the tint is a fifth of the accent's chroma, at most a
  -- whisper.
  local tint = chromatic and math.min(0.024, picked.c * 0.2) or 0.004
  local t = {}
  if dark then
    t.background = ink(0.18, tint, hue)
    t.surface = ink(0.215, tint, hue)
    t.surfaceHover = ink(0.265, tint * 1.1, hue)
    t.border = ink(0.33, tint * 1.2, hue)
    t.text = ink(0.93, math.min(tint, 0.012), hue)
    t.textMuted = ink(0.74, tint * 1.2, hue)
  else
    t.background = ink(0.975, tint * 0.5, hue)
    t.surface = ink(0.945, tint * 0.6, hue)
    t.surfaceHover = ink(0.905, tint * 0.7, hue)
    t.border = ink(0.85, tint * 0.8, hue)
    t.text = ink(0.26, math.min(tint, 0.02), hue)
    t.textMuted = ink(0.47, tint, hue)
  end
  local bg = y_of(t.background)
  t.text = lift(lift(lift(t.text, 7, bg), 4.5, t.surface), 4.5, t.surfaceHover)
  t.textMuted = lift(lift(t.textMuted, 4.5, bg), 4.5, t.surface)

  -- The accent keeps its hue; its lightness is brought into the band an
  -- accent reads in on this ground, and a colour that has chroma is given
  -- at least enough to look chosen.
  local accent
  if chromatic then
    accent = ink(clamp(picked.l, dark and 0.64 or 0.42, dark and 0.84 or 0.62),
      clamp(picked.c, 0.07, 0.2), hue)
  else
    accent = ink(clamp(picked.l, dark and 0.64 or 0.42, dark and 0.84 or 0.62), picked.c, hue)
  end
  accent = lift(accent, 3, bg)
  -- Type on the accent: the ground's own dark or a near-white, whichever
  -- reads better; if neither reaches 4.5, the accent moves instead.
  local on_dark, on_light = ink(0.18, tint, hue), ink(0.985, 0.005, hue)
  local ay = y_of(accent)
  local accent_text = ratio(y_of(on_dark), ay) >= ratio(y_of(on_light), ay) and on_dark or on_light
  accent = lift(accent, 4.5, accent_text)
  local step = away(y_of(accent_text)) * 0.07
  local hover = lift(ink(clamp(accent.l + step, 0, 1), accent.c * 1.05, hue), 4.5, accent_text)
  t.accent, t.accentHover, t.accentText = accent, hover, accent_text

  for _, name in ipairs({ "red", "green", "yellow", "blue", "magenta", "cyan" }) do
    t[name] = lift(semantic(name, dark, picked.h and picked or ink(0, 0, 0), vividness, opts), 4.5, bg)
  end

  local swatch_hex = {}
  for index = 1, math.min(#list, 8) do swatch_hex[index] = list[index].hex end
  return {
    inks = t,
    extra = { mode = dark and "dark" or "light", picked = picked.hex, swatches = swatch_hex },
  }
end

local function derive_rest(half, opts)
  return finish(half.inks, build_terminal(half.inks, opts), half.extra)
end

function palette.derive(swatches, opts)
  opts = opts or {}
  return derive_rest(derive_tokens(swatches, opts), opts)
end

--- A palette from one colour, as if a picture had been nothing but it.
function palette.from_accent(color, opts)
  local o = {}
  for key, value in pairs(opts or {}) do o[key] = value end
  o.accent = color
  return palette.derive({ { color = color, fraction = 1 } }, o)
end

--- A palette table from plain tokens (strings or colours), with the
--- terminal set derived by the same rules. How a fixed scheme becomes a
--- whole palette.
function palette.from_tokens(tokens, opts)
  opts = opts or {}
  local t = {}
  for _, name in ipairs(palette.TOKENS) do
    assert(tokens[name], "palette.from_tokens: missing `" .. name .. "`")
    t[name] = ink_of(tokens[name])
  end
  local p = {}
  for _, name in ipairs(palette.TOKENS) do p[name] = morf.color(tokens[name]) end
  local terminal = build_terminal(t, opts)
  p.terminal = {}
  for _, name in ipairs(palette.TERMINAL) do
    local value = terminal[name]
    if value == t.background then
      p.terminal[name] = p.background
    else
      p.terminal[name] = paint(value)
    end
  end
  p.mode = away(y_of(t.background)) > 0 and "dark" or "light"
  return p
end

-- ── presets ───────────────────────────────────────────────────────────────

-- impasto's nine curated schemes (theme/Palettes.qml), token for token.
local PRESETS = {
  { id = "catppuccin_mocha", name = "Catppuccin Mocha", badge = "Dark Pastel",
    swatches = { "#89b4fa", "#f5c2e7", "#a6e3a1", "#fab387" },
    colors = { background = "#1e1e2e", surface = "#181825", surfaceHover = "#313244",
      border = "#45475a", text = "#cdd6f4", textMuted = "#a6adc8",
      accent = "#89b4fa", accentHover = "#b4befe", accentText = "#11111b",
      red = "#f38ba8", green = "#a6e3a1", yellow = "#f9e2af", blue = "#89b4fa" } },
  { id = "catppuccin_latte", name = "Catppuccin Latte", badge = "Light Clean",
    swatches = { "#1e66f5", "#8839ef", "#40a02b", "#ea76cb" },
    colors = { background = "#eff1f5", surface = "#e6e9ef", surfaceHover = "#ccd0da",
      border = "#bcc0cc", text = "#4c4f69", textMuted = "#6c6f85",
      accent = "#1e66f5", accentHover = "#04a5e5", accentText = "#eff1f5",
      red = "#d20f39", green = "#40a02b", yellow = "#df8e1d", blue = "#1e66f5" } },
  { id = "tokyo_night", name = "Tokyo Night", badge = "Cyber City",
    swatches = { "#7aa2f7", "#bb9af7", "#9ece6a", "#f7768e" },
    colors = { background = "#1a1b26", surface = "#16161e", surfaceHover = "#24283b",
      border = "#292e42", text = "#c0caf5", textMuted = "#7982a9",
      accent = "#7aa2f7", accentHover = "#bb9af7", accentText = "#15161e",
      red = "#f7768e", green = "#9ece6a", yellow = "#e0af68", blue = "#7aa2f7" } },
  { id = "gruvbox_dark", name = "Gruvbox Dark", badge = "Warm Retro",
    swatches = { "#d79921", "#83a598", "#b8bb26", "#fb4934" },
    colors = { background = "#282828", surface = "#1d2021", surfaceHover = "#3c3836",
      border = "#504945", text = "#ebdbb2", textMuted = "#a89984",
      accent = "#d79921", accentHover = "#fabd2f", accentText = "#282828",
      red = "#fb4934", green = "#b8bb26", yellow = "#fabd2f", blue = "#83a598" } },
  { id = "nord", name = "Nord", badge = "Arctic Frost",
    swatches = { "#88c0d0", "#81a1c1", "#a3be8c", "#bf616a" },
    colors = { background = "#2e3440", surface = "#242933", surfaceHover = "#3b4252",
      border = "#434c5e", text = "#eceff4", textMuted = "#d8dee9",
      accent = "#88c0d0", accentHover = "#81a1c1", accentText = "#2e3440",
      red = "#bf616a", green = "#a3be8c", yellow = "#ebcb8b", blue = "#81a1c1" } },
  { id = "rose_pine", name = "Rosé Pine", badge = "Soho Dusk",
    swatches = { "#ebbcba", "#f6c177", "#31748f", "#eb6f92" },
    colors = { background = "#191724", surface = "#1f1d2e", surfaceHover = "#26233a",
      border = "#403d52", text = "#e0def4", textMuted = "#908caa",
      accent = "#ebbcba", accentHover = "#f6c177", accentText = "#191724",
      red = "#eb6f92", green = "#31748f", yellow = "#f6c177", blue = "#9ccfd8" } },
  { id = "cyberpunk", name = "Cyberpunk", badge = "Neon Synth",
    swatches = { "#ff007f", "#00e5ff", "#ffea00", "#00e676" },
    colors = { background = "#0d0f18", surface = "#141726", surfaceHover = "#21253b",
      border = "#ff007f", text = "#e0f7fa", textMuted = "#00e5ff",
      accent = "#ff007f", accentHover = "#00e5ff", accentText = "#ffffff",
      red = "#ff1744", green = "#00e676", yellow = "#ffea00", blue = "#00e5ff" } },
  { id = "bauhaus", name = "Bauhaus", badge = "Modernist",
    swatches = { "#e52521", "#2b82d9", "#f5a623", "#f5f5f5" },
    colors = { background = "#1a1a1a", surface = "#242424", surfaceHover = "#333333",
      border = "#444444", text = "#f5f5f5", textMuted = "#999999",
      accent = "#e52521", accentHover = "#f5a623", accentText = "#ffffff",
      red = "#e52521", green = "#2b82d9", yellow = "#f5a623", blue = "#2b82d9" } },
  { id = "crimson", name = "Crimson Moon", badge = "Vampiric",
    swatches = { "#e63946", "#ff4d6d", "#ad8394", "#f7e1ea" },
    colors = { background = "#120a0d", surface = "#1c1015", surfaceHover = "#2d1822",
      border = "#4a2133", text = "#f7e1ea", textMuted = "#ad8394",
      accent = "#e63946", accentHover = "#ff4d6d", accentText = "#ffffff",
      red = "#e63946", green = "#52b788", yellow = "#ee9b00", blue = "#a2d2ff" } },
}

--- impasto's nine schemes, in its order. Each is `{ id, name, badge,
--- swatches, colors }` with `colors` the thirteen tokens as hex strings;
--- `palette.preset(id)` makes a whole palette (terminal set included) of
--- one. The tokens are kept exactly as their authors chose them, so the
--- contrast floors are not promised for these.
palette.presets = PRESETS

local preset_cache = {}

function palette.preset(id)
  if preset_cache[id] then return preset_cache[id] end
  for _, entry in ipairs(PRESETS) do
    if entry.id == id then
      local p = palette.from_tokens(entry.colors)
      p.id, p.name, p.badge = entry.id, entry.name, entry.badge
      preset_cache[id] = p
      return p
    end
  end
  return nil, "no preset `" .. tostring(id) .. "`"
end

-- ── plain values ──────────────────────────────────────────────────────────

local function is_color(value) return type(value) == "userdata" end

local function hex(value) return morf.color(value):hex() end

--- The palette with every colour as a "#rrggbb" string: what JSON, a
--- `morf.theme`, or a comparison wants.
function palette.to_hex(p)
  local out = {}
  for key, value in pairs(p) do
    if key == "cached" then
      -- Where it came from this time, not what it is.
    elseif key == "terminal" then
      out.terminal = {}
      for name, colour in pairs(value) do out.terminal[name] = hex(colour) end
    elseif is_color(value) then
      out[key] = hex(value)
    elseif type(value) == "table" then
      local copy = {}
      for index, item in ipairs(value) do copy[index] = item end
      out[key] = copy
    else
      out[key] = value
    end
  end
  return out
end

--- The reverse: a table of hex strings (a cache file, a hand-written
--- scheme) made colour values again.
function palette.from_hex(plain)
  local p = {}
  for key, value in pairs(plain) do
    if key == "terminal" then
      p.terminal = {}
      for name, colour in pairs(value) do p.terminal[name] = morf.color(colour) end
    elseif type(value) == "string" and value:match("^#%x%x%x%x%x%x$") then
      p[key] = morf.color(value)
    elseif type(value) == "table" then
      local copy = {}
      for index, item in ipairs(value) do copy[index] = item end
      p[key] = copy
    else
      p[key] = value
    end
  end
  return p
end

local function lookup(p, name)
  local terminal = name:match("^terminal%.(.+)$")
  if terminal then return p.terminal and p.terminal[terminal] end
  return p[name]
end

--- Every contrast floor in `palette.FLOORS`, measured: a list of `{ fg,
--- bg, ratio, minimum, ok }`. `ok` is false for any pair that falls short.
function palette.check(p)
  local out, all = {}, true
  for _, floor in ipairs(palette.FLOORS) do
    local fg, bg = lookup(p, floor[1]), lookup(p, floor[2])
    if fg and bg then
      local value = morf.color(fg):contrast(bg)
      local ok = value >= floor[3]
      all = all and ok
      out[#out + 1] = { fg = floor[1], bg = floor[2], ratio = value, minimum = floor[3], ok = ok }
    end
  end
  return out, all
end

--- Palette `a` taken `t` of the way to `b`, every colour mixed in OkLab.
--- For animating a desk from one wallpaper to the next; descriptive fields
--- come from whichever end is nearer.
function palette.blend(a, b, t)
  t = clamp(t or 0.5, 0, 1)
  local near = t < 0.5 and a or b
  local out = {}
  for key, value in pairs(near) do
    if key ~= "terminal" and not is_color(value) and type(value) ~= "string"
        or type(value) == "string" and not value:match("^#%x+$") then
      out[key] = value
    end
  end
  local function mix(x, y)
    return morf.color(x):mix(morf.color(y), t, "oklab")
  end
  for _, name in ipairs(palette.TOKENS) do
    if a[name] and b[name] then out[name] = mix(a[name], b[name]) end
  end
  if a.terminal and b.terminal then
    out.terminal = {}
    for _, name in ipairs(palette.TERMINAL) do
      if a.terminal[name] and b.terminal[name] then
        out.terminal[name] = mix(a.terminal[name], b.terminal[name])
      end
    end
  end
  return out
end

-- ── from a picture, with a cache ──────────────────────────────────────────

local function hash(text)
  local h = 5381
  for index = 1, #text do h = (h * 33 + text:byte(index)) % 4294967296 end
  return string.format("%08x", h)
end

local OPTION_KEYS = {
  "mode", "accent", "fallback_accent", "population_weight", "min_population",
  "hue_pull", "hue_shift", "cyan_is_accent", "count",
}

local function signature(path, stat, opts)
  local parts = { "v" .. palette.VERSION, path, tostring(stat.modified), tostring(stat.size) }
  for _, key in ipairs(OPTION_KEYS) do
    local value = opts[key]
    if value ~= nil then parts[#parts + 1] = key .. "=" .. tostring(value) end
  end
  return table.concat(parts, "\n")
end

local function cache_dir(opts)
  if opts.cache == false then return nil end
  if opts.cache_dir then return opts.cache_dir end
  if morf.cache_path then
    local ok, dir = pcall(morf.cache_path, "palette")
    if ok then return dir end
  end
  return nil
end

--- Derives a palette from a picture and calls `on_done(true, palette)` or
--- `on_done(false, message)`. The picture is decoded and quantised off the
--- main loop; the derivation runs in the answer.
---
--- A palette is cached by the file's path, modification time, size and the
--- options, as JSON under `opts.cache_dir` (default
--- `morf.cache_path("palette")`; `cache = false` turns it off). A cached
--- palette is answered at once, before this returns, so re-applying a
--- wallpaper costs a file read. `opts.count` is how many colours the
--- picture is quantised to (16). The other options are `palette.derive`'s.
---
--- Returns true when the answer is coming (or came), nil and a message
--- when it cannot.
function palette.from_image(path, opts, on_done)
  if type(opts) == "function" then opts, on_done = nil, opts end
  opts = opts or {}
  assert(type(on_done) == "function", "palette.from_image needs a callback")
  path = morf.fs.expand and morf.fs.expand(path) or path
  local stat, err = morf.fs.stat(path)
  if not stat then
    on_done(false, err)
    return nil, err
  end
  local dir = cache_dir(opts)
  local key = signature(path, stat, opts)
  local file = dir and (dir .. "/palette-" .. hash(key) .. ".json")
  if file then
    local text = morf.fs.read(file)
    if text then
      local ok, doc = pcall(morf.json.decode, text)
      if ok and type(doc) == "table" and doc.key == key and type(doc.palette) == "table" then
        local p = palette.from_hex(doc.palette)
        p.cached = true
        on_done(true, p)
        return true
      end
    end
  end
  local queued, why = morf.image.palette(path, opts.count or 16, function(ok, entries)
    if not ok then return on_done(false, entries) end
    if #entries == 0 then return on_done(false, "the picture has no opaque pixels") end
    local done, half = pcall(derive_tokens, entries, opts)
    if not done then return on_done(false, tostring(half)) end
    -- The tokens, the terminal set and the cache, and the caller each get a
    -- turn of their own: a handler has a fuel budget, and whoever asked for
    -- a palette usually wants to write a few files with it.
    morf.timer(1, function()
      local finished, p = pcall(derive_rest, half, opts)
      if not finished then return on_done(false, tostring(p)) end
      p.source = path
      if file then
        local plain = palette.to_hex(p)
        local encoded = morf.json.encode({ key = key, palette = plain })
        local written, failure = morf.fs.write(file, encoded, { atomic = true, parents = true })
        if not written and morf.log then morf.log.warn("palette cache:", failure) end
      end
      p.cached = false
      morf.timer(1, function() on_done(true, p) end, false)
    end, false)
  end)
  if not queued then
    on_done(false, why)
    return nil, why
  end
  return true
end

-- ── templates ─────────────────────────────────────────────────────────────

local function channel(value) return math.floor(value * 255 + 0.5) end

-- How a colour is written out: `{{accent.hex}}`, or `{{accent | rgb}}`.
local FORMATS = {
  hex = function(c) return c:hex() end,
  strip = function(c) return (c:hex():gsub("^#", "")) end,
  hexa = function(c)
    local rgb = c:rgb()
    return c:hex():sub(1, 7) .. string.format("%02x", channel(rgb.a))
  end,
  xhex = function(c) return "0x" .. c:hex():sub(2) end,
  rgb = function(c)
    local v = c:rgb8()
    return string.format("%d,%d,%d", v.r, v.g, v.b)
  end,
  rgba = function(c)
    local v = c:rgb8()
    return string.format("%d,%d,%d,%s", v.r, v.g, v.b, tostring(v.a))
  end,
  css = function(c) return c:rgb_string() end,
  hsl = function(c) return c:hsl_string() end,
  oklch = function(c) return c:oklch_string() end,
  r = function(c) return tostring(c:rgb8().r) end,
  g = function(c) return tostring(c:rgb8().g) end,
  b = function(c) return tostring(c:rgb8().b) end,
  name = function(c) return c:nearest_name() end,
}
palette.FORMATS = FORMATS

local function argument(p, word)
  local number = tonumber(word)
  if number then return number end
  local value = lookup(p, word)
  if value ~= nil then return value end
  return word
end

-- What a colour can be put through: the `morf.color` changes, by name.
local FILTERS = {
  lighten = function(c, n) return c:lighten(n) end,
  darken = function(c, n) return c:darken(n) end,
  saturate = function(c, n) return c:saturate(n) end,
  desaturate = function(c, n) return c:desaturate(n) end,
  rotate = function(c, n) return c:rotate(n) end,
  alpha = function(c, n) return c:alpha(n) end,
  mix = function(c, other, t, space) return c:mix(other, t or 0.5, space or "oklab") end,
  complement = function(c) return c:complement() end,
  invert = function(c) return c:invert() end,
  gray = function(c) return c:gray() end,
  text_color = function(c) return c:text_color() end,
}
palette.FILTERS = FILTERS

local function words(text)
  local out = {}
  for word in text:gmatch("%S+") do out[#out + 1] = word end
  return out
end

local function expression(p, source)
  local stages = {}
  for stage in (source .. "|"):gmatch("([^|]*)|") do
    stages[#stages + 1] = stage:match("^%s*(.-)%s*$")
  end
  local path = stages[1]
  local value, format
  local parts = {}
  for part in path:gmatch("[^%.]+") do parts[#parts + 1] = part end
  value = p
  for index, part in ipairs(parts) do
    if is_color(value) then
      if index ~= #parts or not FORMATS[part] then
        error("palette.render: `" .. path .. "` does not name a value", 0)
      end
      format = part
    elseif type(value) == "table" then
      value = value[part]
    else
      value = nil
    end
    if value == nil then error("palette.render: no `" .. path .. "` in the palette", 0) end
  end
  for index = 2, #stages do
    local w = words(stages[index])
    local name = w[1]
    if name == "upper" or name == "lower" then
      if is_color(value) then value = FORMATS[format or "hex"](value) format = nil end
      value = name == "upper" and tostring(value):upper() or tostring(value):lower()
    elseif FORMATS[name] and is_color(value) then
      value = FORMATS[name](value)
    elseif FILTERS[name] and is_color(value) then
      local args = {}
      for position = 2, #w do args[position - 1] = argument(p, w[position]) end
      value = FILTERS[name](value, table.unpack(args))
    else
      error("palette.render: unknown filter `" .. tostring(name) .. "` in {{" .. source .. "}}", 0)
    end
  end
  if is_color(value) then return FORMATS[format or "hex"](value) end
  return tostring(value)
end

--- Fills `{{ ... }}` in a template from the palette. A name is a token
--- (`{{accent}}`), a terminal slot (`{{terminal.color4}}`) or a field
--- (`{{mode}}`); a colour is written as "#rrggbb" unless a format follows
--- it (`{{accent.rgb}}`, `.strip`, `.hexa`, `.xhex`, `.rgba`, `.css`,
--- `.hsl`, `.oklch`, `.r`/`.g`/`.b`, `.name`). Filters follow a bar and
--- chain: `{{accent | lighten 0.1 | strip}}`, `{{surface | mix accent 0.2}}`.
--- An unknown name or filter raises, naming it.
function palette.render(template, p)
  return (template:gsub("{{(.-)}}", function(source) return expression(p, source) end))
end

-- ── writers ───────────────────────────────────────────────────────────────
--
-- `palette.build.<name>(p)` is the file as a string; `palette.write.<name>(p,
-- path)` writes it atomically (a temporary file renamed over the old one, so
-- a program reading it mid-write never sees half) and answers `true` or
-- `nil, message`.

local build = {}
palette.build = build

local HEADER = "generated by lib/palette.lua; edits here are lost at the next palette change."

local function term(p, name) return hex(p.terminal[name]) end

-- Terminal-side derivations shared by btop and cava: the palette measured
-- against the terminal's ground rather than its own.
local function on_terminal(p, value, minimum)
  local ground = ink_of(p.terminal.background)
  return hex(paint(lift(ink_of(value), minimum or 4.5, ground)))
end

local function bright_on_terminal(p, value)
  local ground = ink_of(p.terminal.background)
  return hex(paint(brighten(ink_of(value), ground)))
end

local function mixed(a, b, t) return hex(morf.color(a):mix(morf.color(b), t, "oklab")) end

function build.kitty(p)
  local lines = {
    "# " .. HEADER,
    "# kitty: `include " .. "colors.conf` in kitty.conf.",
    "",
    "foreground " .. term(p, "foreground"),
    "background " .. term(p, "background"),
    "selection_foreground " .. term(p, "selection_foreground"),
    "selection_background " .. term(p, "selection"),
    "cursor " .. term(p, "cursor"),
    "cursor_text_color " .. term(p, "cursor_text"),
    "url_color " .. term(p, "color14"),
    "active_border_color " .. hex(p.accent),
    "inactive_border_color " .. hex(p.border),
    "bell_border_color " .. hex(p.red),
    "active_tab_foreground " .. hex(p.accentText),
    "active_tab_background " .. hex(p.accent),
    "inactive_tab_foreground " .. term(p, "color7"),
    "inactive_tab_background " .. term(p, "background"),
    "tab_bar_background " .. term(p, "background"),
    "",
  }
  for slot = 0, 15 do lines[#lines + 1] = "color" .. slot .. " " .. term(p, "color" .. slot) end
  return table.concat(lines, "\n") .. "\n"
end

function build.foot(p)
  local function bare(name) return (term(p, name):gsub("^#", "")) end
  local lines = {
    "# " .. HEADER,
    "# foot: `include=<this file>` in foot.ini.",
    "",
    "[colors]",
    "foreground=" .. bare("foreground"),
    "background=" .. bare("background"),
    "selection-foreground=" .. bare("selection_foreground"),
    "selection-background=" .. bare("selection"),
    "urls=" .. bare("color14"),
  }
  for slot = 0, 7 do lines[#lines + 1] = "regular" .. slot .. "=" .. bare("color" .. slot) end
  for slot = 0, 7 do lines[#lines + 1] = "bright" .. slot .. "=" .. bare("color" .. (slot + 8)) end
  lines[#lines + 1] = ""
  lines[#lines + 1] = "[cursor]"
  lines[#lines + 1] = "color=" .. bare("cursor_text") .. " " .. bare("cursor")
  return table.concat(lines, "\n") .. "\n"
end

local ANSI_NAMES = { "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white" }

function build.alacritty(p)
  local function q(name) return '"' .. term(p, name) .. '"' end
  local lines = {
    "# " .. HEADER,
    "# alacritty: `import = [\"<this file>\"]` under [general].",
    "",
    "[colors.primary]",
    "background = " .. q("background"),
    "foreground = " .. q("foreground"),
    "",
    "[colors.cursor]",
    "text = " .. q("cursor_text"),
    "cursor = " .. q("cursor"),
    "",
    "[colors.selection]",
    "text = " .. q("selection_foreground"),
    "background = " .. q("selection"),
    "",
    "[colors.normal]",
  }
  for slot, name in ipairs(ANSI_NAMES) do lines[#lines + 1] = name .. " = " .. q("color" .. (slot - 1)) end
  lines[#lines + 1] = ""
  lines[#lines + 1] = "[colors.bright]"
  for slot, name in ipairs(ANSI_NAMES) do lines[#lines + 1] = name .. " = " .. q("color" .. (slot + 7)) end
  return table.concat(lines, "\n") .. "\n"
end

-- btop wants forty-eight colours; impasto's mapping of thirteen onto them.
function build.btop(p)
  local ground = term(p, "background")
  local text, muted, accent = on_terminal(p, p.text), on_terminal(p, p.textMuted), on_terminal(p, p.accent)
  local inactive = mixed(muted, ground, 0.55)
  local line = mixed(p.border, ground, 0.45)
  local filling = { on_terminal(p, p.green), on_terminal(p, p.yellow), on_terminal(p, p.red) }
  local emptying = { filling[3], filling[2], filling[1] }
  -- The process box's states: the ground lifted towards the accent (a
  -- followed process), the red (paused) or the text (the banner), and one
  -- foreground that reads on all three.
  local raised = mixed(ground, p.text, 0.14)
  local following = mixed(ground, p.accent, 0.4)
  local paused = mixed(ground, p.red, 0.4)
  local state_ink = ink_of(p.text)
  for _, fill in ipairs({ raised, following, paused }) do state_ink = lift(state_ink, 4.5, ink_of(fill)) end
  local state_fg = hex(paint(state_ink))
  local function ramp(value)
    return { mixed(value, ground, 0.55), on_terminal(p, value), bright_on_terminal(p, value) }
  end
  local entries = {
    { "main_bg", ground }, { "main_fg", text }, { "title", bright_on_terminal(p, p.text) },
    { "hi_fg", accent }, { "inactive_fg", inactive }, { "graph_text", muted },
    { "selected_bg", hex(p.accent) }, { "selected_fg", hex(p.accentText) }, { "proc_misc", accent },
    { "followed_bg", following }, { "followed_fg", state_fg },
    { "proc_follow_bg", following }, { "proc_pause_bg", paused },
    { "proc_banner_bg", raised }, { "proc_banner_fg", state_fg },
    { "cpu_box", line }, { "mem_box", line }, { "net_box", line }, { "proc_box", line },
    { "div_line", line }, { "meter_bg", line },
  }
  local stops = { "start", "mid", "end" }
  local function gradient(name, values)
    for index, stop in ipairs(stops) do entries[#entries + 1] = { name .. "_" .. stop, values[index] } end
  end
  gradient("temp", filling)
  gradient("cpu", filling)
  gradient("used", filling)
  gradient("free", emptying)
  gradient("available", emptying)
  gradient("cached", ramp(p.blue))
  gradient("download", ramp(p.accent))
  gradient("upload", ramp(p.accentHover))
  gradient("process", { text, muted, inactive })
  local lines = { "# " .. HEADER, "# btop: color_theme = \"<this file's name, no .theme>\".", "" }
  for _, entry in ipairs(entries) do
    lines[#lines + 1] = "theme[" .. entry[1] .. "]=\"" .. entry[2] .. "\""
  end
  return table.concat(lines, "\n") .. "\n"
end

-- cava's colour section: four stops in the accent's family, nothing red,
-- because a loud passage is not a warning (impasto).
function build.cava(p)
  local ground = term(p, "background")
  local lines = {
    "# " .. HEADER,
    "# cava: paste into the config, or `theme = '<name>'` with this in themes/.",
    "",
    "[color]",
    "foreground = '" .. on_terminal(p, p.accent) .. "'",
    "",
    "gradient = 1",
    "gradient_count = 4",
    "gradient_color_1 = '" .. mixed(p.accent, ground, 0.6) .. "'",
    "gradient_color_2 = '" .. on_terminal(p, p.accent) .. "'",
    "gradient_color_3 = '" .. on_terminal(p, p.accentHover) .. "'",
    "gradient_color_4 = '" .. bright_on_terminal(p, p.accentHover) .. "'",
  }
  return table.concat(lines, "\n") .. "\n"
end

local function defines(pairs_list)
  local lines = { "/* " .. HEADER .. " */", "" }
  for _, entry in ipairs(pairs_list) do
    lines[#lines + 1] = "@define-color " .. entry[1] .. " " .. entry[2] .. ";"
  end
  return table.concat(lines, "\n") .. "\n"
end

-- libadwaita's named colours: a user gtk.css can redefine all of them.
function build.gtk4(p)
  local ground, raised, border = hex(p.background), hex(p.surface), hex(p.border)
  local text, on_accent = hex(p.text), hex(p.accentText)
  local accent_text = hex(paint(lift(ink_of(p.accent), 4.5, ink_of(p.background))))
  return defines({
    { "window_bg_color", ground }, { "window_fg_color", text },
    { "view_bg_color", ground }, { "view_fg_color", text },
    { "headerbar_bg_color", raised }, { "headerbar_fg_color", text },
    { "headerbar_border_color", border }, { "headerbar_backdrop_color", ground },
    { "headerbar_shade_color", border },
    { "popover_bg_color", raised }, { "popover_fg_color", text },
    { "card_bg_color", raised }, { "card_fg_color", text },
    { "dialog_bg_color", raised }, { "dialog_fg_color", text },
    { "sidebar_bg_color", raised }, { "sidebar_fg_color", text },
    { "sidebar_border_color", border }, { "sidebar_backdrop_color", ground },
    { "secondary_sidebar_bg_color", raised }, { "secondary_sidebar_fg_color", text },
    { "accent_color", accent_text }, { "accent_bg_color", hex(p.accent) },
    { "accent_fg_color", on_accent },
    { "destructive_color", hex(p.red) }, { "destructive_bg_color", hex(p.red) },
    { "destructive_fg_color", hex(p.red:text_color()) },
    { "success_color", hex(p.green) }, { "success_bg_color", hex(p.green) },
    { "success_fg_color", hex(p.green:text_color()) },
    { "warning_color", hex(p.yellow) }, { "warning_bg_color", hex(p.yellow) },
    { "warning_fg_color", hex(p.yellow:text_color()) },
    { "error_color", hex(p.red) }, { "error_bg_color", hex(p.red) },
    { "error_fg_color", hex(p.red:text_color()) },
    { "borders", border },
    { "window_fg_color_muted", hex(p.textMuted) },
  })
end

-- GTK 3's theme colours, the names Adwaita and most GTK 3 themes read.
function build.gtk3(p)
  local ground, raised, hover = hex(p.background), hex(p.surface), hex(p.surfaceHover)
  local text, muted = hex(p.text), hex(p.textMuted)
  return defines({
    { "theme_bg_color", ground }, { "theme_fg_color", text },
    { "theme_base_color", raised }, { "theme_text_color", text },
    { "theme_selected_bg_color", hex(p.accent) }, { "theme_selected_fg_color", hex(p.accentText) },
    { "insensitive_bg_color", ground }, { "insensitive_fg_color", muted },
    { "insensitive_base_color", raised },
    { "theme_unfocused_bg_color", mixed(ground, raised, 0.5) }, { "theme_unfocused_fg_color", muted },
    { "theme_unfocused_base_color", raised }, { "theme_unfocused_text_color", muted },
    { "theme_unfocused_selected_bg_color", hover }, { "theme_unfocused_selected_fg_color", text },
    { "borders", hex(p.border) }, { "unfocused_borders", hex(p.border) },
    { "warning_color", hex(p.yellow) }, { "error_color", hex(p.red) },
    { "success_color", hex(p.green) },
    { "accent_color", hex(p.accent) }, { "accent_bg_color", hex(p.accent) },
    { "accent_fg_color", hex(p.accentText) },
  })
end

-- Both, for a gtk.css that GTK 3 and GTK 4 each read their half of.
function build.gtk(p) return build.gtk4(p) .. "\n" .. build.gtk3(p):gsub("^/%*.-%*/\n\n", "") end

--- Every token and terminal slot as hex, in JSON.
function build.json(p) return morf.json.encode(palette.to_hex(p), true) .. "\n" end

local function sorted_keys(t)
  local keys = {}
  for key in pairs(t) do keys[#keys + 1] = key end
  table.sort(keys)
  return keys
end

local function lua_value(value)
  if type(value) == "string" then return string.format("%q", value) end
  return tostring(value)
end

--- The same as a Lua module: `local colors = dofile(path)`.
function build.lua(p)
  local plain = palette.to_hex(p)
  local lines = { "-- " .. HEADER, "return {" }
  for _, key in ipairs(sorted_keys(plain)) do
    local value = plain[key]
    local name = key:match("^[%a_][%w_]*$") and key or "[" .. string.format("%q", key) .. "]"
    if type(value) == "table" then
      lines[#lines + 1] = "  " .. name .. " = {"
      if #value > 0 then
        for _, item in ipairs(value) do lines[#lines + 1] = "    " .. lua_value(item) .. "," end
      else
        for _, inner in ipairs(sorted_keys(value)) do
          lines[#lines + 1] = "    " .. inner .. " = " .. lua_value(value[inner]) .. ","
        end
      end
      lines[#lines + 1] = "  },"
    else
      lines[#lines + 1] = "  " .. name .. " = " .. lua_value(value) .. ","
    end
  end
  lines[#lines + 1] = "}"
  return table.concat(lines, "\n") .. "\n"
end

--- pywal's `colors.json`: what pywalfox, wal-aware templates and a
--- `morf.theme { source = }` read.
function build.pywal(p)
  local colors = {}
  for slot = 0, 15 do colors["color" .. slot] = term(p, "color" .. slot) end
  return morf.json.encode({
    wallpaper = p.source or "",
    alpha = "100",
    special = {
      background = term(p, "background"),
      foreground = term(p, "foreground"),
      cursor = term(p, "cursor"),
    },
    colors = colors,
  }, true) .. "\n"
end

palette.write = {}
for name, builder in pairs(build) do
  palette.write[name] = function(p, path, opts)
    assert(type(path) == "string", "palette.write." .. name .. " needs a path")
    local text = builder(p, opts)
    return morf.fs.write(path, text, { atomic = true, parents = true })
  end
end

--- Renders a template and writes it, atomically.
function palette.write.template(template, p, path)
  assert(type(path) == "string", "palette.write.template needs a path")
  return morf.fs.write(path, palette.render(template, p), { atomic = true, parents = true })
end

--- What each program needs told after its file is rewritten. Data only;
--- nothing here is run by this library. `signal` goes to every process
--- named `process`; `command` is a program line to run; `note` says the
--- rest.
palette.reload_hints = {
  kitty = {
    process = "kitty", signal = "SIGUSR1",
    command = { "kitten", "@", "set-colors", "--all", "--configured", "<path>" },
    note = "SIGUSR1 re-reads kitty.conf and its includes; the remote-control "
      .. "command repaints open windows without it.",
  },
  foot = { note = "foot reads its colours at start; new windows (or a restarted server) pick it up." },
  alacritty = { note = "live_config_reload (on by default) re-reads imports when the file changes." },
  btop = { note = "read when btop starts, or when the theme is chosen again in its options." },
  cava = { process = "cava", signal = "SIGUSR2", note = "SIGUSR2 re-reads the colours only." },
  gtk3 = { note = "read when an application starts." },
  gtk4 = { note = "read when an application starts." },
  gtk = { note = "read when an application starts." },
  pywal = { note = "readers watch or re-read colors.json themselves; a morf.theme with this source follows it." },
  json = { note = "a morf.theme { source = path } re-reads it when it is rewritten." },
  lua = { note = "whoever dofile()s it, again." },
}

return palette
