-- Display widgets: status. See lib.kit.display.
--
-- Badges, dots and LEDs, tags, progress in its shapes (bar, ring,
-- semicircle, segments), battery and signal bars, and the status
-- surfaces: a card, a banner, an empty state, a result page, a skeleton
-- and a toast. The layouts are shared; the look is the style's --
-- Material tonal, round and sprung, Tsugumori square, hairline, hatched
-- and quantised.
local morf = require("morf")
local ui = require("morf.ui")
local U = require("lib.kit.display.util")

local M = {}
local get = U.get

-- A spec's kind, as a style colour role.
local KIND = { ok = "ok", success = "ok", clear = "ok", warn = "warn", warning = "warn",
  alert = "alert", error = "alert", danger = "alert", critical = "alert",
  info = "info", accent = "accent", extra = "extra" }
-- The kind as an emblem knows it (ok, warn, alert, info).
local EMBLEM = { ok = "ok", warn = "warn", alert = "alert", info = "info", accent = "info", extra = "info" }
-- Tsugumori's code word for a kind.
local CODE = { ok = "CLEAR", warn = "WARNING", alert = "SERVICE", info = "NOTICE", accent = "NOTICE", extra = "NOTICE" }

local function role(kind) return KIND[get(kind) or "info"] or "info" end

--- The colour of a spec: `color` (a colour, function or role name), else
--- its `kind`'s role, else `fallback`.
local function tone(spec, style, fallback)
  if spec.color ~= nil then return U.color(spec, style, fallback) end
  if spec.kind ~= nil then return function() return get(style[role(spec.kind)]) end end
  return style[fallback or "accent"]
end

-- Floors every font size at the smallest the kit allows.
local function fs(style, size) return math.max(size, style.size.small - 3) end

local function text(style, props)
  props.font_size = fs(style, props.font_size or style.size.normal)
  return style.text(props)
end

