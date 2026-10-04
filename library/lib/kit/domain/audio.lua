-- Domain instruments: audio -- the controls and scopes of a mixer, a synth
-- and a mastering chain, over the kit's archetypes and in a theme's style
-- (see lib.kit.display).
--
-- What takes input is built on the kit's behaviours (crates/morf-kit,
-- lib.kit.control): a fader and a knob are a Range, a point on a curve a
-- Plane, a key or a mute button a Press -- the archetype keeps the value,
-- answers the arrows and tells a screen reader its role and value; this
-- draws it. A point is a headless Plane fed by its own handle, so the
-- handle grabs where it is pressed and drags without jumping; a knob is a
-- Range fed by a relative vertical drag.
--
-- What streams -- a visualiser's bands, a scope's x,y pairs, a meter's
-- level, the now-playing bars -- reads a data channel drawn by a
-- `ui.Path`'s `series` and `plot`: no Lua per frame. A curve whose points
-- are dragged (an EQ's response, a compressor's transfer, an automation
-- lane) is laid out in Lua when a point moves, never per frame.
--
-- The look is the style's: Material tonal, rounded and sprung (handles
-- that swell when held, buttons whose corners morph when lit); Tsugumori
-- square, hairline, hatched, mono upper-case captions and registration
-- marks.
local morf = require("morf")
local ui = require("morf.ui")
local U = require("lib.kit.display.util")
local channel = require("lib.util.channel")
local control = require("lib.kit.control")
local get, clamp01 = U.get, U.clamp01

local M = {}
-- What the editor instruments share with these (lib.kit.domain.editor),
-- out of `pairs` so the display registry does not take it for a widget.
local H = {}
setmetatable(M, { __index = { _H = H } })

-- ------------------------------------------------------------ shared --

local serial = 0
function H.uid(kind) serial = serial + 1 return ("kit.audio.%s.%d"):format(kind, serial) end

function H.small(style) return style.size.small - 3 end

--- Text in the style, never under the small size less three.
function H.txt(style, p)
  p.font_size = math.max(p.font_size or style.size.normal, H.small(style))
  if p.color == nil then p.color = style.ink end
  return style.text(p)
end

--- A caption in the style (Tsugumori upper-case mono); a width elides it.
function H.cap(style, p)
  if p.width and p.elide == nil then p.elide = "right" end
  p.font_size = p.font_size or H.small(style)
  if p.color == nil then p.color = style.ink_lo end
  return U.caption(style, p)
end

function H.alpha(c, a) return function() return get(c):alpha(a) end end

function H.add(parent, ...)
  for _, child in ipairs { ... } do if child then ui.reparent(child, parent) end end
  return parent
end

--- A colour from the spec's `color`, else `fallback` of the style.
function H.color(spec, style, fallback) return U.color(spec, style, fallback or "accent") end

--- A path `w` by `h` in its own coordinates.
function H.path(w, h, p)
  p.width, p.height, p.view_box = w, h, { 0, 0, w, h }
  p.fill_color = p.fill_color or "transparent"
  return ui.Path(p)
end

--- The figure an instrument is drawn in: a raised tonal card (Material) or
--- a hairline frame with registration marks (Tsugumori); none when `bare`.
--- Its role is `role` ("figure"), named by the spec's label.
function H.frame(spec, style, name, w, h, role)
  local root = ui.Item(U.place(spec, { width = w, height = h, accessible_role = role or "figure",
    accessible_name = type(spec.label) == "string" and spec.label or (type(spec.title) == "string" and spec.title) or name }))
  if spec.bare ~= true then
    if style.hatched then
      H.add(root, ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1, border_color = style.line },
        style.marks())
    else
      H.add(root, ui.Rect { anchors = { fill = true }, color = style.raised, radius = math.min(20, style.radius(math.min(w, h) / 3) + 4) })
    end
  end
  return root
end

--- The inner padding of a frame.
function H.pad(spec, style) if spec.bare == true then return 0 end return style.hatched and 10 or 12 end

--- A filled shape outlined by `d` (path data or a function of it) in the
--- style: Material one tonal fill; Tsugumori a faint wash with `/` stripes
--- masked to it.
function H.shape(style, w, h, color, d, o)
  o = o or {}
  local function mk(p) p.d = d return H.path(w, h, p) end
  if not style.hatched then return mk { fill_color = H.alpha(color, o.alpha or .22) } end
  local box = ui.Item { width = w, height = h }
  H.add(box, mk { fill_color = H.alpha(color, o.strong and .22 or .1) })
  if w >= 4 and h >= 4 then
    H.add(box, ui.Path { width = w, height = h, view_box = { 0, 0, w, h }, d = style.stripes.hatch_d(w, h, o.gap or 5),
      fill_color = "transparent", stroke_color = H.alpha(color, o.strong and .8 or .5), stroke_width = 1,
      stroke_cap = "butt", mask = mk { fill_color = "#ffffff" } })
  end
  return box
end

