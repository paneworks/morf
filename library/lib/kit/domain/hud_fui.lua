-- Domain instruments: hud_fui -- the "fictional UI" pieces of a sci-fi
-- heads-up display, drawn over the display widgets in a theme's style
-- (see lib.kit.display). Every one lays out the same in each theme:
-- Tsugumori (style.hatched) draws them as an instrument would be etched --
-- hairlines, tick rulers, square cells, upper-case mono, registration
-- corners; Material draws the same in M3 expressive form -- tonal fills,
-- round caps and corners, soft glows, distance-field blobs that merge,
-- shapes that morph and springs that squash.
--
-- Motion is the engine's, not Lua's: rotations, scrolls, sweeps and
-- pulses are `loop`s and looping `morf.animation` groups that run in Rust
-- and end with their node. What turns or slides is a drawing that never
-- changes, rotated or translated (inside a clip where it would leave its
-- box) -- a cached texture moved, nothing redrawn; a path is morphed only
-- where its shape really changes (a level's lobes, Material's orb). Streams are data channels drawn by a path's
-- `plot`. Three keep a `ui.Timer` (a child, so it ends with the widget):
-- `decode_text` (40 ms ticks while it decodes, idle after),
-- `signal_noise` (re-rolls its noise channel at a low rate) and
-- `wireframe` (re-projects a dozen vertices at 25 Hz -- a 3-D turn has
-- no 2-D transform to animate). `countdown_ring` keeps a one-second tick
-- when it counts down on its own.
local ui = require("morf.ui")
local morf = require("morf")
local U = require("lib.kit.display.util")
local channel = require("lib.channel")
local get, clamp01, A = U.get, U.clamp01, U.alpha
local geo = morf.geometry

local M = {}

-- ------------------------------------------------------------ shared --

local serial = 0
local function name(kind) serial = serial + 1 return ("kit.hud_fui.%s.%d"):format(kind, serial) end

--- A 0..1 reader of `v`: a number, a function of one, or a channel (its newest).
local function reader(v, default)
  if channel.is(v) then return function() return clamp01(v:last() or 0) end end
  if v == nil then v = default or 0 end
  return function() return clamp01(get(v)) end
end

local function settle(style) return { duration = style.motion.duration, easing = style.motion.easing } end
local function travel(style, k, d)
  if style.hatched then return settle(style) end
  return style.spring(k, d)
end

--- `f` in whole steps of `1/n`.
local function steps(f, n) return function() return math.floor(f() * n + .5) / n end end

local function figure(props, label)
  props.accessible_role = "figure"
  props.accessible_name = label
  return props
end

local function meter(props, label, v)
  props.accessible_role = "meter"
  props.accessible_name = label
  props.accessible = function() return { value = math.floor(v() * 100 + .5), minimum = 0, maximum = 100 } end
  return props
end

--- Text in the style, never under the small size less three.
local function text(style, props)
  props.font_size = math.max(props.font_size or style.size.normal, style.size.small - 3)
  if props.color == nil then props.color = style.ink end
  return style.text(props)
end

--- Text in a fixed-advance face: Tsugumori's own face is one; Material
--- takes the theme's mono.
local function mono(style, props)
  if not style.hatched and style.mono_font then props.font_family = style.mono_font end
  return text(style, props)
end

--- A string as the style writes it: Tsugumori upper case.
local function cased(style, s)
  s = tostring(s or "")
  return style.hatched and s:upper() or s
end

local caption = U.caption

--- A path in a `w` x `h` box of its own units.
local function path(w, h, props)
  props.width = props.width or w
  props.height = props.height or h
  props.view_box = { 0, 0, w, h }
  if props.fill_color == nil then props.fill_color = "transparent" end
  return ui.Path(props)
end

local function cap(style) return style.hatched and "butt" or "round" end
local function weight(style, material) return style.hatched and 1 or (material or 2) end

--- A full turn every `period` ms (anticlockwise when `back`), from `from` degrees.
local function spin(period, back, from)
  from = from or 0
  return { rotation = { from = from, to = from + (back and -360 or 360), duration = period, easing = "linear" } }
end

local function add(node, ...)
  for _, child in ipairs { ... } do if child then ui.reparent(child, node) end end
  return node
end

local function circle(cx, cy, r) return geo.arc(cx, cy, r, 0, 360) end

--- Makes a ring path (`props`, centred in its own box) turn a full
--- circle every `period` ms (anticlockwise when `back`): a looped
--- rotation, so the drawn path is a cached texture turned, never redrawn.
local function turning(props, _, period, back)
  props.loop = spin(period, back)
  return props
end

--- `n` ticks round `cx, cy` from radius `r0` out to `r1`, `thick` (1)
--- wide, in a `w` x `h` path centred on them: one dashed stroke, turning
--- when given a `period`.
local function tick_ring(w, h, cx, cy, r0, r1, n, props, period, back)
  local r = (r0 + r1) / 2
  local pitch = 2 * math.pi * r / n
  local t = props.thick or 1
  props.thick = nil
  props.d, props.stroke_width, props.dash, props.stroke_cap = circle(cx, cy, r), r1 - r0, { t, pitch - t }, "butt"
  if period then turning(props, r, period, back) end
  return path(w, h, props)
end

--- A scan line `w` wide sweeping down `h` every `period` ms (back up too
--- when `alternate`), a glowing wake `band` tall behind it, at `x, y`:
--- an item translated inside whatever clips it. Returns the item.
local function scanline(style, color, x, y, w, h, band, period, alternate)
  local lh = style.hatched and 1 or 3
  return ui.Item { x = x, y = y - band, width = w, height = band + lh,
    loop = { translate_y = { from = 0, to = h + (alternate and -lh or band), duration = period, easing = alternate and "in_out_sine" or "linear",
      alternate = alternate } },
    ui.Rect { width = w, height = band, gradient = function()
      local k = get(color)
      return { angle = 180, stops = { k:alpha(0), k:alpha(style.hatched and .16 or .22) } } end },
    ui.Rect { y = band, width = w, height = lh, color = color, radius = style.hatched and 0 or lh / 2,
      shadow_color = (not style.hatched) and A(color, .8) or nil, shadow_blur = (not style.hatched) and 8 or nil } }
end

--- Path data for the four L-brackets on the corners of `x, y, w, h`,
--- each `l` long, drawn by a stroke `t` wide.
local function corners_d(x, y, w, h, l, t)
  local o = (t or 1) / 2
  local x0, y0, x1, y1 = x + o, y + o, x + w - o, y + h - o
  return ("M%g %g V%g H%g M%g %g H%g V%g M%g %g V%g H%g M%g %g H%g V%g"):format(
    x0, y0 + l, y0, x0 + l, x1 - l, y0, x1, y0 + l,
    x0, y1 - l, y1, x0 + l, x1 - l, y1, x1, y1 - l)
end

--- The frame a framed widget sits in, `w` x `h`: Material a raised tonal
--- card; Tsugumori a faint plate on a hairline with corner brackets.
local function frame(style, w, h, color)
  if not style.hatched then
    return ui.Rect { width = w, height = h, color = style.raised, radius = style.radius(math.min(w, h) / 3) * 1.4 }
  end
  return ui.Item { width = w, height = h,
    ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1, border_color = style.line },
    path(w, h, { d = corners_d(0, 0, w, h, 8, 1), stroke_color = color or style.stroke_of("mark"), stroke_width = 1,
      stroke_cap = "square" }),
  }
end

--- A clip of `w` x `h`: rounded as the style's corners in Material.
local function clip(style, w, h, props)
  props = props or {}
  props.width, props.height, props.color = w, h, "transparent"
  props.radius = style.hatched and 0 or style.radius(math.min(w, h) / 3) * 1.4
  return ui.ClipRect(props)
end

--- A blinking pip: Tsugumori a square, Material a dot.
local function pip(style, color, props)
  local s = props.size or 6
  props.size = nil
  props.width, props.height = s, s
  props.radius = style.hatched and 0 or s / 2
  props.color = color
  props.loop = props.loop or { opacity = { from = 1, to = .25, duration = 700, alternate = true, easing = "in_out_sine" } }
  return ui.Rect(props)
end

--- A fixed pseudo-random sequence (`seed`), so a widget looks the same
--- every time it is drawn.
local function rng(seed)
  local s = seed or 1
  return function()
    s = (s * 1103515245 + 12345) % 2147483648
    return s / 2147483648
  end
end

