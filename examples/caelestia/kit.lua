-- Small pieces every part of the shell uses: a Material Symbols icon, text
-- in the shell's face, a rounded card.

local ui = require("morf.ui")
local theme = require("theme")

local M = {}

--- A Material Symbols Rounded icon by its ligature name (`"wifi_off"`).
--- `name` and `color` may be bindings; `props.fill` (a boolean or a
--- binding) fills it in through the face's FILL axis.
function M.icon(name, size, color, props)
  props = props or {}
  props.text = name
  props.font_family = theme.icon_font
  props.font_size = size or 18
  props.color = color or function() return theme.color.onSurface end
  if props.fill ~= nil then
    local fill = props.fill
    props.fill = nil
    props.axes = function()
      local on = fill
      if type(fill) == "function" then on = fill() end
      return { FILL = on and 1 or 0 }
    end
    props.behavior = props.behavior or {}
    props.behavior.axes = { duration = theme.duration.small, easing = theme.ease.standard }
  end
  return ui.Text(props)
end

--- Text in Rubik; `props` as a `ui.Text`'s.
function M.text(props)
  props.font_family = props.font_family or theme.font
  if theme.font_file ~= "" and props.font_source == nil then props.font_source = theme.font_file end
  if props.font_weight and props.axes == nil then props.axes = { wght = props.font_weight } end
  props.font_size = props.font_size or theme.size.normal
  if props.color == nil then props.color = function() return theme.color.onSurface end end
  return ui.Text(props)
end

--- A card: a surfaceContainer box with the large rounding.
function M.card(props)
  props.radius = props.radius or theme.ROUNDING
  if props.color == nil then props.color = function() return theme.color.surfaceContainer end end
  return ui.Rect(props)
end

--- An item centred in a box of `w` x `h`.
function M.centred(w, h, child, props)
  props = props or {}
  props.width, props.height = w, h
  child.anchors = { center_in = true }
  props[#props + 1] = child
  return ui.Item(props)
end

--- Gives a MouseArea a rounded background whose colour follows its hover:
--- `color(hovered)`. (Built after the area, so the binding can read it.)
function M.hover(area, color, radius)
  local bg = ui.Rect {
    anchors = { fill = true }, z = -1, radius = radius or 0,
    color = function() return color(area.hovered) end,
    behavior = { color = { duration = theme.duration.small } },
  }
  ui.reparent(bg, area)
  return area
end

-- ----------------------------------------------------------------- gauges --

--- SVG path data for an arc of `sweep` degrees, clockwise from `from`
--- degrees (0 at twelve o'clock), radius `r` about `(cx, cy)`, in pieces of
--- at most 90 degrees.
function M.arc_path(cx, cy, r, from, sweep)
  local function at(deg)
    local a = math.rad(deg)
    return cx + r * math.sin(a), cy - r * math.cos(a)
  end
  local x, y = at(from)
  local d = { ("M%.3f %.3f"):format(x, y) }
  local pieces = math.max(1, math.ceil(sweep / 90))
  for i = 1, pieces do
    local ex, ey = at(from + sweep * i / pieces)
    d[#d + 1] = ("A%.3f %.3f 0 0 1 %.3f %.3f"):format(r, r, ex, ey)
  end
  return table.concat(d, " ")
end

local function clamp01(x)
  x = tonumber(x) or 0
  if x ~= x then return 0 end
  return math.max(0, math.min(1, x))
end

--- A Material 3 expressive progress arc: the value's part in `color`, a
--- gap, then the rest of the track, and a small stop dot at the track's
--- end. `spec`: `value()` (0..1), `size` (the square it sits in), `stroke`,
--- `from` and `sweep` (degrees, clockwise from twelve o'clock), `color`,
--- `track` (bindings), `gap` (px between the two), `dot` (false for none),
--- `id`, and children to lay over it.
function M.gauge(spec)
  local size, stroke = spec.size, spec.stroke or 6
  local r = size / 2 - stroke / 2
  local sweep = spec.sweep or 360
  local d = M.arc_path(size / 2, size / 2, r, spec.from or 0, sweep)
  local length = 2 * math.pi * r * sweep / 360
  -- Round caps reach half a stroke past each end: the gap is between them.
  local gap = ((spec.gap or 4) + stroke) / length
  local function v() return clamp01(spec.value()) end
  local motion = { duration = theme.duration.large, easing = theme.ease.emphasized_decel }
  local node = {
    id = spec.id, width = size, height = size, x = spec.x, y = spec.y, anchors = spec.anchors,
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, size, size }, d = d,
      fill_color = "transparent", stroke_width = stroke, stroke_cap = "round",
      stroke_color = spec.track,
      trim_start = function()
        local x = v()
        return x <= 0 and 0 or math.min(1, x + gap)
      end,
      behavior = { trim_start = motion },
    },
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, size, size }, d = d,
      fill_color = "transparent", stroke_width = stroke, stroke_cap = "round",
      stroke_color = spec.color,
      opacity = function() return v() > 0.002 and 1 or 0 end,
      trim_end = function() return math.max(0.001, v()) end,
      behavior = { trim_end = motion },
    },
  }
  if spec.dot ~= false and sweep < 360 then
    local a = math.rad((spec.from or 0) + sweep)
    local ex, ey = size / 2 + r * math.sin(a), size / 2 - r * math.cos(a)
    local dot = math.max(2, stroke * 0.55)
    node[#node + 1] = ui.Rect {
      x = ex - dot / 2, y = ey - dot / 2, width = dot, height = dot, radius = dot / 2,
      color = spec.color,
    }
  end
  for _, child in ipairs(spec) do node[#node + 1] = child end
  return ui.Item(node)
end

--- A straight M3 expressive progress bar: the value in `color`, a gap, the
--- track, a stop dot at the end. `spec`: `width`, `stroke`, `value()`
--- (0..1), `color`, `track`, `id`.
function M.bar(spec)
  local w, h = spec.width, spec.stroke or 6
  local GAP = 4
  local function v() return clamp01(spec.value()) end
  local motion = { duration = theme.duration.large, easing = theme.ease.emphasized_decel }
  local function split() return math.min(w, v() * w + GAP + h) end
  return ui.Item {
    id = spec.id, width = w, height = h, x = spec.x, y = spec.y, anchors = spec.anchors,
    ui.Rect {
      height = h, radius = h / 2, color = spec.track,
      x = split,
      width = function() return math.max(0, w - split()) end,
      behavior = { x = motion, width = motion },
    },
    ui.Rect {
      height = h, radius = h / 2, color = spec.color,
      width = function() return math.max(h, v() * w) end,
      opacity = function() return v() > 0 and 1 or 0 end,
      behavior = { width = motion },
    },
    ui.Rect {
      x = w - h * 0.7, y = h * 0.15, width = h * 0.7, height = h * 0.7, radius = h * 0.35,
      color = spec.color,
    },
  }
end

--- Bytes as the reference writes them: binary units, one decimal under
--- ten, none above -- "1.1", "MiB". `unit` forces one.
function M.bytes(n, unit)
  n = tonumber(n) or 0
  local units = { "B", "KiB", "MiB", "GiB", "TiB", "PiB" }
  local i = 1
  if unit then
    for k, u in ipairs(units) do if u == unit then i = k end end
    n = n / 1024 ^ (i - 1)
  else
    while n >= 1024 and i < #units do n = n / 1024 i = i + 1 end
  end
  local text = (n < 10 and i > 1) and ("%.1f"):format(n) or ("%d"):format(math.floor(n + 0.5))
  return text, units[i]
end

return M