-- A mono run (Tsugumori's whole face is mono already).
local function mono(style, props)
  props.font_family = props.font_family or style.mono_font
  return text(style, props)
end

local function percent(v) return ("%d%%"):format(math.floor(U.clamp01(get(v)) * 100 + 0.5)) end

local function a11y(node, spec, role_name, name, value)
  node.accessible_role = role_name
  node.accessible_name = spec.label or spec.alt or name
  if value then node.accessible = function() return { value = U.clamp01(get(value)), minimum = 0, maximum = 1 } end end
  return node
end

-- A square of `s` with a hairline edge (Tsugumori's frame for a small thing).
local function hairline(props, color)
  props.color = props.color or "transparent"
  props.border_width = 1
  props.border_color = color
  return ui.Rect(props)
end

-- ------------------------------------------------------------- badges --

--- A count or a word on a small filled tag. `count` (over `max`, 99, it
--- reads `99+`) or `text`, `color`/`kind` ("alert"), `size` (its height, 20).
function M.badge(spec, style)
  local h = spec.size or 20
  local color = tone(spec, style, "alert")
  local function word()
    local c = get(spec.count)
    if c ~= nil then
      c = math.floor(tonumber(c) or 0)
      local max = spec.max or 99
      return c > max and (tostring(max) .. "+") or tostring(c)
    end
    return tostring(get(spec.text) or "")
  end
  local size = fs(style, math.floor(h * 0.62))
  local function width()
    local n = utf8.len(word()) or 0
    return math.max(h, math.ceil(n * size * 0.62) + (style.hatched and 10 or 12))
  end
  local node = ui.Item(U.place(spec, { width = width, height = h,
    behavior = { width = style.spring(420, 26) } }))
  ui.reparent(ui.Rect { anchors = { fill = true }, radius = style.hatched and 0 or h / 2, color = color,
    behavior = { color = { duration = 200 } } }, node)
  ui.reparent(text(style, { anchors = { center_in = true }, text = function()
    local w = word()
    return style.hatched and w:upper() or w
  end, font_size = size, font_weight = 700, color = style.on_accent }), node)
  node.accessible_role = "status"
  node.accessible_name = function() return spec.label or word() end
  return node
end

--- A status dot: `color` or `kind`, `size` (10), `pulse` (a halo that
--- swells and fades, forever, while it is true).
function M.dot(spec, style)
  local s = spec.size or 10
  local color = tone(spec, style, "ok")
  local r = style.hatched and 0 or s / 2
  local box = s * 2.4
  local node = ui.Item(U.place(spec, { width = box, height = box }))
  if spec.pulse then
    ui.reparent(ui.Rect { x = (box - s) / 2, y = (box - s) / 2, width = s, height = s, radius = r,
      color = style.hatched and "transparent" or color,
      border_width = style.hatched and 1 or 0, border_color = color,
      loop = function()
        if not get(spec.pulse) then return nil end
        return { scale = { from = 1, to = 2.3, duration = 1600, easing = "out_cubic" },
          opacity = { from = 0.55, to = 0, duration = 1600, easing = "out_cubic" } }
      end, opacity = 0 }, node)
  end
  ui.reparent(ui.Rect { x = (box - s) / 2, y = (box - s) / 2, width = s, height = s, radius = r, color = color,
    behavior = { color = { duration = 200 } } }, node)
  node.accessible_role = "status"
  node.accessible_name = spec.label or (type(spec.kind) == "string" and spec.kind) or "status"
  return node
end

--- A status lamp with a word: `kind`/`color`, `on` (true; may be a
--- binding), `label`.
function M.led(spec, style)
  local color = tone(spec, style, "ok")
  local function on() local v = get(spec.on) return v == nil or v == true end
  local s = 12
  local lamp
  if style.hatched then
    lamp = ui.Item { width = s, height = s,
      hairline({ anchors = { fill = true } }, style.line),
      ui.Rect { x = 3, y = 3, width = s - 6, height = s - 6,
        color = function() return on() and get(color) or get(style.track) end,
        behavior = { color = { duration = 120 } } },
    }
  else
    lamp = ui.Item { width = s, height = s,
      ui.Rect { x = -4, y = -4, width = s + 8, height = s + 8, radius = (s + 8) / 2,
        color = function() return get(color):alpha(0.22) end,
        opacity = function() return on() and 1 or 0 end, behavior = { opacity = { duration = 250 } } },
      ui.Rect { anchors = { fill = true }, radius = s / 2,
        color = function() return on() and get(color) or get(style.ink_lo):alpha(0.3) end,
        behavior = { color = { duration = 250 } } },
    }
  end
  local row = { gap = 10, align = "center", lamp }
  if spec.label then
    row[#row + 1] = style.hatched
      and U.caption(style, { text = spec.label, font_size = style.size.small - 2,
        color = function() return on() and get(style.ink) or get(style.ink_lo) end })
      or text(style, { text = spec.label, font_size = style.size.small - 1,
        color = function() return on() and get(style.ink) or get(style.ink_lo) end })
  end
  local node = ui.Row(U.place(spec, row))
  node.accessible_role = "status"
  node.accessible_name = spec.label or "indicator"
  node.accessible = function() return { checked = on() } end
  return node
end

--- A display chip with a tone: `text`, `color`/`kind`, `icon` (optional).
--- Material a tonal pill; Tsugumori a hairline tag with a square pip.
function M.tag(spec, style)
  local color = tone(spec, style, "accent")
  local h = spec.height or (style.hatched and 22 or 26)
  local size = fs(style, style.size.small - 2)
  local word = spec.text
  local n = utf8.len(tostring(get(word) or "")) or 0
  local lead = spec.icon and (size + 6) or (style.hatched and 12 or 0)
  local w = spec.width or math.ceil(n * size * (style.hatched and 0.66 or 0.58)) + lead + (style.hatched and 16 or 22)
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = U.alpha(color, 0.1), border_width = 1,
      border_color = color }, node)
    if not spec.icon then
      ui.reparent(ui.Rect { x = 8, y = (h - 5) / 2, width = 5, height = 5, color = color }, node)
    end
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = h / 2, color = U.alpha(color, 0.18) }, node)
  end
  local x = style.hatched and 8 or 11
  if spec.icon then
    ui.reparent(style.icon(spec.icon, size + 2, color, { x = x, anchors = { vertical_center = true } }), node)
  end
  local label = style.hatched
    and U.caption(style, { text = word, font_size = size, color = color, letter_spacing = 0.6 })
    or text(style, { text = word, font_size = size, font_weight = 600, color = color })
  label.x = x + lead
  label.anchors = { vertical_center = true }
  ui.reparent(label, node)
  node.accessible_role = "label"
  node.accessible_name = word
  return node
end

-- ----------------------------------------------------------- progress --

