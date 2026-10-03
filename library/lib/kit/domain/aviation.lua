-- Domain instruments: aviation -- the flight deck's tapes, dials and
-- displays, drawn over the display widgets in a theme's style. See
-- lib.kit.display.
--
-- Every instrument lays out the same in each theme and draws in its style:
-- Material tonal, rounded and springy (a horizon that swings on a spring, a
-- flight path marker that squashes as it flies); Tsugumori square, on
-- hairlines and tick rulers, hatched, mono captions in upper case. Values
-- may be numbers or functions (or data channels where a stream fits), and
-- every one travels to where it is set: nothing in Lua runs per frame.
--
-- How things move without Lua per frame, and without redrawing a path:
-- what moves is drawn once and carried by a transform (`translate_*`,
-- `rotation`) inside a clipping window, so a frame of motion re-uses what
-- was drawn.
--   * a tape's ruler and figures are a short stretch of the scale on a
--     container that slides with the reading, re-based in whole steps
--     under it, so a few nodes cover any range;
--   * a drum's figures slide the same way, a long change spinning less
--     than a turn;
--   * a horizon's ground, line and ladder are distance fields whose shapes
--     track nodes turned and shifted by `rotation` and `translate_y`
--     behaviours, cut to the window by an intersection;
--   * a rose is a card of ticks and figures that turns.
local ui = require("morf.ui")
local morf = require("morf")
local U = require("lib.kit.display.util")
local channel = require("lib.channel")
local get = U.get
local geo = morf.geometry

local M = {}

-- ------------------------------------------------------------ shared --

local serial = 0
local function name(kind) serial = serial + 1 return ("kit.aviation.%s.%d"):format(kind, serial) end

local function settle(style) return { duration = style.motion.duration, easing = style.motion.easing } end
--- What a value travels with: a spring in Material, the theme's settle in Tsugumori.
local function travel(style, k, d)
  if style.hatched then return settle(style) end
  return style.spring(k or 170, d or 17)
end

local function clamp(x, a, b) if x < a then return a elseif x > b then return b end return x end
local function round(x) return math.floor((tonumber(x) or 0) + .5) end

--- A reader of `v`: a number, a function of one, or a channel (its newest).
local function reader(v, default)
  if channel.is(v) then return function() return tonumber(v:last()) or default or 0 end end
  return function() return tonumber(get(v)) or default or 0 end
end

--- Text in the style, never under the small size less three.
local function text(style, props)
  props.font_size = math.max(props.font_size or style.size.normal, style.size.small - 3)
  if props.color == nil then props.color = style.ink end
  return style.text(props)
end

--- Figures: the theme's mono face in Tsugumori, its own in Material.
local function figures(style, props)
  if style.hatched then props.font_family = props.font_family or style.mono_font end
  return text(style, props)
end

local function caption(style, props) return U.caption(style, props) end

--- A reading to assistive technology: a meter, its value and range.
local function meter(props, label, value, lo, hi)
  props.accessible_role = "meter"
  props.accessible_name = label
  props.accessible = function() return { value = round(value()), minimum = lo, maximum = hi } end
  return props
end

--- A composite display to assistive technology: a figure with a name.
local function figure(props, label, describe)
  props.accessible_role = "figure"
  props.accessible_name = label
  if describe then props.accessible = function() return { value = describe() } end end
  return props
end

local function label_of(spec, default)
  return type(spec.label) == "string" and spec.label or default
end

local function add(parent, child) if child then ui.reparent(child, parent) end return child end

--- An instrument's case, `w` by `h` at `x`, `y`: Material a raised tonal
--- card with the style's corners; Tsugumori a hairline frame with
--- registration marks.
local function case(style, parent, x, y, w, h, radius)
  if style.hatched then
    add(parent, ui.Rect { x = x, y = y, width = w, height = h, color = style.surface, border_width = 1,
      border_color = style.line })
    add(parent, ui.Item { x = x, y = y, width = w, height = h, style.marks() })
  else
    add(parent, ui.Rect { x = x, y = y, width = w, height = h, color = style.raised,
      radius = radius or style.radius(math.min(w, h) / 3) })
  end
end

--- A round dial's face, radius `r` about `cx`, `cy`.
local function disc(style, parent, cx, cy, r, color)
  if style.hatched then
    add(parent, ui.Rect { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r, radius = r, color = style.surface })
    add(parent, ui.Path { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r, view_box = { 0, 0, 2 * r, 2 * r },
      d = geo.arc(r, r, r - .5, 0, 360), fill_color = "transparent", stroke_width = 1,
      stroke_color = style.stroke_of("mark", color) })
    add(parent, ui.Item { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r, style.marks() })
  else
    add(parent, ui.Rect { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r, radius = r, color = style.raised })
  end
end

--- A path over `parent`'s box (`w` by `h`), in its pixels.
local function overlay(w, h, props)
  props.width, props.height, props.view_box = w, h, { 0, 0, w, h }
  if props.fill_color == nil then props.fill_color = "transparent" end
  return ui.Path(props)
end

--- A continuous reading of the angle `fn` gives: each turn the shorter
--- way, so a needle crossing north goes on rather than spinning back.
local function unwrap(node, kind, fn)
  local turn = morf.state { angle = 0 }
  local last, acc
  morf.effect(name(kind), function()
    local a = fn() % 360
    if not last then last, acc = a, a
    else
      acc = acc + ((a - last + 540) % 360 - 180)
      last = a
    end
    turn.angle = acc
  end, { owner = node })
  return function() return turn.angle end
end

--- The colours of a tick ruler: minor and major strokes.
local function tick_colors(style, color)
  if style.hatched then return style.stroke_of("mark", color), style.stroke_of("hot", color) end
  return U.alpha(style.ink_lo, .55), U.alpha(style.ink_lo, .9)
end

--- The span a reading is travelling over: `lo()` and `hi()`, the last
--- value and the new one -- what a window passes on the way there is
--- kept laid out under it.
local function sweep(node, kind, fn)
  local st = morf.state { lo = 0, hi = 0 }
  local last
  morf.effect(name(kind), function()
    local x = fn()
    if last == nil then st.lo, st.hi = x, x
    elseif x ~= last then st.lo, st.hi = math.min(last, x), math.max(last, x) end
    last = x
  end, { owner = node })
  return function() return st.lo end, function() return st.hi end
end

-- --------------------------------------------------------------- tape --

