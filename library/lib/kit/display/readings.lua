-- Display widgets: readings -- instruments that show one value (or a short
-- run of them) and take no input. See lib.kit.display.
--
-- Every reading lays out the same in each theme and draws in its style:
-- Material tonal, rounded and springy (a fill that overshoots and settles,
-- shapes that swell when lit); Tsugumori square, on hairlines and tick
-- rulers, hatched, its levels quantised into whole segments. Values may be
-- numbers or functions; the levels that stream (`sparkline`, `vu_meter`,
-- `peak_meter`) also take a data channel, and the run a sparkline draws is
-- a channel's `plot`, never a path built in Lua per sample.
local ui = require("morf.ui")
local morf = require("morf")
local U = require("lib.kit.display.util")
local channel = require("lib.util.channel")
local get, clamp01 = U.get, U.clamp01

local M = {}

-- ------------------------------------------------------------ shared --

local serial = 0
local function name(kind) serial = serial + 1 return ("kit.display.%s.%d"):format(kind, serial) end

local function settle(style) return { duration = style.motion.duration, easing = style.motion.easing } end
--- What a value travels with: a spring in Material (`k`, `d`), the
--- theme's settle in Tsugumori.
local function travel(style, k, d)
  if style.hatched then return settle(style) end
  return style.spring(k, d)
end

--- A 0..1 reader of `v`: a number, a function of one, or a channel (its newest).
local function reader(v)
  if channel.is(v) then return function() return clamp01(v:last() or 0) end end
  return function() return clamp01(get(v)) end
end

--- `f` in whole steps of `1/n` in Tsugumori; as it is in Material.
local function quant(style, f, n)
  if not style.hatched then return f end
  return function() return math.floor(f() * n + .5) / n end
end

local function percent(v) return function() return ("%d%%"):format(math.floor(v() * 100 + .5)) end end
local function round(x) return tostring(math.floor((tonumber(x) or 0) + .5)) end

--- A reading to assistive technology: a meter with its percentage.
local function meter(props, spec, v)
  props.accessible_role = "meter"
  if type(spec.label) == "string" then props.accessible_name = spec.label end
  props.accessible = function() return { value = math.floor(v() * 100 + .5), minimum = 0, maximum = 100 } end
  return props
end

--- Text in the style, never under the small size less three.
local function text(style, props)
  props.font_size = math.max(props.font_size or style.size.normal, style.size.small - 3)
  if props.color == nil then props.color = style.ink end
  return style.text(props)
end

local HEAD = 18
--- A caption row `w` wide: the label at the left, the reading at the right.
local function header(style, w, label, reading, color, y)
  local row = { width = w, height = HEAD, y = y or 0 }
  if label then
    row[#row + 1] = U.caption(style, { text = label, width = math.floor(w * (reading and .62 or 1)),
      elide = (not style.hatched) and "right" or nil })
  end
  if reading then
    row[#row + 1] = text(style, { text = reading, anchors = { right = true }, y = -1, width = math.floor(w * .38),
      height = HEAD, font_size = style.size.small, font_weight = 500, horizontal_alignment = "right",
      color = color or style.ink })
  end
  return ui.Item(row)
end

--- The colour of a level `x` (0..1) against `warn` and `alert`.
local function zone(style, x, warn, alert, base)
  if x >= alert then return style.alert end
  if x >= warn then return style.warn end
  return base
end

--- A Tsugumori segmented run: a stroke `thick` wide along `d`, cut into
--- `n` cells by its dash, drawn up to `trim_end`.
local function segment_line(props, length, n, thick, gap)
  local seg = math.max(0.5, (length - gap * (n - 1)) / n)
  props.fill_color, props.stroke_width, props.dash, props.stroke_cap = "transparent", thick, { seg, gap }, "butt"
  return ui.Path(props)
end

-- ---------------------------------------------------------- thermometer --