--- A linear progress bar: `value` (0..1, a binding; nil for an
--- indeterminate one that sweeps), `width` (200), `height`, `label` (its
--- name to a screen reader). Material: M3's split track with a stop dot;
--- Tsugumori: a hairline sleeve with a hatched run, read in 5% steps.
function M.progress(spec, style)
  local w = spec.width or 200
  local indeterminate = spec.value == nil
  local node
  if not style.hatched then
    local h = spec.height or 8
    local gap = math.max(3, h / 2)
    node = ui.Item(U.place(spec, { width = w, height = h }))
    if indeterminate then
      ui.reparent(ui.Rect { anchors = { fill = true }, radius = h / 2, color = style.track }, node)
      ui.reparent(ui.Rect { height = h, radius = h / 2, color = style.accent, x = 0, width = w * 0.3,
        loop = { x = { from = 0, to = w * 0.55, duration = 1100, easing = "in_out_cubic", alternate = true },
          width = { from = w * 0.2, to = w * 0.45, duration = 700, easing = "in_out_sine", alternate = true } } }, node)
    else
      local function at() return U.clamp01(get(spec.value)) * w end
      ui.reparent(ui.Rect { x = function() return math.min(w, at() + gap) end, height = h, radius = h / 2,
        width = function() return math.max(0, w - at() - gap) end, color = style.track,
        visible = function() return w - at() - gap >= 1 end,
        behavior = { x = style.spring(200, 24), width = style.spring(200, 24) } }, node)
      ui.reparent(ui.Rect { x = w - h, width = h, height = h, radius = h / 2, color = style.accent,
        scale = 0.5, visible = function() return at() < w - h - gap end }, node)
      ui.reparent(ui.Rect { height = h, radius = h / 2, color = style.accent,
        width = function() local v = at() return v > 0 and math.max(h, v) or 0 end,
        visible = function() return at() > 0 end,
        behavior = { width = style.spring(200, 24) } }, node)
    end
  else
    local h = spec.height or 12
    local iw, ih = w - 4, h - 4
    node = ui.Item(U.place(spec, { width = w, height = h }))
    ui.reparent(hairline({ anchors = { fill = true } }, style.line), node)
    local function run(props, rw)
      rw = math.floor(rw or iw)
      props.y, props.height, props.clip = 2, ih, true
      props[1] = ui.Rect { width = rw, height = ih, color = U.alpha(style.accent, 0.16) }
      props[2] = style.stripes.box { width = rw, height = ih, gap = 4, weight = 1.5, color = style.accent }
      return ui.Item(props)
    end
    if indeterminate then
      ui.reparent(run({ x = 2, width = iw * 0.3,
        loop = { x = { from = 2, to = 2 + iw * 0.7, duration = 1400, easing = "linear", alternate = true } } }, iw * 0.3), node)
    else
      -- Quantised: the run moves in twentieths.
      local function at() return math.floor(U.clamp01(get(spec.value)) * 20 + 0.5) / 20 * iw end
      ui.reparent(run { x = 2, width = function() return math.max(1, at()) end,
        visible = function() return at() > 0 end, behavior = { width = style.spring() } }, node)
      ui.reparent(ui.Rect { y = -2, width = 2, height = h + 4, color = style.accent,
        x = function() return 1 + at() end, behavior = { x = style.spring() } }, node)
    end
  end
  return a11y(node, spec, "progress", "Progress", not indeterminate and spec.value or nil)
end

-- A circle's path data, starting at twelve and going clockwise.
local function circle_d(cx, cy, r) return morf.geometry.arc(cx, cy, r, 0, 359.99) end

--- Two strokes of the same outline: the track and the value. Material: a
--- round-capped arc with a gap either side to the rest of the track;
--- Tsugumori: a dashed arc, its run in whole dashes.
local function value_arc(spec, style, d, length, t, color, steps)
  local v = function() return U.clamp01(get(spec.value)) end
  local track, active
  if style.hatched then
    local pitch = length / steps
    local dash = { pitch * 0.62, pitch * 0.38 }
    track = ui.Path { anchors = { fill = true }, d = d, fill_color = "transparent", stroke_color = style.track,
      stroke_width = t, stroke_cap = "butt", dash = dash }
    active = ui.Path { anchors = { fill = true }, d = d, fill_color = "transparent", stroke_color = color,
      stroke_width = t, stroke_cap = "butt", dash = dash,
      trim_end = function() return math.floor(v() * steps + 0.5) / steps end,
      behavior = { trim_end = style.spring() } }
  else
    local g = (t + 4) / length
    track = ui.Path { anchors = { fill = true }, d = d, fill_color = "transparent", stroke_color = style.track,
      stroke_width = t, stroke_cap = "round",
      trim_start = function() return math.min(1, v() + g) end, trim_end = 1 - (v() > 0 and g or 0),
      visible = function() return v() < 1 - 2 * g end,
      behavior = { trim_start = style.spring(180, 24) } }
    active = ui.Path { anchors = { fill = true }, d = d, fill_color = "transparent", stroke_color = color,
      stroke_width = t, stroke_cap = "round", trim_end = v, visible = function() return v() > 0.001 end,
      behavior = { trim_end = style.spring(180, 24) } }
  end
  return track, active
