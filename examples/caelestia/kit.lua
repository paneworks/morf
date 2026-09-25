-- Small pieces every part of the shell uses: a Material Symbols icon, text
-- in the shell's face, a rounded card.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")

local M = {}

local function C() return theme.color end

--- The X keysyms `on_key_pressed` is handed, by name.
M.KEY = {
  Up = 0xff52, Down = 0xff54, Left = 0xff51, Right = 0xff53,
  Tab = 0xff09, ISO_Left_Tab = 0xfe20, Return = 0xff0d, KP_Enter = 0xff8d, Escape = 0xff1b,
}

--- Whether `keysym` is the key named `name`.
function M.is_key(keysym, name)
  return keysym == M.KEY[name] or keysym == name
end

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
  props.font_size = props.font_size or theme.size.normal
  -- The reference's font builder sets `opsz` to the size in points (morf's
  -- automatic optical sizing, like CSS's, uses pixels, which reads a size
  -- larger and sets it tighter) and `ROND` 25 on every face. A face without
  -- those axes ignores them.
  if props.axes == nil and type(props.font_size) == "number" then
    props.axes = { opsz = props.font_size * 3 / 4, ROND = 25, wght = props.font_weight }
  elseif props.font_weight and props.axes == nil then
    props.axes = { wght = props.font_weight }
  end
  if props.color == nil then props.color = function() return theme.color.onSurface end end
  return ui.Text(props)
end

--- A card: a surfaceContainer box with the large rounding.
--- While a collector is set (`M.collect(list)`), a card is not a `Rect`
--- but an item whose background is a layer of a distance field someone
--- else builds: `list` gets `{ node, radius, color, shape }` for each, so
--- the cards of a page can merge, bud and melt as one liquid surface.
local collector
function M.collect(list) collector = list end