--- A thermometer: a bulb and the column over it, filled to `value`.
--- `spec`: `value` (0..1 or a function), `width` (120), `height` (180),
--- `label`, `unit` ("°"), `from`/`to` (0/100: what the reading shows),
--- `text` (fn -> string, the reading), `color`.
function M.thermometer(spec, style)
  local w, h = spec.width or 120, spec.height or 180
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value)
  local lo, hi, unit = spec.from or 0, spec.to or 100, spec.unit or "°"
  local reading = spec.text or function() return round(lo + (hi - lo) * v()) .. unit end
  local cw = spec.tube or math.max(8, math.min(16, math.floor(w * .12)))
  local bd = math.floor(cw * 2.3)
  local cx = math.floor(bd / 2) + 1
  local by = h - bd / 2 - 1                    -- the bulb's centre
  local top = 1                                -- the column's top edge
  local neck = by - bd / 2 + 2                 -- where the column meets the bulb
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), spec, v))
  local right = cx + bd / 2 + 10               -- the reading's column
  local rw = w - right
  if not style.hatched then
    local p = math.max(2, math.floor(cw * .22))
    local fw = cw - 2 * p
    -- The fill's column: where it stands this frame, springing to the level.
    local function level() return neck - (neck - top - p) * v() end
    local tube = ui.Item { x = cx - fw / 2, width = fw, y = level, height = function() return by - level() end,
      behavior = { y = travel(style, 170, 13), height = travel(style, 170, 13) } }
    ui.reparent(tube, node)
    -- Track and fill are distance fields: the column runs into its bulb
    -- through a round fillet, and the fill's neck does the same.
    ui.reparent(ui.Sdf { anchors = { fill = true }, blend = cw * .7, blend_profile = "circular",
      ui.SdfShape { shape = "circle", x = cx - bd / 2, y = by - bd / 2, width = bd, height = bd,
        fill_color = U.alpha(color, .2) },
      ui.SdfShape { shape = "box", x = cx - cw / 2, y = top, width = cw, height = by - top, radius = cw / 2,
        operation = "smooth_union", fill_color = U.alpha(color, .2) },
    }, node)
    ui.reparent(ui.Sdf { anchors = { fill = true }, blend = fw * .8, blend_profile = "circular",
      ui.SdfShape { shape = "circle", x = cx - bd / 2 + p, y = by - bd / 2 + p, width = bd - 2 * p, height = bd - 2 * p,
        fill_color = color },
      ui.SdfShape { shape = "box", radius = fw / 2, track = tube, operation = "smooth_union", fill_color = color },
    }, node)
    if rw >= 40 then
      local size = math.max(style.size.small, math.min(style.size.extra, math.floor(rw / 2.6)))
      ui.reparent(ui.Column { x = right, y = math.floor(h * .3), width = rw, gap = 0,
        text(style, { text = reading, font_size = size, font_weight = 500, height = math.ceil(size * 1.2) }),
        spec.label and U.caption(style, { text = spec.label, width = rw, elide = "right" }) or nil,
      }, node)
      ui.reparent(U.caption(style, { text = round(hi) .. unit, x = right, y = 0 }), node)
      ui.reparent(U.caption(style, { text = round(lo) .. unit, x = right, y = math.floor(neck - 14) }), node)
    end
    return node
  end
  -- Tsugumori: a hairline column of square cells lit from the reservoir,
  -- a tick ruler beside it, and the reservoir a hatched square.
  local colh = neck - top - 2
  local n = math.max(4, math.floor(colh / 7))
  local lit = quant(style, v, n)
  ui.reparent(ui.Rect { x = cx - cw / 2, y = top, width = cw, height = colh + 2, color = "transparent",
    border_width = 1, border_color = style.stroke_of("mark", color) }, node)
  local iw = cw - 4
  ui.reparent(segment_line({ x = cx - iw / 2, y = top + 2, width = iw, height = colh - 2, view_box = { 0, 0, iw, colh - 2 },
    d = ("M%g %g V0"):format(iw / 2, colh - 2), stroke_color = U.alpha(color, .12) }, colh - 2, n, iw, 2), node)
  ui.reparent(segment_line({ x = cx - iw / 2, y = top + 2, width = iw, height = colh - 2, view_box = { 0, 0, iw, colh - 2 },
    d = ("M%g %g V0"):format(iw / 2, colh - 2), stroke_color = color,
    opacity = function() return lit() > 0 and 1 or 0 end,
    trim_end = function() return math.max(.0001, lit()) end, behavior = { trim_end = settle(style) } },
    colh - 2, n, iw, 2), node)
  ui.reparent(U.fill(style, { x = cx - bd / 2, y = by - bd / 2, width = bd, height = bd, color = color, strong = true }), node)
  ui.reparent(ui.Rect { x = cx - 1, y = neck - 2, width = 2, height = 4, color = color }, node)
  local rx = cx + cw / 2 + 3
  ui.reparent(ui.Path { x = rx, y = top + 2, width = 8, height = colh - 2, view_box = { 0, 0, 8, colh - 2 },
    d = morf.geometry.ruler(colh - 2, 8, { pitch = (colh - 2) / n - .01, major = 5, minor = 4, vertical = true }),
    fill_color = "transparent", stroke_color = style.stroke_of("mark", color), stroke_width = 1 }, node)
  -- The head: a bright bar across column and ruler at the level.
  ui.reparent(ui.Rect { x = cx - cw / 2 - 2, width = cw + 15, height = 2, color = style.ink,
    y = function() return top + 2 + (colh - 2) * (1 - lit()) - 1 end, behavior = { y = settle(style) } }, node)
  if rw >= 40 then
    local size = math.max(style.size.small, math.min(style.size.extra, math.floor(rw / 2.8)))
    ui.reparent(ui.Column { x = right + 4, y = math.floor(h * .3), width = rw - 4, gap = 2,
      spec.label and U.caption(style, { text = spec.label, width = rw - 4 }) or nil,
      text(style, { text = reading, font_size = size, font_weight = 500, color = color, height = math.ceil(size * 1.2) }),
    }, node)
    ui.reparent(U.caption(style, { text = round(hi), x = right + 4, y = 0 }), node)
    ui.reparent(U.caption(style, { text = round(lo), x = right + 4, y = math.floor(neck - 14) }), node)
  end
  return node
end

-- ----------------------------------------------------------------- tank --

