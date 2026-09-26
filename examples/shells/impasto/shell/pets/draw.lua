-- The drawing kit the four pet styles share: Qt's shade arithmetic and a
-- small SVG writer.
--
-- impasto draws its pets with QtQuick.Shapes: cubic paths, linear gradients,
-- rotated rounded rectangles. morf has no path node, but an `Image` takes an
-- SVG document as its source (`<svg ...>` inline, no file), rasterised at the
-- node's size, so each style writes its creature as one small document. Every
-- coordinate in the originals is a fraction of the pet's size; here they are
-- written against a 100-unit square, which is the same fractions times 100.
--
-- The shades are Qt's `darker`/`lighter` exactly (HSV value scaled, and a
-- lighter that runs out of value takes it from the saturation instead), so a
-- coat and its shadows land on the same colours the original paints.

local theme = require("theme")

local draw = {}

-- ------------------------------------------------------------------ shades --

local function rgb_of(colour)
  if type(colour) == "string" then colour = morf.color(colour) end
  local c = colour:rgb()
  return c.r, c.g, c.b, c.a or 1
end

local function hsv(r, g, b)
  local max, min = math.max(r, g, b), math.min(r, g, b)
  local d = max - min
  local h = 0
  if d > 0 then
    if max == r then h = ((g - b) / d) % 6
    elseif max == g then h = (b - r) / d + 2
    else h = (r - g) / d + 4 end
    h = h * 60
  end
  local s = max > 0 and d / max or 0
  return h, s, max
end

local function rgb(h, s, v)
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b
  if h < 60 then r, g, b = c, x, 0
  elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return r + m, g + m, b + m
end

local function hex(r, g, b)
  local function byte(v) return math.max(0, math.min(255, math.floor(v * 255 + 0.5))) end
  return string.format("#%02x%02x%02x", byte(r), byte(g), byte(b))
end

--- A colour as `#rrggbb`, for writing into a document.
function draw.hex(colour)
  local r, g, b = rgb_of(colour)
  return hex(r, g, b)
end

--- `Qt.darker(colour, factor)`: the HSV value divided by the factor.
function draw.darker(colour, factor)
  local h, s, v = hsv(rgb_of(colour))
  return hex(rgb(h, s, v / factor))
end

--- `Qt.lighter(colour, factor)`: the value multiplied; what would go past
--- white is taken off the saturation, so a bright coat pales rather than
--- clips.
function draw.lighter(colour, factor)
  local h, s, v = hsv(rgb_of(colour))
  v = v * factor
  if v > 1 then
    s = math.max(0, s - (v - 1))
    v = 1
  end
  return hex(rgb(h, s, v))
end

-- ------------------------------------------------------------------- coats --

local tints = { accent = true, green = true, yellow = true, red = true, blue = true }

--- The coat a species wears: its palette token, so it follows the
--- wallpaper. Inside a binding the binding follows the palette.
function draw.coat(kind)
  local tint = kind and tints[kind.tint] and kind.tint or "accent"
  return draw.hex(theme.color[tint]())
end

-- -------------------------------------------------------------------- svg --

local function n(v)
  -- Short, stable numbers: the document is a cache key, so the same drawing
  -- must print the same text.
  local s = string.format("%.3f", v)
  s = s:gsub("0+$", ""):gsub("%.$", "")
  if s == "-0" then s = "0" end
  return s
end
draw.n = n

--- An SVG document around `body`, framing `view` = { x, y, w, h } of the
--- 100-unit square. `defs` are gradients.
function draw.document(view, defs, body)
  return string.format(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="%s %s %s %s"><defs>%s</defs>%s</svg>',
    n(view[1]), n(view[2]), n(view[3]), n(view[4]), defs or "", body)
end

local function attrs(style)
  if not style then return "" end
  local out = {}
  for _, key in ipairs { "fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin",
    "opacity", "fill-opacity", "transform", "shape-rendering" } do
    local v = style[key]
    if v ~= nil then
      if type(v) == "number" then v = n(v) end
      out[#out + 1] = string.format('%s="%s"', key, v)
    end
  end
  return " " .. table.concat(out, " ")
end
draw.attrs = attrs

--- A path from Qt's pieces: `{ "M", x, y }`, `{ "L", x, y }`,
--- `{ "Q", cx, cy, x, y }`, `{ "C", c1x, c1y, c2x, c2y, x, y }`,
--- `{ "A", rx, ry, rotation, large, sweep, x, y }`, `{ "Z" }`.
function draw.path(steps, style)
  local d = {}
  for _, step in ipairs(steps) do
    d[#d + 1] = step[1]
    for i = 2, #step do d[#d + 1] = n(step[i]) end
  end
  return string.format('<path d="%s"%s/>', table.concat(d, " "), attrs(style))
end

--- A vertical (or any) linear gradient in user units, Qt's
--- `LinearGradient { x1 y1 x2 y2 }` with its stops `{ position, colour }`.
function draw.gradient(id, x1, y1, x2, y2, stops)
  local out = {}
  for _, stop in ipairs(stops) do
    out[#out + 1] = string.format('<stop offset="%s" stop-color="%s"/>', n(stop[1]), stop[2])
  end
  return string.format(
    '<linearGradient id="%s" gradientUnits="userSpaceOnUse" x1="%s" y1="%s" x2="%s" y2="%s">%s</linearGradient>',
    id, n(x1), n(y1), n(x2), n(y2), table.concat(out))
end

--- A QtQuick `Rectangle`: `radius` clamped to half the short side as Qt
--- clamps it, optional per-corner radii `{ tl, tr, br, bl }`, and a
--- `rotation` in degrees about its centre.
function draw.rect(x, y, w, h, radius, style, rotation, corners)
  style = style or {}
  local limit = math.min(w, h) / 2
  if rotation and rotation ~= 0 then
    local turned = {}
    for k, v in pairs(style) do turned[k] = v end
    turned.transform = string.format("rotate(%s %s %s)", n(rotation), n(x + w / 2), n(y + h / 2))
    style = turned
  end
  if corners then
    local tl, tr, br, bl = math.min(corners[1], limit), math.min(corners[2], limit),
      math.min(corners[3], limit), math.min(corners[4], limit)
    return draw.path({
      { "M", x + tl, y },
      { "L", x + w - tr, y }, { "A", tr, tr, 0, 0, 1, x + w, y + tr },
      { "L", x + w, y + h - br }, { "A", br, br, 0, 0, 1, x + w - br, y + h },
      { "L", x + bl, y + h }, { "A", bl, bl, 0, 0, 1, x, y + h - bl },
      { "L", x, y + tl }, { "A", tl, tl, 0, 0, 1, x + tl, y },
      { "Z" },
    }, style)
  end
  local r = math.min(radius or 0, limit)
  return string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s"%s/>',
    n(x), n(y), n(w), n(h), n(r), attrs(style))
end

return draw