--- A moving tape `o.width` by `o.height`: ticks every `o.minor` units and
--- figures every `o.major`, `o.ppu` px a unit, the reading `o.value` at
--- its middle. `o.vertical` (higher values up) or across (higher right);
--- `o.side`: the edge the ticks stand on ("left"/"right", "top"/"bottom").
--- `o.format(L)` writes a figure, `o.show(L)` hides some (below a scale's
--- floor). Returns the window, clipped.
---
--- The ruler and the figures are drawn once, a stretch of the scale a
--- little longer than the window, on a container that slides with the
--- reading (a transform of what is already drawn) and is re-based in
--- whole steps under it, so a short run covers any range.
local function tape(style, o)
  local w, h, vert, ppu = o.width, o.height, o.vertical, o.ppu
  local color = o.color
  local radius = style.hatched and 0 or style.radius(math.min(w, h) / 2)
  -- the window, inset at the rounded ends along the tape
  local inset = radius / 2
  local ww, wh = vert and w or w - 2 * inset, vert and h - 2 * inset or h
  local len = vert and wh or ww
  local mid = len / 2
  if style.hatched and o.parent then
    add(o.parent, ui.Item { x = o.x or 0, y = o.y or 0, width = w, height = h, style.marks() })
  end
  local frame = ui.Rect { x = o.x or 0, y = o.y or 0, width = w, height = h, radius = radius,
    color = style.hatched and U.alpha(color, .04) or style.raised,
    border_width = style.hatched and 1 or 0, border_color = style.hatched and style.stroke_of("idle", color) or nil }
  -- (a plain clip: a rounded mask would cost a layer a frame while it slides)
  local win = add(frame, ui.Item { x = vert and 0 or inset, y = vert and inset or 0, width = ww, height = wh, clip = true })
  local motion = o.motion
  local major, minor = o.major, o.minor
  local lo, hi = sweep(win, "tape", o.value)
  local reach = mid / ppu
  local n = 2 * math.ceil(reach / major) + 8
  local function base()
    local a, b = lo(), hi()
    local start = a - reach
    -- a span longer than the run covers keeps the end it lands on
    if b - a + 2 * reach > n * major and o.value() >= b then start = b + reach - n * major end
    return math.floor(start / major) * major
  end
  local axis = vert and "translate_y" or "translate_x"
  local outer = ui.Item { width = w, height = h,
    [axis] = function() return (vert and 1 or -1) * o.value() * ppu end, behavior = { [axis] = motion } }
  local inner = ui.Item { width = w, height = h,
    [axis] = function() return vert and (mid - base() * ppu) or (mid + base() * ppu) end }
  ui.reparent(inner, outer)
  -- The ruler: static paths, minor ticks and major ones, from the base on.
  local minor_c, major_c = tick_colors(style, color)
  local thick = style.hatched and 1 or 2
  local span = n * major * ppu
  local function ruler(every, tl, c, skip)
    local parts = {}
    for j = 0, math.floor(n * major / every + .5) do
      local at = j * every
      if not skip or math.abs(at / skip - math.floor(at / skip + .5)) > 1e-6 then
        local off = at * ppu
        if vert then
          parts[#parts + 1] = ("M%g %g h%g"):format(o.side == "right" and (w - tl) or 0, span - off, tl)
        else
          parts[#parts + 1] = ("M%g %g v%g"):format(off, o.side == "bottom" and (h - tl) or 0, tl)
        end
      end
    end
    local props = { d = table.concat(parts, " "), fill_color = "transparent", stroke_color = c, stroke_width = thick,
      stroke_cap = "butt" }
    if vert then
      props.x, props.y, props.width, props.height, props.view_box = 0, -span, w, span, { 0, 0, w, span }
    else
      props.x, props.y, props.width, props.height, props.view_box = 0, 0, span, h, { 0, 0, span, h }
    end
    return ui.Path(props)
  end
  local short, long = o.tick or 7, (o.tick or 7) * 1.8
  add(inner, ruler(minor, short, minor_c, major))
  add(inner, ruler(major, long, major_c))
  -- The figures; the window cuts what reaches past it.
  local lh = o.font + 4
  local lw = o.label_w or 44
  for k = 0, n do
    local function L() return base() + k * major end
    local props = { text = function() return o.format(L()) end, font_size = o.font, color = o.label_color or style.ink_lo,
      font_weight = 500,
      visible = function()
        local at = L()
        if o.show and not o.show(at) then return false end
        -- the one under the reading's box stays out from behind it
        return not (o.hide and math.abs(at - o.value()) * ppu < o.hide + lh * .5)
      end }
    if vert then
      props.x, props.width, props.y, props.height = o.label_x, lw, -k * major * ppu - lh / 2, lh
      props.horizontal_alignment, props.vertical_alignment = o.align or "right", "center"
    else
      props.x, props.width, props.y, props.height = k * major * ppu - lw / 2, lw, o.label_y, lh
      props.horizontal_alignment, props.vertical_alignment = "center", "center"
    end
    add(inner, figures(style, props))
  end
  add(win, outer)
  return frame
end

-- ---------------------------------------------------------- the drums --

--- A drum of figures in a window `o.width` by `o.height` (clipped): row
--- `o.n()` (continuous) at its middle, rows `o.rowh` apart, the row for
--- whole `i` reading `o.format(i)` -- higher rows come up from below as
--- `n` grows. The rows are drawn once and slide by a transform.
local function drum(style, o)
  local w, h, rowh = o.width, o.height, o.rowh
  local P = o.period
  local win = ui.Item { x = o.x or 0, y = o.y or 0, width = w, height = h, clip = true,
    mask = o.fade and { gradient = { stops = { 0, { 1, .24 }, { 1, .76 }, 0 } } } or nil }
  -- Where the drum stands (`p`, in rows) and where it set out from: a
  -- long change spins less than a turn, and the figures repeat every `P`
  -- rows, so it lands on the right one.
  local st = morf.state { p = 0, lo = 0 }
  local p, last
  morf.effect(name("drum"), function()
    local n = o.n()
    if last == nil then
      p, last = n, n
      st.p, st.lo = n, n
      return
    end
    local d = n - last
    last = n
    if d == 0 then return end
    if P and math.abs(d) >= P then
      local sign = d < 0 and -1 or 1
      d = sign * (math.abs(d) % P)
    end
    local from = p
    p = p + d
    st.p, st.lo = p, math.min(from, p)
  end, { owner = win })
  local function base() return math.floor(st.lo) - 1 end
  local outer = ui.Item { width = w, height = h, translate_y = function() return -st.p * rowh end,
    behavior = { translate_y = o.motion } }
  local inner = ui.Item { width = w, height = h, translate_y = function() return base() * rowh end }
  ui.reparent(inner, outer)
  for k = 0, (P or 10) + 3 do
    local function i() return base() + k end
    add(inner, figures(style, { x = 0, width = w, y = k * rowh + (h - rowh) / 2, height = rowh,
      text = function() return o.format(i()) end, font_size = o.font, font_weight = o.weight or 600,
      horizontal_alignment = "center", vertical_alignment = "center",
      color = function() return get(o.dim_when and o.dim_when() and o.dim or o.color) end }))
  end
  add(win, outer)
  return win
end

--- A row of drums reading `o.value`: the last `o.roll` figures one drum
--- turning smoothly in steps of `o.step`, each figure above its own drum
--- that turns a whole place when the one below carries. `o.size` is the
--- figure's size; `o.digits` how many; leading zeros dim. Returns the
--- row and its width and height.
local function drum_row(style, o)
  local digits = o.digits or 4
  local roll = clamp(o.roll or 1, 1, digits)
  local step = o.step or 1
  local size = o.size or 30
  local rowh = math.ceil(size * 1.12)
  local dw = math.ceil(size * .64)
  local small = math.max(style.size.small - 3, math.floor(size * (roll > 1 and .8 or 1)))
  local rw = roll > 1 and (math.ceil(small * .6) * roll + 6) or dw
  local h = o.height or rowh
  local width = (digits - roll) * dw + rw
  local row = ui.Item { width = width, height = h }
  local v = o.value
  local dim = o.dim or U.alpha(o.color, .3)
  local motion = o.motion
  for j = digits - 1, roll, -1 do
    local p = 10 ^ j
    local x = (digits - 1 - j) * dw
    local dh = math.min(h, rowh + 2)
    add(row, drum(style, { x = x, y = (h - dh) / 2, width = dw, height = dh, rowh = rowh, font = size, motion = motion, fade = o.fade,
      n = function() return math.floor(math.abs(v()) / p) end, period = 10,
      format = function(i) return tostring(i % 10) end, color = o.color, dim = dim,
      dim_when = j > 0 and function() return math.floor(math.abs(v()) / p) == 0 end or nil }))
  end
  local mod = math.floor(10 ^ roll + .5)
  local fmt = "%0" .. roll .. "d"
  local srow = math.ceil(small * 1.12)
  local sh = roll > 1 and math.min(h, srow + 4) or h
  add(row, drum(style, { x = (digits - roll) * dw, y = (h - sh) / 2, width = rw, height = sh, rowh = srow, font = small,
    motion = motion, fade = o.fade, weight = roll > 1 and 500 or 600, color = o.roll_color or o.color,
    n = function() return math.abs(v()) / step end, period = math.floor(mod / step + .5),
    format = function(i) return fmt:format(math.floor(i * step + .5) % mod) end }))
  return row, width, h
end

-- ------------------------------------------------------------ readout --

--- A boxed reading that points at a tape's ticks: `x`, `y`, `w`, `h`,
--- the notch on `side` ("left"/"right"/"top"/"bottom"); `content` inside.
local function pointer_box(style, parent, x, y, w, h, side, color, content)
  local n = 7
  if not style.hatched then
    local cx, cy = x + w / 2, y + h / 2
    local dx = side == "right" and (x + w) or side == "left" and x or cx
    local dy = side == "bottom" and (y + h) or side == "top" and y or cy
    add(parent, ui.Sdf { anchors = { fill = true }, blend = 4, blend_profile = "circular", fill_color = color,
      ui.SdfShape { shape = "box", x = x, y = y, width = w, height = h, radius = math.min(10, h / 2 - 2) },
      ui.SdfShape { shape = "box", x = dx - n, y = dy - n, width = 2 * n, height = 2 * n, radius = 2, rotation = 45,
        operation = "smooth_union" },
    })
  else
    local d
    local x1, y1 = x + w, y + h
    local cx, cy = x + w / 2, y + h / 2
    if side == "right" then
      d = ("M%g %g H%g V%g L%g %g L%g %g V%g H%g Z"):format(x, y, x1, cy - n, x1 + n, cy, x1, cy + n, y1, x)
    elseif side == "left" then
      d = ("M%g %g H%g V%g H%g V%g L%g %g Z"):format(x, y, x1, y1, x, cy + n, x - n, cy, x, cy - n)
    elseif side == "top" then
      d = ("M%g %g H%g L%g %g L%g %g H%g V%g H%g Z"):format(x, y, cx - n, cx, y - n, cx + n, y, x1, y1, x)
    else
      d = ("M%g %g H%g V%g H%g L%g %g L%g %g H%g Z"):format(x, y, x1, y1, cx + n, cx, y1 + n, cx - n, y1, x)
    end
    local pw, ph = parent.width, parent.height
    add(parent, ui.Path { width = pw, height = ph, view_box = { 0, 0, pw, ph }, d = d,
      fill_color = style.surface, stroke_color = color, stroke_width = 1, stroke_join = "miter" })
  end
  if content then add(parent, content) end
end

-- -------------------------------------------------------- the horizon --

--- A horizon in a window `o.width` by `o.height` (`o.round`: a disc):
--- sky and ground (when `o.ground`), the horizon line and a pitch ladder,
--- turned by `o.roll` and shifted by `o.pitch` (`o.ppd` px a degree).
local function horizon(style, o)
  local w, h = o.width, o.height
  local cx, cy = w / 2, h / 2
  local node = ui.Item { x = o.x or 0, y = o.y or 0, width = w, height = h }
  local motion = o.motion
  local limit = o.limit or 30
  local ppd = o.ppd
  local round_win = o.round
  local radius = o.radius or 0
  local function window(inset)
    inset = inset or 0
    return ui.SdfShape { shape = round_win and "circle" or "box", x = inset, y = inset, width = w - 2 * inset,
      height = h - 2 * inset, radius = (not round_win) and math.max(0, radius - inset) or nil, operation = "intersect" }
  end
  local rot = ui.Item { width = w, height = h, rotation = function() return -o.roll() end, behavior = { rotation = motion } }
  local shift = ui.Item { width = w, height = h, translate_y = function() return clamp(o.pitch(), -limit, limit) * ppd end,
    behavior = { translate_y = motion } }
  ui.reparent(shift, rot)
  local ink = o.ink or style.ink
  if o.ground then
    -- The sky is the window; the ground a field cut to it.
    add(node, ui.Rect { width = w, height = h, radius = round_win and w / 2 or radius, color = o.sky,
      border_width = style.hatched and 1 or 0, border_color = style.hatched and style.line or nil })
    local reach = 2 * math.max(w, h) + limit * ppd
    local ground = add(shift, ui.Item { x = -reach, y = cy, width = w + 2 * reach, height = reach })
    add(node, ui.Sdf { width = w, height = h, fill_color = o.ground_color,
      ui.SdfShape { shape = "box", track = ground }, window() })
    if style.hatched then
      -- Hatching on the ground, masked by the same field.
      add(node, ui.Item { width = w, height = h,
        mask = ui.Sdf { width = w, height = h, fill_color = style.ink,
          ui.SdfShape { shape = "box", track = ground }, window() },
        style.stripes.box { width = w, height = h, gap = 6, weight = 1, color = U.alpha(o.ground_line or style.warn, .45) } })
    end
  end
  -- The horizon line and the ladder's rungs: one field each, cut to the window.
  local t = style.hatched and 1.5 or 2.5
  local hl = add(shift, ui.Item { x = -2 * w, y = cy - t / 2, width = 5 * w, height = t })
  add(node, ui.Sdf { width = w, height = h, fill_color = o.horizon or ink,
    ui.SdfShape { shape = "box", track = hl }, window() })
  -- Each rung one box across, the middle cut out by one tracked gap (a
  -- field holds 16 layers: twelve rungs, the gap and the window).
  local rungs = {}
  local gap = w * .07
  local lsize = o.font or math.max(style.size.small - 3, math.floor(w / 15))
  for p = -limit, limit, 5 do
    if p ~= 0 then
      local rw = (p % 10 == 0) and w * .16 or w * .07
      local y = cy - p * ppd
      local rt = style.hatched and 1 or 2
      rungs[#rungs + 1] = add(shift, ui.Item { x = cx - gap - rw, y = y - rt / 2, width = 2 * (gap + rw), height = rt })
      if p % 10 == 0 and o.labels ~= false then
        local lw = lsize * 2
        local function shown() return math.abs(p - o.pitch()) * ppd < math.min(w, h) * .27 end
        for side, x in ipairs { cx - gap - rw - 3 - lw, cx + gap + rw + 3 } do
          add(shift, figures(style, { x = x, y = y - lsize * .7, width = lw, height = lsize * 1.4,
            text = tostring(math.abs(p)), font_size = lsize, font_weight = 500, color = ink,
            horizontal_alignment = side == 1 and "right" or "left", vertical_alignment = "center", visible = shown }))
        end
      end
    end
  end
  local cut = add(shift, ui.Item { x = cx - gap, y = cy - (limit + 10) * ppd, width = 2 * gap, height = 2 * (limit + 10) * ppd })
  local ladder = { width = w, height = h, fill_color = U.alpha(ink, style.hatched and .85 or .9) }
  for _, r in ipairs(rungs) do
    ladder[#ladder + 1] = ui.SdfShape { shape = "box", track = r, radius = style.hatched and 0 or 1 }
  end
  ladder[#ladder + 1] = ui.SdfShape { shape = "box", track = cut, operation = "subtract" }
  local m = math.min(w, h)
  ladder[#ladder + 1] = ui.SdfShape { shape = "circle", x = cx - m * .36, y = cy - m * .36, width = m * .72, height = m * .72,
    operation = "intersect" }
  add(node, ui.Sdf(ladder))
  -- (what turns carries the figures, over the fields)
  add(node, rot)
  return node, rot
end

--- The fixed aircraft symbol, centred at `cx`, `cy`, `span` wide.
local function aircraft_symbol(style, parent, pw, ph, cx, cy, span, color)
  local a, b = span / 2, span * .17
  local d = ("M%g %g H%g V%g M%g %g H%g V%g"):format(cx - a, cy, cx - b, cy + span * .1, cx + a, cy, cx + b, cy + span * .1)
  if not style.hatched then
    add(parent, overlay(pw, ph, { d = d, stroke_color = style.surface, stroke_width = 8, stroke_cap = "round", stroke_join = "round" }))
    add(parent, overlay(pw, ph, { d = d, stroke_color = color, stroke_width = 4.5, stroke_cap = "round", stroke_join = "round" }))
    add(parent, ui.Rect { x = cx - 4.5, y = cy - 4.5, width = 9, height = 9, radius = 4.5, color = color,
      border_width = 1.5, border_color = style.surface })
  else
    add(parent, overlay(pw, ph, { d = d, stroke_color = style.surface, stroke_width = 4, stroke_join = "miter" }))
    add(parent, overlay(pw, ph, { d = d, stroke_color = color, stroke_width = 2, stroke_join = "miter" }))
    add(parent, ui.Rect { x = cx - 3, y = cy - 3, width = 6, height = 6, color = color })
  end
end

--- A bank scale: an arc of radius `r` over `cx`, `cy` with ticks at 10,
--- 20, 30, 45 and 60 each way, a fixed index at the top and a pointer that
--- turns with `roll`.
local BANK_MINOR = { 10, 20, 45, 350, 340, 315 }
local BANK_MAJOR = { 30, 60, 330, 300 }
local function bank(style, parent, pw, ph, cx, cy, r, roll, motion, color)
  local minor_c, major_c = tick_colors(style, color)
  local lw = style.hatched and 1 or 2
  add(parent, overlay(pw, ph, { d = geo.arc(cx, cy, r, -60, 120), stroke_color = minor_c, stroke_width = lw }))
  add(parent, overlay(pw, ph, { d = geo.ticks(cx, cy, r, r + 6, { angles = BANK_MINOR }), stroke_color = major_c,
    stroke_width = lw, stroke_cap = style.hatched and "butt" or "round" }))
  add(parent, overlay(pw, ph, { d = geo.ticks(cx, cy, r, r + 11, { angles = BANK_MAJOR }), stroke_color = major_c,
    stroke_width = lw + .5, stroke_cap = style.hatched and "butt" or "round" }))
  -- The index at zero, pointing down at the scale.
  local top = cy - r
  add(parent, overlay(pw, ph, { d = ("M%g %g L%g %g L%g %g Z"):format(cx, top - 1, cx - 6, top - 11, cx + 6, top - 11),
    fill_color = style.hatched and "transparent" or style.ink, stroke_color = style.ink, stroke_width = style.hatched and 1 or 0,
    stroke_join = style.hatched and "miter" or "round" }))
  -- The pointer, turning with the horizon.
  local turn = ui.Item { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r,
    rotation = function() return -roll() end, behavior = { rotation = motion } }
  add(turn, ui.Path { x = r - 7, y = 1, width = 14, height = 12, view_box = { 0, 0, 14, 12 },
    d = "M7 0 L13.5 11 L.5 11 Z", fill_color = style.hatched and U.alpha(color, .25) or color,
    stroke_color = style.hatched and color or "transparent", stroke_width = 1, stroke_join = style.hatched and "miter" or "round" })
  add(parent, turn)
end

-- ----------------------------------------------------------- the rose --

--- Static radial ticks about the middle of a `size` square, every
--- `every` degrees, the ones on `major` (a multiple) longer: two paths to
--- turn by `rotation` -- drawn once, turned as a transform.
local function tick_ring(style, parent, size, r_out, every, minor_len, major, major_len, color, angles_from, angles_to)
  local minor_c, major_c = tick_colors(style, color)
  local thick = style.hatched and 1 or 2
  local minors, majors = {}, {}
  local a0, a1 = angles_from or 0, angles_to or (360 - every)
  for a = a0, a1, every do
    if a % major == 0 then majors[#majors + 1] = a % 360 else minors[#minors + 1] = a % 360 end
  end
  local c = size / 2
  local function path(list, len, col)
    if #list == 0 then return end
    add(parent, ui.Path { width = size, height = size, view_box = { 0, 0, size, size },
      d = geo.ticks(c, c, r_out - len, r_out, { angles = list }), fill_color = "transparent", stroke_color = col,
      stroke_width = thick, stroke_cap = "butt" })
  end
  path(minors, minor_len, minor_c)
  path(majors, major_len, major_c)
end

local ROSE = { [0] = "N", [90] = "E", [180] = "S", [270] = "W" }

--- A compass rose of radius `r` about `cx`, `cy` turning with the
--- heading `hd` (continuous): a card of ticks and figures, drawn once and
--- turned.
local function rose(style, parent, cx, cy, r, hd, motion, color, opt)
  opt = opt or {}
  local rim = r - 3
  local turn = ui.Item { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r,
    rotation = function() return -hd() end, behavior = { rotation = motion } }
  tick_ring(style, turn, 2 * r, rim, 5, 6, 10, 11, color)
  local fs = opt.font or math.max(style.size.small - 3, math.min(style.size.normal, math.floor(r / 6)))
  local rl = rim - 13 - fs * .75
  for a = 0, 330, 30 do
    local rad = math.rad(a)
    local cardinal = ROSE[a]
    local lw = fs * 2.2
    add(turn, ui.Item { x = r + rl * math.sin(rad) - lw / 2, y = r - rl * math.cos(rad) - fs * .7, width = lw,
      height = fs * 1.4, rotation = a,
      figures(style, { anchors = { fill = true }, text = cardinal or tostring(a // 10), font_size = fs,
        font_weight = cardinal and 700 or 500, horizontal_alignment = "center", vertical_alignment = "center",
        color = a == 0 and (style.hatched and style.alert or color) or (cardinal and style.ink or style.ink_lo) }) })
  end
  add(parent, turn)
  return turn, rl - fs * .8
end

--- The fixed lubber line over a rose at the top, `cx`, `top`.
local function lubber(style, parent, pw, ph, cx, top, color)
  if style.hatched then
    add(parent, ui.Rect { x = cx - 1, y = top, width = 2, height = 16, color = color })
  else
    add(parent, overlay(pw, ph, { d = ("M%g %g L%g %g L%g %g Z"):format(cx, top + 14, cx - 7, top + 2, cx + 7, top + 2),
      fill_color = color, stroke_color = color, stroke_width = 3, stroke_join = "round" }))
  end
end

local PLANE = "M0 -1 L.11 -.86 L.11 -.28 L.92 .16 L.92 .3 L.11 .1 L.11 .62 L.36 .8 L.36 .92 L0 .84"
  .. " L-.36 .92 L-.36 .8 L-.11 .62 L-.11 .1 L-.92 .3 L-.92 .16 L-.11 -.28 L-.11 -.86 Z"

--- A small aeroplane seen from above, `s` across, centred at `cx`, `cy`.
local function plane(style, parent, cx, cy, s, color)
  add(parent, ui.Path { x = cx - s / 2, y = cy - s / 2, width = s, height = s, view_box = { -1.05, -1.05, 2.1, 2.1 },
    d = PLANE, fill_color = style.hatched and "transparent" or color,
    stroke_color = style.hatched and color or "transparent", stroke_width = style.hatched and .06 or 0,
    stroke_join = style.hatched and "miter" or "round" })
end

-- ======================================================== instruments ==

-- --------------------------------------------------------- airspeed --

--- A vertical airspeed tape: the scale sliding past a boxed reading whose
--- last figure rolls. `spec`: `speed` (knots, a number or function),
--- `width` (96), `height` (186), `span` (80: knots in view), `min` (0:
--- the scale's floor), `max` (400), `label` ("IAS"), `color`.
function M.airspeed_tape(spec, style)
  local w, h = spec.width or 96, spec.height or 186
  local color = U.color(spec, style, "accent")
  local v = reader(spec.speed or spec.value, 0)
  local lo, hi = spec.min or 0, spec.max or 400
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Airspeed"), v, lo, hi))
  local head = 20
  local th = h - head
  local function value() return clamp(v(), lo, hi) end
  add(node, caption(style, { text = spec.label or "IAS", x = 2, y = 0, width = w / 2 }))
  add(node, caption(style, { text = spec.unit or "kt", x = w / 2, y = 0, width = w / 2 - 2, horizontal_alignment = "right" }))
  local fs = math.max(style.size.small - 3, style.size.small - 2)
  local motion = travel(style, 150, 18)
  local win = add(node, tape(style, { parent = node, y = head, width = w, height = th, vertical = true, side = "right",
    value = value, ppu = th / (spec.span or 80), minor = 10, major = 20, motion = motion, color = color,
    font = fs, label_x = 4, label_w = w - 26, hide = 16, format = function(L) return tostring(math.floor(L)) end,
    show = function(L) return L >= lo and L <= hi end }))
  -- The reading: a box at the middle pointing at the ticks, its figures rolling.
  local bh, bw = 32, w - 18
  local by = head + th / 2 - bh / 2
  local layer = add(node, ui.Item { width = w, height = h })
  local ink = style.hatched and color or style.on_accent
  local row, rw, rh = drum_row(style, { value = value, digits = 3, roll = 1, size = style.size.large, color = ink,
    motion = motion, height = bh - 4, fade = not style.hatched, dim = U.alpha(ink, .35) })
  row.x, row.y = 3 + (bw - rw) / 2, by + 2
  pointer_box(style, layer, 3, by, bw, bh, "right", color, row)
  return node
end

-- -------------------------------------------------------- altimeter --

--- A vertical altimeter tape with a rolling readout: the hundreds and
--- above on drums that carry, the last two figures one drum in twenties.
--- `spec`: `altitude` (feet), `width` (110), `height` (186), `span`
--- (800: feet in view), `label` ("ALT"), `color`.
function M.altimeter_tape(spec, style)
  local w, h = spec.width or 110, spec.height or 186
  local color = U.color(spec, style, "accent")
  local v = reader(spec.altitude or spec.value, 0)
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Altitude"), v, -2000, 60000))
  local head = 20
  local th = h - head
  add(node, caption(style, { text = spec.label or "ALT", x = 2, y = 0, width = w / 2 }))
  add(node, caption(style, { text = spec.unit or "ft", x = w / 2, y = 0, width = w / 2 - 2, horizontal_alignment = "right" }))
  local fs = style.size.small - 2
  local motion = travel(style, 120, 18)
  add(node, tape(style, { parent = node, y = head, width = w, height = th, vertical = true, side = "left",
    value = v, ppu = th / (spec.span or 800), minor = 100, major = 200, motion = motion, color = color,
    font = fs, label_x = 16, label_w = w - 22, align = "right", hide = 17, format = function(L) return tostring(math.floor(L)) end }))
  local bh, bw = 34, w - 16
  local by = head + th / 2 - bh / 2
  local layer = add(node, ui.Item { width = w, height = h })
  local ink = style.hatched and color or style.on_accent
  local size = math.min(style.size.large, math.floor((bw - 8) / 3.3))
  local row, rw = drum_row(style, { value = v, digits = 5, roll = 2, step = 20, size = size, color = ink,
    motion = motion, height = bh - 4, fade = not style.hatched, dim = U.alpha(ink, .3) })
  row.x, row.y = 13 + (bw - rw) / 2, by + 2
  pointer_box(style, layer, 13, by, bw, bh, "left", color, row)
  return node
end

-- -------------------------------------------------------- attitude --

--- An attitude indicator: the horizon turning and pitching behind a fixed
--- aircraft symbol, a pitch ladder, and a bank scale whose pointer turns
--- with the sky. Material a round tonal window; Tsugumori a square one,
--- the ground hatched. `spec`: `pitch`, `roll` (degrees), `size` (180),
--- `ppd` (px a degree), `color` (the symbol: warn).
function M.attitude_indicator(spec, style)
  local s = spec.size or math.min(spec.width or 180, spec.height or 180)
  local color = U.color(spec, style, "accent")
  local pitch, roll = reader(spec.pitch, 0), reader(spec.roll, 0)
  local node = ui.Item(figure(U.place(spec, { width = s, height = s }), label_of(spec, "Attitude"),
    function() return ("pitch %d, roll %d"):format(round(pitch()), round(roll())) end))
  local motion = travel(style, 110, 16)
  local sky, ground
  if style.hatched then
    sky, ground = U.alpha(style.info, .1), U.alpha(style.warn, .16)
  else
    sky = function() return get(style.info):mix(get(style.surface), .45) end
    ground = function() return get(style.warn):mix(get(style.surface), .78) end
  end
  add(node, horizon(style, { width = s, height = s, round = not style.hatched,
    ground = true, sky = sky, ground_color = ground, pitch = pitch, roll = roll, ppd = spec.ppd or s / 70,
    motion = motion, limit = 30 }))
  local r = s / 2
  bank(style, node, s, s, r, r, r * .8, roll, motion, style.hatched and style.accent or style.ink)
  aircraft_symbol(style, node, s, s, r, r, s * .5, color)
  if style.hatched then add(node, ui.Item { width = s, height = s, style.marks() }) end
  return node
end

-- ------------------------------------------------------ pitch ladder --

--- A stand-alone pitch ladder (a head-up display's): rungs every five
--- degrees, figures every ten, turned by `roll` and slid by `pitch` past a
--- fixed reference. `spec`: `pitch`, `roll`, `width` (200), `height`
--- (180), `ppd`, `color`.
function M.pitch_ladder(spec, style)
  local w, h = spec.width or spec.size or 200, spec.height or spec.size or 180
  local color = U.color(spec, style, "accent")
  local pitch, roll = reader(spec.pitch, 0), reader(spec.roll, 0)
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Pitch ladder"),
    function() return ("pitch %d, roll %d"):format(round(pitch()), round(roll())) end))
  case(style, node, 0, 0, w, h)
  local motion = travel(style, 110, 16)
  add(node, horizon(style, { width = w, height = h, radius = style.hatched and 0 or style.radius(h),
    ground = false, pitch = pitch, roll = roll, ppd = spec.ppd or h / 50, motion = motion, limit = 30,
    ink = color, horizon = color }))
  -- The fixed reference: a boresight cross in the middle.
  local cx, cy = w / 2, h / 2
  aircraft_symbol(style, node, w, h, cx, cy, w * .34, style.ink)
  add(node, caption(style, { text = function() return ("P %+d°"):format(round(pitch())) end, x = 8, y = h - 22, width = w / 2 - 8 }))
  add(node, caption(style, { text = function() return ("R %+d°"):format(round(roll())) end, x = w / 2, y = h - 22,
    width = w / 2 - 8, horizontal_alignment = "right" }))
  return node
end

-- ------------------------------------------------------- bank scale --

--- A bank scale: the roll arc with ticks at 10, 20, 30, 45 and 60 each
--- way and a pointer that turns with the roll, the angle read beneath.
--- `spec`: `roll` (degrees), `width` (220), `height` (130), `color`.
function M.bank_scale(spec, style)
  local w, h = spec.width or 220, spec.height or 130
  local color = U.color(spec, style, "accent")
  local roll = reader(spec.roll or spec.value, 0)
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Bank"), roll, -60, 60))
  local r = math.min((w / 2 - 6) / .866 - 12, h - 34)
  local cx, cy = w / 2, 13 + r + 2
  local motion = travel(style, 130, 15)
  bank(style, node, w, h, cx, cy, r, roll, motion, color)
  -- A wing bar turning with the aircraft under the arc.
  add(node, overlay(w, h, { d = ("M%g %g H%g M%g %g H%g"):format(cx - r * .62, cy, cx - r * .5, cx + r * .5, cy, cx + r * .62),
    stroke_color = U.alpha(style.ink_lo, .7), stroke_width = style.hatched and 1 or 2, stroke_cap = style.hatched and "butt" or "round" }))
  local turn = add(node, ui.Item { x = cx - r * .5, y = cy - r * .5, width = r, height = r,
    rotation = function() return roll() end, behavior = { rotation = motion } })
  add(turn, ui.Rect { x = r * .06, y = r * .5 - (style.hatched and 1 or 2), width = r * .88, height = style.hatched and 2 or 4,
    radius = style.hatched and 0 or 2, color = style.ink })
  add(turn, ui.Rect { x = r * .5 - 3, y = r * .5 - 3, width = 6, height = 6, radius = style.hatched and 0 or 3, color = color })
  add(node, figures(style, { x = 0, width = w, y = cy - r * .42 - style.size.large * .7, height = style.size.large * 1.4,
    text = function()
      local a = round(roll())
      if a == 0 then return "0°" end
      return (a < 0 and "L " or "R ") .. math.abs(a) .. "°"
    end, font_size = style.size.large, font_weight = 600, horizontal_alignment = "center", vertical_alignment = "center",
    color = style.hatched and color or style.ink }))
  return node
end

-- ------------------------------------------------------- heading ind --

--- A heading indicator: a compass rose turning under a fixed lubber line,
--- an aeroplane in the middle and the heading read beneath it. `spec`:
--- `heading` (degrees), `size` (180), `color`.
function M.heading_indicator(spec, style)
  local s = spec.size or math.min(spec.width or 180, spec.height or 180)
  local color = U.color(spec, style, "accent")
  local hd0 = reader(spec.heading or spec.value, 0)
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), label_of(spec, "Heading"),
    function() return hd0() % 360 end, 0, 360))
  local hd = unwrap(node, "heading", hd0)
  local r = s / 2
  disc(style, node, r, r, r, color)
  rose(style, node, r, r, r, hd, travel(style, 90, 13), color)
  lubber(style, node, s, s, r, 0, style.hatched and style.ink or color)
  plane(style, node, r, r - 6, s * .2, style.hatched and color or style.ink)
  add(node, figures(style, { x = r - 30, width = 60, y = r + s * .07, height = 20,
    text = function() return ("%03d°"):format(round(hd0()) % 360) end, font_size = style.size.small, font_weight = 600,
    horizontal_alignment = "center", color = style.hatched and color or style.ink }))
  return node
end

-- ------------------------------------------------------- heading tape --

--- A horizontal heading tape: the scale sliding under a boxed heading
--- that points up at the ticks. `spec`: `heading`, `width` (250),
--- `height` (64), `span` (60: degrees in view), `color`.
function M.heading_tape(spec, style)
  local w, h = spec.width or 250, spec.height or 64
  local color = U.color(spec, style, "accent")
  local hd0 = reader(spec.heading or spec.value, 0)
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Heading"),
    function() return hd0() % 360 end, 0, 360))
  local hd = unwrap(node, "heading", hd0)
  local motion = travel(style, 120, 16)
  local fs = style.size.small - 1
  add(node, tape(style, { parent = node, width = w, height = h, vertical = false, side = "top", value = hd, ppu = w / (spec.span or 60),
    minor = 5, major = 10, motion = motion, color = color, font = fs, label_y = 14, label_w = 30,
    format = function(L)
      local a = math.floor(L + .5) % 360
      return ROSE[a] or ("%02d"):format(a // 10)
    end }))
  local bw, bh = 52, 26
  local layer = add(node, ui.Item { width = w, height = h })
  local ink = style.hatched and color or style.on_accent
  local row, rw = drum_row(style, { value = function() return hd0() % 360 end, digits = 3, roll = 1, size = style.size.normal,
    color = ink, motion = motion, height = bh - 4, fade = not style.hatched, dim = ink })
  local bx, by = w / 2 - bw / 2, h - bh - 5
  row.x, row.y = bx + (bw - rw) / 2, by + 2
  pointer_box(style, layer, bx, by, bw, bh, "top", color, row)
  return node
end

-- --------------------------------------------------- course deviation --

--- A course deviation indicator: a bar sliding across five dots each way,
--- a to/from flag (a hatched NAV flag when off). `spec`: `deviation`
--- (-1..1, full scale), `to_from` ("to", "from" or nil: off), `course`
--- (degrees, read in the corner), `width` (250), `height` (120), `color`.
function M.course_deviation(spec, style)
  local w, h = spec.width or 250, spec.height or 120
  local color = U.color(spec, style, "accent")
  local dev = reader(spec.deviation or spec.value, 0)
  local function flag() local f = get(spec.to_from) if f == "to" or f == "from" then return f end return nil end
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Course deviation"),
    function() return dev() * 100 end, -100, 100))
  case(style, node, 0, 0, w, h)
  local cx = w / 2
  local sy = h * .58
  local half = w / 2 - 22
  local dot = half / 5
  for i = -5, 5 do
    if i ~= 0 then
      local x = cx + i * dot
      local d = style.hatched and 5 or (i % 5 == 0 and 8 or 6)
      if style.hatched then
        add(node, ui.Rect { x = x - d / 2, y = sy - d / 2, width = d, height = d, color = "transparent",
          border_width = 1, border_color = style.stroke_of(i % 5 == 0 and "hot" or "mark", color) })
      else
        add(node, ui.Rect { x = x - d / 2, y = sy - d / 2, width = d, height = d, radius = d / 2,
          color = U.alpha(style.ink_lo, i % 5 == 0 and .8 or .5) })
      end
    end
  end
  -- The centre index.
  add(node, ui.Rect { x = cx - 1.5, y = sy - 16, width = 3, height = 32, radius = style.hatched and 0 or 1.5,
    color = U.alpha(style.ink, .5) })
  local motion = travel(style, 150, 14)
  local bh = h * .56
  local bar = add(node, ui.Item { y = sy - bh / 2, width = 12, height = bh,
    x = cx - 6, translate_x = function() return clamp(dev(), -1, 1) * half end, behavior = { translate_x = motion },
    stretch = (not style.hatched) and { scale = .1, max = .2 } or nil,
    opacity = function() return flag() and 1 or .35 end })
  if style.hatched then
    add(bar, ui.Rect { x = 5, y = 0, width = 2, height = bh, color = color })
    add(bar, ui.Rect { x = 2, y = bh / 2 - 4, width = 8, height = 8, color = "transparent", border_width = 1, border_color = color })
  else
    add(bar, ui.Rect { x = 2, y = 0, width = 8, height = bh, radius = 4, color = color })
  end
  -- The flag: a triangle pointing up (to) or down (from), or NAV when off.
  local fx, fy, fw, fh = w - 58, 8, 50, 24
  local on = add(node, ui.Item { x = fx, y = fy, width = fw, height = fh, visible = function() return flag() ~= nil end })
  if style.hatched then
    add(on, ui.Rect { width = fw, height = fh, color = "transparent", border_width = 1, border_color = style.stroke_of("mark", color) })
  else
    add(on, ui.Rect { width = fw, height = fh, radius = fh / 2, color = U.alpha(color, .18) })
  end
  add(on, ui.Item { x = 6, y = fh / 2 - 6, width = 12, height = 12,
    rotation = function() return flag() == "from" and 180 or 0 end, behavior = { rotation = travel(style, 260, 18) },
    ui.Path { width = 12, height = 12, view_box = { 0, 0, 12, 12 }, d = "M6 1 L11.5 11 L.5 11 Z", fill_color = color,
      stroke_color = color, stroke_width = style.hatched and 0 or 1.5, stroke_join = "round" } })
  add(on, caption(style, { text = function() return flag() == "from" and "From" or "To" end, x = 20, y = (fh - 16) / 2,
    width = fw - 22, height = 16, color = style.ink }))
  local off = add(node, ui.Item { x = fx, y = fy, width = fw, height = fh, visible = function() return flag() == nil end })
  add(off, U.fill(style, { width = fw, height = fh, color = style.alert, strong = true, radius = fh / 2 }))
  add(off, caption(style, { text = "NAV", width = fw, y = (fh - 16) / 2, height = 16, horizontal_alignment = "center",
    color = style.hatched and style.alert or style.on_accent }))
  add(node, caption(style, { text = spec.label or "Loc", x = 10, y = 10, width = w / 2 }))
  if spec.course ~= nil then
    add(node, figures(style, { x = 10, y = 26, width = w / 2, height = 20, font_size = style.size.small, font_weight = 600,
      text = function() return ("CRS %03d°"):format(round(get(spec.course)) % 360) end,
      color = style.hatched and color or style.ink }))
  end
  return node
end

-- --------------------------------------------------------------- hsi --

--- A horizontal situation indicator: the rose turning with the heading, a
--- course needle turning with the course over it, its middle a deviation
--- bar across two dots each way, and a to/from triangle. `spec`:
--- `heading`, `course` (degrees), `deviation` (-1..1), `to_from`,
--- `width` (250), `height` (186), `color`.
function M.hsi(spec, style)
  local w, h = spec.width or spec.size or 250, spec.height or spec.size or 186
  local color = U.color(spec, style, "accent")
  local hd0 = reader(spec.heading, 0)
  local crs0 = reader(spec.course, 0)
  local dev = reader(spec.deviation, 0)
  local function flag() local f = get(spec.to_from) if f == "to" or f == "from" then return f end return nil end
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Horizontal situation"),
    function() return ("heading %03d, course %03d"):format(round(hd0()) % 360, round(crs0()) % 360) end))
  local s = math.min(w, h)
  local r = s / 2
  local cx, cy = w / 2, h / 2
  local hd = unwrap(node, "hsi", hd0)
  local rel = unwrap(node, "hsi.course", function() return crs0() - hd0() end)
  local motion = travel(style, 90, 13)
  disc(style, node, cx, cy, r, color)
  local _, ri = rose(style, node, cx, cy, r, hd, motion, color)
  lubber(style, node, w, h, cx, cy - r, style.hatched and style.ink or color)
  -- The course needle.
  local turn = add(node, ui.Item { x = cx - r, y = cy - r, width = 2 * r, height = 2 * r,
    rotation = function() return rel() end, behavior = { rotation = travel(style, 110, 15) } })
  local c = r
  local dotp = ri * .26
  local inner = ri * .62
  local nw = style.hatched and 2 or 5
  -- head and shaft above the deviation dots, the tail below
  add(turn, ui.Path { x = c - 8, y = c - ri, width = 16, height = ri - inner, view_box = { 0, 0, 16, ri - inner },
    d = ("M8 0 L15 13 L%g 13 V%g H%g V13 L1 13 Z"):format(8 + nw / 2, ri - inner, 8 - nw / 2),
    fill_color = style.hatched and U.alpha(color, .2) or color, stroke_color = style.hatched and color or "transparent",
    stroke_width = 1, stroke_join = style.hatched and "miter" or "round" })
  add(turn, ui.Rect { x = c - nw / 2, y = c + inner, width = nw, height = ri - inner, radius = style.hatched and 0 or nw / 2,
    color = color })
  for i = -2, 2 do
    if i ~= 0 then
      local d = style.hatched and 5 or 6
      add(turn, ui.Rect { x = c + i * dotp - d / 2, y = c - d / 2, width = d, height = d, radius = style.hatched and 0 or d / 2,
        color = style.hatched and "transparent" or U.alpha(style.ink, .6), border_width = style.hatched and 1 or 0,
        border_color = style.hatched and style.ink or nil })
    end
  end
  add(turn, ui.Rect { y = c - inner + 4, width = nw, height = 2 * inner - 8, radius = style.hatched and 0 or nw / 2,
    color = color, x = c - nw / 2, translate_x = function() return clamp(dev(), -1, 1) * 2 * dotp end,
    behavior = { translate_x = travel(style, 150, 14) },
    opacity = function() return flag() and 1 or .4 end })
  add(turn, ui.Item { x = c - 6, y = c - inner * .55 - 6, width = 12, height = 12, visible = function() return flag() ~= nil end,
    rotation = function() return flag() == "from" and 180 or 0 end, behavior = { rotation = travel(style, 260, 18) },
    ui.Path { width = 12, height = 12, view_box = { 0, 0, 12, 12 }, d = "M6 1 L11 10 L1 10 Z",
      fill_color = style.hatched and "transparent" or style.ink, stroke_color = style.ink, stroke_width = 1 } })
  plane(style, node, cx, cy, ri * .55, style.hatched and style.ink or style.ink)
  -- The readings in the corners when there is room.
  if w - s >= 100 then
    local cw = (w - s) / 2 - 4
    add(node, caption(style, { text = "Hdg", x = 0, y = 2, width = cw }))
    add(node, figures(style, { x = 0, y = 18, width = cw, height = 22, font_size = style.size.normal, font_weight = 600,
      text = function() return ("%03d°"):format(round(hd0()) % 360) end, color = style.ink }))
    add(node, caption(style, { text = "Crs", x = w - cw, y = 2, width = cw, horizontal_alignment = "right" }))
    add(node, figures(style, { x = w - cw, y = 18, width = cw, height = 22, font_size = style.size.normal, font_weight = 600,
      horizontal_alignment = "right", text = function() return ("%03d°"):format(round(crs0()) % 360) end, color = color }))
  end
  return node
