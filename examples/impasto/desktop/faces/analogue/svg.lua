-- The drawing kit the Analogue faces share.
--
-- impasto drew its dials, gauges and parcels with QtQuick Rectangles and
-- Shapes. morf has no path node, but an Image takes an SVG document as its
-- source, rasterised at the node's size; so each object here is one small
-- document written in the node's own pixels, and a moving part (a hand, a
-- needle, a pointer) is a document of its own on a node whose `rotation`
-- is bound, so time passing turns a picture instead of writing a new one.
-- Documents are cached by their text's inputs, so a binding that re-runs
-- with the same values costs a lookup.

local theme = require("theme")
local draw = require("pets.draw")

local svg = {}

svg.n = draw.n

local function rgba(colour)
  if type(colour) == "function" then colour = colour() end
  if type(colour) == "string" then colour = morf.color(colour) end
  local c = colour:rgb()
  return c.r, c.g, c.b, c.a or 1
end

local function byte(v) return math.max(0, math.min(255, math.floor(v * 255 + 0.5))) end

--- `#rrggbb` for a colour (value, string or colour function).
function svg.hex(colour)
  local r, g, b = rgba(colour)
  return string.format("#%02x%02x%02x", byte(r), byte(g), byte(b))
end

--- The colour's alpha, 0..1.
function svg.alpha(colour)
  local _, _, _, a = rgba(colour)
  return a
end

--- `fill="#..." fill-opacity=".."` (or stroke) for a colour, keeping alpha.
function svg.paint(kind, colour, extra_alpha)
  local a = svg.alpha(colour) * (extra_alpha or 1)
  if a >= 0.999 then return string.format('%s="%s"', kind, svg.hex(colour)) end
  return string.format('%s="%s" %s-opacity="%s"', kind, svg.hex(colour), kind, draw.n(a))
end

--- `a` laid over `b` with `a`'s alpha, as a colour value: Qt.tint.
function svg.tint(base, over)
  local r1, g1, b1 = rgba(base)
  local r2, g2, b2, a2 = rgba(over)
  return morf.color(string.format("#%02x%02x%02x",
    byte(r1 + (r2 - r1) * a2), byte(g1 + (g2 - g1) * a2), byte(b1 + (b2 - b1) * a2)))
end

--- The Analogue faces' paper: the text colour under the notes' wash.
function svg.paper(ink) return svg.tint(ink.text(), theme.color.paperWash) end

--- A document `w` by `h` in pixels around `body`.
function svg.doc(w, h, body)
  return string.format('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %s %s">%s</svg>',
    draw.n(w), draw.n(h), body)
end

--- An arc path from `a0` sweeping `sweep` degrees (0 = east, clockwise), as
--- Qt's PathAngleArc draws it.
function svg.arc(cx, cy, r, a0, sweep)
  local a1 = a0 + sweep
  local x0, y0 = cx + r * math.cos(math.rad(a0)), cy + r * math.sin(math.rad(a0))
  local x1, y1 = cx + r * math.cos(math.rad(a1)), cy + r * math.sin(math.rad(a1))
  return string.format("M %s %s A %s %s 0 %d %d %s %s", draw.n(x0), draw.n(y0), draw.n(r), draw.n(r),
    math.abs(sweep) > 180 and 1 or 0, sweep >= 0 and 1 or 0, draw.n(x1), draw.n(y1))
end

--- A rounded bar pointing up from (cx, y) to (cx, y + len) rotated `deg`
--- about (ox, oy): a tick or a hand.
function svg.bar(cx, y, width, len, deg, ox, oy, paint)
  return string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" %s transform="rotate(%s %s %s)"/>',
    draw.n(cx - width / 2), draw.n(y), draw.n(width), draw.n(len), draw.n(width / 2), paint,
    draw.n(deg), draw.n(ox), draw.n(oy))
end

local cache, cached = {}, 0
--- A document, memoised by `key`: `make()` runs only for a key not seen.
function svg.cached(key, make)
  local hit = cache[key]
  if hit then return hit end
  if cached > 400 then cache, cached = {}, 0 end
  local out = make()
  cache[key] = out
  cached = cached + 1
  return out
end

--- Snaps a fraction to a step, so a trickle does not mint a picture per tick.
function svg.snap(v, step)
  step = step or 0.01
  return math.floor((tonumber(v) or 0) / step + 0.5) * step
end

return svg