end

--- A ring of progress: `value` (0..1), `size` (72), `thickness`
--- (a tenth of the size), `color`/`kind`, `text` (the centre's words;
--- the percentage when absent, none when false).
function M.progress_ring(spec, style)
  local s = spec.size or 72
  local t = spec.thickness or math.max(4, math.floor(s / 10))
  local r = (s - t) / 2
  local color = tone(spec, style, "accent")
  local node = ui.Item(U.place(spec, { width = s, height = s }))
  local track, active = value_arc(spec, style, circle_d(s / 2, s / 2, r), 2 * math.pi * r, t, color, 32)
  ui.reparent(track, node)
  ui.reparent(active, node)
  if style.hatched then
    ui.reparent(ui.Path { anchors = { fill = true }, d = circle_d(s / 2, s / 2, r - t / 2 - 3),
      fill_color = "transparent", stroke_color = style.line, stroke_width = 1 }, node)
  end
  if spec.text ~= false and s >= 44 then
    local words = spec.text or function() return percent(spec.value) end
    ui.reparent((style.hatched and mono or text)(style, { anchors = { center_in = true }, text = words,
      font_size = math.floor(s * 0.22), font_weight = 500, color = style.ink }), node)
  end
  return a11y(node, spec, "progress", "Progress", spec.value)
end