--- A closed outline about `cx, cy` of radius `r` bulged by `amp` in `k`
--- lobes, through `n` points: the same moves for every `amp`, so one morphs
--- into another.
local function lobes_d(cx, cy, r, amp, k, phase, n)
  local d = {}
  for i = 0, n - 1 do
    local t = 2 * math.pi * i / n
    local rr = r * (1 + amp * math.sin(k * t + phase))
    d[#d + 1] = ("%s%.2f %.2f"):format(i == 0 and "M" or "L", cx + rr * math.sin(t), cy - rr * math.cos(t))
  end
  d[#d + 1] = "Z"
  return table.concat(d, " ")
end

--- The colour of a level against warn and alert.
local function zone(style, x, base)
  if x >= .75 then return style.alert end
  if x >= .45 then return style.warn end
  return base
end

local function pct(v) return function() return ("%d%%"):format(math.floor(v() * 100 + .5)) end end

-- --------------------------------------------------------- tick_ruler --

--- A graduated scale, a label at every major tick, an optional pointer at
--- `value`. `spec`: `width` (260) / `height`, `vertical`, `from`/`to`
--- (0/100: what the labels read), `majors` (4 labelled intervals),
--- `minor` (5 ticks to an interval), `value` (0..1, a function or a
--- channel; no pointer when nil), `label`, `format` (fn(number) -> string),
--- `color`.
function M.tick_ruler(spec, style)
  local vertical = spec.vertical
  local color = U.color(spec, style, "accent")
  local from, to = spec.from or 0, spec.to or 100
  local majors, minor = spec.majors or 4, spec.minor or 5
  local n = majors * minor
  local fmt = spec.format or function(x) return tostring(math.floor(x + .5)) end
  local v = spec.value ~= nil and reader(spec.value) or nil
  local fs = style.size.small - 2
  local tick = style.hatched and 12 or 11
  local tick_color = style.hatched and style.stroke_of("mark", color) or style.ink_lo
  local major_color = style.hatched and color or style.ink
  local sw = weight(style, 1.6)
  local head = spec.label and 18 or 0
  if not vertical then
    local w = spec.width or 260
    local pad = 16
    local len = w - 2 * pad
    local y0 = head + 10                       -- the baseline
    local h = spec.height or (y0 + tick + fs + 8)
    local pitch = len / n
    local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Scale"))
    if spec.label then add(node, caption(style, { text = spec.label, width = w })) end
    -- Minor and major ticks, the majors drawn again over them.
    local minors, bigs = {}, {}
    for k = 0, n do
      local x = pad + k * pitch
      if k % minor == 0 then bigs[#bigs + 1] = ("M%.2f %.2f V%.2f"):format(x, y0, y0 + tick)
      else minors[#minors + 1] = ("M%.2f %.2f V%.2f"):format(x, y0, y0 + tick * .5) end
    end
    if style.hatched then
      add(node, path(w, h, { d = ("M%g %g H%g"):format(pad, y0 + .5, w - pad), stroke_color = style.stroke_of("idle", color), stroke_width = 1 }))
    else
      add(node, ui.Rect { x = pad - 2, y = y0 - 4, width = len + 4, height = 4, radius = 2, color = style.track })
      if v then
        add(node, ui.Rect { x = pad - 2, y = y0 - 4, height = 4, radius = 2, color = color,
          width = function() return 4 + len * v() end, behavior = { width = travel(style, 200, 18) } })
      end
    end
    add(node, path(w, h, { d = table.concat(minors, " "), stroke_color = tick_color, stroke_width = sw, stroke_cap = cap(style) }),
      path(w, h, { d = table.concat(bigs, " "), stroke_color = major_color, stroke_width = sw, stroke_cap = cap(style) }))
    for k = 0, majors do
      local x = pad + k * pitch * minor
      add(node, mono(style, { text = fmt(from + (to - from) * k / majors), x = x - 22, y = y0 + tick + 2, width = 44,
        font_size = fs, horizontal_alignment = "center", color = style.ink_lo }))
    end
    if v then
      -- The pointer: a notch over the baseline and a hairline down the ticks.
      local pointer = ui.Item { y = y0 - 10, width = 12, height = tick + 10,
        x = function() return pad + len * v() - 6 end, behavior = { x = travel(style, 200, 16) },
        stretch = (not style.hatched) and { scale = .1 } or nil,
        path(12, 9, { d = style.hatched and "M1 1 H11 L6 8 Z" or "M2 1.5 H10 L6 7.5 Z", fill_color = style.hatched and "transparent" or color,
          stroke_color = color, stroke_width = style.hatched and 1 or 2.5, stroke_join = "round" }),
        ui.Rect { x = 5.5, y = 10, width = 1, height = tick, color = color },
      }
      add(node, pointer)
    end
    return node
  end
  local h = spec.height or 160
  local pad = 8
  local len = h - 2 * pad - head
  local x0 = 10                                -- the spine
  local w = spec.width or (x0 + tick + 4 + 40)
  local pitch = len / n
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Scale"))
  if spec.label then add(node, caption(style, { text = spec.label, width = w })) end
  local top = head + pad
  local minors, bigs = {}, {}
  for k = 0, n do
    local y = top + k * pitch
    if k % minor == 0 then bigs[#bigs + 1] = ("M%.2f %.2f H%.2f"):format(x0, y, x0 + tick)
    else minors[#minors + 1] = ("M%.2f %.2f H%.2f"):format(x0, y, x0 + tick * .5) end
  end
  if style.hatched then
    add(node, path(w, h, { d = ("M%g %g V%g"):format(x0 + .5, top, top + len), stroke_color = style.stroke_of("idle", color), stroke_width = 1 }))
  else
    add(node, ui.Rect { x = x0 - 4, y = top - 2, width = 4, height = len + 4, radius = 2, color = style.track })
  end
  add(node, path(w, h, { d = table.concat(minors, " "), stroke_color = tick_color, stroke_width = sw, stroke_cap = cap(style) }),
    path(w, h, { d = table.concat(bigs, " "), stroke_color = major_color, stroke_width = sw, stroke_cap = cap(style) }))
  for k = 0, majors do
    local y = top + k * pitch * minor
    -- Top of the scale is its high end.
    add(node, mono(style, { text = fmt(to - (to - from) * k / majors), x = x0 + tick + 4, y = y - fs * .7, height = math.ceil(fs * 1.4),
      width = w - x0 - tick - 4, font_size = fs, color = style.ink_lo, vertical_alignment = "center" }))
  end
  if v then
    add(node, ui.Item { x = 0, height = 12, width = x0 + tick,
      y = function() return top + len * (1 - v()) - 6 end, behavior = { y = travel(style, 200, 16) },
      path(9, 12, { d = style.hatched and "M1 1 V11 L8 6 Z" or "M1.5 2 V10 L7.5 6 Z", fill_color = style.hatched and "transparent" or color,
        stroke_color = color, stroke_width = style.hatched and 1 or 2.5, stroke_join = "round" }),
      ui.Rect { x = 10, y = 5.5, width = tick, height = 1, color = color },
    })
  end
  return node
end

-- -------------------------------------------------------- radar_sweep --

--- A radar scope: range rings, a beam sweeping round with a fading trail,
--- and blips that flare as the beam passes them. `spec`: `size` (170),
--- `period` (ms a turn, 3600), `blips` ({ { angle (degrees from north),
--- distance (0..1), kind ("accent", "alert", ...) }, ... }), `label`,
--- `color`.
function M.radar_sweep(spec, style)
  local s = spec.size or 170
  local c = s / 2
  local period = spec.period or 3600
  local color = U.color(spec, style, "accent")
  local r = c - (style.hatched and 8 or 2)
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), spec.label or "Radar"))
  if style.hatched then
    add(node,
      path(s, s, { d = geo.ticks(c, c, r + 2, c - 1, { count = 72, major = 6, major_r0 = r - 2 }), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }),
      path(s, s, { d = circle(c, c, r), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }),
      path(s, s, { d = circle(c, c, r * 2 / 3) .. circle(c, c, r / 3), stroke_color = style.stroke_of("quiet", color), stroke_width = 1, dash = { 2, 3 } }),
      path(s, s, { d = ("M%g %g V%g M%g %g H%g"):format(c, c - r, c + r, c - r, c, c + r), stroke_color = style.stroke_of("quiet", color), stroke_width = 1 }))
  else
    add(node,
      ui.Rect { x = c - r, y = c - r, width = 2 * r, height = 2 * r, radius = r, color = A(color, .08),
        border_width = 2, border_color = A(color, .35) },
      path(s, s, { d = circle(c, c, r * 2 / 3) .. circle(c, c, r / 3), stroke_color = A(color, .22), stroke_width = 1.5 }),
      path(s, s, { d = ("M%g %g V%g M%g %g H%g"):format(c, c - r + 4, c + r - 4, c - r + 4, c, c + r - 4), stroke_color = A(color, .16),
        stroke_width = 1.5, stroke_cap = "round" }))
  end
  -- The beam and its trail turn together, one looped rotation of drawings
  -- that never change: Material a disc of conic gradient bright just
  -- behind the beam; Tsugumori the trail quantised into wedges stepping
  -- down in strength.
  local sweep = ui.Item { anchors = { fill = true }, loop = spin(period) }
  if style.hatched then
    local n, span = 12, 8
    for k = 1, n do
      add(sweep, path(s, s, { d = geo.sector(c, c, 4, r - 1, 360 - k * span + .6, span - 1.2),
        fill_color = A(color, .34 * (1 - (k - 1) / n)), stroke_color = "transparent" }))
    end
  else
    add(sweep, ui.Rect { x = c - r, y = c - r, width = 2 * r, height = 2 * r, radius = r, gradient = function()
      local k = get(color)
      return { kind = "conic", stops = { { k:alpha(0), 0 }, { k:alpha(0), .64 }, { k:alpha(.42), 1 } } }
    end })
  end
  add(sweep, ui.Rect { x = c - (style.hatched and .5 or 1), y = c - r, width = style.hatched and 1 or 2, height = r, color = color,
    radius = style.hatched and 0 or 1, shadow_color = (not style.hatched) and A(color, .8) or nil, shadow_blur = (not style.hatched) and 6 or nil })
  add(node, sweep)
  for i, b in ipairs(spec.blips or {}) do
    local a, dist = math.rad(b.angle or b[1] or 0), clamp01(b.distance or b[2] or .5)
    local tone = style[b.kind or b[3] or "accent"] or color
    local bx, by = c + dist * r * math.sin(a), c - dist * r * math.cos(a)
    local bs = style.hatched and 5 or 7
    local blip = ui.Item { x = bx - 7, y = by - 7, width = 14, height = 14, opacity = .15,
      style.hatched and ui.Rect { x = 4.5, y = 4.5, width = bs, height = bs, color = tone } or
        ui.Rect { x = 3.5, y = 3.5, width = bs, height = bs, radius = bs / 2, color = tone, shadow_color = A(tone, .7), shadow_blur = 6 },
      style.hatched and path(14, 14, { d = corners_d(0, 0, 14, 14, 3, 1), stroke_color = tone, stroke_width = 1 }) or
        ui.Rect { anchors = { fill = true }, radius = 7, color = "transparent", border_width = 1.5, border_color = A(tone, .5) },
    }
    add(node, blip)
    -- Lit as the beam crosses it, fading over the turn.
    local deg = ((b.angle or b[1] or 0) % 360)
    morf.animation.play { loops = "forever", delay = math.floor(deg / 360 * period),
      { node = blip, property = "opacity", duration = period, keyframes = {
        { at = 0, value = 1 }, { at = .06, value = 1 }, { at = .8, value = .15 }, { at = 1, value = .15 } } } }
    local _ = i
  end
  add(node, ui.Rect { x = c - 3, y = c - 3, width = 6, height = 6, radius = style.hatched and 0 or 3, color = color })
  return node
end

-- ------------------------------------------------- segmented_arc_ring --

--- A ring of arc segments lit up to `value`, the reading in its centre.
--- `spec`: `size` (160), `value` (0..1, function, channel), `segments`
--- (24; Tsugumori 36), `sweep` (300), `label`, `text` (fn -> string; the
--- percentage), `color`.
function M.segmented_arc_ring(spec, style)
  local s = spec.size or 160
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value, .5)
  local n = spec.segments or (style.hatched and 36 or 24)
  local sweep = spec.sweep or 300
  local from = -sweep / 2
  local thick = math.max(5, math.floor(s * (style.hatched and .075 or .085)))
  local r = c - thick / 2 - (style.hatched and 7 or 2)
  local L = 2 * math.pi * r * sweep / 360
  local gap = style.hatched and 2.5 or thick * .6
  local seg = sweep >= 360 and (L / n - gap) or (L - gap * (n - 1)) / n
  local dash = style.hatched and { seg, gap } or { math.max(.01, seg - thick), gap + thick }
  local d = geo.arc(c, c, r, from, math.min(sweep, 359.99))
  local lit = steps(v, n)
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), spec.label or "Level", v))
  local function ring(props)
    props.d, props.stroke_width, props.dash, props.stroke_cap = d, props.stroke_width or thick, dash, cap(style)
    return path(s, s, props)
  end
  local on = function() return lit() > 0 and 1 or 0 end
  local trim = function() return math.max(.0001, lit()) end
  add(node, ring { stroke_color = A(color, style.hatched and .14 or .18) })
  if not style.hatched then
    add(node, ring { stroke_color = A(color, .22), stroke_width = thick * 1.9, opacity = on, trim_end = trim,
      behavior = { trim_end = settle(style) } })
  end
  add(node, ring { stroke_color = color, opacity = on, trim_end = trim, behavior = { trim_end = settle(style) } })
  local ri = r - thick / 2 - 4
  if style.hatched then
    add(node,
      path(s, s, { d = geo.ticks(c, c, ri - 3, ri, { from = from, sweep = sweep, count = n + 1, major = 6, major_r0 = ri - 6 }),
        stroke_color = style.stroke_of("mark", color), stroke_width = 1 }),
      path(s, s, { d = geo.ticks(c, c, ri - 6, c - 1, { angles = { from, from + sweep } }), stroke_color = color, stroke_width = 1 }),
      path(s, s, turning({ d = circle(c, c, c - 1.5), stroke_color = style.stroke_of("quiet", color), stroke_width = 1, dash = { 14, 6, 2, 6 } }, c - 1.5, 40000)))
  else
    add(node, path(s, s, turning({ d = circle(c, c, ri - 2), stroke_color = A(color, .3), stroke_width = 2, stroke_cap = "round", dash = { .01, 9 } }, ri - 2, 24000, true)))
  end
  local reading = spec.text or pct(v)
  local fs = math.max(style.size.small, math.floor(s * .2))
  add(node, ui.Column { anchors = { center_in = true }, width = math.floor(ri * 1.6), gap = 0, align = "center",
    text(style, { text = reading, font_size = fs, font_weight = style.hatched and 400 or 600, color = style.hatched and color or style.ink,
      horizontal_alignment = "center", height = math.ceil(fs * 1.25) }),
    spec.label and caption(style, { text = spec.label, horizontal_alignment = "center", width = math.floor(ri * 1.6), elide = "right" }) or nil,
  })
  return node
end

-- -------------------------------------------------------- target_lock --