end

-- ------------------------------------------------------- nav display --

--- A heading-up moving map: the compass arc turning over range arcs, the
--- route through `waypoints` (`{ x, y, name }`, nm east and north of the
--- aircraft) turning about the aircraft, a track line. `spec`: `heading`,
--- `track` (degrees; the heading when nil), `range` (nm at the outer arc,
--- 40), `waypoints` (a list or a function of one), `width` (260),
--- `height` (190), `color`.
function M.nav_display(spec, style)
  local w, h = spec.width or 260, spec.height or 190
  local color = U.color(spec, style, "accent")
  local hd0 = reader(spec.heading, 0)
  local trk0 = spec.track ~= nil and reader(spec.track, 0) or hd0
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Navigation display"),
    function() return ("heading %03d, range %d"):format(round(hd0()) % 360, round(get(spec.range) or 40)) end))
  case(style, node, 0, 0, w, h)
  local ax, ay = w / 2, h - 24
  local R = math.min(ay - 10, (w / 2 - 6) / math.sin(math.rad(55)))
  local hd = unwrap(node, "nav", hd0)
  local motion = travel(style, 90, 14)
  local minor_c, major_c = tick_colors(style, color)
  local function range() return tonumber(get(spec.range)) or 40 end
  -- Range arcs (static), their figures.
  local SW = 55
  for k, f in ipairs { .5, 1 } do
    add(node, overlay(w, h, { d = geo.arc(ax, ay, R * f, -SW, 2 * SW), stroke_color = k == 2 and major_c or minor_c,
      stroke_width = 1, dash = (k == 1 and style.hatched) and { 3, 3 } or nil }))
    local a = math.rad(-SW + 4)
    local rr = R * f - (k == 2 and 36 or 4)
    add(node, figures(style, { x = ax + rr * math.sin(a), y = ay - rr * math.cos(a), width = 30, height = 16,
      font_size = style.size.small - 3, color = style.ink_lo,
      text = function() local x = range() * f return (x == math.floor(x)) and tostring(math.floor(x)) or ("%.1f"):format(x) end }))
  end
  -- The compass card: ticks and figures round the whole circle, drawn
  -- once and turned, seen through the arc's window (a static mask).
  local function turning(parent)
    return add(parent, ui.Item { x = ax - R, y = ay - R, width = 2 * R, height = 2 * R,
      rotation = function() return -hd() end, behavior = { rotation = motion } })
  end
  local arc_win = add(node, ui.Item { width = w, height = h,
    mask = ui.Path { width = w, height = h, view_box = { 0, 0, w, h }, fill_color = style.ink,
      d = geo.sector(ax, ay, R - 30, R + 2, -SW - 1, 2 * SW + 2) } })
  local dial = turning(arc_win)
  tick_ring(style, dial, 2 * R, R, 5, 5, 10, 10, color)
  local fs = style.size.small - 3
  for a = 0, 330, 30 do
    local rad = math.rad(a)
    local rl = R - 16
    local cardinal = ROSE[a]
    add(dial, ui.Item { x = R + rl * math.sin(rad) - 14, y = R - rl * math.cos(rad) - 8, width = 28, height = 16, rotation = a,
      -- the ones round the back are left out (the mask would hide them anyway)
      visible = function() return math.abs((a - hd0() + 540) % 360 - 180) <= SW + 10 end,
      figures(style, { anchors = { fill = true }, text = cardinal or tostring(a // 10), font_size = fs, font_weight = 600,
        horizontal_alignment = "center", vertical_alignment = "center", color = cardinal and style.ink or style.ink_lo }) })
  end
  -- What else turns about the aircraft: the route.
  local turn = turning(node)
  local function points()
    local list = get(spec.waypoints) or {}
    return type(list) == "table" and list or {}
  end
  local MAXW = 6
  local route = { width = w, height = h, fill_color = color }
  local legs, marks = {}, {}
  local function ppn() return R / range() end
  local function wp(i) return points()[i] end
  local lt = style.hatched and 1.5 or 2.5
  for i = 1, MAXW do
    -- leg i: from the previous waypoint (the aircraft for the first) to waypoint i.
    local function ends()
      local p = wp(i)
      if not p then return nil end
      local q = i > 1 and wp(i - 1) or { x = 0, y = 0 }
      local k = ppn()
      return R + q.x * k, R - q.y * k, R + p.x * k, R - p.y * k
    end
    local function geom()
      local x0, y0, x1, y1 = ends()
      if not x0 then return 0, 0, 0, 0 end
      local dx, dy = x1 - x0, y1 - y0
      return (x0 + x1) / 2, (y0 + y1) / 2, math.sqrt(dx * dx + dy * dy), math.deg(math.atan(dy, dx))
    end
    local leg = add(turn, ui.Item { height = lt,
      x = function() local mx, _, l = geom() return mx - l / 2 end, y = function() local _, my = geom() return my - lt / 2 end,
      width = function() local _, _, l = geom() return math.max(l, .1) end,
      rotation = function() local _, _, _, a = geom() return a end,
      visible = function() return wp(i) ~= nil end })
    legs[#legs + 1] = leg
    local d = style.hatched and 7 or 9
    local mark = add(turn, ui.Item { width = d, height = d, rotation = 45,
      x = function() local _, _, x1 = ends() return (x1 or 0) - d / 2 end,
      y = function() local _, _, _, y1 = ends() return (y1 or 0) - d / 2 end,
      visible = function() return wp(i) ~= nil end })
    marks[#marks + 1] = mark
    -- The waypoint's name, upright whatever the heading, shown while in view.
    local function inview()
      local p = wp(i)
      if not p then return false end
      local a = math.rad(hd0())
      local k = ppn()
      local sx = (p.x * math.cos(a) - p.y * math.sin(a)) * k
      local sy = (p.x * math.sin(a) + p.y * math.cos(a)) * k
      return math.abs(sx) < w / 2 - 50 and sy > 0 and sy < ay - 40 and (sx * sx + sy * sy) < (R - 18) ^ 2
    end
    add(turn, ui.Item { width = 2, height = 2,
      x = function() local _, _, x1 = ends() return (x1 or 0) - 1 end,
      y = function() local _, _, _, y1 = ends() return (y1 or 0) - 1 end,
      rotation = function() return hd() end, behavior = { rotation = motion },
      figures(style, { x = 8, y = -8, width = 48, height = 16, font_size = fs, font_weight = 600, color = style.ink,
        text = function() local p = wp(i) return p and tostring(p.name or "") or "" end,
        visible = inview }) })
  end
  for _, leg in ipairs(legs) do route[#route + 1] = ui.SdfShape { shape = "box", track = leg, radius = style.hatched and 0 or lt / 2 } end
  for _, mk in ipairs(marks) do route[#route + 1] = ui.SdfShape { shape = "box", track = mk, radius = style.hatched and 0 or 2 } end
  route[#route + 1] = ui.SdfShape { shape = "circle", x = ax - R + 12, y = ay - R + 12, width = 2 * R - 24, height = 2 * R - 24,
    operation = "intersect" }
  route[#route + 1] = ui.SdfShape { shape = "box", x = 2, y = 2, width = w - 4, height = ay + 8, operation = "intersect" }
  add(node, ui.Sdf(route))
  -- The track line: dashed from the aircraft along the track.
  local tl = add(node, ui.Item { x = ax - 1, y = ay - R + 14, width = 2, height = R - 14,
    transform_origin_y = 1, rotation = function() return ((trk0() - hd0() + 540) % 360) - 180 end,
    behavior = { rotation = motion } })
  add(tl, ui.Path { width = 2, height = R - 14, view_box = { 0, 0, 2, R - 14 }, d = ("M1 %g V0"):format(R - 14),
    fill_color = "transparent", stroke_color = U.alpha(style.ink, .7), stroke_width = 1.5, dash = { 4, 4 } })
  -- The aircraft and the heading box.
  plane(style, node, ax, ay, 22, style.hatched and color or style.ink)
  local bw, bh = 46, 22
  local layer = add(node, ui.Item { width = w, height = h })
  pointer_box(style, layer, ax - bw / 2, 4, bw, bh, "bottom", color,
    figures(style, { x = ax - bw / 2, y = 4, width = bw, height = bh, font_size = style.size.small, font_weight = 600,
      horizontal_alignment = "center", vertical_alignment = "center", color = style.hatched and color or style.on_accent,
      text = function() return ("%03d"):format(round(hd0()) % 360) end }))
  add(node, caption(style, { text = function() return ("Rng %d"):format(round(range())) end, x = 8, y = h - 20, width = 70 }))
  add(node, caption(style, { text = function() return ("Trk %03d"):format(round(trk0()) % 360) end, x = w - 88, y = h - 20,
    width = 80, horizontal_alignment = "right" }))
  return node
end

-- -------------------------------------------------------- range rings --

--- Concentric range rings about a centre, each labelled with its range,
--- bearing ticks round the outer one. A change of range swells the rings
--- out from the centre (Material) or steps them (Tsugumori). `spec`:
--- `range` (nm at the outer ring, 40), `rings` (4), `size` (180),
--- `heading` (turns the bearing ticks), `color`.
function M.range_rings(spec, style)
  local s = spec.size or math.min(spec.width or 180, spec.height or 180)
  local color = U.color(spec, style, "accent")
  local n = spec.rings or 4
  local function range() return tonumber(get(spec.range)) or 40 end
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), label_of(spec, "Range"), range, 0, 1000))
  local r = s / 2 - 2
  local c = s / 2
  local minor_c, major_c = tick_colors(style, color)
  if not style.hatched then
    add(node, ui.Rect { x = c - r, y = c - r, width = 2 * r, height = 2 * r, radius = r, color = style.raised })
  else
    add(node, ui.Item { width = s, height = s, style.marks() })
  end
  local hd0 = reader(spec.heading, 0)
  local hd = unwrap(node, "rings", hd0)
  local motion = travel(style, 160, 14)
  -- The rings swell from the centre when the range changes.
  local rings = add(node, ui.Item { width = s, height = s })
  local last
  morf.effect(name("rings"), function()
    local x = range()
    if last and x ~= last and not style.hatched then
      morf.animation.play { { node = rings, property = "scale", from = x < last and 1.12 or .88, to = 1, duration = 520,
        easing = { spline = { .2, 1.3, .4, 1.06, .7, 1, 1, 1 } } } }
    end
    last = x
  end, { owner = node })
  for k = 1, n do
    local rr = (r - 8) * k / n
    add(rings, overlay(s, s, { d = geo.arc(c, c, rr, 0, 360), stroke_color = k == n and major_c or minor_c,
      stroke_width = style.hatched and 1 or 1.5, dash = (style.hatched and k < n) and { 3, 3 } or nil }))
    local a = math.rad(45)
    add(rings, figures(style, { x = c + rr * math.sin(a) - 2, y = c - rr * math.cos(a) - 16, width = 34, height = 16,
      font_size = style.size.small - 3, font_weight = 500, color = k == n and style.ink or style.ink_lo,
      text = function() local x = range() * k / n return (x == math.floor(x)) and tostring(math.floor(x)) or ("%.1f"):format(x) end }))
  end
  -- Bearing ticks round the outside, every 10° with 30° long, and north:
  -- drawn once, turned with any heading.
  local turn = add(node, ui.Item { width = s, height = s, rotation = function() return -hd() end, behavior = { rotation = motion } })
  tick_ring(style, turn, s, r, 10, 6, 30, 10, color)
  add(turn, figures(style, { x = c - 10, y = 12, width = 20, height = 16, text = "N", font_size = style.size.small - 2,
    font_weight = 700, horizontal_alignment = "center", color = style.hatched and style.alert or color }))
  plane(style, node, c, c, 18, style.hatched and color or style.ink)
  add(node, caption(style, { text = spec.unit or "nm", x = 0, y = s - 16, width = 40 }))
  return node
end

-- ---------------------------------------------------- radar altimeter --

--- A radio altitude readout: the height above ground on rolling drums,
--- its box turning to the warning tone under `threshold` (the minimums).
--- `spec`: `altitude` (feet), `threshold` (200), `width` (170), `height`
--- (96), `color`.
function M.radar_altimeter(spec, style)
  local w, h = spec.width or 170, spec.height or 96
  local color = U.color(spec, style, "accent")
  local v = reader(spec.altitude or spec.value, 0)
  local function mins() return tonumber(get(spec.threshold)) or 200 end
  local function low() return v() < mins() end
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Radio altitude"), v, 0, 2500))
  local warn = style.warn
  if not style.hatched then
    add(node, ui.Rect { width = w, height = h, radius = style.radius(h / 2.5),
      color = function() return low() and get(warn) or get(style.raised) end,
      behavior = { color = { duration = style.motion.duration } } })
  else
    add(node, ui.Rect { width = w, height = h, color = function() return low() and get(warn):alpha(.1) or get(style.surface) end,
      border_width = 1, border_color = function() return low() and get(warn) or get(style.line) end,
      behavior = { color = settle(style) } })
    add(node, ui.Item { width = w, height = h, visible = low, style.stripes.box { width = w, height = h, gap = 6, weight = 1,
      color = U.alpha(warn, .2) } })
    add(node, ui.Item { width = w, height = h, style.marks() })
  end
  local ink = function()
    if style.hatched then return get(low() and warn or color) end
    return get(low() and style.on_accent or style.ink)
  end
  add(node, caption(style, { text = "RA", x = 12, y = 8, width = 40, color = ink }))
  add(node, caption(style, { text = function() return ("Mins %d"):format(round(mins())) end, x = w / 2, y = 8, width = w / 2 - 12,
    horizontal_alignment = "right", color = ink }))
  local size = math.min(style.size.extra, math.floor(h * .36))
  local row, rw, rh = drum_row(style, { value = function() return clamp(v(), 0, 9999) end, digits = 4, roll = 1, size = size,
    color = ink, motion = travel(style, 140, 18), fade = not style.hatched,
    dim = function() return get(ink()):alpha(.1) end })
  row.x, row.y = (w - rw) / 2, 26 + (h - 26 - rh) / 2
  add(node, row)
  return node
end

-- -------------------------------------------------- rolling digits --

--- Odometer figures: every figure on a drum, the last `roll` one drum
--- that turns smoothly between values in steps of `step`, each above it
--- turning a place when the one below carries; leading zeros dim.
--- Material a tonal plate, the rolling drum on the accent; Tsugumori
--- hairline cells, the rolling one hatched. `spec`: `value`, `digits`
--- (5), `roll` (1), `step` (1), `size` (the figures' size, 34), `label`,
--- `color`.
function M.rolling_digits(spec, style)
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value, 0)
  local digits = spec.digits or 5
  local roll = spec.roll or 1
  local size = spec.size or 34
  local pad = 8
  local row, rw, rh = drum_row(style, { value = v, digits = digits, roll = roll, step = spec.step, size = size,
    color = style.ink, roll_color = style.hatched and color or style.on_accent, motion = travel(style, 130, 15),
    height = math.ceil(size * 1.4), fade = true, dim = U.alpha(style.ink, .25) })
  local head = spec.label and 22 or 0
  local w, h = rw + 2 * pad, rh + 2 * pad + head
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), label_of(spec, "Counter"), v, 0, 10 ^ digits - 1))
  if spec.label then add(node, caption(style, { text = spec.label, x = 0, y = 0, width = w })) end
  local plate = add(node, ui.Item { y = head, width = w, height = rh + 2 * pad })
  local dw = math.ceil(size * .64)
  local roll_x = pad + (digits - math.min(roll, digits)) * dw
  if style.hatched then
    add(plate, ui.Rect { width = w, height = rh + 2 * pad, color = style.surface, border_width = 1, border_color = style.line })
    for k = 1, digits - roll - 1 do
      add(plate, ui.Rect { x = pad + k * dw, y = pad, width = 1, height = rh, color = style.stroke_of("idle", color) })
    end
    add(plate, ui.Rect { x = roll_x - 2, y = pad - 2, width = w - roll_x - pad + 4, height = rh + 4, color = U.alpha(color, .1),
      border_width = 1, border_color = color })
    add(plate, ui.Item { width = w, height = rh + 2 * pad, style.marks() })
  else
    add(plate, ui.Rect { width = w, height = rh + 2 * pad, radius = 14, color = style.raised })
    add(plate, ui.Rect { x = roll_x - 3, y = pad - 3, width = w - roll_x - pad + 6, height = rh + 6, radius = 10, color = color })
  end
  row.x, row.y = pad, pad
  add(plate, row)
  return node