--- Progress on a half ring, a gauge's face: `value`, `size` (its width,
--- 140), `thickness`, `color`/`kind`, `text` (as progress_ring's),
--- `caption` (a word under the reading).
function M.semicircle(spec, style)
  local s = spec.size or 140
  local t = spec.thickness or math.max(6, math.floor(s / 11))
  local r = (s - t) / 2
  local h = math.ceil(s / 2 + t / 2)
  local color = tone(spec, style, "accent")
  local node = ui.Item(U.place(spec, { width = s, height = h }))
  local d = morf.geometry.arc(s / 2, s / 2, r, -90, 180)
  local track, active = value_arc(spec, style, d, math.pi * r, t, color, 18)
  local face = ui.Item { width = s, height = h }
  ui.reparent(track, face)
  ui.reparent(active, face)
  ui.reparent(face, node)
  if style.hatched then
    ui.reparent(ui.Path { width = s, height = h, d = morf.geometry.ticks(s / 2, s / 2, r - t / 2 - 9, r - t / 2 - 3,
      { angles = { -90, -45, 0, 45, 90 } }), fill_color = "transparent", stroke_color = style.ink_lo,
      stroke_width = 1 }, node)
  end
  if spec.text ~= false then
    local size = math.floor(s * 0.2)
    local words = spec.text or function() return percent(spec.value) end
    local reading = (style.hatched and mono or text)(style, { text = words, font_size = size, font_weight = 500,
      color = style.ink, anchors = { horizontal_center = true }, y = h - math.ceil(size * 1.3) - (spec.caption and 14 or 0) })
    ui.reparent(reading, node)
    if spec.caption then
      ui.reparent(U.caption(style, { text = spec.caption, anchors = { horizontal_center = true },
        y = h - 16, height = 16 }), node)
    end
  end
  return a11y(node, spec, "progress", spec.caption or "Progress", spec.value)
end

--- Progress in separate steps: `value` (0..1), `segments` (10), `width`
--- (200), `height` (10), `gap`, `color`/`kind`. A step lights once the
--- value reaches its half.
function M.segmented_progress(spec, style)
  local n = spec.segments or 10
  local w, h = spec.width or 200, spec.height or 10
  local gap = spec.gap or (style.hatched and 3 or 4)
  local sw = (w - gap * (n - 1)) / n
  local color = tone(spec, style, "accent")
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  for i = 1, n do
    local function lit() return U.clamp01(get(spec.value)) * n >= i - 0.5 end
    local x = (i - 1) * (sw + gap)
    if style.hatched then
      ui.reparent(hairline({ x = x, width = sw, height = h }, style.line), node)
      ui.reparent(ui.Item { x = x, width = math.floor(sw), height = h,
        opacity = function() return lit() and 1 or 0 end, behavior = { opacity = { duration = 90 } },
        U.fill(style, { width = math.floor(sw), height = h, color = color, strong = true }) }, node)
    else
      ui.reparent(ui.Rect { x = x, width = sw, height = h, radius = math.min(h, sw) / 2,
        color = function() return lit() and get(color) or get(style.track) end,
        scale = function() return lit() and 1 or 0.86 end,
        behavior = { color = { duration = 220 }, scale = style.spring(380, 18) } }, node)
    end
  end
  return a11y(node, spec, "progress", "Progress", spec.value)
end

-- ----------------------------------------------------------- readings --

--- A battery: `level` (0..1), `charging` (bool), `width` (56), `height`
--- (26), `percent` (true: the percentage beside it). Its run turns warn
--- under 30%, alert under 15%, ok while charging.
function M.battery(spec, style)
  local w, h = spec.width or 56, spec.height or 26
  local nub = math.max(3, math.floor(w / 14))
  local bw = w - nub - 1
  local function level() return U.clamp01(get(spec.level)) end
  local function charging() return get(spec.charging) and true or false end
  local color = function()
    local l = level()
    if charging() then return get(style.ok) end
    if l < 0.15 then return get(style.alert) end
    if l < 0.3 then return get(style.warn) end
    return get(style.accent)
  end
  local body = ui.Item { width = w, height = h }
  if style.hatched then
    ui.reparent(hairline({ width = bw, height = h }, style.ink_lo), body)
    ui.reparent(ui.Rect { x = bw, y = h * 0.3, width = nub, height = h * 0.4, color = style.ink_lo }, body)
    -- Five cells, lit in whole cells.
    local cells, pad, cg = 5, 3, 2
    local cw = (bw - pad * 2 - cg * (cells - 1)) / cells
    for i = 1, cells do
      ui.reparent(ui.Rect { x = pad + (i - 1) * (cw + cg), y = pad, width = cw, height = h - pad * 2,
        color = function() return level() * cells >= i - 0.5 and color() or get(style.track) end,
        behavior = { color = { duration = 120 } } }, body)
    end
  else
    local r = math.min(h * 0.3, 8)
    ui.reparent(ui.Rect { width = bw, height = h, radius = r, color = "transparent",
      border_width = 2, border_color = style.ink_lo }, body)
    ui.reparent(ui.Rect { x = bw + 1, y = h * 0.32, width = nub, height = h * 0.36, radius = nub / 2,
      color = style.ink_lo }, body)
    local iw = bw - 8
    ui.reparent(ui.Rect { x = 4, y = 4, height = h - 8, radius = math.max(0, r - 3), color = color,
      width = function() return math.max(h - 8 > 0 and 2 or 0, level() * iw) end,
      behavior = { width = style.spring(200, 24), color = { duration = 250 } } }, body)
  end
  ui.reparent(style.icon("bolt", math.floor(h * 0.78), function()
    return level() > 0.55 and get(style.on_accent) or get(style.ink)
  end, { anchors = { center_in = true }, fill = true, visible = charging,
  }), body)
  local node
  if spec.percent then
    node = ui.Row(U.place(spec, { gap = 8, align = "center", body,
      (style.hatched and mono or text)(style, { text = function() return percent(level()) end,
        font_size = math.max(style.size.small - 2, math.floor(h * 0.55)), font_weight = 500, color = style.ink }) }))
  else
    node = body
    U.place(spec, node)
  end
  node.accessible_role = "progress"
  node.accessible_name = spec.label or "Battery"
  node.accessible = function() return { value = level(), minimum = 0, maximum = 1 } end
  node.accessible_description = function() return charging() and "charging" or "" end
  return node
end

--- Signal strength in rising bars: `level` (0..1) or `active` (a count),
--- `bars` (4), `size` (the box, 24), `color`/`kind`.
function M.signal_bars(spec, style)
  local n = spec.bars or 4
  local s = spec.size or 24
  local gap = math.max(2, math.floor(s / 10))
  local bw = (s - gap * (n - 1)) / n
  local color = tone(spec, style, "accent")
  local function lit()
    if spec.active ~= nil then return math.floor(tonumber(get(spec.active)) or 0) end
    local l = U.clamp01(get(spec.level or spec.value))
    return l <= 0 and 0 or math.ceil(l * n - 0.001)
  end
  local node = ui.Item(U.place(spec, { width = s, height = s }))
  for i = 1, n do
    local bh = math.max(3, s * i / n)
    local props = { x = (i - 1) * (bw + gap), y = s - bh, width = bw, height = bh }
    if style.hatched then
      props.color = function() return i <= lit() and get(color) or get(color):alpha(0) end
      props.border_width = 1
      props.border_color = function() return i <= lit() and get(color) or get(style.line) end
      props.behavior = { color = { duration = 100 } }
    else
      props.radius = math.min(bw / 2, 3)
      props.color = function() return i <= lit() and get(color) or get(style.track) end
      props.behavior = { color = { duration = 220 } }
    end
    ui.reparent(ui.Rect(props), node)
  end
  node.accessible_role = "progress"
  node.accessible_name = spec.label or "Signal"
  node.accessible = function() return { value = lit(), minimum = 0, maximum = n } end
  return node
end

-- ----------------------------------------------------- status surfaces --

-- The emblem for a kind, in the theme's own drawing.
local function emblem(style, kind, size)
  return style.kit.emblem { kind = function() return EMBLEM[role(kind)] end, size = size }
end

--- A status card -- WARNING, SERVICE, CLEAR: `kind` ("ok" | "warn" |
--- "alert" | "info"), `title`, `detail`, `width` (260), `height` (84).
function M.status_card(spec, style)
  local w, h = spec.width or 260, spec.height or 84
  local kc = function() return get(style[role(spec.kind)]) end
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  local x0
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1,
      border_color = U.alpha(kc, 0.7) }, node)
    ui.reparent(ui.Item { x = 1, y = 1, width = 8, height = h - 2, clip = true,
      ui.Rect { anchors = { fill = true }, color = U.alpha(kc, 0.18) },
      style.stripes.box { width = 8, height = h - 2, gap = 4, weight = 1.5, color = kc } }, node)
    ui.reparent(ui.Rect { x = 9, y = 1, width = 1, height = h - 2, color = kc }, node)
    local m = style.marks(kc)
    if m then ui.reparent(m, node) end
    x0 = 22
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = style.radius(h),
      color = function() return get(style.surface):mix(kc(), 0.14) end,
      behavior = { color = { duration = 250 } } }, node)
    x0 = 14
  end
  local es = style.hatched and 30 or 40
  local mark = emblem(style, spec.kind, es)
  mark.x, mark.y = x0, math.floor((h - es) / 2)
  ui.reparent(mark, node)
  local tx = x0 + (style.hatched and 42 or 54)
  local tw = w - tx - 12
  local col = { x = tx, anchors = { vertical_center = true }, gap = 2,
    style.hatched
      and text(style, { text = function() return tostring(get(spec.title) or ""):upper() end, width = tw,
        elide = "right", font_size = style.size.small, font_weight = 600, color = style.ink, letter_spacing = 0.4 })
      or text(style, { text = spec.title, width = tw, elide = "right", font_size = style.size.normal,
        font_weight = 500, color = style.ink }),
    text(style, { text = spec.detail, width = tw, wrap = true, max_lines = 2, font_size = style.size.small - 2,
      color = style.ink_lo, line_height = 1.25 }),
  }
  if style.hatched then
    -- The kind's code word heads the column: WARNING, SERVICE, CLEAR.
    table.insert(col, 1, U.caption(style, { text = function() return CODE[role(spec.kind)] end, color = kc,
      font_size = style.size.small - 3, letter_spacing = 1.2, height = 14 }))
  end
  ui.reparent(ui.Column(col), node)
  node.accessible_role = "status"
  if spec.title ~= nil then node.accessible_name = spec.title end
  if spec.detail ~= nil then node.accessible_description = spec.detail end
  return node