--- Corner brackets hunting round a target box that close in and hold when
--- it locks. `spec`: `width` (150), `height` (170), `locked` (bool or
--- fn), `label` (the target's name), `color`.
function M.target_lock(spec, style)
  local w, h = spec.width or 150, spec.height or 170
  local locked = function() return get(spec.locked) and true or false end
  local base = U.color(spec, style, "accent")
  local color = function() return get(locked() and style.alert or base) end
  local area = h - (spec.label and 40 or 22)
  local tw = math.floor(math.min(w, area) * .42)
  local bx, by = math.floor((w - tw) / 2), math.floor((area - tw) / 2)
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Target"))
  -- The target.
  if style.hatched then
    add(node, U.fill(style, { x = bx, y = by, width = tw, height = tw, color = base }))
  else
    add(node, ui.Rect { x = bx, y = by, width = tw, height = tw, radius = tw / 4, color = A(base, .16) })
  end
  local cx, cy = bx + tw / 2, by + tw / 2
  add(node, path(w, area, { d = ("M%g %g H%g M%g %g H%g M%g %g V%g M%g %g V%g"):format(
      cx - tw * .35, cy, cx - 4, cx + 4, cy, cx + tw * .35, cx, cy - tw * .35, cy - 4, cx, cy + 4, cx, cy + tw * .35),
    stroke_color = color, stroke_width = weight(style, 2), stroke_cap = cap(style) }))
  -- A dashed ring circling while it hunts; it stops where it is on a lock.
  local rr = tw * .82
  local hs = math.ceil(2 * rr + 4)
  add(node, path(hs, hs, { x = cx - hs / 2, y = cy - hs / 2, d = circle(hs / 2, hs / 2, rr), stroke_color = A(base, .55),
    stroke_width = weight(style, 2), stroke_cap = cap(style), dash = style.hatched and { 10, 5, 2, 5 } or { .01, 8 },
    opacity = function() return locked() and .25 or 1 end, behavior = { opacity = settle(style) },
    loop = function() if locked() then return nil end return { rotation = { to = 360, duration = 5000, easing = "linear", hold = true } } end }))
  local len = math.floor(tw * .32)
  local t = style.hatched and 1.5 or 3.5
  local o = t / 2
  local spread = math.floor(tw * .3)
  local g = 6
  local CORNERS = {
    { bx - g, by - g, -1, -1, ("M%g %g V%g H%g"):format(o, len, o, len) },
    { bx + tw + g - len, by - g, 1, -1, ("M0 %g H%g V%g"):format(o, len - o, len) },
    { bx - g, by + tw + g - len, -1, 1, ("M%g 0 V%g H%g"):format(o, len - o, len) },
    { bx + tw + g - len, by + tw + g - len, 1, 1, ("M0 %g H%g V0"):format(len - o, len - o) },
  }
  for _, k in ipairs(CORNERS) do
    local sx, sy = k[3], k[4]
    local inner = ui.Item { width = len, height = len,
      loop = function()
        if locked() then return nil end
        return { translate_x = { from = 0, to = -sx * spread * .45, duration = 760, alternate = true, easing = "in_out_sine" },
          translate_y = { from = 0, to = -sy * spread * .45, duration = 760, alternate = true, easing = "in_out_sine" } }
      end,
      path(len, len, { d = k[5], stroke_color = color, stroke_width = t, stroke_cap = style.hatched and "square" or "round",
        stroke_join = style.hatched and "miter" or "round" }) }
    add(node, ui.Item { x = k[1], y = k[2], width = len, height = len,
      translate_x = function() return locked() and 0 or sx * spread end,
      translate_y = function() return locked() and 0 or sy * spread end,
      behavior = { translate_x = travel(style, 260, 14), translate_y = travel(style, 260, 14) },
      stretch = (not style.hatched) and { scale = .2, max = .3 } or nil,
      inner })
  end
  -- The state, and the target's name.
  local state = function() return cased(style, locked() and "Locked" or "Acquiring") end
  add(node, ui.Row { y = h - 18, anchors = { horizontal_center = true }, gap = 6, height = 18,
    pip(style, color, { size = 6, y = 6, loop = function() if locked() then return nil end
      return { opacity = { from = 1, to = .2, duration = 380, alternate = true } } end }),
    caption(style, { text = state, color = color, height = 18, vertical_alignment = "center" }),
  })
  if spec.label then
    add(node, mono(style, { text = function() return cased(style, get(spec.label)) end, y = h - 36, width = w, height = 18,
      horizontal_alignment = "center", vertical_alignment = "center", font_size = style.size.small - 1, font_weight = 600, color = style.ink_lo }))
  end
  return node
end

-- --------------------------------------------------------- scan_sweep --

--- A box a scan line sweeps down over and over, a glowing wake behind it.
--- `spec`: `width` (240), `height` (150), `period` (2400 ms), `label`
--- ("Scanning"), `color`; children are laid in the box under the scan.
function M.scan_sweep(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "accent")
  local period = spec.period or 2400
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Scan"))
  add(node, frame(style, w, h))
  local box = clip(style, w, h)
  -- A faint grid to scan over.
  local gd, step = {}, 16
  for x = step, w - 1, step do gd[#gd + 1] = ("M%d 0 V%d"):format(x, h) end
  for y = step, h - 1, step do gd[#gd + 1] = ("M0 %d H%d"):format(y, w) end
  add(box, path(w, h, { d = table.concat(gd, " "), stroke_color = A(color, style.hatched and .1 or .08), stroke_width = 1 }))
  for _, child in ipairs(spec) do add(box, child) end
  add(box, scanline(style, color, 0, 0, w, h, math.floor(h * .35), period))
  add(node, box)
  add(node, ui.Row { x = 12, y = h - 26, gap = 6, height = 18,
    pip(style, color, { size = 6, y = 6 }),
    caption(style, { text = spec.label or "Scanning", color = color, height = 18, vertical_alignment = "center" }) })
  return node
end

-- ----------------------------------------------------- waveform_rings --

--- Concentric rings that bulge into lobes as `level` rises, each turning
--- its own way, with a ripple running out. `spec`: `size` (160), `level`
--- (0..1, function or channel), `rings` (4), `label`, `color`.
function M.waveform_rings(spec, style)
  local s = spec.size or 160
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local v = reader(spec.level, .6)
  local count = spec.rings or 4
  local R = c - (style.hatched and 10 or 6)
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), spec.label or "Level", v))
  local pts = style.hatched and 60 or 96
  -- The ripple, from the core out.
  add(node, ui.Rect { x = c - R, y = c - R, width = 2 * R, height = 2 * R, radius = R, color = "transparent",
    border_width = style.hatched and 1 or 2, border_color = A(color, .6),
    loop = { scale = { from = .3, to = 1.08, duration = 2200, easing = "out_quad" }, opacity = { from = 1, to = 0, duration = 2200, easing = "in_quad" } } })
  for i = count, 1, -1 do
    local r = R * (.32 + .68 * (i - 1) / math.max(1, count - 1)) * .9
    local k = style.hatched and (6 + 2 * i) or (3 + i)
    local amp = (style.hatched and .07 or .09) * (1 + (count - i) * .15)
    local d0 = lobes_d(c, c, r, 0, k, i, pts)
    local d1 = lobes_d(c, c, r, amp, k, i, pts)
    -- Each ring in a box of its own size, turning its own way (a cached
    -- texture turned; it is redrawn only while the level morphs it).
    local side = math.ceil(2 * r * (1 + amp) + 6)
    local o = c - side / 2
    d0, d1 = lobes_d(side / 2, side / 2, r, 0, k, i, pts), lobes_d(side / 2, side / 2, r, amp, k, i, pts)
    local ring = path(side, side, { d = d0, morph_to = d1, morph_progress = v, behavior = { morph_progress = travel(style, 120, 10) } })
    if style.hatched then
      ring.stroke_color = i == count and color or style.stroke_of(i % 2 == 0 and "mark" or "idle", color)
      ring.stroke_width = 1
      ring.stroke_join = "miter"
    else
      ring.fill_color = A(color, .06 + .05 * (count - i))
      ring.stroke_color = A(color, .35 + .5 * (count - i + 1) / count)
      ring.stroke_width = 2
      ring.stroke_join = "round"
    end
    local motion = spin(6000 + 2500 * i, i % 2 == 0)
    add(node, ui.Item { x = o, y = o, width = side, height = side, loop = motion, ring })
  end
  if style.hatched then
    add(node, tick_ring(s, s, c, c, c - 6, c - 1, 60, { stroke_color = style.stroke_of("quiet", color) }, 30000),
      tick_ring(s, s, c, c, c - 9, c - 1, 12, { stroke_color = style.stroke_of("mark", color) }, 30000))
  end
  local fs = math.max(style.size.small - 1, math.floor(s * .12))
  add(node, mono(style, { text = pct(v), anchors = { center_in = true }, font_size = fs, font_weight = 600,
    color = style.hatched and color or style.ink }))
  return node
end

-- --------------------------------------------------- concentric_rings --

--- Rings of ticks, dashes and arcs turning at their own speeds about a
--- core. `spec`: `size` (160), `text` (fn -> string in the core), `label`,
--- `color`.
function M.concentric_rings(spec, style)
  local s = spec.size or 160
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local second = style.hatched and style.info or style.series(2)
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), spec.label or "Rings"))
  local r1, r2, r3, r4, r5 = c - 2, c * .8, c * .66, c * .52, c * .38
  local tk = style.hatched and 1 or 2
  -- Every ring is its own path, turning at its own speed.
  local function ring(r, period, back, props)
    props.d = circle(c, c, r)
    props.stroke_cap = props.stroke_cap or cap(style)
    return path(s, s, turning(props, r, period, back))
  end
  local function arcs(r, count, fill) local L = 2 * math.pi * r / count return { L * fill, L * (1 - fill) } end
  add(node,
    tick_ring(s, s, c, c, r1 - 5, r1, 90, { stroke_color = style.hatched and style.stroke_of("mark", color) or A(color, .45), thick = style.hatched and 1 or 1.5 }, 60000),
    tick_ring(s, s, c, c, r1 - 10, r1, 15, { stroke_color = style.hatched and style.stroke_of("mark", color) or A(color, .6), thick = style.hatched and 1 or 2 }, 60000),
    ring(r2, 22000, true, { stroke_color = A(color, .55), stroke_width = style.hatched and 2 or 4, dash = style.hatched and { 6, 4 } or { .01, 10 } }),
    ring(r3, 9000, false, { stroke_color = color, stroke_width = style.hatched and 2 or 5,
      dash = style.hatched and arcs(r3, 3, 70 / 120) or { 2 * math.pi * r3 / 3 * 70 / 120 - 5, 2 * math.pi * r3 / 3 * 50 / 120 + 5 } }),
    ring(r4, 30000, true, { stroke_color = A(second, .7), stroke_width = tk, dash = style.hatched and { 1, 4 } or { .01, 6 } }),
    ring(r5, 5000, false, { stroke_color = second, stroke_width = style.hatched and 1.5 or 3,
      dash = style.hatched and arcs(r5, 2, 140 / 180) or { 2 * math.pi * r5 / 2 * 140 / 180 - 3, 2 * math.pi * r5 / 2 * 40 / 180 + 3 } }))
  local core = c * .22
  if style.hatched then
    add(node, ui.Rect { x = c - core * .7, y = c - core * .7, width = core * 1.4, height = core * 1.4, rotation = 45, color = "transparent",
      border_width = 1, border_color = color, loop = { scale = { from = 1, to = .7, duration = 900, alternate = true, easing = "in_out_sine" } } })
  else
    add(node, ui.Item { anchors = { fill = true }, loop = spin(12000),
      path(100, 100, { x = c - core, y = c - core, width = core * 2, height = core * 2, d = geo.shape_path("cookie9", { size = 100 }),
        fill_color = A(color, .9) }) })
  end
  if spec.text then
    add(node, mono(style, { text = spec.text, anchors = { center_in = true }, font_size = style.size.small - 1, font_weight = 700,
      color = style.hatched and color or style.on_accent }))
  end
  return node
end

-- -------------------------------------------------------- decode_text --

local SYMBOLS = { "!", "<", ">", "-", "_", "/", "[", "]", "{", "}", "=", "+", "*", "^", "?", "#", "0", "1", "7", "A", "F", "X" }