end

-- ------------------------------------------------- turn coordinator --

--- A turn coordinator: a miniature aircraft banking with the rate of
--- turn against level and standard-rate marks, and the slip ball in its
--- curved tube. `spec`: `rate` (degrees a second; 3 is standard), `slip`
--- (-1..1), `size` (180), `color`.
function M.turn_coordinator(spec, style)
  local s = spec.size or math.min(spec.width or 180, spec.height or 180)
  local color = U.color(spec, style, "accent")
  local rate = reader(spec.rate or spec.value, 0)
  local slip = reader(spec.slip, 0)
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), label_of(spec, "Turn rate"),
    function() return rate() * 10 end, -60, 60))
  local r = s / 2
  local c = r
  disc(style, node, c, c, r, color)
  local minor_c, major_c = tick_colors(style, color)
  -- Level and standard-rate marks either side.
  local STD = 20
  local marks = {}
  for _, a in ipairs { 90, 90 + STD, 270, 270 - STD } do marks[#marks + 1] = a end
  add(node, overlay(s, s, { d = geo.ticks(c, c, r - 18, r - 5, { angles = marks }), stroke_color = major_c,
    stroke_width = style.hatched and 2 or 4, stroke_cap = style.hatched and "butt" or "round" }))
  add(node, caption(style, { text = "L", x = 10, y = c + r * .36, width = 20, horizontal_alignment = "center" }))
  add(node, caption(style, { text = "R", x = s - 30, y = c + r * .36, width = 20, horizontal_alignment = "center" }))
  add(node, caption(style, { text = "2 min", x = c - 30, y = c + r * .2, width = 60, horizontal_alignment = "center" }))
  -- The miniature aircraft, banking with the rate.
  local motion = travel(style, 120, 13)
  local span = (r - 22) * 2
  local turn = add(node, ui.Item { x = c - span / 2, y = c - span / 2 - 4, width = span, height = span,
    rotation = function() return clamp(rate() / 3 * STD, -40, 40) end, behavior = { rotation = motion } })
  local mid = span / 2
  local wing = style.hatched and 2 or 5
  add(turn, ui.Rect { x = 0, y = mid - wing / 2, width = span, height = wing, radius = style.hatched and 0 or wing / 2, color = style.ink })
  add(turn, ui.Rect { x = mid - 9, y = mid - 9, width = 18, height = 18, radius = style.hatched and 0 or 9,
    color = style.hatched and style.surface or style.ink, border_width = style.hatched and 1.5 or 0, border_color = style.hatched and style.ink or nil })
  add(turn, ui.Rect { x = mid - (style.hatched and 1 or 2), y = mid - 18, width = style.hatched and 2 or 4, height = 10,
    radius = style.hatched and 0 or 2, color = style.ink })
  -- The slip ball's tube: an arc low in the face; the ball rides it.
  local tr = r * 1.2
  local tcx, tcy = c, c + r * .62 - tr
  local SPREAD = 17
  local tube = 14
  add(node, overlay(s, s, { d = geo.arc(tcx, tcy, tr, 180 - SPREAD, 2 * SPREAD),
    stroke_color = style.hatched and style.stroke_of("quiet", color) or style.track, stroke_width = tube,
    stroke_cap = style.hatched and "butt" or "round" }))
  if style.hatched then
    add(node, overlay(s, s, { d = geo.arc(tcx, tcy, tr - tube / 2, 180 - SPREAD, 2 * SPREAD) .. geo.arc(tcx, tcy, tr + tube / 2, 180 - SPREAD, 2 * SPREAD),
      stroke_color = style.stroke_of("hot", color), stroke_width = 1 }))
  end
  -- the wires either side of the middle
  add(node, overlay(s, s, { d = geo.ticks(tcx, tcy, tr - tube / 2 - 2, tr + tube / 2 + 2, { angles = { 180 - 3.6, 180 + 3.6 } }),
    stroke_color = style.ink_lo, stroke_width = style.hatched and 1 or 1.5 }))
  local ball = add(node, ui.Item { x = tcx - tr, y = tcy - tr, width = 2 * tr, height = 2 * tr,
    rotation = function() return -clamp(slip(), -1, 1) * (SPREAD - 3) end, behavior = { rotation = travel(style, 90, 9) } })
  local bd = tube - 3
  add(ball, ui.Rect { x = tr - bd / 2, y = 2 * tr - bd / 2, width = bd, height = bd, radius = style.hatched and 0 or bd / 2,
    color = style.hatched and color or color })
  return node
end

-- --------------------------------------------------- vertical speed --

--- A vertical speed dial: zero at nine o'clock, climb above and descent
--- below to `max` either way, a needle and the rate read in figures.
--- `spec`: `fpm` (feet a minute), `max` (2000), `size` (180), `color`.
function M.vertical_speed(spec, style)
  local s = spec.size or math.min(spec.width or 180, spec.height or 180)
  local color = U.color(spec, style, "accent")
  local v = reader(spec.fpm or spec.value, 0)
  local max = spec.max or 2000
  local node = ui.Item(meter(U.place(spec, { width = s, height = s }), label_of(spec, "Vertical speed"), v, -max, max))
  local r = s / 2
  local c = r
  disc(style, node, c, c, r, color)
  local minor_c, major_c = tick_colors(style, color)
  local SWEEP = 150
  local function angle(f) return 270 + clamp(f / max, -1, 1) * SWEEP end
  local minors, majors = {}, {}
  local step = max / 20
  for i = -20, 20 do
    local a = angle(i * step) % 360
    if i % 5 == 0 then majors[#majors + 1] = a else minors[#minors + 1] = a end
  end
  local lw = style.hatched and 1 or 2
  add(node, overlay(s, s, { d = geo.ticks(c, c, r - 10, r - 4, { angles = minors }), stroke_color = minor_c, stroke_width = lw,
    stroke_cap = "butt" }))
  add(node, overlay(s, s, { d = geo.ticks(c, c, r - 15, r - 4, { angles = majors }), stroke_color = major_c, stroke_width = lw + .5,
    stroke_cap = "butt" }))
  -- climb and descent bands just inside the scale
  if not style.hatched then
    add(node, overlay(s, s, { d = geo.arc(c, c, r - 20, 270, SWEEP), stroke_color = U.alpha(style.ok, .55), stroke_width = 3, stroke_cap = "round" }))
    add(node, overlay(s, s, { d = geo.arc(c, c, r - 20, 270 - SWEEP, SWEEP), stroke_color = U.alpha(style.warn, .55), stroke_width = 3, stroke_cap = "round" }))
  else
    add(node, overlay(s, s, { d = geo.arc(c, c, r - 20, 270, SWEEP), stroke_color = style.stroke_of("mark", style.ok), stroke_width = 1, dash = { 2, 2 } }))
    add(node, overlay(s, s, { d = geo.arc(c, c, r - 20, 270 - SWEEP, SWEEP), stroke_color = style.stroke_of("mark", style.warn), stroke_width = 1, dash = { 2, 2 } }))
  end
  local fs = math.max(style.size.small - 3, math.floor(s / 15))
  for i = 0, 4 do
    for _, sign in ipairs(i == 0 and { 1 } or { 1, -1 }) do
      local f = sign * i * max / 4
      local a = math.rad(angle(f))
      local rl = r - 28
      add(node, figures(style, { x = c + rl * math.sin(a) - 14, y = c - rl * math.cos(a) - 8, width = 28, height = 16,
        text = tostring(math.floor(i * max / 400 + .5)), font_size = fs, font_weight = 500, horizontal_alignment = "center",
        vertical_alignment = "center", color = style.ink_lo }))
    end
  end
  add(node, caption(style, { text = "Up", x = c - r * .42 - 14, y = c - 30, width = 30, horizontal_alignment = "center" }))
  add(node, caption(style, { text = "Dn", x = c - r * .42 - 14, y = c + 14, width = 30, horizontal_alignment = "center" }))
  -- The reading.
  add(node, figures(style, { x = c + 2, y = c - 11, width = r * .6, height = 22, font_size = style.size.normal, font_weight = 600,
    horizontal_alignment = "right", vertical_alignment = "center", color = style.hatched and color or style.ink,
    text = function() local f = round(v() / 10) * 10 return (f > 0 and "+" or "") .. f end }))
  add(node, caption(style, { text = "fpm", x = c + 2, y = c + 10, width = r * .6, horizontal_alignment = "right" }))
  -- The needle.
  local len = r - 12
  local needle = add(node, ui.Item { x = c - len, y = c - len, width = 2 * len, height = 2 * len,
    rotation = function() return angle(v()) end, behavior = { rotation = travel(style, 140, 12) } })
  if style.hatched then
    add(needle, ui.Rect { x = len - 1, y = 0, width = 2, height = len, color = style.ink })
    add(needle, ui.Rect { x = len - 4, y = len - 4, width = 8, height = 8, color = style.surface, border_width = 1, border_color = style.ink })
  else
    add(needle, ui.Path { x = len - 6, y = 0, width = 12, height = len + 6, view_box = { 0, 0, 12, len + 6 },
      d = ("M6 0 L9 %g L9 %g L3 %g L3 %g Z"):format(len * .5, len + 6, len + 6, len * .5),
      fill_color = color, stroke_color = color, stroke_width = 2, stroke_join = "round" })
    add(needle, ui.Rect { x = len - 8, y = len - 8, width = 16, height = 16, radius = 8, color = color })
  end
  return node
end

-- ----------------------------------------------------- weather radar --

local function cells_of(spec)
  local v = spec.cells or spec.values
  if channel.is(v) then v = v:get() else v = get(v) end
  v = type(v) == "table" and v or {}
  if type(v[1]) == "table" then return v end
  -- a flat run, `columns` to a row (nearest range first)
  local cols = spec.columns or 12
  local rows = {}
  for i, x in ipairs(v) do
    local r = (i - 1) // cols + 1
    rows[r] = rows[r] or {}
    rows[r][(i - 1) % cols + 1] = x
  end
  return rows
end

--- A weather radar: an arc of range bins ahead of the aircraft, each cell
--- a sector coloured by its return -- light, moderate, heavy, extreme in
--- the style's ok, warn, alert and extra tones -- with range arcs and a
--- beam sweeping across. `spec`: `cells` (rows of range bins, nearest
--- first, each a run of 0..1 across the arc; or a flat list with
--- `columns`, or a channel), `range` (nm, 40), `sweep` (110: degrees),
--- `width` (260), `height` (190), `color`.
function M.weather_radar(spec, style)
  local w, h = spec.width or 260, spec.height or 190
  local color = U.color(spec, style, "accent")
  local sweep = spec.sweep or 110
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Weather radar")))
  case(style, node, 0, 0, w, h)
  local ax, ay = w / 2, h - 16
  local half = sweep / 2
  local R = math.min(ay - 10, (w / 2 - 14) / math.sin(math.rad(math.min(half + 6, 90))))
  local minor_c, major_c = tick_colors(style, color)
  local LEVELS = { { .2, "ok" }, { .45, "warn" }, { .7, "alert" }, { .88, "extra" } }
  local function band(x)
    local b = 0
    for i, l in ipairs(LEVELS) do if x >= l[1] then b = i end end
    return b
  end
  for i, l in ipairs(LEVELS) do
    local tone = style[l[2]]
    local gap = style.hatched and 0 or .8
    add(node, overlay(w, h, {
      d = function()
        local rows = cells_of(spec)
        local nr = #rows
        local parts = {}
        for ri, row in ipairs(rows) do
          local nc = #row
          local r0, r1 = R * (ri - 1) / nr, R * ri / nr
          for ci, x in ipairs(row) do
            if band(tonumber(x) or 0) == i then
              local a0 = -half + sweep * (ci - 1) / nc
              parts[#parts + 1] = geo.sector(ax, ay, r0 + (ri > 1 and gap or 0), r1 - gap, a0 + gap / 2, sweep / nc - gap)
            end
          end
        end
        return #parts > 0 and table.concat(parts, " ") or "M0 0"
      end,
      fill_color = U.alpha(tone, ({ style.hatched and .06 or .22, .32, .6, .92 })[i]),
      stroke_color = style.hatched and U.alpha(tone, i == 1 and .55 or .9) or "transparent", stroke_width = style.hatched and 1 or 0 }))
  end
  -- Range arcs and the edge of the scan.
  for k = 1, 4 do
    local rr = R * k / 4
    add(node, overlay(w, h, { d = geo.arc(ax, ay, rr, -half, sweep), stroke_color = k == 4 and major_c or minor_c,
      stroke_width = 1, dash = (k < 4) and { 3, 3 } or nil }))
  end
  add(node, overlay(w, h, { d = ("M%g %g L%g %g M%g %g L%g %g"):format(ax, ay, ax + R * math.sin(math.rad(-half)),
      ay - R * math.cos(math.rad(half)), ax, ay, ax + R * math.sin(math.rad(half)), ay - R * math.cos(math.rad(half))),
    stroke_color = minor_c, stroke_width = 1 }))
  local function range() return tonumber(get(spec.range)) or 40 end
  for k = 2, 4, 2 do
    local rr = R * k / 4
    local a = math.rad(-half + 3)
    add(node, figures(style, { x = ax + rr * math.sin(a) + 4, y = ay - rr * math.cos(a) - 2, width = 26, height = 16,
      font_size = style.size.small - 3, color = style.ink_lo,
      text = function() return tostring(round(range() * k / 4)) end }))
  end
  -- The beam, sweeping across and back.
  local beam = add(node, ui.Item { x = ax - R, y = ay - R, width = 2 * R, height = 2 * R, rotation = -half,
    loop = { rotation = { from = -half, to = half, duration = 3200, easing = "in_out_sine", alternate = true } } })
  add(beam, ui.Rect { x = R - (style.hatched and .5 or 1), y = 2, width = style.hatched and 1 or 2, height = R - 2,
    color = U.alpha(style.hatched and style.ink or color, .85) })
  plane(style, node, ax, ay - 2, 16, style.hatched and color or style.ink)
  add(node, caption(style, { text = spec.label or "Wx", x = 8, y = 6, width = 60 }))
  add(node, caption(style, { text = function() return ("%d nm"):format(round(range())) end, x = w - 68, y = 6, width = 60,
    horizontal_alignment = "right" }))
  return node
end

-- -------------------------------------------------------- FPV marker --

--- A flight path marker: the ring-and-wings symbol showing where the
--- aircraft is going, offset from the fixed boresight by the drift
--- (across) and the flight path angle (up), inside a box of degree marks.
--- Material's marker squashes along its motion. `spec`: `drift`,
--- `path` (degrees), `width` (230), `height` (170), `ppd`, `color`.
function M.flight_path_marker(spec, style)
  local w, h = spec.width or 230, spec.height or 170
  local color = U.color(spec, style, "accent")
  local drift, path = reader(spec.drift, 0), reader(spec.path or spec.fpa, 0)
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Flight path"),
    function() return ("drift %.1f, path %.1f"):format(drift(), path()) end))
  case(style, node, 0, 0, w, h)
  local cx, cy = w / 2, h / 2
  local ppd = spec.ppd or math.min(w, h) / 22
  local minor_c, major_c = tick_colors(style, color)
  -- Degree marks along the centre lines, every degree with every fifth long.
  local d = {}
  for i = -10, 10 do
    if i ~= 0 then
      local l = i % 5 == 0 and 7 or 3.5
      local x, y = cx + i * ppd, cy - i * ppd
      if x > 8 and x < w - 8 then d[#d + 1] = ("M%g %g V%g"):format(x, cy - l, cy + l) end
      if y > 8 and y < h - 8 then d[#d + 1] = ("M%g %g H%g"):format(cx - l, y, cx + l) end
    end
  end
  add(node, overlay(w, h, { d = table.concat(d, " "), stroke_color = minor_c, stroke_width = 1 }))
  add(node, overlay(w, h, { d = ("M%g %g H%g M%g %g V%g"):format(10, cy, w - 10, cx, 10, h - 10), stroke_color = U.alpha(minor_c, .5),
    stroke_width = 1, dash = { 2, 4 } }))
  -- The boresight: a fixed waterline mark.
  add(node, overlay(w, h, { d = ("M%g %g H%g L%g %g L%g %g L%g %g H%g"):format(cx - 22, cy, cx - 10, cx - 5, cy + 6, cx, cy, cx + 5,
      cy + 6, cx + 10, cy, cx + 22), stroke_color = style.ink_lo, stroke_width = style.hatched and 1 or 2,
    stroke_join = style.hatched and "miter" or "round", stroke_cap = style.hatched and "butt" or "round" }))
  -- The marker.
  local mw, mh = 54, 26
  local function off(v, lim) return clamp(v, -lim, lim) end
  local marker = add(node, ui.Item { width = mw, height = mh,
    x = cx - mw / 2, y = cy - mh / 2,
    translate_x = function() return off(drift() * ppd, w / 2 - mw / 2 - 6) end,
    translate_y = function() return -off(path() * ppd, h / 2 - mh / 2 - 22) end,
    behavior = { translate_x = travel(style, 120, 12), translate_y = travel(style, 120, 12) },
    stretch = (not style.hatched) and { scale = .14, max = .3 } or nil })
  local rr = 7
  local mcx, mcy = mw / 2, mh / 2 + 2
  local md = ("M%g %g A%g %g 0 1 1 %g %g A%g %g 0 1 1 %g %g M%g %g H%g M%g %g H%g M%g %g V%g"):format(
    mcx - rr, mcy, rr, rr, mcx + rr, mcy, rr, rr, mcx - rr, mcy,
    mcx - rr, mcy, mcx - rr - 14, mcx + rr, mcy, mcx + rr + 14, mcx, mcy - rr, mcy - rr - 8)
  if not style.hatched then
    add(marker, ui.Path { width = mw, height = mh, view_box = { 0, 0, mw, mh }, d = md, fill_color = "transparent",
      stroke_color = style.surface, stroke_width = 6.5, stroke_cap = "round" })
  end
  add(marker, ui.Path { width = mw, height = mh, view_box = { 0, 0, mw, mh }, d = md, fill_color = "transparent",
    stroke_color = color, stroke_width = style.hatched and 1.5 or 3, stroke_cap = style.hatched and "butt" or "round" })
  add(node, caption(style, { text = function() return ("Drift %+.1f°"):format(drift()) end, x = 8, y = h - 20, width = w / 2 - 8 }))
  add(node, caption(style, { text = function() return ("FPA %+.1f°"):format(path()) end, x = w / 2, y = h - 20, width = w / 2 - 8,
    horizontal_alignment = "right" }))
  return node
end

-- ------------------------------------------------------- EICAS strip --

--- An engine strip: each parameter a column -- its reading on top in the
--- tone of its zone, a bar gauge with warn and alert marks, its name at
--- the foot. Material pills that spring to their level; Tsugumori
--- hairline columns of whole cells. `spec`: `params` (a list or a
--- function of one: `{ label, value, min, max, warn, alert, unit }`, the
--- value a number or function), `width` (250), `height` (186).
function M.eicas_strip(spec, style)
  local w, h = spec.width or 250, spec.height or 186
  local list = get(spec.params) or {}
  local node = ui.Item(figure(U.place(spec, { width = w, height = h }), label_of(spec, "Engine indications"),
    function()
      local out = {}
      for _, p in ipairs(list) do out[#out + 1] = ("%s %s"):format(p.label or "", tostring(round(get(p.value)))) end
      return table.concat(out, ", ")
    end))
  local n = math.max(1, #list)
  local cw = w / n
  local top, foot = 24, 20
  local bh = h - top - foot - 6
  local motion = travel(style, 160, 13)
  for i, p in ipairs(list) do
    local x0 = (i - 1) * cw
    local lo, hi = p.min or 0, p.max or 100
    local function frac(x) return clamp(((tonumber(x) or 0) - lo) / (hi - lo), 0, 1) end
    local val = reader(p.value, lo)
    local function f() return frac(val()) end
    local warn, alert = p.warn and frac(p.warn), p.alert and frac(p.alert)
    local function tone()
      local x = val()
      if p.alert and x >= p.alert then return get(style.alert) end
      if p.warn and x >= p.warn then return get(style.warn) end
      return get(style.hatched and style.accent or style.ok)
    end
    local col = add(node, ui.Item { x = x0, width = cw, height = h, accessible_role = "meter",
      accessible_name = p.label or ("Parameter " .. i),
      accessible = { value = function() return round(val()) end, minimum = lo, maximum = hi } })
    add(col, figures(style, { x = 0, y = 0, width = cw, height = 22, font_size = style.size.normal, font_weight = 600,
      horizontal_alignment = "center", vertical_alignment = "center", color = tone,
      text = function() local x = val() return (math.abs(x) < 10 and x ~= math.floor(x)) and ("%.1f"):format(x) or tostring(round(x)) end }))
    local bw = math.min(16, cw * .3)
    local bx = cw / 2 - bw / 2
    local by = top
    if not style.hatched then
      add(col, ui.Rect { x = bx, y = by, width = bw, height = bh, radius = bw / 2, color = style.track })
      add(col, ui.Rect { x = bx, width = bw, radius = bw / 2, color = tone,
        y = function() return by + bh * (1 - f()) end, height = function() return math.max(bw, bh * f()) end,
        behavior = { y = motion, height = motion, color = { duration = style.motion.duration } } })
      for _, m in ipairs { { warn, style.warn }, { alert, style.alert } } do
        if m[1] then
          add(col, ui.Rect { x = bx + bw + 3, y = by + bh * (1 - m[1]) - 1.5, width = 7, height = 3, radius = 1.5, color = m[2] })
        end
      end
    else
      local cells = math.max(6, math.floor(bh / 7))
      local function lit() return math.floor(f() * cells + .5) / cells end
      add(col, ui.Rect { x = bx, y = by, width = bw, height = bh, color = "transparent", border_width = 1,
        border_color = style.stroke_of("mark") })
      local ih = bh - 4
      local seg = (ih - 2 * (cells - 1)) / cells
      -- The cells are drawn once; a clipping window over them rises in
      -- whole cells.
      add(col, ui.Item { x = bx + 2, width = bw - 4, clip = true,
        y = function() return by + 2 + ih * (1 - lit()) end, height = function() return ih * lit() end,
        behavior = { y = settle(style), height = settle(style) },
        ui.Path { anchors = { left = true, bottom = true }, width = bw - 4, height = ih, view_box = { 0, 0, bw - 4, ih },
          d = ("M%g %g V0"):format((bw - 4) / 2, ih), fill_color = "transparent", stroke_width = bw - 4, stroke_cap = "butt",
          dash = { seg, 2 }, stroke_color = tone } })
      for _, m in ipairs { { warn, style.warn }, { alert, style.alert } } do
        if m[1] then
          add(col, ui.Rect { x = bx + bw + 2, y = by + bh * (1 - m[1]) - .5, width = 8, height = 1, color = m[2] })
          add(col, ui.Rect { x = bx - 10, y = by + bh * (1 - m[1]) - .5, width = 8, height = 1, color = m[2] })
        end
      end
    end
    add(col, caption(style, { text = p.label or "", x = 0, y = h - foot, width = cw, horizontal_alignment = "center" }))
  end
  return node
end

return M