end

--- A banner across a page: `title`, `text`, `kind`, `width` (260),
--- `height` (60), `icon` (the kind's emblem when absent).
function M.banner(spec, style)
  local w, h = spec.width or 260, spec.height or 60
  local kc = function() return get(style[role(spec.kind)]) end
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  local s = style.hatched and 26 or 32
  local lead
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = U.alpha(kc, 0.06) }, node)
    ui.reparent(ui.Rect { width = w, height = 1, color = kc }, node)
    ui.reparent(ui.Rect { y = h - 1, width = w, height = 1, color = U.alpha(kc, 0.5) }, node)
    ui.reparent(ui.Item { y = 1, width = 44, height = h - 2, clip = true,
      style.stripes.box { width = 44, height = h - 2, gap = 5, weight = 1, color = U.alpha(kc, 0.45) } }, node)
    ui.reparent(ui.Rect { width = 3, height = h, color = kc }, node)
    lead = 9
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = style.radius(h),
      color = function() return get(style.surface):mix(kc(), 0.2) end }, node)
    lead = 14
  end
  local mark = spec.icon and style.icon(spec.icon, s - 4, kc) or emblem(style, spec.kind, s)
  mark.x, mark.anchors = lead + (style.hatched and 0 or 0), { vertical_center = true }
  ui.reparent(mark, node)
  local tx = lead + s + 14
  ui.reparent(ui.Column { x = tx, anchors = { vertical_center = true }, gap = 1,
    style.hatched
      and U.caption(style, { text = spec.title, font_size = style.size.small - 1, color = kc, letter_spacing = 0.8,
        width = w - tx - 10, elide = "right", height = 18 })
      or text(style, { text = spec.title, font_size = style.size.small, font_weight = 600, color = style.ink,
        width = w - tx - 12, elide = "right" }),
    text(style, { text = spec.text, font_size = style.size.small - 2, color = style.ink_lo,
      width = w - tx - 12, wrap = true, max_lines = 2 }),
  }, node)
  node.accessible_role = role(spec.kind) == "alert" and "alert" or "status"
  if spec.title ~= nil then node.accessible_name = spec.title end
  if spec.text ~= nil then node.accessible_description = spec.text end
  return node