--- Text that decodes out of scrambled symbols, a letter at a time, behind
--- a cursor (Tsugumori's title decode, as a widget). It decodes when shown
--- and whenever its text changes; `every` (ms) decodes it again. `spec`:
--- `text` (string or fn), `font_size` (large), `width` (fits the text),
--- `every`, `color`.
function M.decode_text(spec, style)
  local size = math.max(spec.font_size or style.size.large, style.size.small - 3)
  local color = U.color(spec, style, style.hatched and "accent" or "ink")
  local function final() return cased(style, get(spec.text)) end
  local tick, lead, stagger = 40, 220, 45
  local label = mono(style, { text = "", font_size = size, font_weight = style.hatched and 500 or 600, color = color,
    accessible_hidden = true, height = math.ceil(size * 1.4), vertical_alignment = "center", width = spec.width,
    elide = spec.width and "right" or nil })
  local id = name("decode")
  local cursor = ui.Rect { y = math.floor(size * .25), width = style.hatched and math.floor(size * .55) or 2,
    height = math.ceil(size * .95), radius = style.hatched and 0 or 1, color = style.hatched and color or style.accent,
    x = function() return (label.layout_width or 0) + 3 end,
    loop = { opacity = { from = 1, to = 0, duration = 530, alternate = true, easing = "in_out_quad" } } }
  local node = ui.Item(U.place(spec, { width = spec.width or function() return (label.layout_width or 0) + size end, height = math.ceil(size * 1.4),
    accessible_role = "label", accessible_name = final }))
  add(node, label, cursor)
  -- Tsugumori's split-colour ghosts flash once as a decode lands.
  local ga, gb
  if style.hatched then
    ga = mono(style, { text = "", font_size = size, font_weight = 500, color = style.info, opacity = 0, z = -1, accessible_hidden = true,
      height = math.ceil(size * 1.4), vertical_alignment = "center" })
    gb = mono(style, { text = "", font_size = size, font_weight = 500, color = style.alert, opacity = 0, z = -1, accessible_hidden = true,
      height = math.ceil(size * 1.4), vertical_alignment = "center" })
    add(node, ga, gb)
  end
  local letters, elapsed, duration, shown, target = {}, 0, 0, nil, ""
  local timer
  local function paint()
    local out = {}
    for i, ch in ipairs(letters) do
      if ch:match("%s") or elapsed >= lead + (i - 1) * stagger then out[i] = ch
      else out[i] = SYMBOLS[(i * 7 + math.floor(elapsed / tick) * 11) % #SYMBOLS + 1] end
    end
    local value = table.concat(out)
    if value ~= shown then shown = value label.text = value end
  end
  local function finish()
    timer.running = false
    label.text, shown = target, target
    if ga then
      ga.text, gb.text = target, target
      morf.animation.play { { parallel = {
        { node = ga, property = "translate_x", from = -3, to = 0, duration = 160, easing = "out_cubic" },
        { node = gb, property = "translate_x", from = 3, to = 0, duration = 160, easing = "out_cubic" },
        { node = ga, property = "opacity", duration = 160, keyframes = { { at = 0, value = 0 }, { at = .25, value = .7 }, { at = 1, value = 0 } } },
        { node = gb, property = "opacity", duration = 160, keyframes = { { at = 0, value = 0 }, { at = .25, value = .7 }, { at = 1, value = 0 } } },
      } } }
    end
  end
  local function start()
    letters = {}
    for _, code in utf8.codes(target) do letters[#letters + 1] = utf8.char(code) end
    elapsed, duration = 0, lead + math.max(0, #letters - 1) * stagger
    paint()
    timer.running = true
  end
  timer = ui.Timer { interval = tick, ["repeat"] = true, running = false, on_triggered = function()
    elapsed = elapsed + tick
    if elapsed >= duration then finish() else paint() end
  end }
  add(node, timer)
  if spec.every then
    add(node, ui.Timer { interval = spec.every, ["repeat"] = true, running = true, on_triggered = start })
  end
  morf.effect(id, function()
    local value = final()
    if value == target and shown ~= nil then return end
    target = value
    start()
  end, { owner = node })
  return node
end

-- -------------------------------------------------------- glitch_text --

--- Text that glitches now and then: offset colour copies jitter apart and
--- a slice of it tears sideways, for a quarter second. `spec`: `text`,
--- `font_size` (large), `every` (ms between glitches, 2600), `color`.
function M.glitch_text(spec, style)
  local size = math.max(spec.font_size or style.size.large, style.size.small - 3)
  local color = U.color(spec, style, "ink")
  local value = function() return cased(style, get(spec.text)) end
  local lh = math.ceil(size * 1.4)
  local weight_ = style.hatched and 500 or 700
  local function copy(c, props)
    props = props or {}
    props.text, props.font_size, props.font_weight, props.color, props.height = value, size, weight_, c, lh
    props.vertical_alignment = "center"
    return mono(style, props)
  end
  local base = copy(color)
  local a = copy(style.hatched and style.info or style.series(2), { opacity = 0, z = -1, accessible_hidden = true })
  local b = copy(style.hatched and style.alert or style.series(3), { opacity = 0, z = -1, accessible_hidden = true })
  local sy, sh = math.floor(lh * .4), math.max(3, math.floor(lh * .22))
  local slice = ui.Item { y = sy, height = sh, width = function() return (base.layout_width or 0) end, clip = true, opacity = 0,
    accessible_hidden = true, copy(style.hatched and style.accent or style.series(1), { y = -sy }) }
  local node = ui.Item(U.place(spec, { width = function() return (base.layout_width or 0) + 6 end, height = lh }))
  add(node, a, b, base, slice)
  local quick = 260
  morf.animation.play { loops = "forever", { pause = spec.every or 2600 }, { parallel = {
    { node = a, property = "translate_x", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .2, value = -4 }, { at = .45, value = 2 }, { at = .7, value = -2 }, { at = 1, value = 0 } } },
    { node = b, property = "translate_x", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .2, value = 3 }, { at = .45, value = -3 }, { at = .7, value = 1 }, { at = 1, value = 0 } } },
    { node = a, property = "opacity", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .1, value = .85 }, { at = .8, value = .7 }, { at = 1, value = 0 } } },
    { node = b, property = "opacity", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .1, value = .85 }, { at = .8, value = .7 }, { at = 1, value = 0 } } },
    { node = slice, property = "translate_x", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .3, value = 7 }, { at = .55, value = -5 }, { at = .8, value = 3 }, { at = 1, value = 0 } } },
    { node = slice, property = "opacity", duration = quick, keyframes = { { at = 0, value = 0 }, { at = .08, value = 1 }, { at = .9, value = 1 }, { at = 1, value = 0 } } },
    { node = base, property = "opacity", duration = quick, keyframes = { { at = 0, value = 1 }, { at = .35, value = .55 }, { at = .5, value = 1 }, { at = 1, value = 1 } } },
  } } }
  return node
end

-- ----------------------------------------------------------- hex_grid --