-- A strip of liquid surface `w` wide: a wave of amplitude `a` about `a0`
-- from the top, filled down to `sh`. The same run of moves for every
-- amplitude and phase, so one morphs into another.
local function wave(w, a0, a, phase, sh, waves)
  local n = 28
  local d = { ("M0 %.2f"):format(sh) }
  for i = 0, n do
    local x = w * i / n
    d[#d + 1] = ("L%.2f %.2f"):format(x, a0 + a * math.sin(phase + 2 * math.pi * waves * i / n))
  end
  d[#d + 1] = ("L%.2f %.2f Z"):format(w, sh)
  return table.concat(d, " ")
end

--- A vessel filled to `value`: Material a rounded tonal tank whose liquid
--- springs to its level and sloshes as it lands; Tsugumori a square frame
--- with a hatched fill in whole steps and a ruler up its side. `spec`:
--- `value`, `width` (150), `height` (170), `label`, `text` (fn, the
--- reading; the percentage when nil), `color`.
function M.tank(spec, style)
  local w, h = spec.width or 150, spec.height or 170
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value)
  local reading = spec.text or percent(v)
  local foot = (spec.label or reading) and HEAD + 6 or 0
  local vh = h - foot
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), spec, v))
  if not style.hatched then
    local R = math.min(22, math.floor(w / 5))
    local p = 6
    local iw, ih = w - 2 * p, vh - 2 * p
    local A = math.max(2, math.min(5, ih / 24))
    local function level() return ih * (1 - v()) end
    local function motion() return travel(style, 150, 10) end
    ui.reparent(ui.Rect { width = w, height = vh, radius = R, color = U.alpha(color, .16) }, node)
    -- The liquid is one path: its wavy surface, filled down past the
    -- vessel's floor, riding to the level; the vessel's inner corners cut
    -- it by a mask.
    local sh = ih + 2 * A + 2
    -- (No view box: path units are pixels, and the node, ending at the
    -- floor, cuts what reaches past it.)
    local surface = ui.Path { width = iw,
      d = wave(iw, A, A * .45, 0, sh, 1.5), morph_to = wave(iw, A, A, math.pi, sh, 1.5),
      morph_progress = 0, fill_color = color,
      y = function() return level() - A end, height = function() return ih - level() + A end,
      behavior = { y = motion(), height = motion() },
      opacity = function() return v() > .004 and 1 or 0 end }
    ui.reparent(ui.Item { x = p, y = p, width = iw, height = ih, clip = true,
      mask = ui.Rect { radius = R - p, color = style.ink },
      surface,
    }, node)
    -- A change of level sloshes the surface, which settles back to its
    -- gentle wave: crisp at rest.
    local slosh, last
    morf.effect(name("tank"), function()
      local x = v()
      if last == nil then last = x return end
      if math.abs(x - last) < .005 then return end
      last = x
      if slosh then slosh:stop() end
      slosh = morf.animation.play {
        { node = surface, property = "morph_progress", to = 1, duration = 220, easing = "out_sine" },
        { node = surface, property = "morph_progress", to = .25, duration = 300, easing = "in_out_sine" },
        { node = surface, property = "morph_progress", to = .6, duration = 320, easing = "in_out_sine" },
        { node = surface, property = "morph_progress", to = 0, duration = 520, easing = "out_cubic" },
      }
    end, { owner = node })
  else
    local p = 4
    local iw, ih = w - 2 * p, vh - 2 * p
    local n = math.max(4, math.floor(ih / 8))
    local lit = quant(style, v, n)
    local lx = 14
    local lw = iw - lx
    local function level() return ih * (1 - lit()) end
    ui.reparent(ui.Rect { width = w, height = vh, color = U.alpha(color, .03), border_width = 1,
      border_color = style.stroke_of("idle", color) }, node)
    ui.reparent(ui.Item { width = w, height = vh, style.marks() }, node)
    ui.reparent(ui.Path { x = p + 2, y = p, width = 9, height = ih, view_box = { 0, 0, 9, ih },
      d = morf.geometry.ruler(ih, 9, { pitch = ih / n - .01, major = 5, minor = 4, vertical = true }),
      fill_color = "transparent", stroke_color = style.stroke_of("mark", color), stroke_width = 1 }, node)
    -- The liquid: a hatched run held to the floor, its top in whole steps.
    local stripes = ui.Item { anchors = { left = true, bottom = true }, width = lw, height = ih,
      style.stripes.box { width = lw, height = ih, gap = 6, weight = 1.5, color = U.alpha(color, .75) } }
    ui.reparent(ui.Item { x = p + lx, width = lw, clip = true,
      y = function() return p + level() end, height = function() return ih - level() end,
      behavior = { y = settle(style), height = settle(style) },
      ui.Rect { anchors = { fill = true }, color = U.alpha(color, .16) },
      stripes,
    }, node)
    ui.reparent(ui.Rect { x = p + lx, width = lw, height = 2, color = color,
      y = function() return p + level() - 1 end, behavior = { y = settle(style) },
      opacity = function() return lit() > 0 and 1 or 0 end }, node)
  end
  if foot > 0 then
    ui.reparent(header(style, w, spec.label, reading, style.hatched and color or nil, vh + 6), node)
  end
  return node
end

-- -------------------------------------------------------------- led_bar --

--- A row of `count` lamps lit up to `value`, green, then `warn`'s tone
--- past `warn` (.7) and `alert`'s past `alert` (.9). Material lamps are
--- pills that square up as they light; Tsugumori's square cells, an
--- outline when dark. `spec`: `value`, `count` (10), `width` (240),
--- `height` (22), `warn`, `alert`, `label`, `color` (the low tone, ok).
function M.led_bar(spec, style)
  local w, h = spec.width or 240, spec.height or 22
  local count = spec.count or 10
  local warn, alert = spec.warn or .7, spec.alert or .9
  local base = U.color(spec, style, "ok")
  local v = reader(spec.value)
  local head = spec.label and HEAD + 4 or 0
  local node = ui.Item(meter(U.place(spec, { width = w, height = h + head }), spec, v))
  if spec.label then ui.reparent(header(style, w, spec.label, percent(v)), node) end
  local gap = math.max(3, math.floor(w / count * .18))
  local lw = (w - gap * (count - 1)) / count
  local function on() return math.floor(v() * count + .5) end
  for i = 1, count do
    local tone = zone(style, i / count, warn + 1e-6, alert + 1e-6, base)
    local function lit() return i <= on() end
    local x = (i - 1) * (lw + gap)
    if not style.hatched then
      local round_r = math.min(lw, h) / 2
      ui.reparent(ui.Rect { x = x, y = head, width = lw, height = h,
        radius = function() return lit() and round_r * .45 or round_r end,
        color = function() return lit() and U.get(tone) or U.get(tone):alpha(.18) end,
        behavior = { radius = style.spring(420, 18), color = { duration = style.motion.duration } } }, node)
    else
      ui.reparent(ui.Rect { x = x, y = head, width = lw, height = h,
        color = function() return lit() and U.get(tone) or U.get(tone):alpha(.06) end,
        border_width = 1, border_color = function() return U.get(tone):alpha(lit() and 1 or .45) end,
        behavior = { color = settle(style) } }, node)
    end
  end
  return node