--- Guide lines over a plot: `xs` and `ys` (0..1 places) as faint hairlines.
function H.guides(style, w, h, xs, ys, strong)
  local d = {}
  for _, x in ipairs(xs or {}) do d[#d + 1] = ("M%.1f 0 V%.1f "):format(math.floor(x * w) + .5, h) end
  for _, y in ipairs(ys or {}) do d[#d + 1] = ("M0 %.1f H%.1f "):format(math.floor(y * h) + .5, w) end
  if #d == 0 then return nil end
  local color
  if style.hatched then color = style.stroke_of and style.stroke_of(strong and "idle" or "quiet") or H.alpha(style.line, .5)
  else color = H.alpha(style.line, strong and .8 or .45) end
  return H.path(w, h, { d = table.concat(d), stroke_color = color, stroke_width = 1 })
end

--- A plot's ground: Material a tonal well with the style's corners;
--- Tsugumori a hairline box.
function H.well(style, w, h)
  if style.hatched then
    return ui.Rect { width = w, height = h, color = "transparent", border_width = 1,
      border_color = style.stroke_of and style.stroke_of("quiet") or style.line }
  end
  return ui.Rect { width = w, height = h, radius = math.min(12, style.radius(h / 4)), color = H.alpha(style.track, .7) }
end

--- What a value travels with: a spring in Material, the settle in Tsugumori.
function H.travel(style, k, d)
  if style.hatched then return { duration = style.motion.duration, easing = style.motion.easing } end
  return style.spring(k, d)
end

local function fmt_hz(f)
  if f >= 10000 then return ("%dk"):format(math.floor(f / 1000 + .5)) end
  if f >= 1000 then return (("%.1f"):format(f / 1000):gsub("%.0$", "")) .. "k" end
  return ("%d"):format(math.floor(f + .5))
end
H.fmt_hz = fmt_hz

local function signed(v, p) return (v >= 0 and "+" or "−") .. ("%." .. (p or 1) .. "f"):format(math.abs(v)) end
H.signed = signed

-- ------------------------------------------------------------ behaviours --

--- A draggable point over a plot box: a headless Plane (0..1 each way, up
--- positive) fed by the point's own handle. The handle grabs where it is
--- pressed (no jump), drags in surface coordinates, takes the arrows when
--- focused, and is a slider to a screen reader. `o`: `plot` (the node the
--- point's 0..1 places are of), `w`, `h`, `x()`, `y()` (0..1), `on_moved(x,
--- y)`, `name`, `value()` / `min` / `max` (what a screen reader is told),
--- `color`, `size`, `id`, `text` (a short label on or by the point),
--- `selected()` (drawn larger), `step_x`, `step_y`, and the handle's
--- `on_pressed`, `on_released`, `on_double_clicked`, `on_wheel`.
function H.point(style, o)
  local S = o.size or (style.hatched and 10 or 14)
  local HIT = math.max(S + 10, 24)
  local w, h = o.w, o.h
  local color = o.color or style.accent
  local ctl, grab
  local function local_of(sx, sy) return sx - (o.plot.layout_x or 0), sy - (o.plot.layout_y or 0) end
  local area
  area = ui.MouseArea { id = o.id, width = HIT, height = HIT, z = o.z or 5, cursor = "pointer",
    x = function() return o.x() * w - HIT / 2 end,
    y = function() return (1 - o.y()) * h - HIT / 2 end,
    focus_policy = "strong",
    accessible_role = "slider", accessible_name = o.name or "Point",
    accessible = function()
      return { value = o.value and o.value() or o.x(), minimum = o.min or 0, maximum = o.max or 1 }
    end,
    on_pressed = function(sx, sy)
      local lx, ly = local_of(sx, sy)
      grab = { o.x() * w - lx, (1 - o.y()) * h - ly }
      ctl.send("pressed", lx + grab[1], ly + grab[2], w, h)
      if o.on_pressed then o.on_pressed() end
    end,
    on_dragged = function(sx, sy)
      if not grab then return end
      local lx, ly = local_of(sx, sy)
      ctl.send("dragged", lx + grab[1], ly + grab[2], w, h)
    end,
    on_released = function()
      grab = nil
      ctl.send("released")
      if o.on_released then o.on_released() end
    end,
    on_key_pressed = function(_, text, modifiers, _, name) return ctl.key(name, modifiers, text) end,
    on_focus_changed = function(on) ctl.send("focus", on, area.visual_focus or false) end,
    on_entered = function() ctl.send("entered") end,
    on_exited = function() ctl.send("exited") end,
    on_double_clicked = o.on_double_clicked,
    on_wheel = o.on_wheel,
    on_destroyed = function() ctl.drop() end,
  }
  ctl = control.headless("Plane", { x_from = 0, x_to = 1, y_from = 0, y_to = 1, y_up = true,
    step_x = o.step_x, step_y = o.step_y, x = o.x, y = o.y, owner = area,
    on_moved = function(x, y) o.on_moved(x, y) end })
  local t = ctl.t
  local function lit() return t.down or t.hovered end
  local function big() return (o.selected and o.selected()) or t.down end
  local c = HIT / 2
  if not style.hatched then
    H.add(area,
      ui.Rect { x = c - (S + 8) / 2, y = c - (S + 8) / 2, width = S + 8, height = S + 8, radius = (S + 8) / 2,
        color = H.alpha(color, .22), opacity = function() return lit() and 1 or 0 end,
        scale = function() return lit() and 1 or .6 end,
        behavior = { opacity = { duration = 120 }, scale = style.spring(420, 20) } },
      ui.Rect { x = c - S / 2, y = c - S / 2, width = S, height = S, radius = S / 2, color = color,
        border_width = 2, border_color = style.raised,
        scale = function() return big() and 1.2 or 1 end, behavior = { scale = style.spring(520, 16) } },
      ui.Rect { x = 1, y = 1, width = HIT - 2, height = HIT - 2, radius = (HIT - 2) / 2, color = "transparent",
        border_width = 2, border_color = style.ink, visible = function() return t.visual_focus end })
    if o.text then
      H.add(area, H.txt(style, { text = o.text, x = c - S / 2, y = c - S / 2, width = S, height = S,
        horizontal_alignment = "center", vertical_alignment = "center", font_size = H.small(style), font_weight = 700,
        color = style.on_accent }))
    end
  else
    local tick = 3
    H.add(area,
      ui.Rect { x = c - S / 2, y = c - S / 2, width = S, height = S,
        color = function() return lit() and get(color):alpha(.45) or get(style.surface) end,
        border_width = 1, border_color = color },
      ui.Rect { x = c - .5, y = c - S / 2 - tick - 1, width = 1, height = tick, color = color },
      ui.Rect { x = c - .5, y = c + S / 2 + 1, width = 1, height = tick, color = color },
      ui.Rect { x = c - S / 2 - tick - 1, y = c - .5, width = tick, height = 1, color = color },
      ui.Rect { x = c + S / 2 + 1, y = c - .5, width = tick, height = 1, color = color },
      ui.Rect { x = c - S / 2 - 3, y = c - S / 2 - 3, width = S + 6, height = S + 6, color = "transparent",
        border_width = 1, border_color = style.ink, visible = function() return t.visual_focus or big() end })
    if o.text then
      H.add(area, H.txt(style, { text = o.text, x = c + S / 2 - 1, y = 0, width = 14, height = 12,
        font_size = H.small(style), font_family = style.mono_font, color = color }))
    end
  end
  return area, ctl
end

--- A Range drawn by the caller: a headless Range over a MouseArea. With
--- `relative`, a vertical drag turns it from where it is (a knob), 120 px
--- end to end; else the pointer maps onto the area's length less
--- `handle_size`, the handle grabbed where it is pressed (a fader). `o`:
--- `width`, `height`, `x`, `y`, `id`, `from`, `to`, `value` (a function),
--- `step`, `orientation`, `handle_size`, `relative`, `on_moved(v)`, `name`.
function H.range(style, o)
  local W, Hh = o.width, o.height
  local vertical = (o.orientation or "vertical") == "vertical"
  local L = 120
  local ctl, origin
  local area
  local function pos()
    local from, to = o.from or 0, o.to or 1
    return clamp01(((get(o.value) or from) - from) / ((to - from) ~= 0 and (to - from) or 1))
  end
  local hs = o.handle_size or 0
  local function local_of(sx, sy) return sx - (area.layout_x or 0), sy - (area.layout_y or 0) end
  area = ui.MouseArea { id = o.id, x = o.x, y = o.y, width = W, height = Hh, cursor = "pointer", focus_policy = "strong",
    accessible_role = "slider", accessible_name = o.name or "Level",
    accessible = function()
      return { value = get(o.value) or 0, minimum = math.min(o.from or 0, o.to or 1), maximum = math.max(o.from or 0, o.to or 1),
        orientation = vertical and "vertical" or "horizontal" }
    end,
    on_pressed = function(sx, sy)
      local lx, ly = local_of(sx, sy)
      if o.relative then
        origin = { sy, (1 - pos()) * L }
        ctl.send("pressed", 0, origin[2], 1, L)
        return
      end
      local length = vertical and Hh or W
      local travel = math.max(1, length - hs)
      local at = vertical and ly or lx
      local cap = hs / 2 + (vertical and (1 - pos()) or pos()) * travel
      local off = math.abs(at - cap) <= math.max(hs, 8) and (cap - at) or 0
      origin = { off }
      if vertical then ctl.send("pressed", lx, ly + off, W, Hh) else ctl.send("pressed", lx + off, ly, W, Hh) end
    end,
    on_dragged = function(sx, sy)
      if not origin then return end
      local lx, ly = local_of(sx, sy)
      if o.relative then
        ctl.send("dragged", 0, math.max(0, math.min(L, origin[2] + (sy - origin[1]))), 1, L)
      elseif vertical then ctl.send("dragged", lx, ly + origin[1], W, Hh)
      else ctl.send("dragged", lx + origin[1], ly, W, Hh) end
    end,
    on_released = function() origin = nil ctl.send("released") end,
    on_wheel = function(_, _, _, _, step_x, step_y) ctl.send("wheel", step_x or 0, step_y or 0) end,
    on_key_pressed = function(_, text, modifiers, _, name) return ctl.key(name, modifiers, text) end,
    on_focus_changed = function(on) ctl.send("focus", on, area.visual_focus or false) end,
    on_entered = function() ctl.send("entered") end,
    on_exited = function() ctl.send("exited") end,
    on_destroyed = function() ctl.drop() end,
  }
  ctl = control.headless("Range", { from = o.from or 0, to = o.to or 1, value = o.value, step = o.step,
    orientation = vertical and "vertical" or "horizontal", handle_size = (not o.relative) and hs or 0,
    owner = area, on_moved = function(v) if o.on_moved then o.on_moved(v) end end })
  return area, ctl, pos
end

--- A press drawn by the caller: the kit's Press as an `area` (the theme
--- draws only its keyboard ring and feedback), `children` under it.
function H.press(spec, children)
  spec.widget = "area"
  local node, t = control.make("Press", "area", spec, { children = children })
  return node, t
end

--- A knob: a Range turned by a vertical drag (and the arrows and wheel),
--- drawn as a 270 degree arc -- from the middle when `from` < 0 < `to`.
--- `o`: `size`, `x`, `y`, `id`, `from`, `to`, `value` (fn), `on_moved`,
--- `name`, `color`.
function H.knob(style, o)
  local s = o.size or 32
  local color = o.color or style.accent
  local area, ctl, pos = H.range(style, { id = o.id, x = o.x, y = o.y, width = s, height = s, from = o.from,
    to = o.to, value = o.value, relative = true, on_moved = o.on_moved, name = o.name, step = o.step })
  local cx = s / 2
  local r = s / 2 - (style.hatched and 2 or 3)
  local arc = morf.geometry.arc
  local bipolar = (o.from or 0) < 0 and (o.to or 1) > 0
  local mid = bipolar and clamp01((0 - (o.from or 0)) / ((o.to or 1) - (o.from or 0))) or 0
  local thick = style.hatched and 2 or 3
  local function path(p) return H.path(s, s, p) end
  if style.hatched then
    H.add(area, path { d = arc(cx, cx, r, -135, 270), stroke_color = style.stroke_of and style.stroke_of("idle") or style.line,
      stroke_width = 1 })
    H.add(area, path { d = morf.geometry.ticks(cx, cx, r - 3, r, { from = -135, sweep = 270, count = 10, major = 5, major_r0 = r - 5 }),
      stroke_color = style.stroke_of and style.stroke_of("quiet") or style.line, stroke_width = 1 })
  else
    H.add(area, ui.Rect { x = cx - r + thick + 1, y = cx - r + thick + 1, width = 2 * (r - thick - 1), height = 2 * (r - thick - 1),
      radius = r - thick - 1, color = style.track })
    H.add(area, path { d = arc(cx, cx, r, -135, 270), stroke_color = style.track, stroke_width = thick, stroke_cap = "round" })
  end
  -- The lit part: from the middle (or the start) to the value, a static
  -- arc trimmed.
  local cap = style.hatched and "butt" or "round"
  H.add(area, path { d = arc(cx, cx, r, -135 + 270 * mid, 270 * (1 - mid)), stroke_color = color, stroke_width = thick,
    stroke_cap = cap, trim_end = function() return clamp01((pos() - mid) / math.max(1e-6, 1 - mid)) end,
    visible = function() return pos() > mid + .001 end })
  if bipolar then
    H.add(area, path { d = arc(cx, cx, r, -135, 270 * mid), stroke_color = color, stroke_width = thick, stroke_cap = cap,
      trim_start = function() return clamp01(pos() / math.max(1e-6, mid)) end,
      visible = function() return pos() < mid - .001 end })
  end
  -- The pointer, turning about the middle.
  local nw = style.hatched and 1 or 3
  H.add(area, ui.Item { width = s, height = s, rotation = function() return -135 + 270 * pos() end,
    behavior = { rotation = H.travel(style, 520, 30) },
    ui.Rect { x = cx - nw / 2, y = style.hatched and 3 or thick + 3, width = nw, height = r * .55,
      radius = style.hatched and 0 or nw / 2, color = style.hatched and style.ink or color } })
  H.add(area, ui.Rect { width = s, height = s, radius = style.hatched and 0 or s / 2, color = "transparent",
    border_width = style.hatched and 1 or 2, border_color = style.ink, visible = function() return ctl.t.visual_focus end })
  return area, ctl, pos
end

--- Whether the style's ground is dark (what a piano's naturals contrast with).
function H.dark(style)
  local c = get(style.surface)
  if not c then return true end
  local r, g, b = c.r or 0, c.g or 0, c.b or 0
  if r > 1 or g > 1 or b > 1 then r, g, b = r / 255, g / 255, b / 255 end
  return (.3 * r + .59 * g + .11 * b) < .5
end

local function list_of(v)
  if channel.is(v) then return v:get() or {} end
  v = get(v)
  return type(v) == "table" and v or {}
end

-- ------------------------------------------------------- automation_lane --

--- An automation lane: a breakpoint curve of a value over time. Each point
--- drags (between its neighbours) and takes the arrows; a double click on
--- the lane adds a point there, on a point removes it. `spec`: `points`
--- (`{ {t, v}, ... }` or `{ {t=, v=} }`, t and v 0..1; or a function
--- returning them), `label` ("Automation"), `length` (seconds the lane
--- spans, for the reading), `format(v)` (the value as text), `position`
--- (0..1 or a function: the playhead), `width` (260), `height` (120),
--- `color`, `on_changed(points)`, `id` (handles are `<id>-point-<i>`).
function M.automation_lane(spec, style)
  local W, Hh = spec.width or 260, spec.height or 120
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Automation lane", W, Hh)
  local pad = H.pad(spec, style)
  local head = 18
  local pw, ph = W - 2 * pad, Hh - 2 * pad - head - 4
  local pts = {}
  local function load(list)
    pts = {}
    for _, p in ipairs(list or {}) do
      pts[#pts + 1] = { t = clamp01(p.t or p[1] or 0), v = clamp01(p.v or p[2] or 0) }
    end
    if #pts == 0 then pts = { { t = 0, v = .5 }, { t = 1, v = .5 } } end
    table.sort(pts, function(a, b) return a.t < b.t end)
  end
  load(get(spec.points) or { { 0, .3 }, { .3, .8 }, { .6, .45 }, { 1, .7 } })
  local rev = morf.signal(H.uid("lane"), 0)
  local touched = morf.signal(H.uid("lane.touched"), 0)
  local function bump() rev:set(rev:get() + 1) end
  local function changed()
    bump()
    if spec.on_changed then
      local out = {}
      for i, p in ipairs(pts) do out[i] = { t = p.t, v = p.v } end
      spec.on_changed(out)
    end
  end
  local format = spec.format or function(v) return ("%d%%"):format(math.floor(v * 100 + .5)) end
  local length = spec.length
  -- The header: the label, and the touched point's reading.
  H.add(root, H.cap(style, { text = spec.label or "Automation", x = pad, y = pad, width = pw * .5,
    font_size = style.size.small - 2, color = style.ink }))
  H.add(root, H.txt(style, { x = pad + pw * .5, y = pad - 1, width = pw * .5, height = head,
    horizontal_alignment = "right", font_size = style.size.small - 2, font_family = style.hatched and style.mono_font or nil,
    color = color, text = function()
      rev:get()
      local p = pts[touched:get()] or pts[1]
      if not p then return "" end
      local at = length and ("%.2fs"):format(p.t * length) or ("%d%%"):format(math.floor(p.t * 100 + .5))
      return at .. "  " .. format(p.v)
    end }))
  local plot = ui.Item { x = pad, y = pad + head + 4, width = pw, height = ph }
  H.add(root, plot)
  H.add(plot, H.well(style, pw, ph))
  H.add(plot, H.guides(style, pw, ph, style.hatched and { .125, .25, .375, .5, .625, .75, .875 } or { .25, .5, .75 },
    { .25, .5, .75 }))
  local function xy(p) return p.t * pw, (1 - p.v) * (ph - 2) + 1 end
  local function line_d()
    rev:get()
    local d = {}
    local x0, y0 = xy(pts[1])
    d[1] = ("M0 %.1f L%.1f %.1f"):format(y0, x0, y0)
    for i = 2, #pts do local x, y = xy(pts[i]) d[#d + 1] = ("L%.1f %.1f"):format(x, y) end
    local _, yl = xy(pts[#pts])
    d[#d + 1] = ("L%.1f %.1f"):format(pw, yl)
    return table.concat(d, " ")
  end
  local function area_d() return line_d() .. (" L%.1f %.1f L0 %.1f Z"):format(pw, ph, ph) end
  -- (Rounded wells keep the fill inside their corners.)
  local inside = ui.Item { width = pw, height = ph, clip = true }
  H.add(plot, inside)
  H.add(inside, H.shape(style, pw, ph, color, area_d, { alpha = .2 }))
  H.add(inside, H.path(pw, ph, { d = line_d, stroke_color = color, stroke_width = style.hatched and 1.5 or 2,
    stroke_join = style.hatched and "miter" or "round", stroke_cap = "round" }))
  if spec.position ~= nil then
    H.add(inside, ui.Rect { width = style.hatched and 1 or 2, height = ph, color = style.ink,
      x = function() return clamp01(get(spec.position)) * (pw - 2) end })
  end
  -- A double click on the lane adds a point there.
  local handles = {}
  local rebuild
  H.add(plot, ui.MouseArea { width = pw, height = ph, z = 1,
    on_double_clicked = function(_, _, lx, ly)
      local p = { t = clamp01(lx / pw), v = clamp01(1 - ly / ph) }
      local at = #pts + 1
      for i, q in ipairs(pts) do if q.t > p.t then at = i break end end
      table.insert(pts, at, p)
      touched:set(at)
      rebuild()
      changed()
    end })
  function rebuild()
    for _, node in ipairs(handles) do ui.destroy(node, true) end
    handles = {}
    for i = 1, #pts do
      local node = H.point(style, { plot = plot, w = pw, h = ph, id = spec.id and (spec.id .. "-point-" .. i) or nil,
        name = ("Point %d"):format(i), color = color, step_x = .01, step_y = .01,
        x = function() rev:get() return pts[i] and pts[i].t or 0 end,
        y = function() rev:get() return pts[i] and pts[i].v or 0 end,
        value = function() rev:get() return pts[i] and pts[i].v or 0 end,
        selected = function() return touched:get() == i end,
        on_pressed = function() touched:set(i) end,
        on_moved = function(x, y)
          local p = pts[i]
          if not p then return end
          local lo = i > 1 and pts[i - 1].t + .005 or 0
          local hi = i < #pts and pts[i + 1].t - .005 or 1
          p.t, p.v = math.max(lo, math.min(hi, x)), clamp01(y)
          touched:set(i)
          changed()
        end,
        on_double_clicked = function()
          if #pts <= 2 then return end
          -- After the click is delivered: the handle goes with the rebuild.
          morf.timer(0, function()
            table.remove(pts, i)
            touched:set(math.min(i, #pts))
            rebuild()
            changed()
          end, false)
        end })
      handles[i] = node
      ui.reparent(node, plot)
    end
  end
  rebuild()
  if type(spec.points) == "function" then
    morf.effect(H.uid("lane.points"), function()
      local list = spec.points()
      if list and #list ~= #pts then load(list) rebuild() bump() end
    end, { owner = root })
  end
  return root
end

-- ------------------------------------------------------ audio_visualiser --

--- A circular visualiser: a ring per band of a frame channel, each swept
--- round by its level (`plot` kind "radial"), mirrored half and half --
--- drawn where it is painted, no Lua per frame. `spec`: `channel` (a frame
--- of levels 0..1) or `values` (a list or a function of one), `size`
--- (min of width and height), `width`, `height`, `label`, `color`.
function M.audio_visualiser(spec, style)
  local W = spec.width or spec.size or 260
  local Hh = spec.height or spec.size or 190
  local root = H.frame(spec, style, "Audio visualiser", W, Hh)
  local pad = H.pad(spec, style)
  local s = math.min(W, Hh) - 2 * pad
  local box = ui.Item { x = (W - s) / 2, y = (Hh - s) / 2, width = s, height = s }
  H.add(root, box)
  local ch, start = channel.from(spec.channel or spec.values or spec.data or {})
  local n = math.max(1, spec.bands or #list_of(spec.channel or spec.values or spec.data))
  local inner = spec.inner or .3
  local gap = style.hatched and 2 or 3
  local band = (s / 2 * (1 - inner) - gap * (n - 1)) / n
  local function plot(arcs)
    return { kind = "radial", width = s, height = s, inner = inner, gap = gap, sweep = 180, start = 0,
      bottom = 0, top = 1, arcs = arcs or nil }
  end
  local guide = H.path(s, s, { d = morf.geometry.arc(s / 2, s / 2, s / 2 - 1, 0, 360),
    stroke_color = style.hatched and (style.stroke_of and style.stroke_of("quiet")) or H.alpha(style.line, .5), stroke_width = 1 })
  H.add(box, guide)
  if style.hatched then
    H.add(box, H.path(s, s, { d = morf.geometry.ticks(s / 2, s / 2, s / 2 - 5, s / 2 - 1, { from = 0, sweep = 360, count = 36, major = 9,
      major_r0 = s / 2 - 9 }), stroke_color = style.stroke_of and style.stroke_of("idle") or style.line, stroke_width = 1 }))
  end
  -- Each band's track: a static ring through its middle.
  local tracks = {}
  for i = 1, n do
    local r = s / 2 * inner + (i - 1) * (band + gap) + band / 2
    tracks[#tracks + 1] = morf.geometry.arc(s / 2, s / 2, r, 0, 360)
  end
  H.add(box, H.path(s, s, { d = table.concat(tracks, " "), stroke_width = style.hatched and 1 or math.max(1.5, band),
    stroke_color = style.hatched and (style.stroke_of and style.stroke_of("quiet") or style.line) or H.alpha(style.track, .8) }))
  -- Two halves: the second turned half round, so the rings grow both ways.
  local color = H.color(spec, style)
  for k = 0, 1 do
    local half = ui.Item { width = s, height = s, rotation = k * 180 }
    H.add(box, half)
    local tone = k == 0 and color or style.series(2)
    if style.hatched then
      H.add(half, ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, series = ch.id, plot = plot(false),
        fill_color = H.alpha(tone, .3), stroke_color = tone, stroke_width = 1 })
    else
      H.add(half, ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, series = ch.id, plot = plot(true),
        fill_color = "transparent", stroke_color = tone, stroke_width = math.max(1.5, band), stroke_cap = "round" })
    end
  end
  -- The hub.
  local hub = s * inner * .62
  if style.hatched then
    H.add(box, ui.Rect { x = s / 2 - hub / 2, y = s / 2 - hub / 2, width = hub, height = hub, color = "transparent",
      border_width = 1, border_color = style.ink })
  else
    H.add(box, ui.Rect { x = s / 2 - hub / 2, y = s / 2 - hub / 2, width = hub, height = hub, radius = hub / 2,
      color = H.alpha(H.color(spec, style), .9) })
  end
  if spec.label then
    H.add(root, H.cap(style, { text = spec.label, x = pad, y = pad - 2, width = (W - s) / 2 - pad > 40 and (W - s) / 2 - pad or 60,
      color = style.ink_lo }))
  end
  start(root)
  return root
end

-- ------------------------------------------------------ compressor_curve --

--- A compressor's transfer curve: output level against input, the line
--- bending at `threshold` by `ratio` over a soft `knee`. Two handles: the
--- threshold (on the unity line, dragged across) and the ratio (at 0 dB in,
--- dragged up and down). `spec`: `threshold` (dB, -18), `ratio` (4),
--- `knee` (dB, 6), `floor` (dB, -60), `width` (260), `height` (190),
--- `label`, `color`, `on_changed(threshold, ratio)`, `id` (handles
--- `<id>-threshold`, `<id>-ratio`).
function M.compressor_curve(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Compressor curve", W, Hh)
  local pad = H.pad(spec, style)
  local FLOOR = spec.floor or -60
  local st = morf.state { threshold = get(spec.threshold) or -18, ratio = get(spec.ratio) or 4 }
  local knee = get(spec.knee) or 6
  local side = W - 2 * pad >= Hh - 2 * pad + 70
  local s = side and (Hh - 2 * pad) or math.min(W - 2 * pad, Hh - 2 * pad - 22)
  local plot = ui.Item { x = pad, y = side and pad or pad + 22, width = s, height = s }
  H.add(root, plot)
  H.add(plot, H.well(style, s, s))
  H.add(plot, H.guides(style, s, s, { .25, .5, .75 }, { .25, .5, .75 }))
  local function out(x)
    local T, R = st.threshold, st.ratio
    if 2 * (x - T) < -knee then return x end
    if knee > 0 and 2 * math.abs(x - T) <= knee then return x + (1 / R - 1) * (x - T + knee / 2) ^ 2 / (2 * knee) end
    return T + (x - T) / R
  end
  local function n(db) return clamp01((db - FLOOR) / -FLOOR) end
  local function curve_d()
    local d = {}
    for k = 0, 48 do
      local x = FLOOR + (-FLOOR) * k / 48
      d[#d + 1] = ("%s%.1f %.1f"):format(k == 0 and "M" or "L", n(x) * s, (1 - n(out(x))) * s)
    end
    return table.concat(d, " ")
  end
  -- Unity, for reference.
  H.add(plot, H.path(s, s, { d = ("M0 %g L%g 0"):format(s, s), stroke_color = H.alpha(style.ink_lo, .5), stroke_width = 1,
    dash = { 3, 3 } }))
  local inside = ui.Item { width = s, height = s, clip = true }
  H.add(plot, inside)
  H.add(inside, H.shape(style, s, s, color, function() return curve_d() .. (" L%g %g L0 %g Z"):format(s, s, s) end, { alpha = .16 }))
  H.add(inside, H.path(s, s, { d = curve_d, stroke_color = color, stroke_width = style.hatched and 1.5 or 2.5,
    stroke_cap = "round", stroke_join = "round" }))
  -- The threshold's line across.
  H.add(inside, ui.Rect { x = function() return n(st.threshold) * s end, width = 1, height = s,
    color = H.alpha(style.warn, .7), behavior = { x = { duration = 60 } } })
  local function changed() if spec.on_changed then spec.on_changed(st.threshold, st.ratio) end end
  H.add(plot, (H.point(style, { plot = plot, w = s, h = s, id = spec.id and (spec.id .. "-threshold") or nil,
    name = "Threshold", color = style.warn, step_x = 1 / -FLOOR,
    x = function() return n(st.threshold) end, y = function() return n(st.threshold) end,
    value = function() return st.threshold end, min = FLOOR, max = 0,
    on_moved = function(x)
      st.threshold = math.max(FLOOR + 6, math.min(-1, math.floor((FLOOR + x * -FLOOR) * 2 + .5) / 2))
      changed()
    end })))
  H.add(plot, (H.point(style, { plot = plot, w = s, h = s, id = spec.id and (spec.id .. "-ratio") or nil,
    name = "Ratio", color = color, step_y = .5 / -FLOOR,
    x = function() return 1 end, y = function() return n(out(0)) end,
    value = function() return st.ratio end, min = 1, max = 30,
    on_moved = function(_, y)
      local T = st.threshold
      local db = math.max(T + .05, math.min(0, FLOOR + y * -FLOOR))
      st.ratio = math.max(1, math.min(30, math.floor((0 - T) / (db - T) * 10 + .5) / 10))
      changed()
    end })))
  -- The readings.
  local rows = {
    { "Thresh", function() return ("%s dB"):format(signed(st.threshold, 1)) end, style.warn },
    { "Ratio", function() return st.ratio >= 30 and "∞:1" or ("%.1f:1"):format(st.ratio) end, color },
    { "Knee", function() return ("%d dB"):format(knee) end, style.ink },
  }
  if side then
    local x0 = pad + s + 12
    local cw = W - pad - x0
    for i, r in ipairs(rows) do
      local y = pad + (i - 1) * 40
      H.add(root, H.cap(style, { text = r[1], x = x0, y = y, width = cw }))
      H.add(root, H.txt(style, { text = r[2], x = x0, y = y + 15, width = cw, height = 20, font_size = style.size.small - 1,
        font_weight = 600, color = r[3], font_family = style.hatched and style.mono_font or nil }))
    end
  else
    local cw = (W - 2 * pad) / 2
    for i = 1, 2 do
      local r = rows[i]
      H.add(root, H.txt(style, { text = function() return (style.hatched and r[1]:upper() or r[1]) .. " " .. r[2]() end,
        x = pad + (i - 1) * cw, y = pad, width = cw, height = 18, elide = "right", font_size = H.small(style) + 1,
        color = r[3], font_family = style.hatched and style.mono_font or nil }))
    end
  end
  return root
end

-- --------------------------------------------------------------- eq_bars --

--- Now-playing bars: a few bars bouncing. With a `channel` (or `values`)
--- the bars are its levels (`plot` kind "bars"); without, they are static
--- bars sliding inside their clips on loops of their own -- motion the
--- engine runs, paused when `playing` is false. `spec`: `bars` (4),
--- `width` (40), `height` (32), `playing` (true, or a function), `color`,
--- `label`.
function M.eq_bars(spec, style)
  local W, Hh = spec.width or 40, spec.height or 32
  local color = H.color(spec, style)
  local nbars = spec.bars or 4
  local root = ui.Item(U.place(spec, { width = W, height = Hh, accessible_role = "figure",
    accessible_name = type(spec.label) == "string" and spec.label or "Now playing" }))
  local gap = spec.gap or math.max(2, math.floor(W / (nbars * 4)))
  local bw = (W - gap * (nbars - 1)) / nbars
  local src = spec.channel or spec.values or spec.data
  if src ~= nil then
    local ch, start = channel.from(src)
    local plot = { kind = "bars", width = W, height = Hh, gap = gap, radius = style.hatched and 0 or bw / 2,
      min_bar = style.hatched and 2 or bw, bottom = 0, top = 1, pad_top = 0, pad_bottom = 0, mirror = spec.mirror or nil }
    if style.hatched then
      H.add(root, ui.Path { width = W, height = Hh, view_box = { 0, 0, W, Hh }, series = ch.id, plot = plot,
        fill_color = H.alpha(color, .25), stroke_color = color, stroke_width = 1 })
    else
      H.add(root, ui.Path { width = W, height = Hh, view_box = { 0, 0, W, Hh }, series = ch.id, plot = plot, fill_color = color })
    end
    start(root)
    return root
  end
  local function playing() local p = get(spec.playing) return p == nil or p == true end
  local PERIODS = { 620, 480, 700, 540, 660, 500, 580, 720 }
  local LOWS = { .35, .55, .25, .45, .3, .5, .4, .2 }
  for i = 1, nbars do
    local x = (i - 1) * (bw + gap)
    local clip = ui.Item { x = x, width = bw, height = Hh, clip = true }
    local low = LOWS[(i - 1) % #LOWS + 1]
    -- The bar slides down out of its clip and back; resting low when paused.
    local props = { width = bw, translate_y = Hh * (1 - low),
      loop = function()
        if not playing() then return nil end
        return { translate_y = { from = Hh * (1 - low), to = Hh * .08, duration = PERIODS[(i - 1) % #PERIODS + 1],
          easing = "in_out_sine", alternate = true } }
      end }
    local bar
    if style.hatched then
      props.height = Hh
      props[1] = ui.Rect { width = bw, height = Hh, color = H.alpha(color, .2), border_width = 1, border_color = color }
      if bw >= 6 then props[2] = style.stripes.box { width = bw, height = Hh, gap = 4, weight = 1, color = H.alpha(color, .6) } end
      bar = ui.Item(props)
    else
      props.height, props.radius, props.color = Hh + bw, bw / 2, color
      bar = ui.Rect(props)
    end
    H.add(clip, bar)
    H.add(root, clip)
  end
  return root
end

-- ------------------------------------------------------------- lissajous --

--- An XY scope: the left channel across, the right up, from an interleaved
--- `x, y` channel (`plot` kind "scatter") -- a phase picture of a stereo
--- signal. `spec`: `channel` (frame of x,y pairs, -1..1) or `values` (a
--- flat list or `{ {x, y} }`, or a function of one), `size`, `width`,
--- `height`, `label`, `color`, `point` (dot radius).
function M.lissajous(spec, style)
  local W = spec.width or spec.size or 260
  local Hh = spec.height or spec.size or 190
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Lissajous scope", W, Hh)
  local pad = H.pad(spec, style)
  local s = math.min(W, Hh) - 2 * pad
  local box = ui.Item { x = (W - s) / 2, y = (Hh - s) / 2, width = s, height = s, clip = true }
  H.add(root, box)
  H.add(box, H.well(style, s, s))
  H.add(box, H.guides(style, s, s, { .5 }, { .5 }, true))
  H.add(box, H.guides(style, s, s, { .25, .75 }, { .25, .75 }))
  if style.hatched then
    -- The diagonals: mono (L = R) and out of phase (L = -R).
    H.add(box, H.path(s, s, { d = ("M0 %g L%g 0 M0 0 L%g %g"):format(s, s, s, s), stroke_color = H.alpha(style.ink_lo, .35),
      stroke_width = 1, dash = { 2, 4 } }))
  end
  local src = spec.channel or spec.values or spec.data or {}
  local ch, start
  if channel.is(src) then ch, start = src, function() end
  else
    ch, start = channel.from(function()
      local v = list_of(src)
      if type(v[1]) ~= "table" then return v end
      local out = {}
      for _, p in ipairs(v) do out[#out + 1] = p[1] or 0 out[#out + 1] = p[2] or 0 end
      return out
    end)
  end
  local function plot(point)
    return { kind = "scatter", width = s, height = s, left = -1, right = 1, bottom = -1, top = 1, point = point }
  end
  local point = spec.point or (style.hatched and 1 or 1.4)
  if not style.hatched then
    -- A soft glow under the trace.
    H.add(box, ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, series = ch.id, plot = plot(point * 2.6),
      fill_color = H.alpha(color, .14) })
  end
  H.add(box, ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, series = ch.id, plot = plot(point),
    fill_color = color })
  local lw = (W - s) / 2 - pad
  if lw >= 16 then
    H.add(root, H.cap(style, { text = "L", x = pad, y = Hh / 2 - 8, width = lw }))
    H.add(root, H.cap(style, { text = "R", x = W - pad - lw, y = Hh / 2 - 8, width = lw, horizontal_alignment = "right" }))
    if spec.label and lw >= 40 then H.add(root, H.cap(style, { text = spec.label, x = pad, y = pad - 2, width = lw, color = style.ink })) end
  end
  start(root)
  return root
end

-- ----------------------------------------------------------- mixer_strip --

--- A mixer channel strip: a pan knob, a fader (a vertical Range) beside a
--- level meter, the gain reading, and mute and solo (checkable Presses).
--- `spec`: `label` ("Ch 1"), `volume` (0..1 fader position, or a
--- function), `pan` (-1..1), `mute`, `solo` (booleans or functions),
--- `level` (0..1, a function, or a channel of levels: its newest drawn
--- with no Lua per frame), `width` (84), `height` (190), `color`,
--- `on_volume(v)`, `on_pan(v)`, `on_mute(on)`, `on_solo(on)`, `id` (parts
--- `<id>-fader`, `<id>-pan`, `<id>-mute`, `<id>-solo`).
function M.mixer_strip(spec, style)
  local W, Hh = spec.width or 84, spec.height or 190
  local color = H.color(spec, style)
  local label = spec.label or "Ch 1"
  local root = H.frame({ id = spec.id and (spec.id .. "-strip") or nil, x = spec.x, y = spec.y, anchors = spec.anchors,
    label = label, bare = spec.bare }, style, label, W, Hh, "group")
  local pad = style.hatched and 8 or 8
  local st = morf.state { volume = get(spec.volume) or .75, pan = get(spec.pan) or 0,
    mute = get(spec.mute) == true, solo = get(spec.solo) == true }
  for _, key in ipairs { "volume", "pan", "mute", "solo" } do
    if type(spec[key]) == "function" then
      morf.effect(H.uid("strip." .. key), function() local v = spec[key]() if v ~= nil then st[key] = v end end, { owner = root })
    end
  end
  local function id(part) return spec.id and (spec.id .. "-" .. part) or nil end
  local function db(p) if p < .02 then return "−∞" end return signed((p - .75) * 48, 1) end
  -- The name.
  H.add(root, H.cap(style, { text = label, x = pad, y = pad - 2, width = W - 2 * pad, horizontal_alignment = "center",
    color = style.ink, font_size = style.size.small - 2 }))
  -- The pan knob and its reading.
  local ks = 28
  local top = pad + 18
  local knob = H.knob(style, { id = id("pan"), x = pad, y = top, size = ks, from = -1, to = 1, step = .02,
    value = function() return st.pan end, name = label .. " pan",
    on_moved = function(v) st.pan = v if spec.on_pan then spec.on_pan(v) end end })
  H.add(root, knob)
  H.add(root, H.txt(style, { x = pad + ks + 2, y = top + ks / 2 - 9, width = W - 2 * pad - ks - 2, height = 18,
    horizontal_alignment = "right", font_size = H.small(style), color = style.ink_lo,
    font_family = style.hatched and style.mono_font or nil, text = function()
      local p = st.pan
      if math.abs(p) < .02 then return "C" end
      return (p < 0 and "L" or "R") .. math.floor(math.abs(p) * 100 + .5)
    end }))
  -- Mute and solo at the foot, the reading above them.
  local bh = 22
  local by = Hh - pad - bh
  local ry = by - 20
  local fy = top + ks + 8
  local fh = ry - 4 - fy
  local function button(part, text, tone, key, handler)
    local bw = (W - 2 * pad - 6) / 2
    local x = part == "mute" and pad or pad + bw + 6
    local function on() return st[key] end
    local look
    if style.hatched then
      look = ui.Item { width = bw, height = bh,
        ui.Rect { width = bw, height = bh, color = function() return on() and get(tone):alpha(.22) or get(tone):alpha(0) end,
          border_width = 1, border_color = function() return on() and get(tone) or get(style.line) end } }
      if bw >= 6 then
        H.add(look, style.stripes.box { width = bw, height = bh, gap = 4, weight = 1, color = H.alpha(tone, .6),
          opacity = function() return on() and 1 or 0 end })
      end
      H.add(look, H.txt(style, { text = text, width = bw, height = bh, horizontal_alignment = "center", vertical_alignment = "center",
        font_size = H.small(style) + 1, font_family = style.mono_font, font_weight = 700,
        color = function() return on() and get(tone) or get(style.ink_lo) end }))
    else
      -- Material: a pill that morphs to a rounded square, filled, when lit.
      look = ui.Item { width = bw, height = bh,
        ui.Rect { width = bw, height = bh, radius = function() return on() and 6 or bh / 2 end,
          color = function() return on() and get(tone) or get(style.track) end,
          behavior = { radius = style.spring(380, 18), color = { duration = 160 } } },
        H.txt(style, { text = text, width = bw, height = bh, horizontal_alignment = "center", vertical_alignment = "center",
          font_size = H.small(style) + 1, font_weight = 700,
          color = function() return on() and get(style.on_accent) or get(style.ink) end }) }
    end
    local node = H.press({ id = id(part), x = x, y = by, width = bw, height = bh, checkable = true,
      checked = on, accessible_name = label .. " " .. part,
      on_toggled = function(v) st[key] = v if handler then handler(v) end end }, { look })
    return node
  end
  H.add(root, button("mute", "M", style.alert, "mute", spec.on_mute), button("solo", "S", style.warn, "solo", spec.on_solo))
  H.add(root, H.txt(style, { x = pad, y = ry, width = W - 2 * pad, height = 18, horizontal_alignment = "center",
    font_size = style.size.small - 1, font_weight = 600, font_family = style.hatched and style.mono_font or nil,
    color = function() return st.mute and get(style.ink_lo) or get(style.ink) end,
    text = function() return db(st.volume) .. (style.hatched and " DB" or " dB") end }))
  -- The fader (left of the middle) and the meter (right).
  local CAP = style.hatched and 10 or 14
  local fw = math.floor((W - 2 * pad) * .5)
  local fx = pad
  local fader, fctl, fpos = H.range(style, { id = id("fader"), x = fx, y = fy, width = fw, height = fh, from = 0, to = 1,
    step = .01, handle_size = CAP, value = function() return st.volume end, name = label .. " volume",
    on_moved = function(v) st.volume = v if spec.on_volume then spec.on_volume(v) end end })
  local travel = fh - CAP
  local function cap_y() return (1 - fpos()) * travel end
  local tx = fw / 2
  if style.hatched then
    H.add(fader, ui.Rect { x = tx - .5, y = CAP / 2, width = 1, height = travel, color = style.line })
    H.add(fader, H.path(fw, fh, { d = morf.geometry.ruler(travel, 5, { pitch = travel / 12, major = 3, vertical = true }),
      x = 2, y = CAP / 2, stroke_color = style.stroke_of and style.stroke_of("quiet") or style.line, stroke_width = 1 }))
    -- 0 dB mark.
    H.add(fader, ui.Rect { x = 1, y = CAP / 2 + .25 * travel, width = 7, height = 1, color = style.ink })
    H.add(fader, ui.Item { x = 2, y = cap_y, width = fw - 4, height = CAP, behavior = { y = { duration = 60 } },
      ui.Rect { width = fw - 4, height = CAP, color = style.surface, border_width = 1,
        border_color = function() return fctl.t.visual_focus and get(color) or get(style.ink) end },
      ui.Rect { y = CAP / 2 - .5, width = fw - 4, height = 1, color = color } })
  else
    local tw = 6
    H.add(fader, ui.Rect { x = tx - tw / 2, y = CAP / 2, width = tw, height = travel, radius = tw / 2, color = style.track })
    H.add(fader, ui.Rect { x = tx - tw / 2, width = tw, radius = tw / 2, color = color,
      y = function() return cap_y() + CAP / 2 end, height = function() return travel - cap_y() end,
      behavior = { y = style.spring(600, 40), height = style.spring(600, 40) } })
    H.add(fader, ui.Rect { x = 1, y = CAP / 2 + .25 * travel, width = 6, height = 2, radius = 1, color = style.ink_lo })
    H.add(fader, ui.Rect { x = 2, y = cap_y, width = fw - 4, height = CAP, radius = 5,
      color = function() return fctl.t.down and get(color) or get(style.ink) end,
      border_width = function() return fctl.t.visual_focus and 2 or 0 end, border_color = color,
      scale = function() return fctl.t.down and 1.08 or 1 end,
      behavior = { y = style.spring(600, 40), scale = style.spring(500, 18), color = { duration = 120 } },
      ui.Rect { x = 6, y = CAP / 2 - 1, width = fw - 16, height = 2, radius = 1, color = style.raised } })
  end
  H.add(root, fader)
  -- The meter.
  local mw = style.hatched and 8 or 8
  local mx = W - pad - math.floor((W - 2 * pad - fw) / 2) - mw / 2
  local meter = ui.Item { x = mx, y = fy + CAP / 2, width = mw, height = travel, clip = true,
    accessible_role = "meter", accessible_name = label .. " level" }
  H.add(root, meter)
  local mh = travel
  local src = spec.level
  local function grad() return { angle = 0, stops = { { get(style.ok), 0 }, { get(style.ok), .6 }, { get(style.warn), .8 }, { get(style.alert), 1 } } } end
  if style.hatched then
    H.add(meter, ui.Rect { width = mw, height = mh, color = "transparent", border_width = 1, border_color = style.line })
  else
    H.add(meter, ui.Rect { width = mw, height = mh, radius = mw / 2, color = style.track })
  end
  if channel.is(src) then
    local plot = { kind = "bars", width = mw, height = mh, samples = 1, gap = 0, bottom = 0, top = 1,
      radius = style.hatched and 0 or mw / 2, min_bar = 0, pad_top = 0, pad_bottom = 0 }
    H.add(meter, ui.Rect { width = mw, height = mh, gradient = grad,
      mask = ui.Path { width = mw, height = mh, view_box = { 0, 0, mw, mh }, series = src.id, plot = plot, fill_color = "#ffffff" } })
  else
    local function lvl() return st.mute and 0 or clamp01(get(src) or 0) end
    local fill = ui.Item { width = mw, clip = true, y = function() return (1 - lvl()) * mh end,
      height = function() return lvl() * mh end, behavior = { y = { duration = 90 }, height = { duration = 90 } } }
    H.add(fill, ui.Rect { width = mw, height = mh, radius = style.hatched and 0 or mw / 2, gradient = grad,
      y = function() return -(1 - lvl()) * mh end, behavior = { y = { duration = 90 } } })
    H.add(meter, fill)
  end
  if style.hatched then
    -- Segments: the surface's gaps across the run.
    local d = {}
    for y = mh - 4, 1, -4 do d[#d + 1] = ("M0 %d.5 H%d"):format(y, mw) end
    H.add(meter, H.path(mw, mh, { d = table.concat(d, " "), stroke_color = style.surface, stroke_width = 1 }))
  end
  return root
end

-- -------------------------------------------------------- piano_keyboard --

local NAMES = { "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B" }
local BLACK = { [1] = true, [3] = true, [6] = true, [8] = true, [10] = true }
H.NOTE_NAMES, H.BLACK = NAMES, BLACK
function H.note_name(n) return NAMES[n % 12 + 1] .. (math.floor(n / 12) - 1) end

--- A piano keyboard: a Press per key, naturals and sharps laid out as on
--- a piano, a held key lit. `spec`: `octaves` (2), `start` (MIDI note of
--- the first C, 48), `notes` (a function returning the notes held from
--- elsewhere, a list of MIDI numbers), `on_note_on(n, velocity)`,
--- `on_note_off(n)`, `keyboard` (true: the computer keys A W S E D F T G Y
--- H U J K play an octave from `start` while it has focus; Z and X shift
--- it), `labels` (true: the C's named), `width` (260), `height` (110),
--- `color`, `id` (keys `<id>-key-<n>`).
function M.piano_keyboard(spec, style)
  local W, Hh = spec.width or 260, spec.height or 110
  local color = H.color(spec, style)
  local oct = spec.octaves or 2
  local first = spec.start or 48
  local whites = {}
  for n = first, first + 12 * oct do if not BLACK[n % 12] then whites[#whites + 1] = n end end
  local ww = W / #whites
  local bw, bh = ww * .62, Hh * .6
  -- The notes held here, as a list (what a signal holds is an array).
  local held = morf.signal(H.uid("piano.held"), {})
  local function is_held(n) for _, v in ipairs(held:get()) do if v == n then return true end end return false end
  local function set(n, on)
    local next = {}
    for _, v in ipairs(held:get()) do if v ~= n then next[#next + 1] = v end end
    if on then next[#next + 1] = n end
    held:set(next)
  end
  local function down(n)
    if is_held(n) then return end
    set(n, true)
    if spec.on_note_on then spec.on_note_on(n, 100) end
  end
  local function up(n)
    if not is_held(n) then return end
    set(n, false)
    if spec.on_note_off then spec.on_note_off(n) end
  end
  local function lit(n)
    return function()
      if is_held(n) then return true end
      local ext = get(spec.notes)
      if type(ext) == "table" then for _, v in ipairs(ext) do if v == n then return true end end end
      return false
    end
  end
  local dark = H.dark(style)
  local natural = dark and style.ink or style.raised
  local sharp = (dark and not style.hatched) and style.surface or style.ink
  local root
  local shift = 0
  local KEYS = { a = 0, w = 1, s = 2, e = 3, d = 4, f = 5, t = 6, g = 7, y = 8, h = 9, u = 10, j = 11, k = 12 }
  local held_keys = {}
  local props = U.place(spec, { width = W, height = Hh, accessible_role = "group",
    accessible_name = type(spec.label) == "string" and spec.label or "Piano keyboard" })
  if spec.keyboard then
    props.focus_policy = "strong"
    props.on_key_pressed = function(_, text, _, repeat_)
      local ch = (text or ""):lower()
      if ch == "z" then shift = math.max(-2, shift - 1) return true end
      if ch == "x" then shift = math.min(2, shift + 1) return true end
      local k = KEYS[ch]
      if k == nil then return false end
      if repeat_ then return true end
      local n = first + 12 * shift + k
      held_keys[ch] = n
      down(n)
      return true
    end
    props.on_key_released = function(_, text)
      local ch = (text or ""):lower()
      local n = held_keys[ch]
      if n then held_keys[ch] = nil up(n) return true end
      return false
    end
    root = ui.MouseArea(props)
  else
    root = ui.Item(props)
  end
  local function key(n, x, w, h, black)
    local on = lit(n)
    local look
    if style.hatched then
      look = ui.Item { width = w, height = h,
        ui.Rect { width = w, height = h, color = function()
          if on() then return get(color):alpha(black and .9 or .3) end
          return black and get(sharp) or get(style.surface)
        end, border_width = 1, border_color = function() return on() and get(color) or get(style.line) end } }
      if not black and w >= 6 then
        H.add(look, style.stripes.box { y = h * .55, width = w, height = h * .45, gap = 4, weight = 1, color = H.alpha(color, .7),
          opacity = function() return on() and 1 or 0 end })
      end
    else
      local r = math.min(6, w / 3)
      look = ui.Item { width = w, height = h,
        ui.Rect { x = black and 0 or 1, width = black and w or w - 2, height = h,
          bottom_left_radius = r, bottom_right_radius = r, top_left_radius = black and 0 or 2, top_right_radius = black and 0 or 2,
          color = function() if on() then return get(color) end return black and get(sharp) or get(natural) end,
          translate_y = function() return on() and 2 or 0 end,
          behavior = { color = { duration = 90 }, translate_y = style.spring(700, 22) } } }
    end
    if not black and spec.labels ~= false and n % 12 == 0 and h >= 40 then
      H.add(look, H.txt(style, { text = H.note_name(n), y = h - 18, width = w, height = 16, horizontal_alignment = "center",
        font_size = H.small(style), font_family = style.hatched and style.mono_font or nil,
        color = function()
          if on() then return style.hatched and get(color) or get(style.on_accent) end
          return style.hatched and get(style.ink_lo) or get(dark and style.surface or style.ink_lo)
        end }))
    end
    local pointer = false
    local node = H.press({ id = spec.id and (spec.id .. "-key-" .. n) or nil, x = x, y = 0, width = w, height = h,
      z = black and 2 or 1, accessible_name = H.note_name(n),
      on_pressed = function() pointer = true down(n) end,
      on_released = function() up(n) end,
      -- A press from the keys or a screen reader: a short note.
      on_clicked = function()
        if pointer then pointer = false return end
        down(n)
        morf.timer(180, function() up(n) end, false)
      end }, { look })
    ui.reparent(node, root)
  end
  for i, n in ipairs(whites) do key(n, (i - 1) * ww, ww, Hh, false) end
  for i, n in ipairs(whites) do
    if BLACK[(n + 1) % 12] and n + 1 <= first + 12 * oct then key(n + 1, i * ww - bw / 2, bw, bh, true) end
  end
  return root
end

-- -------------------------------------------------------- parametric_eq --

--- A parametric EQ: the summed frequency response over 20 Hz..20 kHz (a
--- log scale) and a handle per band -- a Plane: frequency across, gain up
--- -- that drags and takes the arrows, the wheel over it its Q; and the
--- band list, each band a Press that picks it. `spec`: `bands` (`{ {freq=,
--- gain=, q=} }`), `range` (dB either way, 15), `width` (260), `height`
--- (190), `label`, `on_changed(i, band)`, `id` (handles `<id>-band-<i>`,
--- chips `<id>-chip-<i>`).
function M.parametric_eq(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local root = H.frame(spec, style, "Parametric equaliser", W, Hh)
  local pad = H.pad(spec, style)
  local R = spec.range or 15
  local LO, HI = math.log(20, 10), math.log(20000, 10)
  local bands = {}
  for i, b in ipairs(get(spec.bands) or { { freq = 80, gain = 4, q = .9 }, { freq = 450, gain = -3, q = 1.4 },
    { freq = 2500, gain = 5, q = 1.2 }, { freq = 9000, gain = -4, q = .8 } }) do
    bands[i] = { freq = b.freq or b[1] or 1000, gain = b.gain or b[2] or 0, q = b.q or b[3] or 1 }
  end
  local rev = morf.signal(H.uid("peq"), 0)
  local sel = morf.signal(H.uid("peq.sel"), 1)
  local function changed(i)
    rev:set(rev:get() + 1)
    if spec.on_changed then spec.on_changed(i, { freq = bands[i].freq, gain = bands[i].gain, q = bands[i].q }) end
  end
  local chips = Hh >= 150
  local CH = 24
  local pw = W - 2 * pad
  local ph = Hh - 2 * pad - (chips and CH + 8 or 0)
  local plot = ui.Item { x = pad, y = pad, width = pw, height = ph }
  H.add(root, plot)
  H.add(plot, H.well(style, pw, ph))
  local function fx(f) return (math.log(f, 10) - LO) / (HI - LO) end
  local decades, minors = {}, {}
  for _, f in ipairs { 100, 1000, 10000 } do decades[#decades + 1] = fx(f) end
  for _, f in ipairs { 50, 200, 500, 2000, 5000 } do minors[#minors + 1] = fx(f) end
  H.add(plot, H.guides(style, pw, ph, style.hatched and minors or {}, { .25, .75 }))
  H.add(plot, H.guides(style, pw, ph, decades, { .5 }, true))
  for i, f in ipairs { 100, 1000, 10000 } do
    H.add(plot, H.cap(style, { text = fmt_hz(f), x = fx(f) * pw + 3, y = ph - 15, width = 30 }))
  end
  local function bell(f, b)
    local r = f / b.freq
    local x = b.q * (r - 1 / r)
    return b.gain / (1 + x * x)
  end
  local function gy(g) return (.5 - g / (2 * R)) * ph end
  local N = 96
  local function response_d(only)
    rev:get()
    local d = {}
    for k = 0, N do
      local f = 10 ^ (LO + (HI - LO) * k / N)
      local g = 0
      if only then g = bell(f, only) else for _, b in ipairs(bands) do g = g + bell(f, b) end end
      d[#d + 1] = ("%s%.1f %.1f"):format(k == 0 and "M" or "L", pw * k / N, math.max(1, math.min(ph - 1, gy(g))))
    end
    return table.concat(d, " ")
  end
  local mid = gy(0)
  local inside = ui.Item { width = pw, height = ph, clip = true }
  H.add(plot, inside)
  -- The picked band's own bell, faint, under the sum.
  local function bell_area() local b = bands[sel:get()] return response_d(b) .. (" L%g %g L0 %g Z"):format(pw, mid, mid) end
  local sel_color = function() return get(style.series(sel:get())) end
  H.add(inside, H.path(pw, ph, { d = bell_area, fill_color = function() return sel_color():alpha(.16) end }))
  H.add(inside, H.shape(style, pw, ph, style.accent, function() return response_d() .. (" L%g %g L0 %g Z"):format(pw, mid, mid) end,
    { alpha = .18 }))
  H.add(inside, H.path(pw, ph, { d = function() return response_d() end, stroke_color = style.accent,
    stroke_width = style.hatched and 1.5 or 2.5, stroke_cap = "round", stroke_join = "round" }))
  for i, b in ipairs(bands) do
    H.add(plot, (H.point(style, { plot = plot, w = pw, h = ph, id = spec.id and (spec.id .. "-band-" .. i) or nil,
      name = ("Band %d"):format(i), color = style.series(i), text = tostring(i), size = style.hatched and 10 or 16,
      step_x = .005, step_y = .5 / (2 * R),
      x = function() rev:get() return fx(b.freq) end,
      y = function() rev:get() return .5 + b.gain / (2 * R) end,
      value = function() rev:get() return b.freq end, min = 20, max = 20000,
      selected = function() return sel:get() == i end,
      on_pressed = function() sel:set(i) end,
      on_wheel = function(_, _, _, _, _, steps)
        b.q = math.max(.2, math.min(12, b.q * (1.12 ^ -(steps or 0))))
        sel:set(i)
        changed(i)
      end,
      on_moved = function(x, y)
        b.freq = math.floor(10 ^ (LO + (HI - LO) * clamp01(x)) + .5)
        b.gain = math.floor((clamp01(y) - .5) * 2 * R * 10 + .5) / 10
        sel:set(i)
        changed(i)
      end })))
  end
  if chips then
    local n = #bands
    local gap = 6
    local cw = (pw - gap * (n - 1)) / n
    for i, b in ipairs(bands) do
      local tone = style.series(i)
      local function on() return sel:get() == i end
      local look
      local function text()
        rev:get()
        return fmt_hz(b.freq) .. " " .. signed(b.gain, 0)
      end
      if style.hatched then
        look = ui.Item { width = cw, height = CH,
          ui.Rect { width = cw, height = CH, color = function() return on() and get(tone):alpha(.2) or get(tone):alpha(0) end,
            border_width = 1, border_color = function() return on() and get(tone) or get(style.line) end },
          ui.Rect { x = 0, y = 0, width = 3, height = CH, color = tone },
          H.txt(style, { text = text, x = 6, width = cw - 8, height = CH, vertical_alignment = "center", elide = "right",
            font_size = H.small(style), font_family = style.mono_font, color = function() return on() and get(tone) or get(style.ink) end }) }
      else
        look = ui.Item { width = cw, height = CH,
          ui.Rect { width = cw, height = CH, radius = function() return on() and 8 or CH / 2 end,
            color = function() return on() and get(tone):alpha(.28) or get(style.track) end,
            behavior = { radius = style.spring(380, 18), color = { duration = 140 } } },
          ui.Rect { x = 6, y = CH / 2 - 3, width = 6, height = 6, radius = 3, color = tone },
          H.txt(style, { text = text, x = 15, width = cw - 18, height = CH, vertical_alignment = "center", elide = "right",
            font_size = H.small(style), font_weight = 600, color = style.ink }) }
      end
      H.add(root, (H.press({ id = spec.id and (spec.id .. "-chip-" .. i) or nil, x = pad + (i - 1) * (cw + gap),
        y = Hh - pad - CH, width = cw, height = CH, accessible_name = ("Band %d"):format(i),
        on_clicked = function() sel:set(i) end }, { look })))
    end
  end
  return root
end

-- ----------------------------------------------------------------- tuner --

--- A tuner: a needle over a scale of ±50 cents, the note's name below and
--- the offset read out, the middle lit when in tune. `spec`: `frequency`
--- (Hz, or a function: the note and cents are worked out from it against
--- `reference`, 440), or `note` and `cents` (values or functions);
--- `tolerance` (cents, 5), `width` (260), `height` (190), `label`, `color`.
function M.tuner(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local color = H.color(spec, style)
  local tol = spec.tolerance or 5
  local ref = spec.reference or 440
  local function reading()
    local f = tonumber(get(spec.frequency))
    if f and f > 0 then
      local m = 69 + 12 * math.log(f / ref, 2)
      local n = math.floor(m + .5)
      return H.note_name(n), (m - n) * 100, f
    end
    return tostring(get(spec.note) or "A4"), tonumber(get(spec.cents)) or 0, nil
  end
  local function cents() local _, c = reading() return math.max(-50, math.min(50, c)) end
  local function tuned() return math.abs(cents()) <= tol end
  local root = H.frame(spec, style, "Tuner", W, Hh, "meter")
  root.accessible = function() return { value = math.floor(cents() + .5), minimum = -50, maximum = 50 } end
  local pad = H.pad(spec, style)
  local SWEEP = 100
  local cx = W / 2
  local big = math.min(style.size.extra, math.max(style.size.large, math.floor((Hh - 2 * pad) * .2)))
  local cy = Hh - pad - big - 10
  local Rr = math.min(W / 2 - pad - 20, cy - pad - 14)
  local arc, ticks = morf.geometry.arc, morf.geometry.ticks
  local function path(p) return H.path(W, Hh, p) end
  local zone = SWEEP * tol / 100
  if style.hatched then
    H.add(root, path { d = arc(cx, cy, Rr, -SWEEP / 2, SWEEP), stroke_color = style.stroke_of and style.stroke_of("mark", color) or style.line,
      stroke_width = 1 })
    H.add(root, path { d = ticks(cx, cy, Rr - 5, Rr, { from = -SWEEP / 2, sweep = SWEEP, count = 20, major = 5, major_r0 = Rr - 11 }),
      stroke_color = style.stroke_of and style.stroke_of("hot", color) or style.ink_lo, stroke_width = 1 })
    H.add(root, path { d = arc(cx, cy, Rr + 5, -zone, 2 * zone), stroke_color = style.ok, stroke_width = 4, stroke_cap = "butt" })
  else
    local thick = 6
    H.add(root, path { d = arc(cx, cy, Rr, -SWEEP / 2, SWEEP), stroke_color = style.track, stroke_width = thick, stroke_cap = "round" })
    H.add(root, path { d = arc(cx, cy, Rr, -zone, 2 * zone), stroke_color = style.ok, stroke_width = thick, stroke_cap = "round" })
    H.add(root, path { d = ticks(cx, cy, Rr - 12, Rr - 11.9, { from = -SWEEP / 2, sweep = SWEEP, count = 10 }), stroke_width = 3,
      stroke_cap = "round", stroke_color = H.alpha(style.ink_lo, .6) })
  end
  for _, m in ipairs { { -50, "♭" }, { 50, "♯" } } do
    local a = math.rad(SWEEP / 2 * (m[1] / 50))
    local rr = Rr + 14
    H.add(root, H.txt(style, { text = m[2], x = cx + rr * math.sin(a) - 12, y = cy - rr * math.cos(a) - 10, width = 24, height = 20,
      horizontal_alignment = "center", font_size = style.size.small, color = style.ink_lo }))
  end
  -- The needle, turning about the pivot.
  local L = Rr - 4
  local nw = style.hatched and 2 or 4
  H.add(root, ui.Item { x = cx - L, y = cy - L, width = 2 * L, height = 2 * L,
    rotation = function() return SWEEP / 2 * cents() / 50 end, behavior = { rotation = H.travel(style, 120, 12) },
    ui.Rect { x = L - nw / 2, y = 2, width = nw, height = L - 2, radius = style.hatched and 0 or nw / 2,
      color = function() return tuned() and get(style.ok) or get(style.hatched and style.ink or color) end,
      behavior = { color = { duration = 160 } } } })
  local hub = style.hatched and 8 or 14
  H.add(root, ui.Rect { x = cx - hub / 2, y = cy - hub / 2, width = hub, height = hub, radius = style.hatched and 0 or hub / 2,
    color = function() return tuned() and get(style.ok) or get(style.hatched and color or style.ink) end })
  -- The note under the pivot, the frequency and the offset either side.
  H.add(root, H.txt(style, { text = function() local n = reading() return n end, x = W / 2 - 50, y = cy + hub / 2 + 2,
    width = 100, height = big + 4, horizontal_alignment = "center", font_size = big, font_weight = 700,
    font_family = style.hatched and style.mono_font or nil,
    color = function() return tuned() and get(style.ok) or get(style.ink) end }))
  local ry = cy + hub / 2 + 2 + (big + 4) / 2 - 9
  H.add(root, H.txt(style, { x = pad, y = ry, width = W / 2 - pad - 46, height = 18, elide = "right",
    font_size = style.size.small - 1, font_family = style.hatched and style.mono_font or nil, color = style.ink_lo,
    horizontal_alignment = "left",
    text = function()
      local _, _, f = reading()
      return f and ("%.1f Hz"):format(f) or (spec.label or (style.hatched and "TUNER" or "Tuner"))
    end }))
  H.add(root, H.txt(style, { x = W / 2 + 46, y = ry, width = W / 2 - pad - 46, height = 18,
    horizontal_alignment = "right", font_size = style.size.small - 1, font_weight = 600,
    font_family = style.hatched and style.mono_font or nil,
    color = function() return tuned() and get(style.ok) or get(style.warn) end,
    text = function() return signed(cents(), 0) .. " ¢" end }))
  return root
end

return M
