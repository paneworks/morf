-- The weather drawn as shapes: a sun with rays, a crescent, an outlined
-- cloud, and precipitation or fog under it.
--
-- Port of Sky.qml. The original keyed on the glyph weather.py chose; here
-- the kind comes from the WMO code (`services.weather.kind`) and whether it
-- is day, which is where that glyph came from. `quiet` draws in the muted
-- colour, for the forecast. Rain and lightning use the accent.

local ui = require("morf.ui")
local common = require("desktop.faces.common")
local weather = require("services.weather")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

--- The drawing's kind for a code: sun, moon, partly, partlyNight, cloud,
--- fog, drizzle, rain, snow, storm.
function M.kind_of(code, is_day, known)
  if not known then return "cloud" end
  local k = weather.kind(code)
  local night = is_day == false
  if k == "clear" then return night and "moon" or "sun" end
  if k == "mostly_clear" or k == "partly_cloudy" then return night and "partlyNight" or "partly" end
  if k == "fog" then return "fog" end
  if k == "drizzle" or k == "showers" then return "drizzle" end
  if k == "rain" or k == "freezing" then return "rain" end
  if k == "snow" then return "snow" end
  if k == "storm" then return "storm" end
  return "cloud"
end

local function doc(s, kind, line, accent, muted)
  return svg.cached(table.concat({ "sky", s, kind, line, accent, muted }, ":"), function()
    local stroke = math.max(1.5, s * 0.03)
    local has_sun = kind == "sun" or kind == "partly"
    local has_moon = kind == "moon" or kind == "partlyNight"
    local has_cloud = not (kind == "sun" or kind == "moon")
    local behind = kind == "partly" or kind == "partlyNight"
    local falls = kind == "drizzle" or kind == "rain" or kind == "snow" or kind == "storm" or kind == "fog"
    local parts = {}
    if has_sun then
      local cx = behind and s * 0.32 or s / 2
      local rr = behind and s * 0.15 or s * 0.24
      parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="%s"/>',
        n(cx), n(cx), n(rr), line, n(stroke))
      local reach = rr + rr * 0.32 + rr * 0.5
      for i = 0, 7 do
        parts[#parts + 1] = svg.bar(cx, cx - reach, stroke, rr * 0.5, i * 45, cx, cx, string.format('fill="%s"', line))
      end
    end
    if has_moon then
      local cx = behind and s * 0.34 or s / 2
      local rr = behind and s * 0.17 or s * 0.28
      parts[#parts + 1] = string.format('<path d="M %s %s A %s %s 0 0 1 %s %s A %s %s 0 0 0 %s %s Z" fill="%s"/>',
        n(cx), n(cx - rr), n(rr), n(rr), n(cx), n(cx + rr), n(rr * 1.15), n(rr * 1.15), n(cx), n(cx - rr), line)
    end
    local cw = behind and s * 0.68 or s * 0.84
    local x0 = behind and s * 0.28 or s * 0.08
    local y0 = behind and s * 0.8 or (falls and s * 0.56 or s * 0.68)
    if has_cloud then
      local fill = behind and string.format(' fill="%s" fill-opacity="0"', line) or ' fill="none"'
      parts[#parts + 1] = string.format(
        '<path d="M %s %s A %s %s 0 0 1 %s %s A %s %s 0 0 1 %s %s A %s %s 0 0 1 %s %s L %s %s" stroke="%s" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"%s/>',
        n(x0), n(y0),
        n(cw * 0.19), n(cw * 0.19), n(x0 + cw * 0.30), n(y0 - cw * 0.22),
        n(cw * 0.26), n(cw * 0.26), n(x0 + cw * 0.72), n(y0 - cw * 0.20),
        n(cw * 0.18), n(cw * 0.18), n(x0 + cw), n(y0),
        n(x0), n(y0), line, n(stroke), fill)
    end
    if kind == "drizzle" or kind == "rain" then
      for i = 0, 2 do
        local w = kind == "rain" and stroke or stroke * 0.7
        local h = kind == "rain" and s * 0.2 or s * 0.12
        local x, y = x0 + cw * (0.25 + 0.25 * i), y0 + s * 0.06
        parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s" transform="rotate(20 %s %s)"/>',
          n(x), n(y), n(w), n(h), n(w / 2), accent, n(x + w / 2), n(y + h / 2))
      end
    elseif kind == "snow" then
      for i = 0, 2 do
        parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="%s"/>',
          n(x0 + cw * (0.22 + 0.25 * i)), n(y0 + s * (i == 1 and 0.16 or 0.09) + s * 0.035), n(s * 0.035), line, n(stroke * 0.7))
      end
    elseif kind == "fog" then
      for i = 0, 2 do
        parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>',
          n(x0 + cw * (i == 1 and 0.18 or 0.05)), n(y0 + s * (0.09 + 0.09 * i)), n(cw * (i == 1 and 0.7 or 0.9)),
          n(stroke), n(stroke / 2), muted)
      end
    elseif kind == "storm" then
      local bx, by, u = x0 + cw * 0.5, y0 + s * 0.04, s * 0.055
      local pts = { { 1, 0 }, { -1.4, 2.6 }, { 0.2, 2.6 }, { -0.6, 5 }, { 1.6, 2 }, { 0.1, 2 } }
      local out = {}
      for _, p in ipairs(pts) do out[#out + 1] = n(bx + u * p[1]) .. "," .. n(by + u * p[2]) end
      parts[#parts + 1] = string.format('<polygon points="%s" fill="%s"/>', table.concat(out, " "), accent)
    end
    return svg.doc(s, s, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `kind` (a function returning a kind), `quiet`.
function M.build(values)
  local s, ink = values.size, values.ink
  return ui.Image {
    x = values.x, y = values.y, width = s, height = s,
    source = function()
      local quiet = common.read(values.quiet)
      return doc(s, common.read(values.kind) or "cloud", svg.hex(quiet and ink.muted() or ink.text()),
        svg.hex(ink.accent()), svg.hex(ink.muted()))
    end,
  }
end

return M