end

-- A centred column of a mark, a title and some words, in `w` x `h`.
local function centred_page(spec, style, w, h, mark, title_size)
  local tw = w - 24
  local col = { anchors = { center_in = true }, gap = 8, align = "center",
    mark,
    style.hatched
      and text(style, { text = function() return tostring(get(spec.title) or ""):upper() end,
        font_size = title_size - 2, font_weight = 600, color = style.ink, letter_spacing = 1,
        width = tw, horizontal_alignment = "center", elide = "right" })
      or text(style, { text = spec.title, font_size = title_size, font_weight = 500, color = style.ink,
        width = tw, horizontal_alignment = "center", elide = "right" }),
  }
  if spec.text then
    col[#col + 1] = text(style, { text = spec.text, font_size = style.size.small - 2, color = style.ink_lo,
      width = tw, wrap = true, max_lines = 3, horizontal_alignment = "center", line_height = 1.3 })
  end
  return ui.Column(col)
end

--- Nothing to show: `icon` ("inbox"), `title`, `text`, `width` (260),
--- `height` (190).
function M.empty_state(spec, style)
  local w, h = spec.width or 260, spec.height or 190
  local icon = spec.icon or "inbox"
  local s = math.min(72, math.floor(h * 0.38))
  local mark
  if style.hatched then
    mark = ui.Item { width = s, height = s,
      ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1, border_color = style.line },
      style.stripes.box { x = 1, y = 1, width = s - 2, height = s - 2, gap = 6, weight = 1,
        color = U.alpha(style.ink_lo, 0.25) },
      style.icon(icon, math.floor(s * 0.5), style.accent, { anchors = { center_in = true } }),
      style.marks(),
    }
  else
    mark = ui.Item { width = s, height = s,
      ui.Rect { anchors = { fill = true }, radius = s / 2, color = style.track },
      style.icon(icon, math.floor(s * 0.5), style.ink_lo, { anchors = { center_in = true } }),
    }
  end
  local node = ui.Item(U.place(spec, { width = w, height = h,
    centred_page(spec, style, w, h, mark, style.size.larger) }))
  node.accessible_role = "status"
  if spec.title ~= nil then node.accessible_name = spec.title end
  if spec.text ~= nil then node.accessible_description = spec.text end
  return node
end

--- The end of a task: `kind` ("success" | "error" | "info" | "warn"),
--- `title`, `text`, `width` (260), `height` (190). Its emblem springs in.
function M.result_page(spec, style)
  local w, h = spec.width or 260, spec.height or 190
  local s = math.min(76, math.floor(h * 0.38))
  local mark = ui.Item { width = s, height = s, enter = { scale = 0.4, opacity = 0 },
    behavior = { scale = style.spring(260, 14), opacity = { duration = 200 } },
    emblem(style, spec.kind or "success", s) }
  local node = ui.Item(U.place(spec, { width = w, height = h,
    centred_page(spec, style, w, h, mark, style.size.large) }))
  if style.hatched then
    local m = style.marks(function() return get(style[role(spec.kind or "success")]) end)
    if m then ui.reparent(m, node) end
  end
  node.accessible_role = role(spec.kind or "success") == "alert" and "alert" or "status"
  if spec.title ~= nil then node.accessible_name = spec.title end
  if spec.text ~= nil then node.accessible_description = spec.text end
  return node