--- A field of flat-topped hexagons, a few lit and pulsing. `spec`:
--- `width` (240), `height` (160), `radius` (cell's, 15), `lit` (a list of
--- cell numbers, counted row by row from 1; a few by default), `color`.
function M.hex_grid(spec, style)
  local w, h = spec.width or 240, spec.height or 160
  local R = spec.radius or 15
  local color = U.color(spec, style, "accent")
  local hh = math.sqrt(3) * R / 2              -- half a cell's height
  local cols = math.floor((w - R / 2) / (1.5 * R))
  local rows = math.floor((h - hh) / (2 * hh))
  local ox = (w - ((cols - 1) * 1.5 * R + 2 * R)) / 2 + R
  local oy = (h - (rows * 2 * hh + hh)) / 2 + hh
  local inset = style.hatched and 1.5 or 2.5
  local function hex(cx, cy, r)
    local d = {}
    for k = 0, 5 do
      local a = math.rad(60 * k)
      d[#d + 1] = ("%s%.2f %.2f"):format(k == 0 and "M" or "L", cx + r * math.cos(a), cy + r * math.sin(a))
    end
    return table.concat(d, " ") .. " Z"
  end
  local cells, all = {}, {}
  for row = 0, rows - 1 do
    for col = 0, cols - 1 do
      local cx, cy = ox + col * 1.5 * R, oy + row * 2 * hh + (col % 2 == 1 and hh or 0)
      cells[#cells + 1] = { cx, cy }
      all[#all + 1] = hex(cx, cy, R - inset)
    end
  end
  local lit = spec.lit
  if not lit then
    lit = {}
    local rand = rng(7)
    for _ = 1, math.max(3, math.floor(#cells / 9)) do lit[#lit + 1] = 1 + math.floor(rand() * #cells) end
  end
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Hex grid"))
  if style.hatched then
    add(node, path(w, h, { d = table.concat(all, " "), stroke_color = style.stroke_of("quiet", color), stroke_width = 1 }))
  else
    add(node, path(w, h, { d = table.concat(all, " "), fill_color = style.track, stroke_color = style.track, stroke_width = 3, stroke_join = "round" }))
  end
  for i, k in ipairs(lit) do
    local cell = cells[k]
    if cell then
      local tone = (i % 4 == 0) and style.alert or color
      local d = hex(cell[1], cell[2], R - inset)
      local pulse = { opacity = { from = 1, to = style.hatched and .3 or .45, duration = 900 + 230 * i, alternate = true, easing = "in_out_sine", delay = 170 * i } }
      if style.hatched then
        add(node, path(w, h, { d = d, fill_color = A(tone, .22), stroke_color = tone, stroke_width = 1, loop = pulse }))
      else
        add(node, path(w, h, { d = d, fill_color = tone, stroke_color = tone, stroke_width = 3, stroke_join = "round", loop = pulse }))
      end
      if style.hatched then
        add(node, ui.Rect { x = cell[1] - 1.5, y = cell[2] - 1.5, width = 3, height = 3, color = tone })
      end
    end
  end
  return node
end

-- ----------------------------------------------------- countdown_ring --

--- A ring of ticks that drains as the seconds run out, the time in its
--- centre. `spec`: `size` (150), `total` (60 seconds), `seconds` (number
--- or fn: what is left), or, without it, a countdown of its own from
--- `start` (total) that stops at zero (`loop = true` starts it again);
--- `ticks` (60), `label`, `color`. The last tenth turns to alert.
function M.countdown_ring(spec, style)
  local s = spec.size or 150
  local c = s / 2
  local total = spec.total or 60
  local color = U.color(spec, style, "accent")
  local node = ui.Item(U.place(spec, { width = s, height = s }))
  local left
  if spec.seconds ~= nil then
    left = function() return math.max(0, tonumber(get(spec.seconds)) or 0) end
  else
    local state = morf.signal(name("countdown"), spec.start or total)
    left = function() return state:get() end
    add(node, ui.Timer { interval = 1000, ["repeat"] = true, running = true, on_triggered = function()
      local t = state:get() - 1
      if t < 0 then t = spec.loop and total or 0 end
      state:set(t)
    end })
  end
  local frac = function() return clamp01(left() / total) end
  node.accessible_role = "timer"
  node.accessible_name = spec.label or "Countdown"
  node.accessible = function() return { value = math.floor(left()), minimum = 0, maximum = total } end
  local tone = function() return get(frac() <= .1 and style.alert or color) end
  local n = spec.ticks or 60
  local angles = {}
  for i = 1, n do angles[i] = (i - 1) * 360 / n end
  local r1 = c - 2
  local r0 = r1 - (style.hatched and 8 or 9)
  local ticks = geo.ticks(c, c, r0, r1, { angles = angles, major = style.hatched and 5 or 0, major_r0 = r0 - 4 })
  local q = steps(frac, n)
  local sw = style.hatched and 1.2 or 3
  add(node, path(s, s, { d = ticks, stroke_color = A(color, style.hatched and .2 or .18), stroke_width = sw, stroke_cap = cap(style) }),
    path(s, s, { d = ticks, stroke_color = tone, stroke_width = sw, stroke_cap = cap(style),
      opacity = function() return q() > 0 and 1 or 0 end, trim_end = function() return math.max(.0001, q()) end,
      behavior = { trim_end = settle(style) } }))
  -- An inner arc follows the seconds smoothly; the head marks where it is.
  local ri = r0 - (style.hatched and 9 or 10)
  local arc = geo.arc(c, c, ri, 0, 359.99)
  add(node, path(s, s, { d = arc, stroke_color = A(color, .12), stroke_width = style.hatched and 1 or 4 }),
    path(s, s, { d = arc, stroke_color = tone, stroke_width = style.hatched and 1 or 4, stroke_cap = cap(style),
      trim_end = function() return math.max(.0001, frac()) end, behavior = { trim_end = { duration = 1000, easing = "linear" } } }),
    ui.Item { anchors = { fill = true }, rotation = function() return 360 * frac() end, behavior = { rotation = { duration = 1000, easing = "linear" } },
      ui.Rect { x = c - (style.hatched and 1 or 2), y = c - r1, width = style.hatched and 2 or 4, height = r1 - ri + 3, radius = style.hatched and 0 or 2,
        color = style.ink } })
  local fs = math.max(style.size.normal, math.floor(s * .2))
  local reading = function()
    local t = math.ceil(left())
    if total >= 60 then return ("%d:%02d"):format(math.floor(t / 60), t % 60) end
    return ("%02d"):format(t)
  end
  add(node, ui.Column { anchors = { center_in = true }, width = math.floor(ri * 1.6), gap = 0, align = "center",
    mono(style, { text = reading, font_size = fs, font_weight = style.hatched and 400 or 600, color = tone, horizontal_alignment = "center",
      height = math.ceil(fs * 1.2) }),
    s >= 110 and caption(style, { text = spec.label or "Remaining", horizontal_alignment = "center", width = math.floor(ri * 1.6) }) or nil,
  })
  return node
end

-- ------------------------------------------------ dot_matrix_progress --

--- Progress as a matrix of dots filling column by column, its leading
--- column flickering. `spec`: `width` (240), `columns` (30), `rows` (5),
--- `value` (0..1, function, channel), `label`, `color`.
function M.dot_matrix_progress(spec, style)
  local w = spec.width or 240
  local cols, rows = spec.columns or 30, spec.rows or 5
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value, .5)
  local pitch = w / cols
  local dot = pitch * (style.hatched and .55 or .62)
  local head = 20
  local gh = rows * pitch
  local node = ui.Item(meter(U.place(spec, { width = w, height = head + gh }), spec.label or "Progress", v))
  add(node, caption(style, { text = spec.label or "Progress", width = math.floor(w * .65) }),
    mono(style, { text = pct(v), anchors = { right = true }, width = math.floor(w * .35), height = 18, font_size = style.size.small,
      font_weight = 600, horizontal_alignment = "right", color = style.hatched and color or style.ink }))
  local d = {}
  for col = 0, cols - 1 do
    for row = 0, rows - 1 do
      local cx, cy = (col + .5) * pitch, (row + .5) * pitch
      if style.hatched then
        d[#d + 1] = ("M%.2f %.2f h%.2f v%.2f h%.2f Z"):format(cx - dot / 2, cy - dot / 2, dot, dot, -dot)
      else
        d[#d + 1] = ("M%.2f %.2f a%.2f %.2f 0 1 0 %.2f 0 a%.2f %.2f 0 1 0 %.2f 0 Z"):format(cx - dot / 2, cy, dot / 2, dot / 2, dot, dot / 2, dot / 2, -dot)
      end
    end
  end
  d = table.concat(d, " ")
  local q = steps(v, cols)
  add(node, path(w, gh, { y = head, d = d, fill_color = A(color, style.hatched and .14 or .16) }),
    ui.Item { y = head, height = gh, clip = true, width = function() return w * q() end, behavior = { width = travel(style, 140, 18) },
      path(w, gh, { d = d, fill_color = color }) },
    ui.Rect { y = head, width = pitch, height = gh, radius = style.hatched and 0 or pitch / 2, color = A(color, .35),
      x = function() return math.min(w - pitch, w * q()) end, behavior = { x = travel(style, 140, 18) },
      opacity = function() return q() < 1 and 1 or 0 end,
      loop = { scale_y = { from = 1, to = .6, duration = 420, alternate = true, easing = "in_out_sine" } } })
  return node
end

-- ----------------------------------------------------- biometric_scan --

--- A fingerprint read: ridges lighting up from the bottom as `value`
--- grows, a scan line combing over them, the reading under it. `spec`:
--- `width` (150), `height` (180), `value` (0..1, function, channel),
--- `label` ("Scanning"; "Verified" once full), `color`.
function M.biometric_scan(spec, style)
  local w, h = spec.width or 150, spec.height or 180
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value, .6)
  local foot = 24
  local area = h - foot
  local ph = area - 16
  local pw = math.floor(ph * .76)
  local px, py = math.floor((w - pw) / 2), 8
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), spec.label or "Biometric scan", v))
  -- The ridges: nested arches, broken where a print breaks.
  local cx, cy = pw / 2, ph * .56
  local rand = rng(11)
  local d = {}
  local function arcd(rx, ry, a0, a1)
    local seg, out = 18, {}
    for k = 0, seg do
      local t = math.rad(a0 + (a1 - a0) * k / seg)
      out[#out + 1] = ("%s%.2f %.2f"):format(k == 0 and "M" or "L", cx + rx * math.sin(t), cy - ry * math.cos(t))
    end
    return table.concat(out, " ")
  end
  local nr = 8
  for i = 1, nr do
    local rx = (pw / 2 - 3) * i / nr
    local ry = math.min(cy - 2, rx * 1.35)
    local span = 120 + i * 6
    local brk = -span + rand() * span * 2
    local gapw = 14 + rand() * 18
    if i > 2 then
      d[#d + 1] = arcd(rx, ry, -span, brk - gapw / 2)
      d[#d + 1] = arcd(rx, ry, brk + gapw / 2, span)
    else
      d[#d + 1] = arcd(rx, ry, -span, span)
    end
  end
  d = table.concat(d, " ")
  local sw = style.hatched and 1.2 or 2.4
  if style.hatched then
    add(node, path(w, area, { d = corners_d(px - 8, py - 6, pw + 16, ph + 12, 10, 1), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }))
  else
    add(node, ui.Rect { x = px - 10, y = py - 8, width = pw + 20, height = ph + 16, radius = 18, color = A(color, .08) })
  end
  add(node, path(pw, ph, { x = px, y = py, d = d, stroke_color = A(color, .25), stroke_width = sw, stroke_cap = cap(style), stroke_join = "round" }))
  add(node, ui.Item { x = px, y = py, width = pw, height = ph,
    ui.Item { anchors = { left = true, right = true, bottom = true }, clip = true,
      height = function() return ph * v() end, behavior = { height = travel(style, 120, 16) },
      path(pw, ph, { anchors = { bottom = true }, d = d, stroke_color = color, stroke_width = sw, stroke_cap = cap(style), stroke_join = "round" }) } })
  -- The comb: a bright line with a wake, up and down the print.
  add(node, ui.Item { x = px - 6, y = py, width = pw + 12, height = ph, clip = true,
    scanline(style, color, 0, 0, pw + 12, ph, 16, 1700, true) })
  local done = function() return v() >= .999 end
  add(node, ui.Row { y = h - foot + 4, anchors = { horizontal_center = true }, gap = 8, height = 20,
    mono(style, { text = pct(v), font_size = style.size.normal, font_weight = 600, height = 20, vertical_alignment = "center",
      color = function() return get(done() and style.ok or (style.hatched and color or style.ink)) end }),
    caption(style, { text = function() return done() and "Verified" or (spec.label or "Scanning") end, height = 20, vertical_alignment = "center",
      color = function() return get(done() and style.ok or style.ink_lo) end }) })
  return node
end

-- -------------------------------------------------------- data_stream --

--- Columns of hex words scrolling up at their own speeds, fading at the
--- edges. The words are made once; only the columns move. `spec`: `width`
--- (240), `height` (150), `font_size` (small - 2), `seed`, `color`.
function M.data_stream(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "accent")
  local fs = math.max(spec.font_size or style.size.small - 2, style.size.small - 3)
  local lh = math.floor(fs * 1.35)
  local colw = math.floor(fs * 3.6)
  local cols = math.max(1, math.floor((w - 16) / colw))
  local ox = math.floor((w - cols * colw) / 2)
  -- Each column holds its words twice over and glides up by one copy's
  -- height, forever: a translate of text shaped once.
  local lines = math.ceil(h / lh) + 1
  local rand = rng(spec.seed or 5)
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Data stream"))
  add(node, frame(style, w, h))
  local box = clip(style, w, h)
  for i = 1, cols do
    local words = {}
    for k = 1, lines do words[k] = ("%04X"):format(math.floor(rand() * 65536)) end
    local block = table.concat(words, "\n")
    local speed = 14 + rand() * 26              -- px a second
    local tone = (i % 3 == 0) and color or (i % 3 == 1 and style.ink_lo or A(color, .7))
    add(box, mono(style, { x = ox + (i - 1) * colw, y = 0, width = colw, text = block .. "\n" .. block, font_size = fs,
      line_height = lh .. "px", color = tone, horizontal_alignment = "center",
      loop = { translate_y = { from = 0, to = -lines * lh, duration = math.floor(lines * lh / speed * 1000), easing = "linear" } } }))
  end
  -- The columns fade into the frame at top and bottom (gradients over
  -- them in the frame's own tone -- no offscreen mask).
  local under = style.hatched and style.surface or style.raised
  local fade = math.floor(h * .2)
  add(box, ui.Rect { width = w, height = fade, gradient = function() local k = get(under) return { angle = 180, stops = { k, k:alpha(0) } } end },
    ui.Rect { y = h - fade, width = w, height = fade, gradient = function() local k = get(under) return { angle = 180, stops = { k:alpha(0), k } } end })
  -- A read head across the middle.
  local my = math.floor(h / 2 - lh / 2)
  add(box, ui.Rect { y = my, width = w, height = lh, color = A(color, style.hatched and .1 or .12) })
  if style.hatched then
    add(box, ui.Rect { y = my, width = w, height = 1, color = A(color, .5) }, ui.Rect { y = my + lh - 1, width = w, height = 1, color = A(color, .5) })
  end
  add(node, box)
  return node
end

-- ---------------------------------------------------- striped_loading --

--- A barber-pole bar: stripes running along it while something loads,
--- filled to `value` when it is known. `spec`: `width` (240), `height`
--- (12), `value` (0..1, function, channel; the whole bar when nil),
--- `label`, `color`.
function M.striped_loading(spec, style)
  local w = spec.width or 240
  local bh = spec.height or 12
  local color = U.color(spec, style, "accent")
  local v = spec.value ~= nil and reader(spec.value) or nil
  local head = spec.label and 20 or 0
  local props = U.place(spec, { width = w, height = head + bh })
  if v then meter(props, spec.label or "Loading", v) props.accessible_role = "progress"
  else props.accessible_role, props.accessible_name = "progress", spec.label or "Loading" end
  local node = ui.Item(props)
  if spec.label then
    add(node, caption(style, { text = spec.label, width = math.floor(w * .65) }))
    if v then
      add(node, mono(style, { text = pct(v), anchors = { right = true }, width = math.floor(w * .35), height = 18, font_size = style.size.small,
        font_weight = 600, horizontal_alignment = "right", color = style.hatched and color or style.ink }))
    end
  end
  local gap = math.max(8, math.floor(bh * 1.3))
  -- The stripes: a path a few stripes wider than the bar, sliding one
  -- stripe along and back again forever inside the bar's clip.
  local pw = w + gap * 4
  local d = geo.hatch(pw, bh, gap)
  local function stripes(props, period)
    props.d, props.stroke_cap = d, "butt"
    return ui.Item { x = -gap * 2, width = pw, height = bh,
      loop = { translate_x = { from = 0, to = gap, duration = period, easing = "linear" } }, path(pw, bh, props) }
  end
  local fillw = v and function() return w * v() end or w
  if style.hatched then
    add(node, ui.Rect { y = head, width = w, height = bh, color = "transparent", border_width = 1, border_color = style.stroke_of("idle", color) })
    add(node, ui.Item { y = head, height = bh, width = fillw, behavior = v and { width = settle(style) } or nil, clip = true,
      ui.Rect { anchors = { fill = true }, color = A(color, .16) },
      stripes({ stroke_color = color, stroke_width = math.max(2, gap * .4) }, 520) })
    add(node, ui.Rect { y = head - 2, width = 2, height = bh + 4, color = color })
    add(node, ui.Rect { y = head - 2, width = 2, height = bh + 4, color = color,
      x = v and function() return math.max(0, w * v() - 2) end or w - 2, behavior = v and { x = settle(style) } or nil })
  else
    add(node, ui.Rect { y = head, width = w, height = bh, radius = bh / 2, color = style.track })
    add(node, ui.ClipRect { y = head, height = bh, radius = bh / 2, color = "transparent",
      width = v and function() return math.max(bh, w * v()) end or w, behavior = v and { width = travel(style, 160, 18) } or nil,
      ui.Rect { anchors = { fill = true }, color = color },
      stripes({ stroke_color = A(style.on_accent, .28), stroke_width = gap * .45 }, 640) })
  end
  return node
end

-- ------------------------------------------------------- signal_noise --

--- Static: a field of cells re-rolled a few times a second (a data
--- channel drawn as cells -- no node per cell), a rolling bar and an
--- optional message. `spec`: `width` (240), `height` (150), `cell` (6),
--- `rate` (ms between rolls, 110), `text` ("No signal"), `color`.
function M.signal_noise(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "ink")
  local cell = spec.cell or 6
  local cols, rows = math.floor(w / cell), math.floor(h / cell)
  local gw, gh = cols * cell, rows * cell
  local ch = morf.channel { size = cols * rows, mode = "frame" }
  local rand = rng(spec.seed or 3)
  local function roll()
    local t = {}
    for i = 1, cols * rows do t[i] = rand() end
    ch:set(t)
  end
  roll()
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "No signal"))
  add(node, frame(style, w, h))
  local box = clip(style, w, h)
  local radius = style.hatched and 0 or 1.2
  for _, band in ipairs { { .55, .8, .14 }, { .8, .93, .3 }, { .93, 1, .55 } } do
    add(box, ui.Path { x = (w - gw) / 2, y = (h - gh) / 2, width = gw, height = gh, view_box = { 0, 0, gw, gh }, series = ch.id,
      plot = { kind = "cells", rows = rows, columns = cols, gap = 1, lo = band[1], hi = band[2], radius = radius },
      fill_color = A(color, band[3]) })
  end
  -- A rolling bar, through the box and out of it (clipped).
  add(box, ui.Rect { width = w, height = math.floor(h * .22), y = -math.floor(h * .22),
    gradient = function() local k = get(color) return { angle = 180, stops = { k:alpha(0), k:alpha(.12), k:alpha(0) } } end,
    loop = { translate_y = { from = 0, to = h + math.floor(h * .22), duration = 3200, easing = "linear" } } })
  add(box, ui.Timer { interval = spec.rate or 110, ["repeat"] = true, running = true, on_triggered = roll })
  add(node, box)
  local message = spec.text == nil and "No signal" or spec.text
  if message and message ~= "" then
    local tag = M.bracket_tag({ text = message, color = "alert" }, style)
    add(node, ui.Item { anchors = { center_in = true }, width = function() return tag.width end, height = tag.height,
      ui.Rect { anchors = { fill = true, margins = -4 }, color = style.surface, radius = style.hatched and 0 or 17 }, tag })
  end
  return node
end

-- ------------------------------------------------------ crt_scanlines --

--- A CRT over whatever it holds: scan lines, a rolling bright band and a
--- vignette at the edges. `spec`: `width` (240), `height` (150), `pitch`
--- (3 px between lines); children are drawn under it.
function M.crt_scanlines(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local pitch = spec.pitch or 3
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Display"))
  local box = clip(style, w, h)
  add(box, ui.Rect { anchors = { fill = true }, color = style.surface })
  for _, child in ipairs(spec) do add(box, child) end
  local d = {}
  for y = 0, h, pitch do d[#d + 1] = ("M0 %d.5 H%d"):format(y, w) end
  add(box, path(w, h, { d = table.concat(d, " "), stroke_color = A(style.surface, .55), stroke_width = 1 }))
  local band = math.floor(h * .25)
  add(box, ui.Rect { y = -band, width = w, height = band,
    gradient = function() local k = get(style.ink) return { angle = 180, stops = { k:alpha(0), k:alpha(.07), k:alpha(0) } } end,
    loop = { translate_y = { from = 0, to = h + band, duration = 4600, easing = "linear" } } })
  add(box, ui.Rect { anchors = { fill = true },
    gradient = function() local k = get(style.surface) return { kind = "radial", stops = { { k:alpha(0), .55 }, { k:alpha(.85), 1 } } } end })
  add(node, box)
  if style.hatched then
    add(node, path(w, h, { d = corners_d(0, 0, w, h, 8, 1), stroke_color = style.stroke_of("mark"), stroke_width = 1 }))
  end
  return node
end

-- ----------------------------------------------------- crosshair_grid --

--- A plotting grid: minor and major lines, a centre cross, coordinates
--- along two edges, a marker at `at` and its readout. `spec`: `width`
--- (240), `height` (160), `step` (16 px), `major` (4 steps), `at` ({ x, y }
--- as -1..1 from the centre, or a function of one), `color`.
function M.crosshair_grid(spec, style)
  local w, h = spec.width or 240, spec.height or 160
  local color = U.color(spec, style, "accent")
  local step, major = spec.step or 16, spec.major or 4
  local fs = style.size.small - 3
  local gx, gy = 22, 16                         -- room for the coordinates
  local gw, gh = w - gx, h - gy
  local cx, cy = gx + gw / 2, gy + gh / 2
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Grid"))
  local minors, majors = {}, {}
  local kx = math.floor(gw / 2 / step)
  for k = -kx, kx do
    local x = cx + k * step
    local list = (k % major == 0) and majors or minors
    list[#list + 1] = ("M%.1f %d V%d"):format(x, gy, h)
    if k % major == 0 and k ~= 0 then
      add(node, mono(style, { text = tostring(k), x = x - 14, y = 0, width = 28, height = gy, font_size = fs, horizontal_alignment = "center",
        color = style.ink_lo, vertical_alignment = "center" }))
    end
  end
  local ky = math.floor(gh / 2 / step)
  for k = -ky, ky do
    local y = cy + k * step
    local list = (k % major == 0) and majors or minors
    list[#list + 1] = ("M%d %.1f H%d"):format(gx, y, w)
    if k % major == 0 and k ~= 0 then
      add(node, mono(style, { text = tostring(-k), x = 0, y = y - gy / 2, width = gx - 4, height = gy, font_size = fs, horizontal_alignment = "right",
        color = style.ink_lo, vertical_alignment = "center" }))
    end
  end
  if not style.hatched then
    add(node, ui.Rect { x = gx, y = gy, width = gw, height = gh, radius = 10, color = A(color, .05) })
  end
  add(node,
    path(w, h, { d = table.concat(minors, " "), stroke_color = style.hatched and style.stroke_of("faint", color) or A(color, .08), stroke_width = 1 }),
    path(w, h, { d = table.concat(majors, " "), stroke_color = style.hatched and style.stroke_of("quiet", color) or A(color, .2), stroke_width = 1 }),
    path(w, h, { d = ("M%.1f %d V%d M%d %.1f H%d"):format(cx, gy, h, gx, cy, w), stroke_color = A(color, .6), stroke_width = weight(style, 1.5) }))
  -- The marker: a reticle at `at`, pulsing.
  local at = function() local p = get(spec.at) or { .35, -.3 } return p end
  local mx = function() return cx + (at()[1] or at().x or 0) * gw / 2 end
  local my = function() return cy - (at()[2] or at().y or 0) * gh / 2 end
  add(node, path(w, h, { d = ("M%.1f %.1f m-10 0 h7 m6 0 h7 M%.1f %.1f m0 -10 v7 m0 6 v7"):format(cx, cy, cx, cy), stroke_color = color, stroke_width = weight(style, 2), stroke_cap = cap(style) }))
  add(node, ui.Item { width = 18, height = 18, x = function() return mx() - 9 end, y = function() return my() - 9 end,
    behavior = { x = travel(style, 160, 16), y = travel(style, 160, 16) },
    ui.Rect { anchors = { fill = true }, radius = style.hatched and 0 or 9, color = "transparent", border_width = weight(style, 2), border_color = style.alert,
      rotation = style.hatched and 45 or 0, loop = { scale = { from = 1, to = .6, duration = 700, alternate = true, easing = "in_out_sine" } } },
    ui.Rect { x = 7.5, y = 7.5, width = 3, height = 3, radius = style.hatched and 0 or 1.5, color = style.alert } })
  add(node, mono(style, { text = function() local p = at() return ("X%+.2f Y%+.2f"):format(p[1] or p.x or 0, p[2] or p.y or 0) end,
    x = gx + 4, y = h - fs - 8, font_size = fs, color = style.hatched and color or style.ink_lo, height = fs + 6 }))
  return node
end

-- ------------------------------------------------------------ callout --

--- A leader from a point to a label box: a pinging dot, an elbowed line
--- that draws itself on, the box with a title and a value. `spec`:
--- `width` (240), `height` (150), `point` ({ x, y } fractions, {.18, .8}),
--- `title`, `value` (string or fn), `color`.
function M.callout(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "accent")
  local p = spec.point or { .18, .8 }
  local px, py = math.floor(w * (p[1] or p.x)), math.floor(h * (p[2] or p.y))
  local bw, bh = math.min(spec.box_width or 150, w - 40), 54
  local bx, by = w - bw, 0
  local ey = by + bh / 2
  local ex = math.min(bx - 10, px + math.abs(py - ey))
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.title or "Callout"))
  local leader = path(w, h, { d = ("M%d %d L%.1f %.1f H%d"):format(px, py, ex, ey, bx), stroke_color = color,
    stroke_width = weight(style, 2.5), stroke_cap = cap(style), stroke_join = style.hatched and "miter" or "round" })
  add(node, leader)
  morf.animation.play { { node = leader, property = "trim_end", from = 0, to = 1, duration = 700, easing = "out_cubic" } }
  if style.hatched then
    add(node, ui.Rect { x = ex - 2, y = ey - 2, width = 4, height = 4, color = color })
  end
  -- The point and its ping.
  add(node, ui.Rect { x = px - 12, y = py - 12, width = 24, height = 24, radius = style.hatched and 0 or 12, color = "transparent",
    border_width = weight(style, 2), border_color = color, rotation = style.hatched and 45 or 0,
    loop = { scale = { from = .3, to = 1, duration = 1500, easing = "out_quad" }, opacity = { from = 1, to = 0, duration = 1500, easing = "in_quad" } } })
  add(node, ui.Rect { x = px - 4, y = py - 4, width = 8, height = 8, radius = style.hatched and 0 or 4, color = color,
    shadow_color = (not style.hatched) and A(color, .7) or nil, shadow_blur = (not style.hatched) and 8 or nil })
  -- The box.
  local box = ui.Item { x = bx, y = by, width = bw, height = bh }
  if style.hatched then
    add(box, ui.Rect { anchors = { fill = true }, color = A(color, .08), border_width = 1, border_color = style.stroke_of("idle", color) },
      path(bw, bh, { d = corners_d(0, 0, bw, bh, 7, 1), stroke_color = color, stroke_width = 1 }),
      ui.Rect { x = 0, y = 0, width = 3, height = bh, color = color })
  else
    add(box, ui.Rect { anchors = { fill = true }, radius = 16, color = style.raised, border_width = 2, border_color = A(color, .5) })
  end
  add(box, ui.Column { x = 12, y = 7, width = bw - 20, gap = 0,
    caption(style, { text = spec.title or "Target", width = bw - 20, elide = "right" }),
    mono(style, { text = spec.value or "—", font_size = style.size.small, font_weight = 600, width = bw - 20, elide = "right",
      color = style.hatched and color or style.ink, height = math.ceil(style.size.small * 1.6) }) })
  add(node, box)
  return node
end

-- ---------------------------------------------------------- wireframe --

local PHI = (1 + math.sqrt(5)) / 2
local SOLIDS = {}
local function solid(kind)
  if SOLIDS[kind] then return SOLIDS[kind] end
  local v = {}
  if kind == "cube" then
    for _, x in ipairs { -1, 1 } do for _, y in ipairs { -1, 1 } do for _, z in ipairs { -1, 1 } do v[#v + 1] = { x, y, z } end end end
  elseif kind == "octahedron" then
    v = { { 1, 0, 0 }, { -1, 0, 0 }, { 0, 1, 0 }, { 0, -1, 0 }, { 0, 0, 1 }, { 0, 0, -1 } }
  else
    for _, a in ipairs { -1, 1 } do for _, b in ipairs { -PHI, PHI } do
      v[#v + 1] = { 0, a, b } v[#v + 1] = { a, b, 0 } v[#v + 1] = { b, 0, a }
    end end
  end
  -- Unit radius; edges join the nearest pairs.
  local best = math.huge
  for _, p in ipairs(v) do
    local l = math.sqrt(p[1] ^ 2 + p[2] ^ 2 + p[3] ^ 2)
    p[1], p[2], p[3] = p[1] / l, p[2] / l, p[3] / l
  end
  local function dist(a, b) return math.sqrt((a[1] - b[1]) ^ 2 + (a[2] - b[2]) ^ 2 + (a[3] - b[3]) ^ 2) end
  for i = 1, #v do for j = i + 1, #v do best = math.min(best, dist(v[i], v[j])) end end
  local e = {}
  for i = 1, #v do for j = i + 1, #v do if dist(v[i], v[j]) < best * 1.01 then e[#e + 1] = { i, j } end end end
  SOLIDS[kind] = { v = v, e = e }
  return SOLIDS[kind]
end

--- A wireframe solid turning in space, its far edges dimmed. A 3-D turn
--- is no 2-D transform, so it re-projects its vertices on a 25 Hz timer
--- of its own (ends with the widget). `spec`: `size` (150), `shape`
--- ("icosahedron", "cube", "octahedron"), `period` (ms a turn, 9000),
--- `label`, `color`.
function M.wireframe(spec, style)
  local s = spec.size or 150
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local shape = solid(spec.shape or "icosahedron")
  local period = spec.period or 9000
  local R = c * .62
  local tilt = math.rad(spec.tilt or 22)
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), spec.label or "Wireframe"))
  -- The floor it turns over.
  if style.hatched then
    add(node, tick_ring(s, s, c, c, c - 6, c - 1, 72, { stroke_color = style.stroke_of("quiet", color) }, 30000),
      tick_ring(s, s, c, c, c - 10, c - 1, 12, { stroke_color = style.stroke_of("quiet", color) }, 30000))
    add(node, path(s, s, { d = ("M%g %g A%g %g 0 1 0 %g %g A%g %g 0 1 0 %g %g"):format(c - R, c + R * 1.05, R, R * .22, c + R, c + R * 1.05, R, R * .22, c - R, c + R * 1.05),
      stroke_color = style.stroke_of("quiet", color), stroke_width = 1, dash = { 2, 3 } }))
  else
    add(node, ui.Rect { x = c - R, y = c + R * .85, width = 2 * R, height = R * .4, radius = R * .2,
      gradient = function() local k = get(color) return { kind = "radial", stops = { k:alpha(.28), k:alpha(0) } } end })
  end
  local back = path(s, s, { d = "M0 0", stroke_color = A(color, style.hatched and .3 or .25), stroke_width = weight(style, 1.5),
    dash = style.hatched and { 2, 3 } or nil, stroke_cap = cap(style) })
  local front = path(s, s, { d = "M0 0", stroke_color = color, stroke_width = weight(style, 2.2), stroke_cap = cap(style), stroke_join = "round" })
  local dots = path(s, s, { d = "M0 0", fill_color = style.hatched and color or style.ink })
  add(node, back, front, dots)
  local angle = 0
  local function project()
    local ca, sa, ct, st = math.cos(angle), math.sin(angle), math.cos(tilt), math.sin(tilt)
    local P = {}
    for i, p in ipairs(shape.v) do
      local x = p[1] * ca + p[3] * sa
      local z = -p[1] * sa + p[3] * ca
      local y = p[2] * ct - z * st
      z = p[2] * st + z * ct
      local f = 3.2 / (3.2 + z)
      P[i] = { c + x * R * f, c + y * R * f, z }
    end
    local fd, bd, dd = {}, {}, {}
    for _, e in ipairs(shape.e) do
      local a, b = P[e[1]], P[e[2]]
      local seg = ("M%.1f %.1f L%.1f %.1f"):format(a[1], a[2], b[1], b[2])
      if a[3] + b[3] > 0 then bd[#bd + 1] = seg else fd[#fd + 1] = seg end
    end
    local r = style.hatched and 1.5 or 2
    for _, q in ipairs(P) do
      if q[3] <= 0 then
        if style.hatched then dd[#dd + 1] = ("M%.1f %.1f h3 v3 h-3 Z"):format(q[1] - r, q[2] - r)
        else dd[#dd + 1] = ("M%.1f %.1f a2 2 0 1 0 4 0 a2 2 0 1 0 -4 0 Z"):format(q[1] - r, q[2]) end
      end
    end
    front.d, back.d, dots.d = table.concat(fd, " "), table.concat(bd, " "), #dd > 0 and table.concat(dd, " ") or "M0 0"
  end
  project()
  local dt = 40
  add(node, ui.Timer { interval = dt, ["repeat"] = true, running = true, on_triggered = function()
    angle = (angle + 2 * math.pi * dt / period) % (2 * math.pi)
    project()
  end })
  return node
end

-- ---------------------------------------------------- telemetry_block --

--- Label/value rows in a framed block; a marker beside a value flashes
--- when it changes, and a live pip blinks in the header. `spec`: `width`
--- (240), `title`, `rows` ({ { label, value (string or fn) }, ... }),
--- `color`.
function M.telemetry_block(spec, style)
  local w = spec.width or 240
  local color = U.color(spec, style, "accent")
  local rows = spec.rows or {}
  local rh, head = 22, 28
  local h = head + #rows * rh + 8
  local node = ui.Item(U.place(spec, { width = w, height = h, accessible_role = "group", accessible_name = spec.title or "Telemetry" }))
  add(node, frame(style, w, h))
  add(node, caption(style, { text = spec.title or "Telemetry", x = 12, y = 6, width = w - 70, color = style.hatched and color or style.ink_lo }),
    ui.Row { anchors = { right = true, right_margin = 12 }, y = 6, gap = 5, height = 16,
      pip(style, style.ok, { size = 6, y = 5 }),
      caption(style, { text = "Live", height = 16, vertical_alignment = "center", color = style.ok }) })
  add(node, ui.Rect { x = 12, y = head - 3, width = w - 24, height = 1, color = style.hatched and style.stroke_of("idle", color) or style.line })
  for i, row in ipairs(rows) do
    local y = head + (i - 1) * rh
    local value = row.value or row[2]
    local mark = ui.Rect { x = w - 16, y = y + 6, width = 4, height = rh - 12, radius = style.hatched and 0 or 2, color = color, opacity = .15 }
    add(node, caption(style, { text = row.label or row[1], x = 12, y = y + 2, width = math.floor((w - 24) * .5), height = rh - 2,
      vertical_alignment = "center" }),
      mono(style, { text = function() return tostring(get(value) or "") end, x = math.floor(w * .45), y = y, width = math.floor(w * .55) - 24, height = rh,
        font_size = style.size.small, font_weight = 500, horizontal_alignment = "right", vertical_alignment = "center",
        color = style.hatched and style.ink or style.ink }),
      mark)
    if style.hatched and i < #rows then
      add(node, path(w, 1, { y = y + rh, d = ("M12 .5 H%d"):format(w - 12), stroke_color = style.stroke_of("faint", color), stroke_width = 1, dash = { 2, 3 } }))
    end
    local previous
    morf.effect(name("telemetry"), function()
      local now = tostring(get(value) or "")
      if previous ~= nil and now ~= previous then
        morf.animation.play { { node = mark, property = "opacity", duration = 700, keyframes = {
          { at = 0, value = .15 }, { at = .1, value = 1 }, { at = .3, value = .3 }, { at = .45, value = 1 }, { at = 1, value = .15 } } } }
      end
      previous = now
    end, { owner = node })
  end
  return node
end

-- ----------------------------------------------------- motion_tracker --

--- A motion tracker: a half disc of range arcs a pulse runs out over, and
--- pings that flash as the pulse reaches them; the nearest's range under
--- it. `spec`: `width` (220), `pings` ({ { angle (-90..90), distance
--- (0..1) }, ... } or fn), `range` (metres at the rim, 30), `period`
--- (1800), `label`, `color`.
function M.motion_tracker(spec, style)
  local w = spec.width or 220
  local color = U.color(spec, style, "accent")
  local period = spec.period or 1800
  local R = math.floor(w / 2 - 3)
  local cx, cy = w / 2, R + 3
  local foot = 22
  local h = cy + 4 + foot
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Motion tracker"))
  if style.hatched then
    add(node, path(w, cy + 2, { d = geo.sector(cx, cy, 0, R, -90, 180), fill_color = A(color, .05), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }),
      path(w, cy + 2, { d = geo.ticks(cx, cy, R - 4, R, { from = -90, sweep = 180, count = 37, major = 6, major_r0 = R - 8 }), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }))
  else
    add(node, path(w, cy + 2, { d = geo.sector(cx, cy, 0, R, -90, 180), fill_color = A(color, .1), stroke_color = A(color, .1), stroke_width = 4, stroke_join = "round" }))
  end
  add(node, path(w, cy + 2, { d = geo.arc(cx, cy, R * 2 / 3, -90, 180) .. geo.arc(cx, cy, R / 3, -90, 180),
      stroke_color = style.hatched and style.stroke_of("quiet", color) or A(color, .25), stroke_width = weight(style, 1.5), dash = style.hatched and { 2, 3 } or nil }),
    path(w, cy + 2, { d = geo.ticks(cx, cy, R * .12, R - (style.hatched and 8 or 4), { angles = { -60, -30, 0, 30, 60 } }),
      stroke_color = style.hatched and style.stroke_of("faint", color) or A(color, .15), stroke_width = weight(style, 1.5), stroke_cap = cap(style) }))
  -- The pulse running out from the tracker.
  -- (A disc -- a plain rounded rect -- scaled about the tracker, its
  -- lower half clipped away.)
  add(node, ui.Item { width = w, height = cy, clip = true,
    ui.Rect { x = cx - R, y = cy - R, width = 2 * R, height = 2 * R, radius = R, color = A(color, style.hatched and .04 or .08),
      border_width = weight(style, 2.5), border_color = color,
      loop = { scale = { from = .04, to = 1, duration = period, easing = "out_quad" }, opacity = { from = 1, to = .1, duration = period, easing = "in_quad" } } } })
  local pings = get(spec.pings) or {}
  local near = 1
  for _, p in ipairs(pings) do
    local a, dist = math.rad(p.angle or p[1] or 0), clamp01(p.distance or p[2] or .5)
    near = math.min(near, dist)
    local x, y = cx + dist * R * math.sin(a), cy - dist * R * math.cos(a)
    local dot = ui.Item { x = x - 8, y = y - 8, width = 16, height = 16, opacity = .1,
      style.hatched and ui.Rect { x = 4, y = 4, width = 8, height = 8, color = "transparent", border_width = 1, border_color = color } or nil,
      ui.Rect { x = 5, y = 5, width = 6, height = 6, radius = style.hatched and 0 or 3, color = color,
        shadow_color = (not style.hatched) and A(color, .9) or nil, shadow_blur = (not style.hatched) and 10 or nil } }
    add(node, dot)
    -- Out-quad: the pulse reaches distance `d` at 1 - sqrt(1 - d) of its run.
    local reach = 1 - math.sqrt(1 - math.min(dist, .999))
    morf.animation.play { loops = "forever", delay = math.floor(reach * period),
      { node = dot, property = "opacity", duration = period, keyframes = {
        { at = 0, value = 1 }, { at = .5, value = .45 }, { at = 1, value = .1 } } } }
  end
  add(node, ui.Rect { x = cx - 4, y = cy - 4, width = 8, height = 8, radius = style.hatched and 0 or 4, color = color })
  local range = spec.range or 30
  add(node, caption(style, { text = spec.label or "Motion", y = h - foot + 2, width = math.floor(w * .6), height = 20, vertical_alignment = "center" }),
    mono(style, { text = #pings > 0 and ("%d m"):format(math.floor(near * range + .5)) or "—", anchors = { right = true }, y = h - foot + 2,
      width = math.floor(w * .4), height = 20, font_size = style.size.normal, font_weight = 600, horizontal_alignment = "right",
      vertical_alignment = "center", color = style.hatched and color or style.ink }))
  return node
end

-- ----------------------------------------------------- proximity_ring --

--- A threat ring: sectors round the bearer lit by how close or strong
--- what lies that way is; the hot ones pulse. `spec`: `size` (160),
--- `values` (a list of 0..1 per sector, clockwise from ahead, or fn),
--- `sectors` (the list's length, else 12), `label`, `color`.
function M.proximity_ring(spec, style)
  local s = spec.size or 160
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local values = function() return get(spec.values) or {} end
  local n = spec.sectors or #(get(spec.values) or {})
  if n < 3 then n = 12 end
  local r1 = c - (style.hatched and 9 or 3)
  local r0 = r1 * .58
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), spec.label or "Proximity"))
  local span = 360 / n
  for i = 1, n do
    local function at() return clamp01(values()[i] or 0) end
    local from = (i - 1) * span - span / 2
    local tone = function() local x = at() return get(zone(style, x, color)):alpha(x < .03 and (style.hatched and .06 or .1) or (.18 + .82 * x)) end
    local pulse = function() if at() < .75 then return nil end return { opacity = { from = 1, to = .45, duration = 520, alternate = true, easing = "in_out_sine" } } end
    if style.hatched then
      add(node, path(s, s, { d = geo.sector(c, c, r0, r1, from + 1.2, span - 2.4), fill_color = tone, loop = pulse,
        stroke_color = function() return get(zone(style, at(), color)):alpha(at() > .03 and 1 or .3) end, stroke_width = 1 }))
    else
      add(node, path(s, s, { d = geo.sector(c, c, r0 + 2, r1 - 2, from + 2.6, span - 5.2), fill_color = tone, stroke_color = tone,
        stroke_width = 4, stroke_join = "round", loop = pulse }))
    end
  end
  if style.hatched then
    add(node, path(s, s, { d = geo.ticks(c, c, r1 + 3, c - 1, { count = n * 3, major = 3, major_r0 = r1 + 1 }), stroke_color = style.stroke_of("mark", color), stroke_width = 1 }),
      path(s, s, { d = circle(c, c, r0 - 5), stroke_color = style.stroke_of("quiet", color), stroke_width = 1, dash = { 2, 3 } }))
  else
    add(node, ui.Rect { x = c - r0 + 6, y = c - r0 + 6, width = 2 * (r0 - 6), height = 2 * (r0 - 6), radius = r0 - 6, color = style.raised })
  end
  -- The bearer: a chevron pointing ahead.
  local k = r0 * .42
  add(node, path(s, s, { d = ("M%.1f %.1f L%.1f %.1f L%.1f %.1f L%.1f %.1f Z"):format(c, c - k, c + k * .7, c + k * .7, c, c + k * .3, c - k * .7, c + k * .7),
    fill_color = style.hatched and "transparent" or color, stroke_color = color, stroke_width = style.hatched and 1 or 3, stroke_join = "round" }))
  return node
end

-- -------------------------------------------------------- bracket_tag --

--- A label held in brackets: Tsugumori square `[ ]` hairlines with a
--- blinking pip; Material a tonal pill whose ends curve like `( )`.
--- `spec`: `text`, `color` (a role: "accent", "ok", "warn", "alert"),
--- `height` (26), `blink` (the pip, true).
function M.bracket_tag(spec, style)
  local h = spec.height or 26
  local color = U.color(spec, style, "accent")
  local label = text(style, { text = function() return cased(style, get(spec.text)) end, font_size = math.max(style.size.small - 2, math.floor(h * .5)),
    font_weight = 600, color = color, height = h, vertical_alignment = "center", letter_spacing = style.hatched and 1 or .3 })
  local bw = style.hatched and 6 or 9
  local lead = (spec.blink ~= false) and 12 or 0
  local W = function() return (label.layout_width or 0) + 2 * bw + 12 + lead end
  label.x = bw + 6 + lead
  local node = ui.Item(U.place(spec, { width = W, height = h, accessible_role = "status", accessible_name = function() return tostring(get(spec.text) or "") end }))
  if style.hatched then
    add(node, ui.Rect { x = 2, y = 3, height = h - 6, width = function() return W() - 4 end, color = A(color, .1) },
      path(bw, h, { d = ("M%g 0.5 H1 V%g H%g"):format(bw, h - .5, bw), stroke_color = color, stroke_width = 1.5, stroke_cap = "square" }),
      path(bw, h, { x = function() return W() - bw end, d = ("M0 0.5 H%g V%g H0"):format(bw - 1, h - .5), stroke_color = color, stroke_width = 1.5, stroke_cap = "square" }))
  else
    add(node, ui.Rect { anchors = { fill = true }, radius = h / 2, color = A(color, .16) },
      path(bw, h, { d = ("M%g 3 Q2 %g %g %g"):format(bw, h / 2, bw, h - 3), stroke_color = color, stroke_width = 2.2, stroke_cap = "round" }),
      path(bw, h, { x = function() return W() - bw end, d = ("M0 3 Q%g %g 0 %g"):format(bw - 2, h / 2, h - 3), stroke_color = color, stroke_width = 2.2, stroke_cap = "round" }))
  end
  if lead > 0 then add(node, pip(style, color, { size = 6, x = bw + 6, y = (h - 6) / 2 })) end
  add(node, label)
  return node
end

-- ------------------------------------------------------ orbit_diagram --

--- Bodies circling a centre on tilted elliptical orbits. Each body rides a
--- turning frame squashed into its ellipse and turned back upright, so it
--- moves with no Lua per frame. `spec`: `size` (180), `bodies` ({ { orbit
--- (0..1 of the radius), period (ms), size (px), phase (degrees), kind },
--- ... }), `flat` (the ellipses' height to width, .42), `tilt` (-16),
--- `label`, `color`.
function M.orbit_diagram(spec, style)
  local s = spec.size or 180
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local flat = spec.flat or .42
  local R = c - 6
  local bodies = spec.bodies or {
    { orbit = .42, period = 5200, size = 7, phase = 30 },
    { orbit = .64, period = 9800, size = 10, phase = 200, kind = "info" },
    { orbit = .94, period = 17000, size = 7, phase = 300, kind = "warn" },
  }
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), spec.label or "Orbits"))
  local plane = ui.Item { anchors = { fill = true }, rotation = spec.tilt or -16 }
  add(node, plane)
  for _, b in ipairs(bodies) do
    local a = R * (b.orbit or .5)
    local bb = a * flat
    -- Drawn tilted, not turned: the path keeps the widget's box.
    local t = math.rad(spec.tilt or -16)
    local ex, ey = a * math.cos(t), a * math.sin(t)
    local rot = spec.tilt or -16
    add(node, path(s, s, { d = ("M%.1f %.1f A%.1f %.1f %g 1 0 %.1f %.1f A%.1f %.1f %g 1 0 %.1f %.1f"):format(c - ex, c - ey, a, bb, rot, c + ex, c + ey, a, bb, rot, c - ex, c - ey),
      stroke_color = style.hatched and style.stroke_of("idle", color) or A(color, .3), stroke_width = weight(style, 1.5),
      dash = style.hatched and { 3, 3 } or nil }))
  end
  -- The centre.
  local core = math.floor(s * .085)
  if style.hatched then
    add(plane, ui.Rect { x = c - core / 2, y = c - core / 2, width = core, height = core, color = A(color, .2), border_width = 1, border_color = color,
      loop = spin(16000) },
      path(2 * core, 2 * core, { x = c - core, y = c - core, d = ("M0 %g H%g M%g 0 V%g"):format(core, 2 * core, core, 2 * core), stroke_color = color, stroke_width = 1 }))
  else
    add(plane, ui.Item { anchors = { fill = true }, loop = spin(20000),
      path(100, 100, { x = c - core, y = c - core, width = core * 2, height = core * 2, d = geo.shape_path("sunny", { size = 100 }), fill_color = color }) })
  end
  for _, b in ipairs(bodies) do
    local a = R * (b.orbit or .5)
    local d = b.size or 8
    local period, phase = b.period or 8000, b.phase or 0
    local tone = style[b.kind or "accent"] or color
    local dot = style.hatched
      and ui.Rect { anchors = { center_in = true }, width = d, height = d, color = tone, scale_y = 1 / flat }
      or ui.Rect { anchors = { center_in = true }, width = d, height = d, radius = d / 2, color = tone, scale_y = 1 / flat,
        shadow_color = A(tone, .7), shadow_blur = 6 }
    add(plane, ui.Item { anchors = { fill = true }, scale_y = flat,
      ui.Item { anchors = { fill = true }, loop = spin(period, false, phase),
        ui.Item { x = c - d, y = c - a - d, width = 2 * d, height = 2 * d, loop = spin(period, true, -phase), dot } } })
  end
  return node
end

-- ---------------------------------------------------------- starfield --

--- A drifting field of stars in three depths, the near ones faster and
--- brighter, a few twinkling: three paths that slide, nothing redrawn.
--- `spec`: `width` (240), `height` (150), `count` (stars a layer, 26),
--- `speed` (px a second for the nearest, 24), `seed`, `color`.
function M.starfield(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "ink")
  local count = spec.count or 26
  local speed = spec.speed or 24
  local rand = rng(spec.seed or 9)
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), spec.label or "Starfield"))
  local box = clip(style, w, h)
  add(box, ui.Rect { anchors = { fill = true }, color = style.hatched and style.surface or
    function() return get(style.surface):mix(get(style.accent), .06) end })
  local function star(x, y, r)
    if style.hatched then return ("M%.1f %.1f h%.1f v%.1f h%.1f Z"):format(x - r, y - r, 2 * r, 2 * r, -2 * r) end
    return ("M%.1f %.1f a%.1f %.1f 0 1 0 %.1f 0 a%.1f %.1f 0 1 0 %.1f 0 Z"):format(x - r, y, r, r, 2 * r, r, r, -2 * r)
  end
  -- Each depth is one path two widths wide -- every star has a twin a
  -- width to its right -- sliding left a width and over again.
  for _, layer in ipairs { { .7, .35, .3 }, { 1, .6, .6 }, { 1.5, 1, 1 } } do
    local d = {}
    for _ = 1, count do
      local x, y = rand() * w, rand() * h
      d[#d + 1] = star(x, y, layer[1]) .. " " .. star(x + w, y, layer[1])
    end
    add(box, ui.Item { width = 2 * w, height = h,
      loop = { translate_x = { from = 0, to = -w, duration = math.floor(w / (speed * layer[3]) * 1000), easing = "linear" } },
      path(2 * w, h, { d = table.concat(d, " "), fill_color = A(color, layer[2]) }) })
  end
  -- A few bright ones twinkling where they are.
  for i = 1, 4 do
    local x, y = 10 + rand() * (w - 20), 10 + rand() * (h - 20)
    local tone = i % 2 == 0 and style.accent or color
    if style.hatched then
      add(box, path(9, 9, { x = x - 4.5, y = y - 4.5, d = "M4.5 0 V9 M0 4.5 H9", stroke_color = tone, stroke_width = 1,
        loop = { opacity = { from = 1, to = .15, duration = 900 + 300 * i, alternate = true, easing = "in_out_sine" } } }))
    else
      add(box, path(100, 100, { x = x - 5, y = y - 5, width = 10, height = 10, d = geo.shape_path("sunny", { size = 100 }), fill_color = tone,
        loop = { opacity = { from = 1, to = .15, duration = 900 + 300 * i, alternate = true, easing = "in_out_sine" },
          scale = { from = 1, to = .5, duration = 900 + 300 * i, alternate = true, easing = "in_out_sine" } } }))
    end
  end
  add(node, box)
  if style.hatched then
    add(node, path(w, h, { d = corners_d(0, 0, w, h, 8, 1), stroke_color = style.stroke_of("mark"), stroke_width = 1 }))
  end
  return node
end

-- ------------------------------------------------------ assistant_orb --

--- A voice assistant's orb, swelling with the voice's `level`. Material: a
--- distance-field blob -- a core and three satellites orbiting it, merged
--- by a smooth union -- in a morphing halo; Tsugumori: a segmented ring
--- lit by the level round a pulsing diamond core. `spec`: `size` (150),
--- `level` (0..1, function, channel), `label` ("Listening"), `color`.
function M.assistant_orb(spec, style)
  local s = spec.size or 150
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local v = reader(spec.level, .5)
  local foot = spec.label ~= false and 22 or 0
  local node = ui.Item(meter(U.place(spec, { width = s, height = s + foot }), spec.label or "Assistant", v))
  local face = ui.Item { width = s, height = s }
  add(node, face)
  if not style.hatched then
    local second = style.series(3)
    -- The halo: a soft disc breathing with the voice.
    add(face, ui.Rect { x = 0, y = 0, width = s, height = s, radius = c,
      gradient = function() local k = get(color) return { kind = "radial", stops = { { k:alpha(.35), .35 }, { k:alpha(0), 1 } } } end,
      scale = function() return .75 + .25 * v() end, behavior = { scale = style.spring(120, 9) } })
    -- A ring outline morphing between two shapes, turning.
    add(face, ui.Item { anchors = { fill = true }, loop = spin(14000, true),
      path(100, 100, { x = c * .14, y = c * .14, width = s - c * .28, height = s - c * .28, d = geo.shape_path("cookie12", { size = 100 }),
        morph_to = geo.shape_path("soft_burst", { size = 100 }), stroke_color = A(color, .5), stroke_width = 2.5,
        loop = { morph_progress = { from = 0, to = 1, duration = 2600, alternate = true, easing = "in_out_sine" } } }) })
    -- The blob: a core and three satellites on turning frames, merged.
    local r0 = s * .2
    local core = ui.Item { x = c - r0, y = c - r0, width = 2 * r0, height = 2 * r0,
      scale = function() return .85 + .3 * v() end, behavior = { scale = style.spring(220, 10) } }
    add(face, core)
    local shapes = { ui.SdfShape { shape = "circle", track = core, fill_color = color } }
    for i, k in ipairs { { 2600, false, .62 }, { 3700, true, .5 }, { 5200, false, .56 } } do
      local rs = r0 * k[3]
      local sat = ui.Item { x = c - rs, y = c - r0 * 1.05 - rs, width = 2 * rs, height = 2 * rs,
        translate_y = function() return -r0 * .35 * v() end, behavior = { translate_y = style.spring(160, 9) } }
      add(face, ui.Item { anchors = { fill = true }, loop = spin(k[1], k[2], i * 120), sat })
      shapes[#shapes + 1] = ui.SdfShape { shape = "circle", track = sat, operation = "smooth_union", fill_color = i == 2 and second or color }
    end
    local field = { anchors = { fill = true }, blend = r0 * .7 }
    for _, sh in ipairs(shapes) do field[#field + 1] = sh end
    add(face, ui.Sdf(field))
    add(face, ui.Rect { x = c - r0 * .35, y = c - r0 * .55, width = r0 * .5, height = r0 * .3, radius = r0 * .15,
      color = function() return get(style.on_accent):alpha(.35) end, rotation = -25 })
  else
    local r = c - 6
    local n = 36
    local L = 2 * math.pi * r
    local gap = 3
    local seg = L / n - gap
    local ring = geo.arc(c, c, r, 0, 359.99)
    local lit = steps(v, n)
    -- The level ring holds still; the rings inside it turn.
    add(face, path(s, s, { d = ring, stroke_color = A(color, .14), stroke_width = 6, dash = { seg, gap } }),
      path(s, s, { d = ring, stroke_color = color, stroke_width = 6, dash = { seg, gap }, opacity = function() return lit() > 0 and 1 or 0 end,
        trim_end = function() return math.max(.0001, lit()) end, behavior = { trim_end = settle(style) } }),
      tick_ring(s, s, c, c, r - 14, r - 9, 48, { stroke_color = style.stroke_of("mark", color) }, 7000, true),
      tick_ring(s, s, c, c, r - 17, r - 9, 12, { stroke_color = style.stroke_of("mark", color) }, 7000, true),
      path(s, s, turning({ d = circle(c, c, r * .58), stroke_color = style.info, stroke_width = 1.5,
        dash = { math.pi * r * .58 / 2, math.pi * r * .58 / 2 } }, r * .58, 4000)))
    local k = r * .5
    add(face, ui.Item { x = c - k / 2, y = c - k / 2, width = k, height = k,
      scale = function() return .6 + .5 * v() end, behavior = { scale = settle(style) },
      ui.Rect { anchors = { fill = true }, rotation = 45, color = A(color, .18), border_width = 1, border_color = color,
        loop = { scale = { from = 1, to = .82, duration = 800, alternate = true, easing = "in_out_sine" } } },
      ui.Rect { x = k / 2 - 3, y = k / 2 - 3, width = 6, height = 6, color = color } })
  end
  if foot > 0 then
    add(node, ui.Row { y = s + 2, anchors = { horizontal_center = true }, gap = 6, height = 20,
      pip(style, color, { size = 6, y = 7 }),
      caption(style, { text = spec.label or "Listening", height = 20, vertical_alignment = "center", color = style.hatched and color or style.ink_lo }) })
  end
  return node
end

return M