end

-- -------------------------------------------------------- seven_segment --

-- Lit segments of each glyph (a top, b top right, c bottom right, d
-- bottom, e bottom left, f top left, g middle).
local GLYPH = {
  ["0"] = "abcdef", ["1"] = "bc", ["2"] = "abdeg", ["3"] = "abcdg", ["4"] = "bcfg", ["5"] = "acdfg",
  ["6"] = "acdefg", ["7"] = "abc", ["8"] = "abcdefg", ["9"] = "abcdfg", ["-"] = "g", ["_"] = "d", [" "] = "",
  ["°"] = "abfg", A = "abcefg", b = "cdefg", C = "adef", c = "deg", d = "bcdeg", E = "adefg", F = "aefg",
  G = "acdef", H = "bcefg", h = "cefg", I = "bc", J = "bcde", L = "def", n = "ceg", o = "cdeg", P = "abefg",
  r = "eg", S = "acdfg", t = "defg", U = "bcdef", u = "cde", y = "bcdfg",
}
local SEGMENTS = { "a", "b", "c", "d", "e", "f", "g" }

--- The cells `s` fills, right-aligned into `digits`: each its lit
--- segments, and whether a point or a colon follows it.
local function cells_of(s, digits)
  local cells = {}
  for _, cp in utf8.codes(s) do
    local ch = utf8.char(cp)
    if ch == "." or ch == "," then
      if #cells == 0 or cells[#cells].dp then cells[#cells + 1] = { segs = "" } end
      cells[#cells].dp = true
    elseif ch == ":" then
      if #cells == 0 then cells[#cells + 1] = { segs = "" } end
      cells[#cells].colon = true
    else
      cells[#cells + 1] = { segs = GLYPH[ch] or GLYPH[ch:upper()] or GLYPH[ch:lower()] or "" }
    end
  end
  local out = {}
  for k = 1, digits do out[k] = cells[#cells - digits + k] or { segs = "" } end
  return out
end

--- A seven-segment display of `digits` cells: real segment glyphs, the
--- unlit ones faint behind, points and colons between cells. Material
--- round-capped bars on a tonal plate; Tsugumori mitred bars in a
--- hairline frame. `spec`: `text` (fn -> string; digits, A-F and a few
--- letters, `-`, `.`, `:`) or `value` (fn -> number), `digits` (4),
--- `size` (the digit's height, 56), `label`, `color`.
function M.seven_segment(spec, style)
  local digits = spec.digits or 4
  local dh = spec.size or 56
  local color = U.color(spec, style, "accent")
  local source = spec.text or function()
    local x = get(spec.value)
    if type(x) == "number" then return round(x) end
    return tostring(x or "")
  end
  local t = math.max(3, dh * .11)
  local dw = math.floor(dh * .56)
  local sp = math.max(t * 1.7, math.floor(dh * .2))
  local pad = math.max(8, math.floor(dh * .22))
  local inner_w = digits * dw + (digits - 1) * sp + sp * .7
  local head = spec.label and HEAD + 2 or 0
  local W = math.floor(inner_w + 2 * pad)
  local H = math.floor(dh + 2 * pad + head)
  local L, R, T, MID, B = t / 2, dw - t / 2, t / 2, dh / 2, dh - t / 2
  local ENDS = {
    a = { L, T, R, T }, b = { R, T, R, MID }, c = { R, MID, R, B }, d = { L, B, R, B },
    e = { L, MID, L, B }, f = { L, T, L, MID }, g = { L, MID, R, MID },
  }
  -- One segment's outline, at `ox`: a round-capped line (Material) or a
  -- mitred bar (Tsugumori).
  local function segment_d(key, ox)
    local e = ENDS[key]
    local x1, y1, x2, y2 = e[1] + ox, e[2], e[3] + ox, e[4]
    local ux, uy = (x2 - x1), (y2 - y1)
    local len = math.sqrt(ux * ux + uy * uy)
    ux, uy = ux / len, uy / len
    if not style.hatched then
      local s = t * .78
      return ("M%.2f %.2f L%.2f %.2f "):format(x1 + ux * s, y1 + uy * s, x2 - ux * s, y2 - uy * s)
    end
    local g, k = t * .16 + .5, t / 2
    local nx, ny = -uy, ux
    local ax, ay, bx, by = x1 + ux * g, y1 + uy * g, x2 - ux * g, y2 - uy * g
    return ("M%.2f %.2f L%.2f %.2f L%.2f %.2f L%.2f %.2f L%.2f %.2f L%.2f %.2f Z "):format(
      ax, ay, ax + ux * k + nx * k, ay + uy * k + ny * k, bx - ux * k + nx * k, by - uy * k + ny * k,
      bx, by, bx - ux * k - nx * k, by - uy * k - ny * k, ax + ux * k - nx * k, ay + uy * k - ny * k)
  end
  local function seg_path(props)
    props.width, props.height, props.view_box = inner_w, dh, { 0, 0, inner_w, dh }
    if not style.hatched then
      props.stroke_color, props.fill_color = props.color, "transparent"
      props.stroke_width, props.stroke_cap = t, "round"
    else
      props.fill_color = props.color
    end
    props.color = nil
    return ui.Path(props)
  end
  local cache_s, cache_c
  local function cells()
    local s = tostring(get(source) or "")
    if s ~= cache_s then cache_s, cache_c = s, cells_of(s, digits) end
    return cache_c
  end
  local face = ui.Item { x = pad, y = pad + head, width = inner_w, height = dh }
  local dark = {}
  local fade = { duration = style.hatched and 90 or style.motion.duration, easing = "out_cubic" }
  for k = 1, digits do
    local ox = (k - 1) * (dw + sp)
    for _, key in ipairs(SEGMENTS) do
      dark[#dark + 1] = segment_d(key, ox)
      ui.reparent(seg_path { d = segment_d(key, ox), color = color,
        opacity = function() return cells()[k].segs:find(key, 1, true) and 1 or 0 end,
        behavior = { opacity = fade } }, face)
    end
    -- The point and the colon in the gap after the digit.
    local dot = math.max(3, math.floor(t))
    local px = ox + dw + (sp - dot) / 2
    ui.reparent(ui.Rect { x = px, y = dh - dot, width = dot, height = dot, radius = style.hatched and 0 or dot / 2,
      color = color, opacity = function() return cells()[k].dp and 1 or .08 end, behavior = { opacity = fade } }, face)
    if k < digits then
      for _, at in ipairs { .3, .7 } do
        ui.reparent(ui.Rect { x = px, y = dh * at - dot / 2, width = dot, height = dot,
          radius = style.hatched and 0 or dot / 2, color = color,
          opacity = function() return cells()[k].colon and 1 or 0 end, behavior = { opacity = fade } }, face)
      end
    end
  end
  -- Every segment, unlit, faint behind the lit ones.
  local unlit = seg_path { d = table.concat(dark), color = U.alpha(color, style.hatched and .07 or .08) }
  unlit.x, unlit.y = pad, pad + head
  local kids = { unlit, face }
  if spec.label then
    local row = header(style, W - 2 * pad, spec.label, nil, nil, pad - 4)
    row.x = pad
    kids[#kids + 1] = row
  end
  local props = U.place(spec, { width = W, height = H })
  props.accessible_role, props.accessible_name = "label", type(spec.label) == "string" and spec.label or nil
  return U.box(style, props, kids)
end

-- ------------------------------------------------------------- vu_meter --

--- A VU meter: a needle swinging over an arc scale from -20 to +3, the
--- red zone past 0 VU. Material's needle springs and overshoots like a
--- real movement; Tsugumori's settles over a tick ruler. `spec`: `value`
--- (0..1, a function or a channel of levels: its newest), `width` (240),
--- `height` (150), `label` ("VU"), `color`.
function M.vu_meter(spec, style)
  local w, h = spec.width or 240, spec.height or 150
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value)
  local SWEEP, FROM = 100, -50
  local cx, cy = w / 2, h - 16
  local R = math.floor(math.min((w / 2 - 22) / math.sin(math.rad(50)), cy - 36))
  local red = .75
  local function path(props)
    props.width, props.height, props.view_box, props.fill_color = w, h, { 0, 0, w, h }, props.fill_color or "transparent"
    return ui.Path(props)
  end
  local kids = {}
  local arc = morf.geometry.arc
  if not style.hatched then
    local thick = math.max(5, math.floor(R / 16))
    local gapdeg = math.deg((thick + 4) / R)
    kids[#kids + 1] = path { d = arc(cx, cy, R, FROM, SWEEP * red - gapdeg), stroke_width = thick, stroke_cap = "round",
      stroke_color = U.alpha(color, .22) }
    kids[#kids + 1] = path { d = arc(cx, cy, R, FROM + SWEEP * red + gapdeg / 2, SWEEP * (1 - red) - gapdeg / 2),
      stroke_width = thick, stroke_cap = "round", stroke_color = style.alert }
    -- Dots at the five marks, a pill-round stroke too short to be a line.
    kids[#kids + 1] = path { d = morf.geometry.ticks(cx, cy, R - thick - 6, R - thick - 5.9,
      { from = FROM, sweep = SWEEP, count = 4 }), stroke_width = 4, stroke_cap = "round", stroke_color = style.ink_lo }
    kids[#kids + 1] = path { d = morf.geometry.ticks(cx, cy, R - thick - 6, R - thick - 5.9,
      { from = FROM, sweep = SWEEP, count = 20 }), stroke_width = 2, stroke_cap = "round",
      stroke_color = U.alpha(style.ink_lo, .5) }
  else
    kids[#kids + 1] = path { d = arc(cx, cy, R, FROM, SWEEP), stroke_width = 1, stroke_color = style.stroke_of("mark", color) }
    kids[#kids + 1] = path { d = morf.geometry.ticks(cx, cy, R - 5, R, { from = FROM, sweep = SWEEP, count = 20, major = 5,
      major_r0 = R - 10 }), stroke_width = 1, stroke_color = style.stroke_of("hot", color) }
    local seg = 2 * math.pi * (R + 6) * SWEEP * (1 - red) / 360
    kids[#kids + 1] = path { d = arc(cx, cy, R + 6, FROM + SWEEP * red, SWEEP * (1 - red)), stroke_width = 5,
      stroke_cap = "butt", dash = { math.max(0.5, seg / 6 - 2), 2 }, stroke_color = style.alert }
    kids[#kids + 1] = path { d = arc(cx, cy, R + 6, FROM, SWEEP * red), stroke_width = 5, stroke_cap = "butt",
      dash = { 3, 3 }, stroke_color = U.alpha(color, .25) }
  end
  -- The scale's figures, above the arc.
  for k, mark in ipairs { "-20", "-10", "-5", "0", "+3" } do
    local a = math.rad(FROM + SWEEP * (k - 1) / 4)
    local rr = R + (style.hatched and 20 or 16)
    kids[#kids + 1] = text(style, { text = mark, x = cx + rr * math.sin(a) - 18, y = cy - rr * math.cos(a) - 9,
      width = 36, height = 18, font_size = style.size.small - 3, horizontal_alignment = "center",
      color = k == 5 and style.alert or style.ink_lo })
  end
  kids[#kids + 1] = U.caption(style, { text = spec.label or "VU", x = cx - 40, y = cy - R * .48, width = 80,
    horizontal_alignment = "center", font_size = style.size.small })
  -- The needle, turning about the pivot.
  local L = R + 4
  local nw = style.hatched and 2 or 4
  kids[#kids + 1] = ui.Item { x = cx - L, y = cy - L, width = 2 * L, height = 2 * L,
    rotation = function() return FROM + SWEEP * v() end, behavior = { rotation = travel(style, 160, 9) },
    ui.Rect { x = L - nw / 2, y = 0, width = nw, height = L - 6, radius = style.hatched and 0 or nw / 2,
      color = style.hatched and style.ink or color } }
  local hub = style.hatched and 8 or 16
  kids[#kids + 1] = ui.Rect { x = cx - hub / 2, y = cy - hub / 2, width = hub, height = hub,
    radius = style.hatched and 0 or hub / 2, color = style.hatched and color or style.ink }
  local node = ui.Item(meter(U.place(spec, { width = w, height = h }), spec, v))
  ui.reparent(U.box(style, { width = w, height = h }, kids), node)
  return node
end

-- ----------------------------------------------------------- peak_meter --

--- A level bar with a peak-hold marker: the peak jumps with the level,
--- holds, then falls back. `spec`: `value` (0..1, a function or a channel
--- of levels), `peak` (fn -> 0..1; held here when nil), `hold` (ms, 700),
--- `width` (240), `height` (14), `warn` (.75), `alert` (.9), `label`,
--- `text` (the reading), `color`.
function M.peak_meter(spec, style)
  local w, h = spec.width or 240, spec.height or 14
  local color = U.color(spec, style, "accent")
  local warn, alert = spec.warn or .75, spec.alert or .9
  local v = reader(spec.value)
  local head = (spec.label or spec.text) and HEAD + 4 or 0
  local held = morf.state { peak = 0 }
  local peak = spec.peak and reader(spec.peak) or function() return held.peak end
  local props = meter(U.place(spec, { width = w, height = h + head }), spec, v)
  local hold_timer, fall_timer
  props.on_destroyed = function()
    if hold_timer then hold_timer:cancel() end
    if fall_timer then fall_timer:cancel() end
  end
  local node = ui.Item(props)
  if not spec.peak then
    -- Hold the highest level; after `hold` ms fall back towards the level.
    local top = 0
    local function fall()
      fall_timer = morf.timer(40, function()
        local now = v()
        top = math.max(now, top - .015)
        held.peak = top
        if top <= now and fall_timer then fall_timer:cancel() fall_timer = nil end
      end, true)
    end
    morf.effect(name("peak"), function()
      local x = v()
      if x < top then
        -- Below a peak no longer held: fall towards the level.
        if not hold_timer and not fall_timer then fall() end
        return
      end
      top = x
      held.peak = x
      if hold_timer then hold_timer:cancel() end
      if fall_timer then fall_timer:cancel() fall_timer = nil end
      hold_timer = morf.timer(spec.hold or 700, function()
        hold_timer = nil
        fall()
      end, false)
    end, { owner = node })
  end
  local reading = spec.text or (spec.label and function()
    local x = v()
    if x <= .001 then return "-∞ dB" end
    return ("%.1f dB"):format(20 * math.log(x, 10))
  end) or nil
  if head > 0 then ui.reparent(header(style, w, spec.label, reading), node) end
  local function tone_of(x) return function() return U.get(zone(style, x(), warn, alert, color)) end end
  if not style.hatched then
    ui.reparent(ui.Rect { y = head, width = w, height = h, radius = h / 2, color = U.alpha(color, .18) }, node)
    ui.reparent(ui.Rect { y = head, height = h, radius = h / 2, color = tone_of(v),
      width = function() local x = v() return x <= 0 and 0 or math.max(h, x * w) end,
      behavior = { width = style.spring(520, 34), color = { duration = style.motion.duration } } }, node)
    local mw = 4
    ui.reparent(ui.Rect { y = head - 3, width = mw, height = h + 6, radius = mw / 2, color = tone_of(peak),
      x = function() return math.max(0, math.min(w - mw, peak() * w - mw / 2)) end,
      opacity = function() return peak() > .004 and 1 or 0 end,
      behavior = { x = { duration = 60 } } }, node)
  else
    local n = math.max(8, math.floor(w / 7))
    local lit = quant(style, v, n)
    local d = ("M0 %g H%g"):format(h / 2, w)
    local function run(props)
      props.x, props.y, props.width, props.height, props.view_box, props.d = 0, head, w, h, { 0, 0, w, h }, d
      return segment_line(props, w, n, h, 2)
    end
    ui.reparent(run { stroke_color = U.alpha(color, .12) }, node)
    -- The run in its zones: the tone, then warn, then alert.
    local edges = { { 0, warn, color }, { warn, alert, style.warn }, { alert, 1, style.alert } }
    for _, z in ipairs(edges) do
      local a, b, tone = z[1], z[2], z[3]
      ui.reparent(run { stroke_color = tone, trim_start = a,
        trim_end = function() return math.max(a + .0001, math.min(b, lit())) end,
        opacity = function() return lit() > a and 1 or 0 end, behavior = { trim_end = settle(style) } }, node)
    end
    local cell = (w - 2 * (n - 1)) / n
    local qpeak = quant(style, peak, n)
    ui.reparent(ui.Rect { y = head - 2, width = math.max(2, cell), height = h + 4, color = style.ink,
      x = function() return math.max(0, math.min(w - cell, (math.max(1, math.floor(qpeak() * n + .5)) - 1) * (cell + 2))) end,
      opacity = function() return qpeak() > 0 and 1 or 0 end, behavior = { x = { duration = 60 } } }, node)
  end
  return node
end

-- -------------------------------------------------------------- compass --

local POINTS = { "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW" }

--- A compass: a card with its ticks and N / E / S / W turning under a
--- fixed index, the heading read out in the middle. The card takes the
--- short way round. `spec`: `heading` (degrees, or a function), `size`
--- (170), `label`, `color`.
function M.compass(spec, style)
  local s = spec.size or 170
  local c = s / 2
  local color = U.color(spec, style, "accent")
  local function heading() return (tonumber(get(spec.heading)) or 0) % 360 end
  local turn = morf.state { angle = 0 }
  local props = U.place(spec, { width = s, height = s, accessible_role = "meter",
    accessible_name = type(spec.label) == "string" and spec.label or "Heading",
    accessible = function() return { value = math.floor(heading() + .5), minimum = 0, maximum = 360 } end })
  local node = ui.Item(props)
  local last, acc
  morf.effect(name("compass"), function()
    local hd = heading()
    if not last then last, acc = hd, hd
    else
      acc = acc + ((hd - last + 540) % 360 - 180)
      last = hd
    end
    turn.angle = acc
  end, { owner = node })
  local function path(props2)
    props2.anchors, props2.view_box, props2.fill_color = { fill = true }, { 0, 0, s, s }, props2.fill_color or "transparent"
    return ui.Path(props2)
  end
  local ro = c - (style.hatched and 2 or 1)
  local card = { anchors = { fill = true }, rotation = function() return -turn.angle end,
    behavior = { rotation = travel(style, 90, 11) } }
  local rl = ro - (style.hatched and 26 or 28)            -- the letters' radius
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = c, color = style.raised }, node)
    card[#card + 1] = path { d = morf.geometry.ticks(c, c, ro - 10, ro - 6, { count = 72, major = 6, major_r0 = ro - 14 }),
      stroke_width = 1.5, stroke_cap = "round", stroke_color = U.alpha(style.ink_lo, .55) }
  else
    ui.reparent(ui.Item { anchors = { fill = true }, style.marks() }, node)
    card[#card + 1] = path { d = morf.geometry.arc(c, c, ro, 0, 360), stroke_width = 1,
      stroke_color = style.stroke_of("mark", color) }
    card[#card + 1] = path { d = morf.geometry.ticks(c, c, ro - 5, ro, { count = 72, major = 6, major_r0 = ro - 11 }),
      stroke_width = 1, stroke_color = style.stroke_of("hot", color) }
    card[#card + 1] = path { d = morf.geometry.arc(c, c, ro - 18, 0, 360), stroke_width = 1, dash = { 2, 3 },
      stroke_color = style.stroke_of("idle", color) }
  end
  for k, letter in ipairs { "N", "E", "S", "W" } do
    local a = math.rad((k - 1) * 90)
    local north = letter == "N"
    card[#card + 1] = ui.Item { x = c + rl * math.sin(a) - 14, y = c - rl * math.cos(a) - 12, width = 28, height = 24,
      rotation = (k - 1) * 90,
      text(style, { text = letter, anchors = { fill = true }, font_size = style.size.normal, font_weight = north and 700 or 500,
        horizontal_alignment = "center", vertical_alignment = "center",
        color = north and (style.hatched and style.alert or color) or style.ink_lo }) }
  end
  if not style.hatched then
    -- North's marker: a small expressive triangle at the card's rim.
    card[#card + 1] = style.kit.shape { x = c - 6, y = c - ro + 3, width = 12, height = 11, shape = "triangle",
      color = color }
  else
    card[#card + 1] = ui.Rect { x = c - 3, y = c - ro + 13, width = 6, height = 6, color = style.alert }
  end
  ui.reparent(ui.Item(card), node)
  -- The index over the card and the readout in the middle.
  local ri = math.floor(s * .25)
  local reading = function() return round(heading()) % 360 .. "°" end
  local point = function() return POINTS[math.floor(heading() / 22.5 + .5) % 16 + 1] end
  if not style.hatched then
    ui.reparent(ui.Rect { x = c - ri, y = c - ri, width = 2 * ri, height = 2 * ri, radius = ri, color = style.surface }, node)
    -- The index: an expressive triangle on the disc, pointing at the card.
    ui.reparent(style.kit.shape { x = c - 7, y = c - ri - 9, width = 14, height = 12, shape = "triangle",
      color = style.ink }, node)
  else
    ui.reparent(ui.Rect { x = c - 1, y = 0, width = 2, height = 18, color = style.ink }, node)
    ui.reparent(ui.Rect { x = c - ri, y = c - ri * .8, width = 2 * ri, height = 1.6 * ri, color = style.surface,
      border_width = 1, border_color = style.stroke_of("mark", color) }, node)
  end
  ui.reparent(ui.Column { x = c - ri, y = c - math.floor(s * .1), width = 2 * ri, gap = 0, align = "center",
    text(style, { text = reading, font_size = math.max(style.size.normal, math.min(style.size.large, math.floor(s * .12))),
      font_weight = 500, width = 2 * ri, horizontal_alignment = "center", color = style.hatched and color or style.ink }),
    U.caption(style, { text = function() return spec.label and (get(spec.label) .. " " .. point()) or point() end,
      width = 2 * ri, horizontal_alignment = "center" }),
  }, node)
  return node
end

-- ------------------------------------------------------------ sparkline --

--- A small trend line with no axes: the newest value at the right edge
--- and a dot on it. Drawn from a data channel by the engine (no path
--- built in Lua per sample). `spec`: `values` (a list, a function of one,
--- or a channel), `width` (220), `height` (56), `area` (true: filled
--- under the line), `samples` (how many across; the list's length when
--- nil), `bottom` (0), `top` (the peak with headroom when nil), `label`,
--- `format` (fn(value) -> string, the reading), `color`.
function M.sparkline(spec, style)
  local w, h = spec.width or 220, spec.height or 56
  local color = U.color(spec, style, "accent")
  local ch, feed = channel.from(spec.values or {}, { size = 4096 })
  local samples = spec.samples
  if not samples then
    local list = spec.values
    if type(list) == "function" then list = list() end
    samples = type(list) == "table" and not channel.is(list) and #list or 60
  end
  samples = math.max(2, samples)
  local head = spec.label and HEAD + 4 or 0
  local ph = h - head
  local bottom = spec.bottom or 0
  local function top()
    if spec.top then return math.max(bottom + 1e-9, get(spec.top)) end
    return math.max((ch:peak() or 0) * 1.12, bottom + (spec.floor or 1))
  end
  local PT, PB = 4, 2
  local dot = style.hatched and 5 or 9
  local pw = w - dot / 2 - 1
  local function plot(kind)
    return function()
      return { kind = kind, width = pw, height = ph, samples = samples, bottom = bottom, top = top(),
        pad_top = PT, pad_bottom = PB, smooth = not style.hatched, hatch = 5 }
    end
  end
  local box = { 0, 0, w, ph }
  local area = spec.area ~= false
  local plot_box = { y = head, width = w, height = ph }
  if style.hatched then
    plot_box[#plot_box + 1] = ui.Rect { y = ph - 1, width = w, height = 1, color = style.stroke_of("idle", color) }
    if area then
      plot_box[#plot_box + 1] = ui.Path { anchors = { fill = true }, view_box = box, series = ch.id, plot = plot("steps_area"),
        fill_color = U.alpha(color, .08) }
      plot_box[#plot_box + 1] = ui.Path { anchors = { fill = true }, view_box = box, series = ch.id, plot = plot("hatch_steps"),
        fill_color = "transparent", stroke_color = U.alpha(color, .5), stroke_width = 1, stroke_cap = "butt" }
    end
    plot_box[#plot_box + 1] = ui.Path { anchors = { fill = true }, view_box = box, series = ch.id, plot = plot("steps"),
      fill_color = "transparent", stroke_color = color, stroke_width = 1.5, stroke_join = "miter" }
  else
    if area then
      plot_box[#plot_box + 1] = ui.Path { anchors = { fill = true }, view_box = box, series = ch.id, plot = plot("area"),
        fill_color = U.alpha(color, .1) }
    end
    plot_box[#plot_box + 1] = ui.Path { anchors = { fill = true }, view_box = box, series = ch.id, plot = plot("line"),
      fill_color = "transparent", stroke_color = color, stroke_width = 2.5, stroke_cap = "round", stroke_join = "round" }
  end
  -- The newest value's dot.
  local function last_y()
    local t, x = top(), tonumber(ch:last()) or bottom
    local f = math.max(0, math.min(1, (x - bottom) / (t - bottom)))
    return PT + (1 - f) * (ph - PT - PB)
  end
  plot_box[#plot_box + 1] = ui.Rect { x = pw - dot / 2, width = dot, height = dot,
    radius = style.hatched and 0 or dot / 2, color = style.hatched and style.ink or color,
    border_width = style.hatched and 0 or 2, border_color = style.surface,
    y = function() return last_y() - dot / 2 end, behavior = { y = { duration = 120 } },
    opacity = function() return (ch:len() or 0) > 0 and 1 or 0 end }
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  if spec.label then
    local fmt = spec.format or round
    ui.reparent(header(style, w, spec.label, function() return fmt(tonumber(ch:last()) or 0) end,
      style.hatched and color or nil), node)
  end
  ui.reparent(ui.Item(plot_box), node)
  feed(node)
  return node
end

-- ------------------------------------------------------ segmented_meter --

--- A level in `segments` cells: Material pills, the lit ones swelling to
--- full height and the dark ones slim; Tsugumori square cells, outlined
--- dark, lit solid in whole steps. `spec`: `value`, `segments` (12),
--- `width` (240), `height` (18), `label`, `text` (the reading; the
--- percentage when nil), `color`.
function M.segmented_meter(spec, style)
  local w, h = spec.width or 240, spec.height or 18
  local n = spec.segments or 12
  local color = U.color(spec, style, "accent")
  local v = reader(spec.value)
  local head = spec.label and HEAD + 4 or 0
  local node = ui.Item(meter(U.place(spec, { width = w, height = h + head }), spec, v))
  if spec.label then ui.reparent(header(style, w, spec.label, spec.text or percent(v)), node) end
  local gap = math.max(3, math.floor(w / n * .15))
  local sw = (w - gap * (n - 1)) / n
  if not style.hatched then
    local function on() return math.floor(v() * n + .5) end
    local slim = math.max(4, math.floor(h * .4))
    for i = 1, n do
      local function lit() return i <= on() end
      local function motion() return style.spring(380, 16) end
      ui.reparent(ui.Rect { x = (i - 1) * (sw + gap), width = sw,
        y = function() return head + (lit() and 0 or (h - slim) / 2) end,
        height = function() return lit() and h or slim end,
        radius = function() return math.min(sw, lit() and h or slim) / 2 end,
        color = function() return lit() and U.get(color) or U.get(color):alpha(.22) end,
        behavior = { y = motion(), height = motion(), radius = motion(), color = { duration = style.motion.duration } } }, node)
    end
    return node
  end
  local lit = quant(style, v, n)
  ui.reparent(ui.Path { x = 0, y = head, width = w, height = h, view_box = { -.5, -.5, w + 1, h + 1 },
    d = morf.geometry.segments(w, h, n, gap), fill_color = U.alpha(color, .05), stroke_width = 1,
    stroke_color = style.stroke_of("idle", color) }, node)
  ui.reparent(segment_line({ x = 0, y = head, width = w, height = h, view_box = { 0, 0, w, h },
    d = ("M0 %g H%g"):format(h / 2, w), stroke_color = color,
    opacity = function() return lit() > 0 and 1 or 0 end,
    trim_end = function() return math.max(.0001, lit()) end, behavior = { trim_end = settle(style) } }, w, n, h, gap), node)
  return node
end

return M