end

--- A placeholder while content loads: `width` (240), `height`, `lines`
--- (3), `avatar` (a round/square lead). The bars breathe in a wave.
function M.skeleton(spec, style)
  local w = spec.width or 240
  local lines = spec.lines or 3
  local lh, gap = 12, 10
  local lead = spec.avatar and 44 or 0
  local h = spec.height or math.max(lead, lines * lh + (lines - 1) * gap)
  local node = ui.Item(U.place(spec, { width = w, height = h, clip = true }))
  local function bar(props, i)
    props.loop = { opacity = { from = 1, to = 0.35, duration = 900, easing = "in_out_sine", alternate = true,
      delay = (i - 1) * 140 } }
    if style.hatched then
      local b = ui.Item { x = props.x, y = props.y, width = props.width, height = props.height, clip = true,
        loop = props.loop,
        ui.Rect { anchors = { fill = true }, color = U.alpha(style.ink_lo, 0.08), border_width = 1,
          border_color = U.alpha(style.ink_lo, 0.3) },
        style.stripes.box { width = math.floor(props.width), height = props.height, gap = 5, weight = 1,
          color = U.alpha(style.ink_lo, 0.25) } }
      return b
    end
    props.color = style.track
    props.radius = props.height / 2
    return ui.Rect(props)
  end
  if spec.avatar then
    local a = bar({ x = 0, y = 0, width = lead, height = lead }, 1)
    if not style.hatched then a.radius = lead / 2 end
    ui.reparent(a, node)
  end
  local x0 = lead > 0 and lead + 14 or 0
  for i = 1, lines do
    local lw = (w - x0) * (i == lines and lines > 1 and 0.6 or (i == 1 and 0.85 or 1))
    ui.reparent(bar({ x = x0, y = (i - 1) * (lh + gap) + (lead > 0 and 4 or 0), width = math.floor(lw), height = lh }, i + 1), node)
  end
  node.accessible_role = "progress"
  node.accessible_name = spec.label or "Loading"
  return node
end

--- A toast's content: `title`, `text`, `icon` (or `kind` for its emblem),
--- `width` (260). Material a snackbar in the inverse tones; Tsugumori a
--- hairline slip with an accent lead. It rises in.
function M.toast(spec, style)
  local w = spec.width or 260
  local h = spec.height or (spec.text and 62 or 48)
  local color = tone(spec, style, "accent")
  local node = ui.Item(U.place(spec, { width = w, height = h,
    enter = { translate_y = 14, opacity = 0 },
    behavior = { translate_y = style.spring(300, 22), opacity = { duration = 200 } } }))
  local ink, ink_lo
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.raised, border_width = 1, border_color = style.line }, node)
    ui.reparent(ui.Rect { width = 3, height = h, color = color }, node)
    ui.reparent(ui.Rect { x = 3, y = h - 2, width = 28, height = 2, color = color }, node)
    local m = style.marks()
    if m then ui.reparent(m, node) end
    ink, ink_lo = style.ink, style.ink_lo
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = style.radius(h), color = style.ink }, node)
    ink, ink_lo = style.surface, function() return get(style.surface):alpha(0.75) end
  end
  local s = 24
  local mark
  if spec.kind and not spec.icon then
    mark = emblem(style, spec.kind, s)
  else
    local ic = style.hatched and color or function() return get(style.surface):mix(get(color), 0.5) end
    mark = style.icon(spec.icon or "notifications", s, ic, { fill = true })
  end
  mark.x, mark.anchors = 14, { vertical_center = true }
  ui.reparent(mark, node)
  local tx = 14 + s + 12
  local tw = w - tx - 12
  local col = { x = tx, anchors = { vertical_center = true }, gap = 2,
    style.hatched
      and U.caption(style, { text = spec.title, font_size = style.size.small - 1, color = ink, letter_spacing = 0.6,
        width = tw, elide = "right", height = 18 })
      or text(style, { text = spec.title, font_size = style.size.small, font_weight = 600, color = ink,
        width = tw, elide = "right" }),
  }
  if spec.text then
    col[#col + 1] = text(style, { text = spec.text, font_size = style.size.small - 2, color = ink_lo,
      width = tw, elide = "right" })
  end
  ui.reparent(ui.Column(col), node)
  node.accessible_role = "alert"
  if spec.title ~= nil then node.accessible_name = spec.title end
  if spec.text ~= nil then node.accessible_description = spec.text end
  return node
end

return M