function M.card(props)
  props.radius = props.radius or theme.ROUNDING
  if props.color == nil then props.color = function() return theme.color.surfaceContainer end end
  if not collector then return ui.Rect(props) end
  local radius, color = props.radius, props.color
  props.radius, props.color = nil, nil
  -- Cards grow in place, evenly about their centres (the default
  -- origin): no squash and stretch, which skewed them as they grew.
  local node = ui.Item(props)
  local entry = { node = node, radius = radius, color = color }
  entry.shape = ui.SdfShape {
    shape = "box", radius = radius, track = node,
    operation = #collector == 0 and "union" or "smooth_union",
    fill_color = color,
  }
  collector[#collector + 1] = entry
  return node
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
  radius = radius or 0
  local bg = ui.Rect {
    anchors = { fill = true }, z = -1,
    -- M3 expressive: pressed, a round button squares up, and springs back.
    radius = function() return area.pressed and radius * 0.45 or radius end,
    color = function() return color(area.hovered) end,
    behavior = {
      color = { duration = theme.duration.small },
      radius = ui.spring { stiffness = 520, damping = 22 },
    },
  }
  ui.reparent(bg, area)
  return area
end

-- ----------------------------------------------------------------- motion --

--- A spring for a `behavior`: the port's one feel for things that move
--- under a hand (selections, thumbs, indicators).
function M.spring(stiffness, damping)
  return ui.spring { stiffness = stiffness or 320, damping = damping or 24 }
end

--- Squash and stretch for something that travels (see UI.md, `stretch`).
M.STRETCH = { stiffness = 260, damping = 14, scale = 0.14, max = 0.3 }

--- Moves a bar from `[l0, r0]` to `[l1, r1]` along `axis` ("x" or "y")
--- the way an M3 indicator does: the edge in front leaves first and fast,
--- the one behind follows, so the bar stretches out towards its target and
--- draws itself in there. `node`'s position and size are driven with dense
--- keyframes of the two edges' curves.
local running = setmetatable({}, { __mode = "k" })
function M.elastic(node, axis, l0, r0, l1, r1, opts)
  opts = opts or {}
  local size = axis == "x" and "width" or "height"
  local duration = opts.duration or 500
  local lead = opts.lead or theme.ease.emphasized_decel
  local trail = opts.trail or theme.ease.standard
  local forward = l1 >= l0
  -- The leading edge covers its way in the first 55 % of the time, the
  -- trailing one starts a little late and takes the rest.
  local function edge(from, to, t, leading)
    local u
    if leading then u = math.min(1, t / 0.55)
    else u = math.max(0, math.min(1, (t - 0.18) / 0.82)) end
    local k = morf.easing.value(leading and lead or trail, u)
    return from + (to - from) * k
  end
  local pos, len = {}, {}
  local N = 16
  for i = 0, N do
    local t = i / N
    local l = edge(l0, l1, t, not forward)
    local r = edge(r0, r1, t, forward)
    pos[#pos + 1] = { at = t, value = l }
    len[#len + 1] = { at = t, value = math.max(0, r - l) }
  end
  if running[node] then running[node]:stop() end
  running[node] = morf.animation.play {
    {
      parallel = {
        { node = node, property = axis, duration = duration, keyframes = pos },
        { node = node, property = size, duration = duration, keyframes = len },
      },
    },
  }
  return running[node]
end

-- --------------------------------------------------------------- shapes --

local shapes -- lib/m3shapes, loaded on first use

--- An M3 expressive shape that morphs whenever `shape()` changes (see
--- lib/m3shapes: `shapes.Shape`). `props` as a `ui.Path`'s; `color` a
--- binding; `duration`, `easing`.
function M.shape(props)
  shapes = shapes or require("lib.m3shapes")
  props.easing = props.easing or theme.ease.spatial
  props.duration = props.duration or 450
  return shapes.Shape(props)
end

--- An M3 expressive shape as an inline SVG document, for an `SdfShape`'s
--- `source`: a drawing is an outline to a field, so the shape unions, melts
--- and morphs with the other layers.
local svgs = {}
function M.svg(name)
  shapes = shapes or require("lib.m3shapes")
  if not svgs[name] then
    svgs[name] = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="%s"/></svg>')
      :format(shapes.path(name))
  end
  return svgs[name]
end

--- A field layer in an M3 expressive shape that morphs, outline to outline,
--- whenever `shape()` changes. `props` as an `SdfShape`'s, and `duration`,
--- `easing`.
function M.sdf_shape(props)
  local source = props.shape
  local current = type(source) == "function" and source() or source
  local motion = { duration = props.duration or 450, easing = props.easing or theme.ease.spatial }
  props.shape, props.duration, props.easing = nil, nil, nil
  props.source = M.svg(current)
  props.source_morph_to = M.svg(current)
  props.morph_progress = 0
  props.behavior = props.behavior or {}
  props.behavior.morph_progress = motion
  local node = ui.SdfShape(props)
  if type(source) == "function" then
    -- As lib/m3shapes' Shape: the two ends take turns, so a change never
    -- jumps back to a start.
    local at_end = false
    morf.effect("caelestia.sdf_shape", function()
      local name = source()
      if name == current then return end
      current = name
      if at_end then
        node.source = M.svg(name)
        node.morph_progress = 0
      else
        node.source_morph_to = M.svg(name)
        node.morph_progress = 1
      end
      at_end = not at_end
    end, { owner = node })
  end
  return node
end

-- The loading indicator's shapes, in the order M3 expressive cycles them.
local LOADING = { "soft_burst", "cookie9", "pentagon", "pill", "sunny", "cookie4", "oval", "flower" }

--- M3 expressive's loading indicator: a shape that morphs from one to the
--- next every 650 ms while turning, in `color`, `size` across. `active()`
--- (a binding, default always) runs it; stopped, it rests.
function M.loading(size, color, props)
  props = props or {}
  local step = morf.signal("caelestia.loading." .. tostring(props.id or math.random(1e9)), 1)
  local active = props.active or function() return true end
  props.active = nil
  local timer
  morf.effect("caelestia.loading.run." .. tostring(step), function()
    if active() then
      if not timer then
        timer = morf.timer(650, function() step:set(step:get() % #LOADING + 1) end, true)
      end
    elseif timer then
      timer:cancel()
      timer = nil
    end
  end)
  props.width, props.height = size, size
  props.shape = function() return LOADING[step:get()] end
  props.color = color
  props.duration = 500
  props.loop = function()
    if not active() then return nil end
    return { rotation = { to = 360, duration = 2600, hold = true } }
  end
  return M.shape(props)
end

-- ---------------------------------------------------------------- controls --

--- A Material 3 switch, 52 x 32: `on()` (a binding) and `on_toggled(now)`.
--- Off, an outlined dark track and a small handle with a cross; on, a
--- primary track and a large handle with a tick.
function M.switch(spec)
  local motion = { duration = theme.duration.small, easing = theme.ease.standard }
  local function on() return spec.on() == true end
  local area
  -- The thumb springs across, grows when on and more when pressed, and
  -- morphs: a circle off, a scalloped cookie on (M3 expressive).
  local function thumb() return area and area.pressed and 28 or (on() and 24 or 16) end
  local jump = M.spring(520, 22)
  area = ui.MouseArea {
    id = spec.id, width = 52, height = 32, cursor = "pointer",
    anchors = spec.anchors, x = spec.x, y = spec.y,
    on_clicked = function() if spec.on_toggled then spec.on_toggled(not on()) end end,
    ui.Rect {
      anchors = { fill = true }, radius = 16,
      color = function() return on() and C().primary or C().surfaceContainerHighest end,
      border_width = function() return on() and 0 or 2 end,
      border_color = function() return C().outline end,
      behavior = { color = motion },
    },
    ui.Item {
      x = function() return (on() and 36 or 16) - thumb() / 2 end,
      y = function() return 16 - thumb() / 2 end,
      width = thumb, height = thumb,
      behavior = { x = jump, y = jump, width = jump, height = jump },
      stretch = M.STRETCH,
      M.shape {
        anchors = { fill = true },
        shape = function() return on() and "cookie12" or "circle" end,
        color = function() return on() and C().onPrimary or C().outline end,
      },
      M.icon(function() return on() and "check" or "close" end, 14, function()
        return on() and C().primary or C().surfaceContainerHighest
      end, { anchors = { center_in = true }, visible = function() return thumb() >= 20 end }),
    },
  }
  return area
end

--- A pill-shaped filled button: `icon`, `label`, `on_clicked`, `width`,
--- `height` (32), and `color`/`ink` (primaryContainer and its ink).
function M.pill(spec)
  local h = spec.height or 32
  local color = spec.color or function() return C().primaryContainer end
  local ink = spec.ink or function() return C().onPrimaryContainer end
  local area = ui.MouseArea {
    id = spec.id, width = spec.width, height = h, cursor = "pointer",
    x = spec.x, y = spec.y, anchors = spec.anchors,
    on_clicked = spec.on_clicked,
    ui.Row {
      anchors = { center_in = true }, gap = 8, align = "center",
      spec.icon and M.icon(spec.icon, 18, ink) or nil,
      M.text { text = spec.label, font_size = theme.size.normal, color = ink },
    },
  }
  return M.hover(area, function(hovered)
    local c = color()
    return hovered and c:mix(ink(), 0.08) or c
  end, h / 2)
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
