-- Domain instruments: hud_game -- a game's heads-up display, drawn over
-- the display widgets in a theme's style (see lib.kit.display).
--
-- Bars and rings of health, shield, stamina and mana; the cooldowns, the
-- hotbar and the buffs; the radial and the minimap, the compass strip and
-- the markers that point off it; the feeds and counters that flash and
-- fade; the race, the boss and the scoreboard. The layouts are shared;
-- the look is the style's -- Material tonal, round, sprung and morphing
-- (shapes that swell, liquid that sloshes); Tsugumori square, hairline,
-- hatched, mono and quantised, with registration marks.
--
-- Values are numbers or functions of one; lists (a feed, the blips, the
-- numbers that float up) are lists or functions returning one, kept in a
-- list model so rows that stay keep their nodes. Every meter tells a
-- screen reader its value; every list is a list or a log.
local morf = require("morf")
local ui = require("morf.ui")
local U = require("lib.kit.display.util")
local channel = require("lib.channel")
local get, clamp01 = U.get, U.clamp01

local M = {}

-- ------------------------------------------------------------ shared --

local serial = 0
local function uid(kind) serial = serial + 1 return ("kit.hud.%s.%d"):format(kind, serial) end

local function fs(style, n) return math.max(math.floor(n), style.size.small - 3) end

--- Text in the style, never under the small size less three.
local function txt(style, p)
  p.font_size = fs(style, p.font_size or style.size.normal)
  if p.color == nil then p.color = style.ink end
  return style.text(p)
end

local function cap(style, p) return U.caption(style, p) end

--- A number of `v` (a value or a function), 0 when it is none.
local function num(v) return tonumber(get(v)) or 0 end

--- A 0..1 reader of `v` over `max` (1): a number, a function or a channel.
local function reader(v, max)
  if channel.is(v) then return function() return clamp01((v:last() or 0) / (num(max) ~= 0 and num(max) or 1)) end end
  return function()
    local m = num(max)
    if m == 0 then m = 1 end
    return clamp01(num(v) / m)
  end
end

--- `f` in whole steps of `1/n` in Tsugumori; as it is in Material.
local function quant(style, f, n)
  if not style.hatched then return f end
  return function() return math.floor(f() * n + .5) / n end
end

--- Accessibility on `props`: `role`, `name` and, when `value` is given, its
--- value against `min`..`max` (0..100, `value` a 0..1 function, read as a
--- percentage).
local function a11y(props, role, name, value, min, max)
  props.accessible_role = role
  props.accessible_name = name
  if value then
    props.accessible = function()
      if min then return { value = value(), minimum = min, maximum = max } end
      return { value = math.floor(value() * 100 + .5), minimum = 0, maximum = 100 }
    end
  end
  return props
end

-- A box with a hairline edge.
local function hair(props, color)
  props.color = props.color or "transparent"
  props.border_width = props.border_width or 1
  props.border_color = color
  return ui.Rect(props)
end

-- The roles a kind names: a team, a rarity, a signal.
local KIND = { enemy = "alert", hostile = "alert", foe = "alert", ally = "info", friend = "info", party = "ok",
  self = "accent", player = "accent", objective = "warn", quest = "warn", neutral = "warn", loot = "extra",
  heal = "ok", crit = "warn", damage = "ink", buff = "ok", debuff = "alert",
  common = "ink_lo", uncommon = "ok", rare = "info", epic = "extra", legendary = "warn", mythic = "alert" }

--- The colour of a kind (a role name or one of KIND), else `fallback`'s.
local function kind_color(style, kind, fallback)
  local k = get(kind)
  local r = KIND[k] or (type(k) == "string" and style[k] ~= nil and k) or fallback or "accent"
  return get(style[r])
end
local function tone_fn(style, kind, fallback) return function() return kind_color(style, kind, fallback) end end

--- The colour of a level `x` (0..1): alert under `alert`, warn under `warn`, else `base`'s.
local function level_color(style, x, warn, alert, base)
  if x < alert then return get(style.alert) end
  if x < warn then return get(style.warn) end
  return get(base)
end

-- The width a run of `s` takes at `size` (an estimate, for boxes that hug text).
local function est(style, s, size)
  return math.ceil((utf8.len(tostring(s or "")) or 0) * size * (style.hatched and .66 or .62))
end

local function int(x)
  local n = tonumber(x)
  if not n then return tostring(x or "") end
  if n == math.floor(n) then return ("%d"):format(n) end
  return tostring(n)
end

local function fmt_time(t)
  t = tonumber(t)
  if not t then return "--:--.--" end
  local m = math.floor(t / 60)
  return ("%d:%05.2f"):format(m, t - m * 60)
end

--- A list model following the list (or function of one) `src`: each row a
--- copy of its entry with `key` (its `id`, else `keyf(entry)` numbered by
--- occurrence) and `rank` (its place, 1 first). `reverse` takes the list
--- newest last and puts the newest first. Returns the model and the
--- starter that feeds it once its owner node exists.
local function listed(src, keyf, opts)
  opts = opts or {}
  local model = morf.list_model({})
  local function start(owner)
    morf.effect(uid("list"), function()
      local list = get(src) or {}
      local n = #list
      local first = opts.max and math.max(1, n - opts.max + 1) or 1
      local rows, seen = {}, {}
      for i = first, n do
        local entry = list[i]
        local row = {}
        if type(entry) == "table" then for k, x in pairs(entry) do row[k] = x end else row.value = entry end
        local key = row.id ~= nil and tostring(row.id) or keyf(row)
        seen[key] = (seen[key] or 0) + 1
        row.key = key .. "#" .. seen[key]
        rows[#rows + 1] = row
      end
      if opts.reverse then
        local out = {}
        for i = #rows, 1, -1 do out[#out + 1] = rows[i] end
        rows = out
      end
      for i, row in ipairs(rows) do row.rank = i end
      model:replace(rows, "key")
    end, { owner = owner })
  end
  return model, start
end

--- Follows the heading `f()` (degrees) the short way round: returns a
--- reader of the unwrapped angle and the starter that runs it.
local function unwrap(f)
  local st = morf.state { angle = 0 }
  local last, acc
  local function start(owner)
    morf.effect(uid("turn"), function()
      local hd = (num(f()) % 360)
      if not last then last, acc = hd, hd
      else
        acc = acc + ((hd - last + 540) % 360 - 180)
        last = hd
      end
      st.angle = acc
    end, { owner = owner })
  end
  return function() return st.angle end, start
end

-- A circle's outline from twelve, clockwise.
local function circle_d(cx, cy, r) return morf.geometry.arc(cx, cy, r, 0, 359.99) end

--- A HUD bar `width` x `height` filled to `value()` (0..1) in `color` (a
--- function). `trail` (ms): what is lost lingers that long in the trail's
--- tone, then drains. `notches`: fractions where the bar is cut. `pulse`
--- (fn -> bool): the fill throbs while it is true. `steps`: Tsugumori's
--- quantum (the width over six).
local function hbar(style, o)
  local w, h = o.width, o.height
  local color = o.color
  local v = quant(style, o.value, o.steps or math.max(8, math.floor(w / 6)))
  local timer
  local node = ui.Item { x = o.x, y = o.y, width = w, height = h,
    on_destroyed = o.trail and function() if timer then timer:cancel() end end or nil }
  local trail
  if o.trail then
    local st = morf.state { at = v() }
    local held = v()
    morf.effect(uid("trail"), function()
      local x = v()
      if x >= held then
        held = x
        st.at = x
        if timer then timer:cancel() timer = nil end
        return
      end
      if timer then timer:cancel() end
      timer = morf.timer(o.trail, function()
        timer = nil
        held = v()
        st.at = held
      end, false)
    end, { owner = node })
    trail = function() return st.at end
  end
  local loop = o.pulse and function()
    if not o.pulse() then return nil end
    return { opacity = { from = 1, to = style.hatched and .35 or .55, duration = style.hatched and 260 or 520,
      easing = style.hatched and "linear" or "in_out_sine", alternate = true } }
  end or nil
  if not style.hatched then
    local r = h / 2
    ui.reparent(ui.Rect { width = w, height = h, radius = r, color = o.track or style.track }, node)
    if trail then
      ui.reparent(ui.Rect { height = h, radius = r, color = U.alpha(o.trail_color or style.warn, .85),
        width = function() local x = trail() return x <= 0 and 0 or math.max(h, x * w) end,
        behavior = { width = { duration = 520, easing = "in_out_cubic" } } }, node)
    end
    ui.reparent(ui.Rect { height = h, radius = r, color = color, loop = loop,
      width = function() local x = v() return x <= 0 and 0 or math.max(h, x * w) end,
      visible = function() return v() > 0 end,
      behavior = { width = style.spring(260, 26), color = { duration = 260 } } }, node)
    -- A sheen along the top of the fill.
    if h >= 10 then
      ui.reparent(ui.Rect { x = r * .6, y = 2, height = math.max(2, math.floor(h * .22)), radius = h,
        color = U.alpha(style.on_accent, .22),
        width = function() return math.max(0, v() * w - r * 1.2) end,
        behavior = { width = style.spring(260, 26) } }, node)
    end
    for _, f in ipairs(o.notches or {}) do
      ui.reparent(ui.Rect { x = math.floor(f * w) - 1, width = 2, height = h, color = o.cut or style.surface }, node)
    end
  else
    local iw, ih = w - 4, h - 4
    ui.reparent(hair({ width = w, height = h }, style.line), node)
    if trail then
      ui.reparent(ui.Rect { x = 2, y = 2, height = ih, color = U.alpha(o.trail_color or style.warn, .55),
        width = function() return math.max(0, trail() * iw) end,
        behavior = { width = { duration = 420, easing = "in_out_cubic" } } }, node)
    end
    ui.reparent(ui.Item { x = 2, y = 2, height = ih, clip = true, loop = loop,
      width = function() return math.max(0, v() * iw) end,
      visible = function() return v() > 0 end,
      behavior = { width = style.spring() },
      ui.Rect { width = iw, height = ih, color = function() return get(color):alpha(.22) end },
      style.stripes.box { width = iw, height = ih, gap = 4, weight = 1.5, color = color },
    }, node)
    ui.reparent(ui.Rect { y = -2, width = 2, height = h + 4, color = color,
      x = function() return 1 + v() * iw end, behavior = { x = style.spring() } }, node)
    for _, f in ipairs(o.notches or {}) do
      ui.reparent(ui.Rect { x = math.floor(2 + f * iw), y = -3, width = 1, height = h + 6, color = style.ink_lo }, node)
    end
  end
  return node
end

--- A header row `w` wide: an icon and a caption at the left, a reading at the right.
local function header(style, w, icon, label, reading, color)
  local row = ui.Item { width = w, height = 20 }
  local x = 0
  if icon then
    ui.reparent(style.icon(icon, 16, color or style.ink_lo, { y = 1, fill = not style.hatched }), row)
    x = 20
  end
  if label then
    ui.reparent(cap(style, { x = x, y = 2, text = label, color = style.ink_lo }), row)
  end
  if reading then
    ui.reparent(txt(style, { anchors = { right = true }, width = math.floor(w * .6), height = 20,
      horizontal_alignment = "right", vertical_alignment = "center", text = reading,
      font_size = style.size.small, font_weight = 600, font_family = style.mono_font, color = style.ink }), row)
  end
  return row
end

--- A ring arc: `size`, `thickness`, `from`, `sweep` (degrees), `value`
--- (fn 0..1), `color`, `track` (its colour; false for none), `steps`
--- (Tsugumori's dashes), `reverse` (the value is what remains, drawn
--- from the end back). Returns a node `size` square.
local function arc(style, o)
  local s, t = o.size, o.thickness
  local c, r = s / 2, (s - t) / 2 - (o.inset or 0)
  local d = morf.geometry.arc(c, c, r, o.from or 0, o.sweep or 359.99)
  local length = math.rad(o.sweep or 360) * r
  local v = o.value
  local node = ui.Item { x = o.x, y = o.y, width = s, height = s, opacity = o.opacity, behavior = o.behavior }
  local base = { anchors = { fill = true }, view_box = { 0, 0, s, s }, d = d, fill_color = "transparent",
    stroke_width = t }
  local function path(extra)
    local p = {}
    for k, x in pairs(base) do p[k] = x end
    for k, x in pairs(extra) do p[k] = x end
    return ui.Path(p)
  end
  if style.hatched then
    local steps = o.steps or 24
    local pitch = length / steps
    local dash = { pitch * .62, pitch * .38 }
    if o.track ~= false then
      ui.reparent(path { stroke_color = o.track or style.track, stroke_cap = "butt", dash = dash }, node)
    end
    local q = function() return math.floor(v() * steps + .5) / steps end
    ui.reparent(path { stroke_color = o.color, stroke_cap = "butt", dash = dash,
      trim_start = o.reverse and function() return 1 - q() end or 0, trim_end = o.reverse and 1 or q,
      visible = function() return q() > 0 end,
      behavior = o.reverse and { trim_start = style.spring() } or { trim_end = style.spring() } }, node)
  else
    if o.track ~= false then
      ui.reparent(path { stroke_color = o.track or style.track, stroke_cap = "round" }, node)
    end
    ui.reparent(path { stroke_color = o.color, stroke_cap = "round",
      trim_start = o.reverse and function() return 1 - v() end or 0, trim_end = o.reverse and 1 or v,
      visible = function() return v() > .002 end,
      behavior = o.reverse and { trim_start = style.spring(180, 24) } or { trim_end = style.spring(180, 24) } }, node)
  end
  return node
end

--- A cooldown's shade over a `w` x `h` box: a pie from the hand to twelve,
--- `remaining()` (0..1) of the turn, drawn as one thick stroke trimmed.
--- Material a dark scrim; Tsugumori a hatched wash.
local function sweep(style, w, h, remaining, radius)
  local R = math.sqrt(w * w + h * h) / 2 + 1
  local d = morf.geometry.arc(w / 2, h / 2, R / 2, 0, 359.99)
  local box = ui.Item { width = w, height = h, clip = true,
    mask = (not style.hatched and radius and radius > 0) and ui.Rect { radius = radius, color = style.ink } or nil }
  local pie = { width = w, height = h, view_box = { 0, 0, w, h }, d = d, fill_color = "transparent",
    stroke_width = R, stroke_cap = "butt",
    trim_start = function() return 1 - remaining() end, trim_end = 1,
    visible = function() return remaining() > .001 end }
  if not style.hatched then
    pie.stroke_color = U.alpha(style.surface, .74)
    pie.behavior = { trim_start = { duration = 120 } }
    ui.reparent(ui.Path(pie), box)
  else
    pie.stroke_color = U.alpha(style.surface, .8)
    local shade = ui.Path(pie)
    ui.reparent(shade, box)
    pie.stroke_color = "#ffffff"
    ui.reparent(ui.Item { width = w, height = h, visible = function() return remaining() > .001 end,
      mask = ui.Path(pie),
      style.stripes.box { width = w, height = h, gap = 5, weight = 1, color = U.alpha(style.ink_lo, .6) } }, box)
  end
  return box
end

-- An arrowhead pointing up, centred on (0, 0) in a `s` box.
local function arrow_d(s)
  local h = s / 2
  return ("M%g %g L%g %g L%g %g L%g %g Z"):format(h, 0, s * .92, s, h, s * .72, s * .08, s)
end

-- ------------------------------------------------------------- health --

--- A health bar: `value` over `max` (100), `segments` (cuts every
--- `max/segments`; none when nil), `width` (240), `height`, `label`
--- ("HP"), `icon` ("favorite"), `text` (fn, the reading; "72 / 100"),
--- `warn` (.35), `alert` (.15). It turns warn, then alert, and throbs
--- under alert.
function M.health_bar(spec, style)
  local w = spec.width or 240
  local h = spec.height or (style.hatched and 12 or 14)
  local v = reader(spec.value, spec.max or 100)
  local warn, alert = spec.warn or .35, spec.alert or .15
  local color = function() return level_color(style, v(), warn, alert, style.ok) end
  local reading = spec.text or function() return ("%d / %d"):format(math.floor(num(spec.value) + .5), num(spec.max or 100)) end
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 24 + h }), "meter", spec.label or "Health", v))
  ui.reparent(header(style, w, spec.icon or "favorite", spec.label or "HP", reading, color), node)
  local notches
  if spec.segments and spec.segments > 1 then
    notches = {}
    for i = 1, spec.segments - 1 do notches[i] = i / spec.segments end
  end
  ui.reparent(hbar(style, { y = 24, width = w, height = h, value = v, color = color, notches = notches,
    pulse = function() return v() < alert end }), node)
  return node
end

--- A health bar whose lost part lingers, then drains: `value` over `max`
--- (100), `delay` (ms, 600), `width` (240), `height`, `label`, `icon`.
function M.damage_trail_bar(spec, style)
  local w = spec.width or 240
  local h = spec.height or (style.hatched and 14 or 16)
  local v = reader(spec.value, spec.max or 100)
  local color = function() return level_color(style, v(), .35, .15, spec.color or style.ok) end
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 24 + h }), "meter", spec.label or "Health", v))
  ui.reparent(header(style, w, spec.icon or "favorite", spec.label or "Health",
    function() return ("%d"):format(math.floor(num(spec.value) + .5)) end, color), node)
  ui.reparent(hbar(style, { y = 24, width = w, height = h, value = v, color = color, trail = spec.delay or 600,
    trail_color = style.alert }), node)
  return node
end

--- Health with a shield over it: `health` over `max` (100), `shield` over
--- `shield_max` (`max`), `segments` (the shield's cells, 4), `width`
--- (240). The shield is a cut bar above the health, which keeps a trail.
function M.shield_bar(spec, style)
  local w = spec.width or 240
  local hv = reader(spec.health, spec.max or 100)
  local sv = reader(spec.shield, spec.shield_max or spec.max or 100)
  local n = spec.segments or 4
  local notches = {}
  for i = 1, n - 1 do notches[i] = i / n end
  local sh, hh = style.hatched and 8 or 8, style.hatched and 12 or 14
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 24 + sh + 4 + hh }), "meter",
    spec.label or "Health and shield", hv))
  ui.reparent(header(style, w, "shield", spec.label or "Shield",
    function() return ("%d + %d"):format(math.floor(num(spec.health) + .5), math.floor(num(spec.shield) + .5)) end,
    style.info), node)
  ui.reparent(hbar(style, { y = 24, width = w, height = sh, value = sv, color = style.info, notches = notches,
    steps = n * 4 }), node)
  ui.reparent(hbar(style, { y = 24 + sh + 4, width = w, height = hh, value = hv, trail = 500,
    color = function() return level_color(style, hv(), .35, .15, style.ok) end, trail_color = style.alert,
    pulse = function() return hv() < .15 end }), node)
  return node
end

--- A boss's bar: `name`, `title` (under it), `value` over `max` (1),
--- `phases` (fractions where its phases change, { .66, .33 }), `width`
--- (260), `delay` (the trail's, 700). Phase notches cut it; the phase
--- it is in reads under it.
function M.boss_bar(spec, style)
  local w = spec.width or 260
  local h = style.hatched and 12 or 14
  local v = reader(spec.value, spec.max or 1)
  local phases = spec.phases or { .66, .33 }
  local name_h = style.hatched and 20 or 24
  local top = name_h + (spec.title and 18 or 0) + 12
  local node = ui.Item(a11y(U.place(spec, { width = w, height = top + h + 22 }), "meter",
    get(spec.name) or "Boss", v))
  if style.hatched then
    ui.reparent(cap(style, { width = w, horizontal_alignment = "center", text = spec.name, font_size = style.size.small,
      height = name_h, letter_spacing = 2, color = style.ink }), node)
  else
    ui.reparent(txt(style, { width = w, horizontal_alignment = "center", text = spec.name, height = name_h,
      font_size = style.size.large, font_weight = 600 }), node)
  end
  if spec.title then
    ui.reparent(cap(style, { y = name_h, width = w, horizontal_alignment = "center", text = spec.title,
      color = style.ink_lo }), node)
  end
  local color = function() return get(spec.color or style.alert) end
  ui.reparent(hbar(style, { y = top, width = w, height = h, value = v, color = color, notches = phases,
    trail = spec.delay or 700, trail_color = style.warn }), node)
  -- A pip over each notch, lit once the phase is passed.
  for _, f in ipairs(phases) do
    local passed = function() return v() <= f end
    if style.hatched then
      ui.reparent(ui.Rect { x = math.floor(2 + f * (w - 4)) - 3, y = top - 9, width = 7, height = 5,
        color = function() return passed() and get(style.ink) or "transparent" end,
        border_width = 1, border_color = style.ink_lo }, node)
    else
      ui.reparent(ui.Rect { x = math.floor(f * w) - 4, y = top - 10, width = 8, height = 8, rotation = 45, radius = 2,
        color = function() return passed() and get(style.ink) or get(style.track) end,
        scale = function() return passed() and 1 or .75 end,
        behavior = { color = { duration = 240 }, scale = style.spring(400, 16) } }, node)
    end
  end
  local function phase()
    local x, p = v(), 1
    for _, f in ipairs(phases) do if x <= f then p = p + 1 end end
    return p
  end
  ui.reparent(cap(style, { y = top + h + 6, text = function()
    return ("Phase %d / %d"):format(phase(), #phases + 1) end, color = style.ink_lo }), node)
  ui.reparent(txt(style, { y = top + h + 4, anchors = { right = true }, width = 80, horizontal_alignment = "right",
    font_size = style.size.small, font_weight = 600, font_family = style.mono_font,
    text = function() return ("%d%%"):format(math.floor(v() * 100 + .5)) end, color = color }), node)
  return node
end

--- A name over a unit: `name`, `level`, `value` (its health, over `max`,
--- 1), `kind` ("enemy", "ally", "neutral"), `title` (a line under the
--- name, optional), `width` (180).
function M.nameplate(spec, style)
  local w = spec.width or 180
  local v = reader(spec.value, spec.max or 1)
  local color = tone_fn(style, spec.kind or "enemy", "alert")
  local bs = 22
  local name_size = style.hatched and style.size.small or style.size.normal
  local top = spec.title and 16 or 0
  local node = ui.Item(a11y(U.place(spec, { width = w, height = top + bs + 12 }), "meter",
    tostring(get(spec.name) or "Unit"), v))
  if spec.title then
    ui.reparent(cap(style, { width = w, horizontal_alignment = "center", text = spec.title, color = style.ink_lo }), node)
  end
  local badge
  local level = function() return tostring(get(spec.level) or "") end
  if style.hatched then
    badge = ui.Item { width = bs, height = bs,
      hair({ anchors = { fill = true } }, color),
      ui.Rect { x = 0, y = 0, width = 4, height = 4, color = color },
      txt(style, { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        text = level, font_size = style.size.small - 2, font_weight = 600, color = color }) }
  else
    badge = ui.Item { width = bs, height = bs,
      style.kit.shape { anchors = { fill = true }, shape = "cookie6", color = color },
      txt(style, { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        text = level, font_size = style.size.small - 2, font_weight = 700, color = style.on_accent }) }
  end
  local name = tostring(get(spec.name) or "")
  local nw = math.min(w - bs - 8, est(style, style.hatched and name:upper() or name, name_size) + 4)
  local row = ui.Row { y = top, gap = 8, align = "center", anchors = { horizontal_center = true },
    badge,
    txt(style, { text = style.hatched and function() return tostring(get(spec.name) or ""):upper() end or spec.name,
      width = nw, elide = "right", font_size = name_size, font_weight = 600, color = color, letter_spacing = style.hatched and .6 or nil }),
  }
  ui.reparent(row, node)
  ui.reparent(hbar(style, { y = top + bs + 4, width = w, height = style.hatched and 7 or 6, value = v, color = color,
    trail = 450, trail_color = style.ink }), node)
  return node
end

--- Hearts (or pips) filled by `value` in halves (7 is three and a half)
--- out of `max` (10) whole ones, `per_row` (10), `size` (20).
function M.pip_container(spec, style)
  local max = spec.max or 10
  local per = spec.per_row or 10
  local s = spec.size or 20
  local gap = style.hatched and 4 or 3
  local cols = math.min(per, max)
  local rows = math.ceil(max / per)
  local w, h = cols * s + (cols - 1) * gap, rows * s + (rows - 1) * gap
  local color = U.color(spec, style, "alert")
  local node = ui.Item(a11y(U.place(spec, { width = w, height = h }), "meter", spec.label or "Hearts",
    function() return num(spec.value) / 2 end, 0, max))
  for i = 1, max do
    local x = ((i - 1) % per) * (s + gap)
    local y = math.floor((i - 1) / per) * (s + gap)
    -- 0, .5 or 1 of this one.
    local function part() return math.max(0, math.min(1, (num(spec.value) - (i - 1) * 2) / 2)) end
    local function shown() return math.floor(part() * 2 + .5) / 2 end
    local cell = ui.Item { x = x, y = y, width = s, height = s }
    if not style.hatched then
      ui.reparent(style.kit.shape { anchors = { fill = true }, shape = "heart", color = style.track }, cell)
      ui.reparent(ui.Item { height = s, clip = true, width = function() return shown() * s end,
        behavior = { width = style.spring(300, 22) },
        ui.Item { width = s, height = s,
          scale = function() return shown() > 0 and 1 or .4 end, behavior = { scale = style.spring(380, 14) },
          style.kit.shape { anchors = { fill = true }, shape = "heart", color = color } } }, cell)
    else
      local p = 3
      ui.reparent(hair({ x = p, y = p, width = s - 2 * p, height = s - 2 * p, rotation = 45 },
        function() return shown() > 0 and get(color) or get(style.line) end), cell)
      ui.reparent(ui.Item { height = s, clip = true, width = function() return shown() * s end,
        ui.Item { width = s, height = s,
          ui.Rect { x = p + 3, y = p + 3, width = s - 2 * p - 6, height = s - 2 * p - 6, rotation = 45, color = color } } }, cell)
    end
    ui.reparent(cell, node)
  end
  return node
end

-- -------------------------------------------------------------- rings --

--- A hold to charge: a ring filling round an icon. `value` (0..1),
--- `icon` ("bolt"), `size` (96), `label`, `color`. Full, Material's
--- backdrop bursts into a star and the icon fills; Tsugumori's frame
--- lights.
function M.charge_ring(spec, style)
  local s = spec.size or 96
  local v = reader(spec.value)
  local color = U.color(spec, style, "accent")
  local full = function() return v() >= .999 end
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s + 22 }), "progress", spec.label or "Charge", v))
  local face = ui.Item { width = s, height = s }
  local t = math.max(5, math.floor(s / 12))
  if not style.hatched then
    local inner = s - 2 * t - 10
    ui.reparent(style.kit.shape { x = (s - inner) / 2, y = (s - inner) / 2, width = inner, height = inner,
      shape = function() return full() and "sunny" or "cookie12" end,
      color = function() return get(color):alpha(full() and .9 or .18) end,
      rotation = function() return full() and 22.5 or 0 end,
      behavior = { rotation = style.spring(120, 10) } }, face)
    ui.reparent(arc(style, { size = s, thickness = t, value = v, color = color }), face)
    ui.reparent(style.icon(spec.icon or "bolt", math.floor(s * .34),
      function() return full() and get(style.on_accent) or get(color) end,
      { anchors = { center_in = true }, fill = full,
        scale = function() return full() and 1.12 or 1 end, behavior = { scale = style.spring(420, 12) } }), face)
  else
    local inner = s - 2 * t - 14
    ui.reparent(arc(style, { size = s, thickness = t, value = v, color = color, steps = 24 }), face)
    ui.reparent(hair({ x = (s - inner) / 2, y = (s - inner) / 2, width = inner, height = inner,
      color = function() return full() and get(color):alpha(.2) or "transparent" end },
      function() return full() and get(color) or get(style.line) end), face)
    ui.reparent(ui.Item { x = (s - inner) / 2, y = (s - inner) / 2, width = inner, height = inner, style.marks() }, face)
    ui.reparent(style.icon(spec.icon or "bolt", math.floor(s * .3), color, { anchors = { center_in = true } }), face)
  end
  ui.reparent(face, node)
  ui.reparent(cap(style, { y = s + 4, width = s, horizontal_alignment = "center",
    text = function() return full() and (style.hatched and "Ready" or "Ready") or
      ((spec.label or "Charge") .. " " .. math.floor(v() * 100) .. "%") end,
    color = function() return full() and get(color) or get(style.ink_lo) end }), node)
  return node
end

--- A stamina ring: `value` (0..1), `size` (72), `label`, `icon` ("bolt").
--- A 300° arc, ok, then warn under .3 and alert under .15; Material's
--- dims once full, as a game hides it at rest.
function M.stamina_ring(spec, style)
  local s = spec.size or 72
  local v = reader(spec.value)
  local color = function() return level_color(style, v(), .3, .15, style.ok) end
  local t = math.max(5, math.floor(s / 9))
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s }), "meter", spec.label or "Stamina", v))
  local ring = arc(style, { size = s, thickness = t, from = -150, sweep = 300, value = v, color = color, steps = 15,
    track = function() return get(color):alpha(.16) end,
    opacity = (not style.hatched) and function() return v() >= .999 and .4 or 1 end or nil,
    behavior = (not style.hatched) and { opacity = { duration = 600, easing = "out_cubic" } } or nil })
  if style.hatched then
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = morf.geometry.ticks(s / 2, s / 2, s / 2 - t - 6, s / 2 - t - 3, { from = -150, sweep = 300, count = 11 }),
      stroke_color = style.line, stroke_width = 1 }, node)
  end
  ui.reparent(ring, node)
  ui.reparent(style.icon(spec.icon or "bolt", math.floor(s * .32), color, { anchors = { center_in = true },
    fill = not style.hatched,
    loop = function()
      if v() >= .15 then return nil end
      return { opacity = { from = 1, to = .3, duration = 300, alternate = true } }
    end }), node)
  return node
end

--- An icon under a cooldown: `icon`, `value` (what is left, 0..1),
--- `seconds` (fn -> the seconds left), `size` (64), `key` (a key label
--- in its corner, optional), `label`. A shade sweeps off it clockwise and
--- the seconds count down over it; ready, it lights.
function M.cooldown_sweep(spec, style)
  local s = spec.size or 64
  local rem = reader(spec.value)
  local ready = function() return rem() <= .001 end
  local radius = style.hatched and 0 or math.floor(s * .3)
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s }), "timer", spec.label or "Cooldown",
    function() return 1 - rem() end))
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = function() return ready() and s / 2 * .7 or radius end,
      color = function() return ready() and get(style.accent):alpha(.28) or get(style.raised) end,
      behavior = { radius = style.spring(300, 16), color = { duration = 240 } } }, node)
  else
    ui.reparent(hair({ anchors = { fill = true }, color = style.surface },
      function() return ready() and get(style.accent) or get(style.line) end), node)
  end
  ui.reparent(style.icon(spec.icon or "bolt", math.floor(s * .5),
    function() return ready() and get(style.accent) or get(style.ink_lo) end, { anchors = { center_in = true },
      fill = not style.hatched and ready or nil, opacity = function() return ready() and 1 or .35 end,
      behavior = { opacity = { duration = 200 } } }), node)
  ui.reparent(sweep(style, s, s, rem, radius), node)
  if style.hatched then
    ui.reparent(ui.Item { anchors = { fill = true }, visible = ready, style.marks() }, node)
  end
  local secs = spec.seconds or function() return nil end
  ui.reparent(txt(style, { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
    font_size = math.floor(s * .34), font_weight = 700, font_family = style.mono_font, visible = function() return not ready() end,
    text = function()
      local x = tonumber(get(secs))
      if not x then return "" end
      return x < 10 and ("%.1f"):format(x) or tostring(math.ceil(x))
    end }), node)
  if spec.key then
    ui.reparent(cap(style, { x = 5, y = 3, text = spec.key, color = style.ink }), node)
  end
  -- Ready: a ring flashes out from it.
  local flash = ui.Rect { anchors = { fill = true }, radius = radius, color = "transparent", border_width = 2,
    border_color = style.accent, opacity = 0 }
  ui.reparent(flash, node)
  local was = ready()
  morf.effect(uid("ready"), function()
    local now = ready()
    if now and not was then
      morf.animation.play { { parallel = {
        { node = flash, property = "opacity", from = 1, to = 0, duration = 520, easing = "out_cubic" },
        { node = flash, property = "scale", from = 1, to = style.hatched and 1.12 or 1.3, duration = 520, easing = "out_cubic" },
      } } }
    end
    was = now
  end, { owner = node })
  return node
end

--- A row of slots: `slots` (a list, or fn, of { icon, key, cooldown
--- (0..1 left), count }), `active` (fn -> the slot in hand), `size` (44),
--- `gap`. The active slot swells round (Material) or takes the marks
--- (Tsugumori); a cooling slot is shaded.
function M.hotbar(spec, style)
  local s = spec.size or 44
  local gap = spec.gap or 6
  local slots = get(spec.slots) or {}
  local n = #slots
  local w = n * s + (n - 1) * gap
  local node = ui.Item(a11y(U.place(spec, { width = w, height = s + 2 }), "toolbar", spec.label or "Hotbar"))
  local function active() return num(spec.active) end
  for i = 1, n do
    local function slot() return (get(spec.slots) or {})[i] or {} end
    local function on() return active() == i end
    local function cd() return clamp01(num(slot().cooldown)) end
    local cell = ui.Item { x = (i - 1) * (s + gap), y = 1, width = s, height = s,
      accessible_role = "button", accessible_name = function() return slot().label or slot().icon or ("Slot " .. i) end,
      accessible = function() return { selected = on() } end }
    local r = math.floor(s * .27)
    if not style.hatched then
      ui.reparent(ui.Rect { anchors = { fill = true },
        radius = function() return on() and s / 2 or r end,
        color = function() return on() and get(style.accent):alpha(.3) or get(style.raised) end,
        border_width = function() return on() and 2 or 0 end, border_color = style.accent,
        behavior = { radius = style.spring(320, 18), color = { duration = 200 } } }, cell)
    else
      ui.reparent(hair({ anchors = { fill = true }, color = function() return on() and get(style.accent):alpha(.14) or get(style.surface) end },
        function() return on() and get(style.accent) or get(style.line) end), cell)
      ui.reparent(ui.Item { anchors = { fill = true }, visible = on, style.marks() }, cell)
    end
    ui.reparent(style.icon(function() return slot().icon or "" end, math.floor(s * .5),
      function() return on() and get(style.accent) or get(style.ink) end, { anchors = { center_in = true },
        scale = function() return on() and 1.08 or 1 end, behavior = { scale = style.spring(400, 16) } }), cell)
    ui.reparent(sweep(style, s, s, cd, not style.hatched and r or 0), cell)
    ui.reparent(cap(style, { x = style.hatched and 4 or 6, y = 1, text = function() return tostring(slot().key or i) end,
      color = function() return on() and get(style.accent) or get(style.ink_lo) end }), cell)
    ui.reparent(txt(style, { x = 2, y = s - 17, width = s - 6, height = 16,
      horizontal_alignment = "right", font_size = style.size.small - 3, font_weight = 700, font_family = style.mono_font,
      text = function() local c = slot().count return c and int(c) or "" end, color = style.ink }), cell)
    ui.reparent(cell, node)
  end
  return node
end

--- Icons with their time: `buffs` (a list, or fn, of { icon, remaining
--- (0..1), seconds, debuff, stacks }), `size` (36). A ring (Material) or
--- an edge trace (Tsugumori) runs down round each; a buff is ok's tone, a
--- debuff alert's; one about to end blinks.
function M.buff_row(spec, style)
  local s = spec.size or 36
  local gap = 8
  local model, start = listed(spec.buffs, function(b) return tostring(b.icon) .. (b.debuff and "-" or "+") end)
  local function delegate(row)
    local st = morf.state { remaining = row.remaining or 0, seconds = row.seconds or false, stacks = row.stacks or 0 }
    local function rem() return clamp01(num(st.remaining)) end
    local color = function() return get(row.debuff and style.alert or style.ok) end
    local cell = ui.Item { width = s, height = s + 18,
      accessible_role = "list_item", accessible_name = (row.label or row.icon or "Effect") .. (row.debuff and " (debuff)" or ""),
      enter = { opacity = 0, scale = .6 }, opacity = 1, scale = 1,
      behavior = { opacity = { duration = 200 }, scale = style.spring(380, 18) },
      exit = { opacity = 0, scale = .6, duration = 180 } }
    local face = ui.Item { width = s, height = s,
      loop = function()
        if rem() > .2 or rem() <= 0 then return nil end
        return { opacity = { from = 1, to = .35, duration = 380, alternate = true } }
      end }
    if not style.hatched then
      ui.reparent(ui.Rect { x = 3, y = 3, width = s - 6, height = s - 6, radius = (s - 6) / 2,
        color = function() return color():alpha(.2) end }, face)
      ui.reparent(arc(style, { size = s, thickness = 3, value = rem, color = color, track = false }), face)
    else
      local d = ("M%g 0.5 H%g V%g H0.5 V0.5 H%g"):format(s / 2, s - .5, s - .5, s / 2)
      ui.reparent(hair({ anchors = { fill = true }, color = function() return color():alpha(.1) end }, style.line), face)
      ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, d = d, fill_color = "transparent",
        stroke_color = color, stroke_width = 2, trim_end = function() return math.floor(rem() * 16 + .5) / 16 end,
        behavior = { trim_end = style.spring() } }, face)
    end
    ui.reparent(style.icon(row.icon or "star", math.floor(s * .48), color, { anchors = { center_in = true } }), face)
    ui.reparent(txt(style, { x = s - 14, y = s - 12, width = 16, height = 14,
      horizontal_alignment = "right", font_size = style.size.small - 3, font_weight = 700, font_family = style.mono_font,
      text = function() local k = tonumber(st.stacks) return (k and k > 1) and int(k) or "" end }), face)
    ui.reparent(face, cell)
    ui.reparent(txt(style, { y = s + 2, width = s, height = 16, horizontal_alignment = "center",
      font_size = style.size.small - 3, font_family = style.mono_font, color = style.ink_lo,
      text = function()
        local x = tonumber(get(st.seconds))
        if not x then return "" end
        if x >= 60 then return ("%dm"):format(math.floor(x / 60)) end
        return ("%ds"):format(math.ceil(x))
      end }), cell)
    return cell, function(next)
      st.remaining, st.seconds, st.stacks = next.remaining or 0, next.seconds or false, next.stacks or 0
    end
  end
  local node = ui.Item(a11y(U.place(spec, { width = spec.width or 260, height = s + 18 }), "list", spec.label or "Effects"))
  ui.reparent(ui.Repeater { as = "row", gap = gap, model = model, delegate = delegate }, node)
  start(node)
  return node
end

-- ------------------------------------------------------------- radial --

--- A radial menu, shown: `items` (a list of { icon, label }),
--- `highlighted` (fn -> the item pointed at), `size` (180). The
--- highlighted sector swells out (Material) or is hatched (Tsugumori);
--- its label reads in the middle.
function M.pie_menu(spec, style)
  local s = spec.size or 180
  local c = s / 2
  local items = get(spec.items) or {}
  local n = math.max(1, #items)
  local step = 360 / n
  local r1, r0 = c - 10, math.floor(s * .2)
  local function hi() return num(spec.highlighted) end
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s }), "menu", spec.label or "Radial menu"))
  local gap = style.hatched and 2 or 3
  for i, item in ipairs(items) do
    local from = (i - 1) * step - step / 2 + gap
    local sw = step - 2 * gap
    local on = function() return hi() == i end
    local rest = morf.geometry.sector(c, c, r0, r1, from, sw)
    local sector = { width = s, height = s, view_box = { 0, 0, s, s } }
    if not style.hatched then
      sector.d = rest
      sector.morph_to = morf.geometry.sector(c, c, r0 + 2, r1 + 8, from - 1, sw + 2)
      sector.morph_progress = function() return on() and 1 or 0 end
      sector.fill_color = function() return on() and get(style.accent) or get(style.raised) end
      sector.behavior = { morph_progress = style.spring(340, 18), fill_color = { duration = 200 } }
      ui.reparent(ui.Path(sector), node)
    else
      sector.d = rest
      sector.fill_color = function() return on() and get(style.accent):alpha(.16) or get(style.surface) end
      sector.stroke_color = function() return on() and get(style.accent) or get(style.line) end
      sector.stroke_width = 1
      ui.reparent(ui.Path(sector), node)
      ui.reparent(ui.Item { width = s, height = s, visible = on,
        mask = ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, d = rest, fill_color = "#ffffff" },
        style.stripes.box { width = s, height = s, gap = 6, weight = 1, color = U.alpha(style.accent, .55) } }, node)
    end
    local a = math.rad((i - 1) * step)
    local rm = (r0 + r1) / 2 + (style.hatched and 0 or 2)
    local is = math.floor(s * .13)
    ui.reparent(style.icon(item.icon or "circle", is,
      function() return on() and get(style.hatched and style.accent or style.on_accent) or get(style.ink) end,
      { x = c + rm * math.sin(a) - is / 2, y = c - rm * math.cos(a) - is / 2, width = is, height = is,
        horizontal_alignment = "center", vertical_alignment = "center",
        scale = function() return on() and 1.15 or 1 end, behavior = { scale = style.spring(360, 16) } }), node)
  end
  local ri = r0 - 6
  local label = function() local it = items[hi()] return it and (it.label or "") or "" end
  if not style.hatched then
    ui.reparent(ui.Rect { x = c - ri, y = c - ri, width = 2 * ri, height = 2 * ri, radius = ri, color = style.surface }, node)
  else
    ui.reparent(hair({ x = c - ri, y = c - ri, width = 2 * ri, height = 2 * ri, color = style.surface }, style.line), node)
    ui.reparent(txt(style, { x = c - ri, y = c - 16, width = 2 * ri, horizontal_alignment = "center",
      font_size = style.size.small - 3, color = style.accent,
      text = function() return ("%02d/%02d"):format(math.max(0, hi()), #items) end }), node)
  end
  ui.reparent(txt(style, { x = c - ri, y = style.hatched and c - 2 or c - 10, width = 2 * ri, height = 20,
    horizontal_alignment = "center", vertical_alignment = "center", elide = "right",
    font_size = style.hatched and style.size.small - 3 or style.size.small - 2, font_weight = 600,
    text = style.hatched and function() return label():upper() end or label }), node)
  return node
end

--- A circular minimap, north up: `blips` (a list, or fn, of { x, y, kind }
--- in metres east and north of the player; "enemy", "ally",
--- "objective", else neutral), `heading` (the player's, degrees), `range`
--- (the rim's distance, 100), `size` (170). Blips past the rim sit on it.
function M.minimap(spec, style)
  local s = spec.size or 170
  local c = s / 2
  local range = spec.range or 100
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s }), "figure", spec.label or "Minimap"))
  node.accessible_description = function() return ("%d contacts within %d m"):format(#(get(spec.blips) or {}), num(range)) end
  local face = ui.Item { width = s, height = s, clip = true, mask = ui.Rect { radius = c, color = style.ink } }
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = c, color = style.raised }, face)
    for _, f in ipairs { .33, .66 } do
      local rr = (c - 4) * f
      ui.reparent(ui.Rect { x = c - rr, y = c - rr, width = 2 * rr, height = 2 * rr, radius = rr, color = "transparent",
        border_width = 1, border_color = U.alpha(style.ink_lo, .25) }, face)
    end
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.surface }, face)
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = ("M%g 0 V%g M0 %g H%g"):format(c, s, c, s), stroke_color = style.line, stroke_width = 1 }, face)
    for _, f in ipairs { .33, .66 } do
      ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
        d = circle_d(c, c, (c - 4) * f), stroke_color = style.line, stroke_width = 1, dash = { 2, 3 } }, face)
    end
  end
  -- The blips: one scatter path per kind, over the same box.
  local KINDS = { { "enemy", "alert" }, { "ally", "info" }, { "objective", "warn" }, { "other", "ink_lo" } }
  local MAP = { enemy = "enemy", hostile = "enemy", ally = "ally", friend = "ally", party = "ally",
    objective = "objective", quest = "objective" }
  local edge = (c - 6) / c
  local starts = {}
  for _, k in ipairs(KINDS) do
    local ch, st = channel.from(function()
      local out, R = {}, num(range)
      for _, b in ipairs(get(spec.blips) or {}) do
        if (MAP[b.kind] or "other") == k[1] then
          local x, y = (tonumber(b.x) or 0) / R, (tonumber(b.y) or 0) / R
          local d = math.sqrt(x * x + y * y)
          if d > edge then x, y = x / d * edge, y / d * edge end
          out[#out + 1] = x
          out[#out + 1] = y
        end
      end
      return out
    end, { size = 512 })
    starts[#starts + 1] = st
    ui.reparent(ui.Path { width = s, height = s, view_box = { 0, 0, s, s }, series = ch.id,
      plot = { kind = "scatter", width = s, height = s, left = -1, right = 1, bottom = -1, top = 1,
        point = k[1] == "objective" and 4.5 or 3.5 },
      fill_color = style[k[2]] }, face)
  end
  ui.reparent(face, node)
  for _, st in ipairs(starts) do st(node) end
  -- The rim, the north mark and the range.
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = c, color = "transparent", border_width = 2,
      border_color = style.line }, node)
    ui.reparent(ui.Rect { x = c - 11, y = 2, width = 22, height = 18, radius = 9, color = style.accent }, node)
    ui.reparent(txt(style, { x = c - 11, y = 2, width = 22, height = 18, horizontal_alignment = "center",
      vertical_alignment = "center", text = "N", font_size = style.size.small - 2, font_weight = 700,
      color = style.on_accent }), node)
  else
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = morf.geometry.ticks(c, c, c - 6, c - 1, { count = 36, major = 9, major_r0 = c - 10 }),
      stroke_color = style.stroke_of("mark"), stroke_width = 1 }, node)
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = circle_d(c, c, c - .5), stroke_color = style.stroke_of("hot"), stroke_width = 1 }, node)
    ui.reparent(ui.Rect { x = c - 9, y = 10, width = 18, height = 16, color = style.surface }, node)
    ui.reparent(txt(style, { x = c - 9, y = 10, width = 18, height = 16, horizontal_alignment = "center",
      vertical_alignment = "center", text = "N", font_size = style.size.small - 3, font_weight = 700, color = style.alert }), node)
  end
  ui.reparent(cap(style, { x = c - 30, y = s - 22, width = 60, horizontal_alignment = "center",
    text = function() return ("%d m"):format(num(range)) end, color = style.ink_lo }), node)
  -- The player: an arrow turned to the heading.
  local angle, turn = unwrap(function() return get(spec.heading) or 0 end)
  local as = 16
  ui.reparent(ui.Path { x = c - as / 2, y = c - as / 2, width = as, height = as, view_box = { 0, 0, as, as },
    d = arrow_d(as), fill_color = style.hatched and style.accent or style.ink,
    stroke_color = style.hatched and "transparent" or style.raised, stroke_width = 1.5,
    rotation = angle, behavior = { rotation = style.spring(160, 18) } }, node)
  turn(node)
  return node
end

--- A heading strip: `heading` (degrees), `markers` (a list, or fn, of {
--- angle, icon, kind, label }), `width` (260), `fov` (the degrees across,
--- 120). The card slides under a fixed index, the short way round.
function M.compass_strip(spec, style)
  local w = spec.width or 260
  local fov = spec.fov or 120
  local ppd = w / fov
  local h = 46
  local band = 24
  local angle, turn = unwrap(function() return get(spec.heading) or 0 end)
  local node = ui.Item(a11y(U.place(spec, { width = w, height = h + 16 }), "meter", spec.label or "Heading",
    function() return math.floor(angle() % 360 + .5) end, 0, 360))
  local strip = ui.Item { y = 14, width = w, height = h, clip = true,
    mask = { gradient = { angle = 90, stops = { 0, { 1, .15 }, { 1, .85 }, 0 } } } }
  if not style.hatched then
    ui.reparent(ui.Rect { y = 4, width = w, height = h - 8, radius = (h - 8) / 2, color = style.raised }, strip)
  else
    ui.reparent(ui.Rect { y = 4, width = w, height = h - 8, color = style.surface }, strip)
    ui.reparent(ui.Rect { y = 4, width = w, height = 1, color = style.line }, strip)
    ui.reparent(ui.Rect { y = h - 5, width = w, height = 1, color = style.line }, strip)
  end
  -- The card: three turns of ticks and letters, rebased every turn, slid
  -- by the unwrapped heading.
  local length = 360 * ppd
  local card = ui.Item { width = 1, height = h,
    translate_x = function() return w / 2 - angle() * ppd end,
    behavior = { translate_x = style.spring(140, 20) } }
  local LETTERS = { [0] = "N", [45] = "NE", [90] = "E", [135] = "SE", [180] = "S", [225] = "SW", [270] = "W", [315] = "NW" }
  local pitch = 15 * ppd
  local ticks = ("M0 0 V10 M%g 5 V10 M%g 5 V10"):format(pitch / 3, 2 * pitch / 3)
  for k = -1, 1 do
    local turnk = ui.Item { width = length, height = h,
      x = function() return (math.floor(angle() / 360) + k) * length end }
    for deg = 0, 345, 15 do
      local letter = LETTERS[deg]
      local tw = 40
      -- Shown only while the whole step is inside the strip.
      local function shown()
        local a = angle()
        local pos = (math.floor(a / 360) + k) * 360 + deg
        return (pos - a) * ppd >= -w / 2 + tw / 2 and (pos + 15 - a) * ppd <= w / 2
      end
      ui.reparent(ui.Item { x = deg * ppd, width = pitch, height = h, visible = shown,
        ui.Path { y = h - 5 - 10, width = pitch, height = 10, view_box = { 0, 0, pitch, 10 }, d = ticks,
          fill_color = "transparent", stroke_color = style.hatched and style.stroke_of("mark") or U.alpha(style.ink_lo, .6),
          stroke_width = 1 },
        txt(style, { x = -tw / 2, y = 6, width = tw, height = 18, horizontal_alignment = "center",
          vertical_alignment = "center", text = letter or tostring(deg),
          font_size = letter and (#letter == 1 and style.size.small or style.size.small - 2) or style.size.small - 3,
          font_weight = letter and 700 or 400,
          color = (deg == 0 and (style.hatched and style.alert or style.accent)) or (letter and style.ink or style.ink_lo) }),
      }, turnk)
    end
    ui.reparent(turnk, card)
  end
  -- The markers, each on the card nearest the heading.
  local model, start = listed(spec.markers, function(m) return tostring(m.angle) .. tostring(m.icon) .. tostring(m.label) end)
  local function marker(row)
    local color = tone_fn(style, row.kind or "objective", "warn")
    local ms = 18
    local function near()
      local a = tonumber(row.angle) or 0
      return a + 360 * math.floor((angle() - a) / 360 + .5)
    end
    return ui.Item { width = ms, height = ms, y = h - 5 - ms - 1,
      x = function() return near() * ppd - ms / 2 end,
      visible = function() return math.abs((near() - angle()) * ppd) <= w / 2 - ms end,
      accessible_role = "figure", accessible_name = row.label or row.icon or "marker",
      style.hatched and ui.Rect { x = 4, y = 4, width = 10, height = 10, rotation = 45, color = "transparent",
        border_width = 1, border_color = color } or ui.Rect { x = 1, y = 1, width = 16, height = 16, radius = 8, color = color },
      style.icon(row.icon or "flag", 12, style.hatched and color or style.on_accent,
        { anchors = { center_in = true }, fill = not style.hatched }) }
  end
  ui.reparent(ui.Repeater { model = model, delegate = marker }, card)
  ui.reparent(card, strip)
  ui.reparent(strip, node)
  start(node)
  turn(node)
  -- The index and the reading over it.
  if not style.hatched then
    ui.reparent(ui.Rect { x = w / 2 - 22, y = 0, width = 44, height = 20, radius = 10, color = style.accent }, node)
    ui.reparent(ui.Rect { x = w / 2 - 1, y = 18, width = 2, height = h - 4, radius = 1, color = style.accent }, node)
  else
    ui.reparent(hair({ x = w / 2 - 22, y = 0, width = 44, height = 18, color = style.surface }, style.accent), node)
    ui.reparent(ui.Rect { x = w / 2, y = 18, width = 1, height = h - 2, color = style.accent }, node)
  end
  ui.reparent(txt(style, { x = w / 2 - 22, y = 0, width = 44, height = style.hatched and 18 or 20,
    horizontal_alignment = "center", vertical_alignment = "center", font_size = style.size.small - 2, font_weight = 700,
    font_family = style.mono_font, color = style.hatched and style.accent or style.on_accent,
    text = function() return ("%03d"):format(math.floor((num(get(spec.heading))) % 360 + .5) % 360) end }), node)
  return node
end

-- -------------------------------------------------------------- feeds --

--- Who took whom: `entries` (a list, or fn, oldest first, of { killer,
--- victim, icon, ally (the killer is on your side), id }), `max` (5),
--- `width` (260). The newest is on top and the older fade.
function M.kill_feed(spec, style)
  local w = spec.width or 260
  local rh = style.hatched and 22 or 26
  local gap = 4
  local max = spec.max or 5
  local model, start = listed(spec.entries, function(e) return tostring(e.killer) .. ">" .. tostring(e.victim) end,
    { reverse = true, max = max })
  local size = style.size.small - 1
  local function delegate(row)
    local st = morf.state { rank = row.rank }
    local killer = tostring(row.killer or "")
    local victim = tostring(row.victim or "")
    if style.hatched then killer, victim = killer:upper(), victim:upper() end
    local kw, vw = est(style, killer, size), est(style, victim, size)
    local inner = kw + vw + 18 + 16
    local pw = math.min(w, inner + 20)
    local kc = function() return get(row.ally and style.info or style.ink) end
    local vc = function() return get(row.ally and style.alert or style.info) end
    local box = ui.Item { width = w, height = rh,
      opacity = function() return math.max(.3, 1 - (st.rank - 1) * .17) end,
      behavior = { opacity = { duration = 300 }, translate_x = style.spring(300, 24) },
      enter = { opacity = 0, translate_x = 30 }, translate_x = 0,
      exit = { opacity = 0, duration = 200 },
      accessible_role = "list_item", accessible_name = ("%s took %s"):format(row.killer or "?", row.victim or "?") }
    local pill = ui.Item { anchors = { right = true }, width = pw, height = rh }
    if not style.hatched then
      ui.reparent(ui.Rect { anchors = { fill = true }, radius = rh / 2,
        color = function() return st.rank == 1 and get(style.accent):alpha(.22) or get(style.raised) end,
        behavior = { color = { duration = 300 } } }, pill)
    else
      ui.reparent(ui.Rect { anchors = { fill = true }, color = U.alpha(style.surface, .9) }, pill)
      ui.reparent(ui.Rect { y = rh - 1, width = pw, height = 1, color = style.line }, pill)
      ui.reparent(ui.Rect { width = 2, height = rh,
        color = function() return st.rank == 1 and get(style.accent) or get(style.line) end }, pill)
    end
    ui.reparent(ui.Row { anchors = { center_in = true }, gap = 8, align = "center",
      txt(style, { text = killer, font_size = size, font_weight = 600, color = kc, width = kw, elide = "right" }),
      style.icon(row.icon or "close", 16, style.ink_lo),
      txt(style, { text = victim, font_size = size, font_weight = 600, color = vc, width = vw, elide = "right" }),
    }, pill)
    ui.reparent(pill, box)
    return box, function(next) st.rank = next.rank end
  end
  local h = max * rh + (max - 1) * gap
  local node = ui.Item(a11y(U.place(spec, { width = w, height = h }), "log", spec.label or "Kill feed"))
  ui.reparent(ui.Repeater { as = "column", gap = gap, width = w, model = model, delegate = delegate }, node)
  start(node)
  return node
end

--- Numbers that float up from hits: `numbers` (a list, or fn, of { value,
--- x, y (0..1 across the box), crit, kind ("heal"), id }), `width` (240),
--- `height` (170), `life` (ms, 1100). Each pops, rises and fades.
function M.damage_numbers(spec, style)
  local w, h = spec.width or 240, spec.height or 170
  local life = spec.life or 1100
  local model, start = listed(spec.numbers, function(n) return tostring(n.value) .. "@" .. tostring(n.x) .. "," .. tostring(n.y) end)
  local function delegate(row)
    local crit = row.crit
    local heal = row.kind == "heal"
    local size = crit and style.size.extra or style.size.large
    local word = int(row.value)
    if heal then word = "+" .. word end
    if crit then word = style.hatched and ("[" .. word .. "]") or (word .. "!") end
    local tw = est(style, word, size) + 12
    local color = heal and style.ok or (crit and style.warn or style.ink)
    local x = clamp01(tonumber(row.x) or .5) * (w - tw)
    local y = clamp01(tonumber(row.y) or .5) * (h - size * 1.3 - 40) + 40
    return ui.Item { x = x, y = y, width = tw, height = math.ceil(size * 1.3),
      accessible_role = "list_item", accessible_name = word,
      translate_y = -36, enter = { translate_y = 0, duration = life, easing = "out_cubic" },
      exit = { opacity = 0, duration = 120 },
      ui.Item { anchors = { fill = true },
        opacity = 0, enter = { opacity = 1, duration = life, easing = "in_quart" },
        ui.Item { anchors = { fill = true },
          scale = 1, enter = { scale = crit and 1.7 or 1.3, duration = style.hatched and 140 or 320,
            easing = style.hatched and "out_cubic" or "out_back" },
          txt(style, { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
            text = word, font_size = size, font_weight = crit and 800 or 700, color = color,
            font_family = style.hatched and style.mono_font or nil }) } } }
  end
  local node = ui.Item(a11y(U.place(spec, { width = w, height = h }), "log", spec.label or "Damage"))
  ui.reparent(ui.Repeater { model = model, delegate = delegate }, node)
  start(node)
  return node
end

--- A title and its objectives: `title`, `objectives` (a list, or fn, of {
--- text, done, value (0..1) or count and total }), `width` (240).
function M.objective_tracker(spec, style)
  local w = spec.width or 240
  local objectives = get(spec.objectives) or {}
  local rows = {}
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 10 }), "list", get(spec.title) or "Objectives"))
  local y = 0
  -- The title.
  if not style.hatched then
    ui.reparent(ui.Rect { y = 2, width = 4, height = 18, radius = 2, color = style.accent }, node)
    ui.reparent(txt(style, { x = 12, width = w - 12, height = 22, elide = "right", text = spec.title,
      font_size = style.size.normal, font_weight = 600 }), node)
  else
    ui.reparent(ui.Rect { y = 4, width = 6, height = 6, color = style.accent }, node)
    ui.reparent(cap(style, { x = 12, width = w - 12, text = spec.title, font_size = style.size.small - 2, color = style.ink,
      letter_spacing = 1.2 }), node)
    ui.reparent(ui.Rect { y = 20, width = w, height = 1, color = style.line }, node)
  end
  y = 28
  for i = 1, #objectives do
    local function o() return (get(spec.objectives) or {})[i] or {} end
    local function done() return o().done and true or false end
    local function frac()
      local it = o()
      if it.value ~= nil then return clamp01(num(it.value)) end
      if it.total then return clamp01(num(it.count) / math.max(1, num(it.total))) end
      return done() and 1 or 0
    end
    local has_bar = objectives[i].value ~= nil or objectives[i].total ~= nil
    local row = ui.Item { y = y, width = w, height = has_bar and 30 or 22,
      accessible_role = "list_item", accessible_name = function() return tostring(o().text or "") end,
      accessible = function() return { checked = done() } end }
    local bs = 14
    if not style.hatched then
      ui.reparent(ui.Rect { y = 3, width = bs, height = bs, radius = bs / 2,
        color = function() return done() and get(style.ok) or "transparent" end,
        border_width = 2, border_color = function() return done() and get(style.ok) or get(style.ink_lo) end,
        behavior = { color = { duration = 200 } } }, row)
    else
      ui.reparent(hair({ y = 3, width = bs, height = bs, color = function() return done() and get(style.ok):alpha(.2) or "transparent" end },
        function() return done() and get(style.ok) or get(style.ink_lo) end), row)
    end
    ui.reparent(style.icon("check", 12, function() return style.hatched and get(style.ok) or get(style.on_accent) end,
      { x = 1, y = 4, width = 12, height = 12, horizontal_alignment = "center", vertical_alignment = "center",
        scale = function() return done() and 1 or 0 end, behavior = { scale = style.spring(420, 14) } }), row)
    local count = function()
      local it = o()
      if it.total then return ("%d/%d"):format(num(it.count), num(it.total)) end
      if it.value ~= nil then return ("%d%%"):format(math.floor(frac() * 100 + .5)) end
      return ""
    end
    ui.reparent(txt(style, { x = 22, width = w - 22 - 44, height = 20, elide = "right", vertical_alignment = "center",
      text = function() return tostring(o().text or "") end, font_size = style.size.small - 1,
      color = function() return done() and get(style.ink_lo) or get(style.ink) end,
      decoration = function() return done() and { line = "through" } or {} end }), row)
    ui.reparent(txt(style, { anchors = { right = true }, width = 44, height = 20, horizontal_alignment = "right",
      vertical_alignment = "center", text = count, font_size = style.size.small - 2, font_family = style.mono_font,
      color = function() return done() and get(style.ok) or get(style.ink_lo) end }), row)
    if has_bar then
      ui.reparent(hbar(style, { x = 22, y = 23, width = w - 22, height = style.hatched and 5 or 4, value = frac,
        color = function() return done() and get(style.ok) or get(style.accent) end, steps = 20 }), row)
    end
    ui.reparent(row, node)
    rows[i] = row
    y = y + (has_bar and 32 or 24)
  end
  node.height = y
  return node
end

-- -------------------------------------------------------------- orbs --

-- A wave across `w`, `periods` whole periods of amplitude `a`, filled down to `h`.
local function wave_d(w, a, h, periods, phase)
  local parts = { ("M0 %g"):format(a + a * math.sin(phase or 0)) }
  local n = math.max(16, periods * 16)
  for i = 1, n do
    local x = w * i / n
    parts[#parts + 1] = ("L%g %g"):format(x, a + a * math.sin((phase or 0) + 2 * math.pi * periods * i / n))
  end
  parts[#parts + 1] = ("L%g %g L0 %g Z"):format(w, h, h)
  return table.concat(parts, " ")
end

--- A round orb filled by `value` (mana, energy): `value` over `max` (1),
--- `size` (120), `label` ("Mana"), `color` (info), `text` (fn, the
--- reading). Material liquid with a running wave; Tsugumori hatched in
--- whole steps under a level line.
function M.resource_orb(spec, style)
  local s = spec.size or 120
  local v = reader(spec.value, spec.max or 1)
  local color = U.color(spec, style, "info")
  local reading = spec.text or function() return ("%d%%"):format(math.floor(v() * 100 + .5)) end
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s + 22 }), "meter", spec.label or "Mana", v))
  local orb = ui.Item { width = s, height = s }
  if not style.hatched then
    local p = 5
    local d = s - 2 * p
    local A = math.max(2, d / 30)
    local function level() return d * (1 - v()) end
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = s / 2, color = function() return get(color):alpha(.16) end }, orb)
    local liquid = ui.Item { x = p, y = p, width = d, height = d, clip = true, mask = ui.Rect { radius = d / 2, color = style.ink } }
    -- Two waves, a period and a half across, each rocking between its
    -- crest and its trough (a morph of the same outline): the surface
    -- sloshes.
    local function layer(alpha, dur, phase)
      return ui.Path { width = d, height = d + 2 * A, view_box = { 0, 0, d, d + 2 * A },
        y = function() return level() - A end, behavior = { y = style.spring(110, 12) },
        d = wave_d(d, A, d + 2 * A, 1.5, phase), morph_to = wave_d(d, A, d + 2 * A, 1.5, phase + math.pi),
        morph_progress = 0, loop = { morph_progress = { from = 0, to = 1, duration = dur, easing = "in_out_sine", alternate = true } },
        fill_color = function() return get(color):alpha(alpha) end }
    end
    ui.reparent(layer(.45, 1900, math.pi / 2), liquid)
    ui.reparent(layer(1, 1400, 0), liquid)
    ui.reparent(liquid, orb)
    -- Glass: a highlight up and to the left.
    ui.reparent(ui.Rect { x = s * .22, y = s * .12, width = s * .26, height = s * .12, radius = s * .06,
      rotation = -30, color = U.alpha(style.ink, .16) }, orb)
  else
    local p = 6
    local d = s - 2 * p
    local n = 12
    local lit = quant(style, v, n)
    local function level() return d * (1 - lit()) end
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = morf.geometry.ticks(s / 2, s / 2, s / 2 - 4, s / 2, { count = 24, major = 6, major_r0 = s / 2 - 6 }),
      stroke_color = style.stroke_of("mark", color), stroke_width = 1 }, orb)
    local inside = ui.Item { x = p, y = p, width = d, height = d, mask = ui.Rect { radius = d / 2, color = style.ink } }
    ui.reparent(ui.Rect { anchors = { fill = true }, color = function() return get(color):alpha(.05) end }, inside)
    ui.reparent(ui.Item { width = d, clip = true,
      y = function() return level() end, height = function() return d - level() end,
      behavior = { y = style.spring(), height = style.spring() },
      ui.Item { width = d, height = d, y = function() return -level() end, behavior = { y = style.spring() },
        ui.Rect { width = d, height = d, color = function() return get(color):alpha(.16) end },
        style.stripes.box { width = d, height = d, gap = 5, weight = 1.5, color = U.alpha(color, .8) } } }, inside)
    ui.reparent(ui.Rect { width = d, height = 2, color = color, y = function() return level() - 1 end,
      behavior = { y = style.spring() }, visible = function() return lit() > 0 and lit() < 1 end }, inside)
    ui.reparent(inside, orb)
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
      d = circle_d(s / 2, s / 2, d / 2 + .5), stroke_color = style.stroke_of("hot", color), stroke_width = 1 }, orb)
  end
  ui.reparent(orb, node)
  ui.reparent(ui.Row { y = s + 4, anchors = { horizontal_center = true }, gap = 6, align = "center",
    cap(style, { text = spec.label or "Mana", color = style.ink_lo }),
    txt(style, { text = reading, font_size = style.size.small - 1, font_weight = 600, font_family = style.mono_font,
      color = color }) }, node)
  return node
end

--- Experience: `level`, `value` (0..1 into it), `gain` (fn -> the last
--- gain, read out as it lands), `width` (240). What a gain adds flashes,
--- and a level badge leads.
function M.xp_bar(spec, style)
  local w = spec.width or 240
  local v = reader(spec.value)
  local bs = 40
  local bw = w - bs - 10
  local h = style.hatched and 10 or 10
  local node = ui.Item(a11y(U.place(spec, { width = w, height = bs }), "progress", spec.label or "Experience", v))
  local level = function() return tostring(get(spec.level) or 1) end
  local badge = ui.Item { width = bs, height = bs }
  if not style.hatched then
    ui.reparent(style.kit.shape { anchors = { fill = true }, shape = "cookie9", color = style.accent,
      loop = { rotation = { from = 0, to = 360, duration = 24000, easing = "linear" } } }, badge)
    ui.reparent(txt(style, { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
      text = level, font_size = style.size.normal, font_weight = 700, color = style.on_accent }), badge)
  else
    ui.reparent(hair({ anchors = { fill = true } }, style.accent), badge)
    ui.reparent(cap(style, { x = 4, y = 2, text = "LV", color = style.ink_lo }), badge)
    ui.reparent(txt(style, { y = 12, width = bs, height = 24, horizontal_alignment = "center", vertical_alignment = "center",
      text = level, font_size = style.size.large, font_weight = 600, color = style.accent }), badge)
  end
  ui.reparent(badge, node)
  local bx = bs + 10
  ui.reparent(cap(style, { x = bx, y = 2, text = function() return "Level " .. level() end, color = style.ink_lo }), node)
  ui.reparent(txt(style, { x = bx, y = 0, width = bw, height = 18, horizontal_alignment = "right",
    font_size = style.size.small - 2, font_family = style.mono_font, color = style.ink_lo,
    text = function() return ("%d%%"):format(math.floor(v() * 100)) end }), node)
  local by = bs - h - 2
  ui.reparent(hbar(style, { x = bx, y = by, width = bw, height = h, value = v, color = style.accent }), node)
  -- The gain: the stretch it added flashes, and its amount rises over it.
  local flash = ui.Rect { x = bx, y = by - 2, width = 0, height = h + 4, radius = style.hatched and 0 or (h + 4) / 2,
    color = style.hatched and style.ink or style.on_accent, opacity = 0 }
  local word = txt(style, { x = bx, y = by - 20, width = bw - 44, height = 18, horizontal_alignment = "right", opacity = 0,
    font_size = style.size.small - 1, font_weight = 700, color = style.accent,
    text = function() local g = tonumber(get(spec.gain)) return g and ("+%d XP"):format(g) or "" end })
  ui.reparent(flash, node)
  ui.reparent(word, node)
  local last = v()
  morf.effect(uid("xp"), function()
    local now = v()
    if now == last then return end
    local from = now > last and last or 0
    last = now
    flash.x = bx + from * bw
    flash.width = math.max(4, (now - from) * bw)
    morf.animation.play { { parallel = {
      { node = flash, property = "opacity", keyframes = { { at = 0, value = .9 }, { at = .3, value = .7 }, { at = 1, value = 0 } }, duration = 900 },
      { node = word, property = "opacity", keyframes = { { at = 0, value = 0 }, { at = .15, value = 1 }, { at = .7, value = 1 }, { at = 1, value = 0 } }, duration = 1600 },
      { node = word, property = "translate_y", from = 6, to = -2, duration = 1600, easing = "out_cubic" },
    } } }
  end, { owner = node })
  return node
end

-- ------------------------------------------------------------ banners --

--- An unlocked achievement: `icon` ("emoji_events"), `title`,
--- `description`, `caption` ("Achievement unlocked"), `points` (optional),
--- `width` (260). It enters: Material drops in on a spring with its badge
--- turning; Tsugumori slides in square.
function M.achievement_banner(spec, style)
  local w = spec.width or 260
  local h = 76
  local is = 52
  local node = ui.Item(U.place(spec, { width = w, height = h, accessible_role = "alert",
    accessible_name = function() return (spec.caption or "Achievement unlocked") .. ": " .. tostring(get(spec.title) or "") end,
    accessible_description = spec.description }))
  local card = ui.Item { width = w, height = h, opacity = 1, translate_y = 0, scale = 1, translate_x = 0,
    enter = style.hatched and { opacity = 0, translate_x = -24, duration = 280, easing = "out_cubic" }
      or { opacity = 0, translate_y = -24, scale = .9 },
    behavior = (not style.hatched) and { opacity = { duration = 260 }, translate_y = style.spring(260, 16),
      scale = style.spring(260, 14) } or nil }
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = h / 2, color = style.raised }, card)
    local badge = ui.Item { x = 12, y = (h - is) / 2, width = is, height = is }
    ui.reparent(style.kit.shape { anchors = { fill = true }, shape = "sunny", color = style.accent,
      rotation = 0, enter = { rotation = -120, scale = .3 }, scale = 1,
      behavior = { rotation = style.spring(90, 10), scale = style.spring(220, 12) } }, badge)
    ui.reparent(style.icon(spec.icon or "emoji_events", 26, style.on_accent, { anchors = { center_in = true }, fill = true }), badge)
    ui.reparent(badge, card)
  else
    ui.reparent(hair({ anchors = { fill = true }, color = style.surface }, style.stroke_of("mark")), card)
    ui.reparent(ui.Item { anchors = { fill = true }, style.marks() }, card)
    local box = ui.Item { x = 12, y = (h - is) / 2, width = is, height = is,
      U.fill(style, { width = is, height = is, color = style.accent }) }
    ui.reparent(style.icon(spec.icon or "emoji_events", 26, style.accent, { anchors = { center_in = true } }), box)
    ui.reparent(box, card)
    ui.reparent(ui.Rect { x = 12 + is + 10, y = 10, width = 1, height = h - 20, color = style.line }, card)
  end
  local tx = 12 + is + (style.hatched and 20 or 12)
  local right = spec.points and 44 or 16
  local tw = w - tx - right
  ui.reparent(ui.Column { x = tx, y = style.hatched and 10 or 11, width = tw, gap = 1,
    cap(style, { text = spec.caption or "Achievement unlocked", color = style.accent, width = tw }),
    txt(style, { text = spec.title, width = tw, elide = "right", font_size = style.size.normal, font_weight = 700 }),
    txt(style, { text = spec.description, width = tw, elide = "right", font_size = style.size.small - 2, color = style.ink_lo }),
  }, card)
  if spec.points then
    ui.reparent(txt(style, { x = w - 36 - (style.hatched and 8 or 16), width = 36, height = h,
      horizontal_alignment = "right", vertical_alignment = "center", text = tostring(spec.points),
      font_size = style.size.large, font_weight = 700, font_family = style.mono_font, color = style.accent }), card)
  end
  ui.reparent(card, node)
  return node
end

--- A line of dialogue: `speaker`, `line`, `width` (260), `kind` (the
--- speaker's tone, accent), `max_lines` (3). On a backdrop; a new line
--- fades in.
function M.subtitle_box(spec, style)
  local w = spec.width or 260
  local pad = 12
  local size = style.size.small
  local max_lines = spec.max_lines or 3
  local function lines()
    local n = est(style, get(spec.line) or "", size) * (style.hatched and 1.3 or 1.12) / (w - 2 * pad)
    return math.max(1, math.min(max_lines, math.ceil(n)))
  end
  local lh = math.ceil(size * 1.4)
  local function height() return pad * 2 + 20 + lines() * lh end
  local color = tone_fn(style, spec.kind or "accent", "accent")
  local node = ui.Item(U.place(spec, { width = w, height = height, accessible_role = "paragraph",
    accessible_name = function() return tostring(get(spec.speaker) or "") .. ": " .. tostring(get(spec.line) or "") end }))
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = 16, color = U.alpha(style.raised, .92) }, node)
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, color = U.alpha(style.surface, .9), border_width = 1,
      border_color = style.line }, node)
    ui.reparent(ui.Rect { width = 2, height = height, color = color }, node)
  end
  ui.reparent(cap(style, { x = pad, y = pad - 2, text = spec.speaker, color = color, font_size = style.size.small - 2,
    font_weight = 700 }), node)
  local body = txt(style, { x = pad, y = pad + 18, width = w - 2 * pad, wrap = true, max_lines = max_lines,
    text = spec.line, font_size = size, line_height = 1.3, opacity = 1 })
  ui.reparent(body, node)
  local last = get(spec.line)
  morf.effect(uid("line"), function()
    local now = get(spec.line)
    if now == last then return end
    last = now
    morf.animation.play { { node = body, property = "opacity", from = 0, to = 1, duration = 260, easing = "out_cubic" } }
  end, { owner = node })
  return node
end

-- ---------------------------------------------------------- reticles --

--- A crosshair: `style` ("cross", "dot", "circle"), `spread` (fn 0..1:
--- how far it opens), `size` (64), `color` (accent), `thickness`.
function M.crosshair(spec, style)
  local s = spec.size or 64
  local c = s / 2
  local kind = spec.style or "cross"
  local t = spec.thickness or (style.hatched and 1.5 or 3)
  local color = U.color(spec, style, "accent")
  local sp = reader(spec.spread or 0)
  local node = ui.Item(U.place(spec, { width = s, height = s, accessible_role = "figure",
    accessible_name = spec.label or "Crosshair" }))
  local move = style.spring(380, 22)
  local L = s * .2
  local function gap() return s * .08 + sp() * s * .2 end
  local function bar(dx, dy)
    local horizontal = dx ~= 0
    local p = { width = horizontal and L or t, height = horizontal and t or L, color = color,
      radius = style.hatched and 0 or t / 2, behavior = { x = move, y = move } }
    if horizontal then
      p.y = c - t / 2
      p.x = function() return dx > 0 and (c + gap()) or (c - gap() - L) end
    else
      p.x = c - t / 2
      p.y = function() return dy > 0 and (c + gap()) or (c - gap() - L) end
    end
    return ui.Rect(p)
  end
  local dot = style.hatched and 3 or 4
  if kind == "cross" then
    for _, d in ipairs { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } } do ui.reparent(bar(d[1], d[2]), node) end
    if style.hatched then
      ui.reparent(ui.Rect { x = c - 1, y = c - 1, width = 2, height = 2, color = color }, node)
    end
  elseif kind == "dot" then
    ui.reparent(ui.Rect { x = c - dot, y = c - dot, width = 2 * dot, height = 2 * dot,
      radius = style.hatched and 0 or dot, color = color }, node)
    -- The spread: a faint ring that opens out and fades.
    local function rr() return s * .12 + sp() * s * .3 end
    ui.reparent(ui.Rect { x = function() return c - rr() end, y = function() return c - rr() end,
      width = function() return 2 * rr() end, height = function() return 2 * rr() end,
      radius = style.hatched and 0 or function() return rr() end, color = "transparent", border_width = 1,
      border_color = color, opacity = function() return .25 + .5 * (1 - sp()) end,
      behavior = { x = move, y = move, width = move, height = move, radius = move, opacity = { duration = 200 } } }, node)
  else
    local function rr() return s * .2 + sp() * s * .22 end
    if not style.hatched then
      ui.reparent(ui.Rect { x = function() return c - rr() end, y = function() return c - rr() end,
        width = function() return 2 * rr() end, height = function() return 2 * rr() end,
        radius = function() return rr() end, color = "transparent", border_width = 2, border_color = color,
        behavior = { x = move, y = move, width = move, height = move, radius = move } }, node)
    else
      -- Square brackets at the corners, opening out.
      local k = 7
      for _, q in ipairs { { -1, -1 }, { 1, -1 }, { 1, 1 }, { -1, 1 } } do
        local d = ("M%g %g L0 0 L%g %g"):format(0, -q[2] * k, -q[1] * k, 0)
        ui.reparent(ui.Path { width = 1, height = 1, d = d, fill_color = "transparent", stroke_color = color, stroke_width = 1.5,
          x = function() return c + q[1] * rr() * .8 end, y = function() return c + q[2] * rr() * .8 end,
          behavior = { x = move, y = move } }, node)
      end
    end
    ui.reparent(ui.Rect { x = c - dot / 2, y = c - dot / 2, width = dot, height = dot,
      radius = style.hatched and 0 or dot / 2, color = color }, node)
  end
  return node
end

--- A hit's mark: four strokes round the centre that flash and fade.
--- `hit` (a value; each change flashes it), `kill` (fn -> bool: alert's
--- tone), `size` (40), `linger` (ms, 420).
function M.hit_marker(spec, style)
  local s = spec.size or 40
  local c = s / 2
  local t = style.hatched and 1.5 or 3
  local color = function() return get(get(spec.kill) and style.alert or style.ink) end
  local node = ui.Item(U.place(spec, { width = s, height = s, accessible_role = "figure",
    accessible_name = spec.label or "Hit marker" }))
  local g, L = s * .16, s * .3
  local parts = {}
  for _, q in ipairs { { -1, -1 }, { 1, -1 }, { 1, 1 }, { -1, 1 } } do
    local x0, y0 = c + q[1] * g, c + q[2] * g
    local x1, y1 = c + q[1] * (g + L), c + q[2] * (g + L)
    parts[#parts + 1] = ("M%g %g L%g %g"):format(x0, y0, x1, y1)
  end
  local mark = ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, d = table.concat(parts, " "),
    fill_color = "transparent", stroke_color = color, stroke_width = t, stroke_cap = style.hatched and "butt" or "round",
    opacity = 0 }
  ui.reparent(mark, node)
  if style.hatched then
    ui.reparent(ui.Rect { x = c - 1, y = c - 1, width = 2, height = 2, color = color, opacity = 0 }, node)
  end
  local last = get(spec.hit)
  local linger = spec.linger or 420
  morf.effect(uid("hit"), function()
    local now = get(spec.hit)
    if now == last then return end
    last = now
    morf.animation.play { { parallel = {
      { node = mark, property = "opacity", keyframes = { { at = 0, value = 1 }, { at = .45, value = 1 }, { at = 1, value = 0 } }, duration = linger },
      { node = mark, property = "scale", from = style.hatched and 1.15 or 1.4, to = 1, duration = style.hatched and 120 or 220,
        easing = style.hatched and "out_cubic" or "out_back" },
    } } }
  end, { owner = node })
  return node
end

--- Where hits come from: `hits` (a list, or fn, of { angle (degrees,
--- clockwise from ahead), strength (0..1) }), `size` (170), `segments`
--- (8). The arcs round the centre light towards each hit and fade.
function M.damage_direction(spec, style)
  local s = spec.size or 170
  local c = s / 2
  local n = spec.segments or 8
  local step = 360 / n
  local t = style.hatched and 8 or 10
  local r = c - t / 2 - 2
  local node = ui.Item(U.place(spec, { width = s, height = s, accessible_role = "figure",
    accessible_name = spec.label or "Damage direction" }))
  node.accessible_description = function()
    local best, a = 0, nil
    for _, hit in ipairs(get(spec.hits) or {}) do
      if (tonumber(hit.strength) or 1) > best then best, a = tonumber(hit.strength) or 1, tonumber(hit.angle) end
    end
    return a and ("Hit from %d°"):format(math.floor(a % 360)) or "No hits"
  end
  for i = 1, n do
    local mid = (i - 1) * step
    local function heat()
      local best = 0
      for _, hit in ipairs(get(spec.hits) or {}) do
        local d = math.abs(((tonumber(hit.angle) or 0) - mid + 540) % 360 - 180)
        local k = math.max(0, 1 - d / step)
        best = math.max(best, k * clamp01(tonumber(hit.strength) or 1))
      end
      return best
    end
    local d = morf.geometry.arc(c, c, r, mid - step / 2 + 4, step - 8)
    ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, d = d, fill_color = "transparent",
      stroke_color = style.hatched and style.line or U.alpha(style.ink_lo, .14), stroke_width = style.hatched and 1 or t,
      stroke_cap = style.hatched and "butt" or "round" }, node)
    local lit = { anchors = { fill = true }, view_box = { 0, 0, s, s }, d = d, fill_color = "transparent",
      stroke_color = style.alert, stroke_width = t, stroke_cap = style.hatched and "butt" or "round",
      opacity = style.hatched and function() return math.floor(heat() * 4 + .5) / 4 end or heat,
      behavior = { opacity = { duration = style.hatched and 120 or 360, easing = "out_cubic" } } }
    if style.hatched then lit.dash = { 3, 2 } end
    ui.reparent(ui.Path(lit), node)
  end
  -- The player at the centre.
  if not style.hatched then
    ui.reparent(ui.Rect { x = c - 5, y = c - 5, width = 10, height = 10, radius = 5, color = style.ink_lo }, node)
  else
    ui.reparent(hair({ x = c - 5, y = c - 5, width = 10, height = 10 }, style.ink_lo), node)
    ui.reparent(ui.Item { x = c - 30, y = c - 30, width = 60, height = 60, style.marks() }, node)
  end
  return node
end

-- ------------------------------------------------------------ counters --

--- Rounds in hand: `mag` (fn), `capacity` (30), `total` (fn, in
--- reserve), `low` (.3: warn under, alert at none), `weapon` (a caption),
--- `width` (220). A pip per round, spent ones dark.
function M.ammo_counter(spec, style)
  local w = spec.width or 220
  local cap_n = spec.capacity or 30
  local frac = function() return clamp01(num(spec.mag) / cap_n) end
  local color = function()
    local m = num(spec.mag)
    if m <= 0 then return get(style.alert) end
    if frac() < (spec.low or .3) then return get(style.warn) end
    return get(style.ink)
  end
  local big = style.size.extra + 6
  local node = ui.Item(a11y(U.place(spec, { width = w, height = big + 28 }), "meter", spec.label or "Ammunition",
    function() return num(spec.mag) end, 0, cap_n))
  local digits = txt(style, { y = 0, height = math.ceil(big * 1.15), font_size = big, font_weight = style.hatched and 500 or 600,
    font_family = style.mono_font, color = color, text = function() return ("%d"):format(num(spec.mag)) end,
    loop = function()
      if num(spec.mag) > 0 then return nil end
      return { opacity = { from = 1, to = .35, duration = 380, alternate = true } }
    end })
  ui.reparent(ui.Row { gap = 6, align = "end", digits,
    txt(style, { text = function() return "/ " .. ("%d"):format(num(spec.total)) end, font_size = style.size.normal,
      font_family = style.mono_font, color = style.ink_lo, height = math.ceil(big * 1.05) }) }, node)
  ui.reparent(cap(style, { anchors = { right = true }, y = 4, width = 110, horizontal_alignment = "right",
    text = function()
      if num(spec.mag) <= 0 then return "Reload" end
      return get(spec.weapon) or ""
    end,
    color = function() return num(spec.mag) <= 0 and get(style.alert) or get(style.ink_lo) end }), node)
  -- The pips.
  local py = big + 12
  local ph = 12
  local pg = cap_n > 40 and 1 or 2
  local pw = (w - (cap_n - 1) * pg) / cap_n
  for i = 1, cap_n do
    local on = function() return num(spec.mag) >= i end
    local x = (i - 1) * (pw + pg)
    if not style.hatched then
      ui.reparent(ui.Rect { x = x, y = py, width = pw, height = ph, radius = math.min(pw, ph) / 2,
        color = function() return on() and color() or get(style.track) end,
        scale = function() return on() and 1 or .7 end,
        behavior = { color = { duration = 160 }, scale = style.spring(420, 18) } }, node)
    else
      ui.reparent(ui.Rect { x = x, y = py, width = pw, height = ph,
        color = function() return on() and color() or "transparent" end,
        border_width = 1, border_color = function() return on() and color() or get(style.line) end }, node)
    end
  end
  return node
end

--- A combo: `count` (fn), `multiplier` (fn), `decay` (fn 0..1, the time
--- left to keep it), `label` ("Combo"). Each change punches the number.
function M.combo_counter(spec, style)
  local w = spec.width or 200
  local big = style.hatched and 52 or 60
  local node = ui.Item(a11y(U.place(spec, { width = w, height = big + 40 }), "status", spec.label or "Combo"))
  node.accessible = function() return { value = num(spec.count) } end
  local decay = reader(spec.decay or 1)
  local number = ui.Item { width = w * .6, height = math.ceil(big * 1.1) }
  ui.reparent(txt(style, { anchors = { fill = true }, horizontal_alignment = "left", vertical_alignment = "center",
    text = function() return tostring(math.floor(num(spec.count))) end, font_size = big, font_weight = style.hatched and 500 or 800,
    font_family = style.hatched and style.mono_font or nil, color = style.accent }), number)
  ui.reparent(number, node)
  local mult = function() return ("×%.1f"):format(num(spec.multiplier)) end
  local rx = math.floor(w * .6)
  local rw = w - rx
  if not style.hatched then
    ui.reparent(ui.Rect { x = rx, y = 12, width = rw, height = 30, radius = 15, color = U.alpha(style.extra, .25) }, node)
    ui.reparent(txt(style, { x = rx, y = 12, width = rw, height = 30, horizontal_alignment = "center", vertical_alignment = "center",
      text = mult, font_size = style.size.large, font_weight = 700, color = style.extra }), node)
  else
    ui.reparent(hair({ x = rx, y = 12, width = rw, height = 28, color = U.alpha(style.extra, .1) }, style.extra), node)
    ui.reparent(txt(style, { x = rx, y = 12, width = rw, height = 28, horizontal_alignment = "center", vertical_alignment = "center",
      text = mult, font_size = style.size.large, font_weight = 600, color = style.extra }), node)
  end
  ui.reparent(cap(style, { x = rx, y = 46, width = rw, horizontal_alignment = "center", text = spec.label or "Combo",
    color = style.ink_lo }), node)
  ui.reparent(hbar(style, { y = big + 20, width = w, height = style.hatched and 8 or 8, value = decay,
    color = function() return level_color(style, decay(), .35, .15, style.accent) end, steps = 20 }), node)
  local last = num(spec.count)
  morf.effect(uid("combo"), function()
    local now = num(spec.count)
    if now == last then return end
    last = now
    if style.hatched then
      morf.animation.play { { node = number, property = "scale", from = 1.14, to = 1, duration = 140, easing = "out_cubic" } }
    else
      morf.animation.play { { parallel = {
        { node = number, property = "scale", keyframes = { { at = 0, value = 1.35 }, { at = 1, value = 1, easing = "out_back" } }, duration = 360 },
        { node = number, property = "rotation", keyframes = { { at = 0, value = -6 }, { at = 1, value = 0, easing = "out_back" } }, duration = 420 },
      } } }
    end
  end, { owner = node })
  return node
end

-- ------------------------------------------------------------ markers --

--- A target off the screen: an arrow on the box's edge pointing at it.
--- `angle` (degrees clockwise from up, from the box's centre), `distance`
--- (fn, metres), `label`, `kind` (objective), `width` (240), `height`
--- (170).
function M.offscreen_arrow(spec, style)
  local w, h = spec.width or 240, spec.height or 170
  local m = 20
  local color = tone_fn(style, spec.kind or "objective", "warn")
  local node = ui.Item(U.place(spec, { width = w, height = h, accessible_role = "figure",
    accessible_name = spec.label or "Off-screen target" }))
  node.accessible_description = function() return ("%d m at %d°"):format(math.floor(num(spec.distance)), math.floor(num(spec.angle) % 360)) end
  if not style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = 20, color = "transparent", border_width = 1,
      border_color = style.line }, node)
  else
    ui.reparent(hair({ anchors = { fill = true } }, style.line), node)
    ui.reparent(ui.Item { anchors = { fill = true }, style.marks() }, node)
  end
  -- You, at the centre.
  ui.reparent(ui.Rect { x = w / 2 - 3, y = h / 2 - 3, width = 6, height = 6, radius = style.hatched and 0 or 3,
    color = style.ink_lo }, node)
  local angle, turn = unwrap(function() return get(spec.angle) or 0 end)
  local function at(inset)
    local a = math.rad(num(spec.angle))
    local dx, dy = math.sin(a), -math.cos(a)
    local hx, hy = w / 2 - inset, h / 2 - inset
    local k = math.min(math.abs(dx) > 1e-6 and hx / math.abs(dx) or 1e9, math.abs(dy) > 1e-6 and hy / math.abs(dy) or 1e9)
    return w / 2 + dx * k, h / 2 + dy * k
  end
  local as = 24
  local move = style.spring(200, 20)
  ui.reparent(ui.Path { width = as, height = as, view_box = { 0, 0, as, as }, d = arrow_d(as),
    fill_color = style.hatched and "transparent" or color, stroke_color = color,
    stroke_width = style.hatched and 1.5 or 0,
    x = function() local x = at(m) return x - as / 2 end, y = function() local _, y = at(m) return y - as / 2 end,
    rotation = angle, behavior = { x = move, y = move, rotation = move } }, node)
  local lw, lh = 64, 34
  local label = ui.Column { width = lw, gap = 0, align = "center",
    x = function() local x = at(m + 34) return math.max(4, math.min(w - lw - 4, x - lw / 2)) end,
    y = function() local _, y = at(m + 30) return math.max(4, math.min(h - lh - 4, y - lh / 2)) end,
    behavior = { x = move, y = move },
    txt(style, { width = lw, horizontal_alignment = "center", font_size = style.size.small, font_weight = 700,
      font_family = style.mono_font, color = color, text = function() return ("%dm"):format(math.floor(num(spec.distance))) end }),
    cap(style, { width = lw, horizontal_alignment = "center", text = spec.label or "", color = style.ink_lo }) }
  ui.reparent(label, node)
  turn(node)
  return node
end

--- A waypoint: a diamond with an icon, its `label` above and `distance`
--- (fn, metres) below. `icon` ("flag"), `kind` (objective), `size` (44).
--- It pulses.
function M.waypoint_marker(spec, style)
  local s = spec.size or 44
  local w = math.max(120, s + 40)
  local color = tone_fn(style, spec.kind or "objective", "warn")
  local top = 24
  local node = ui.Item(U.place(spec, { width = w, height = top + s + 26, accessible_role = "figure",
    accessible_name = function() return tostring(get(spec.label) or "Waypoint") end }))
  node.accessible_description = function() return ("%d m"):format(math.floor(num(spec.distance))) end
  local cx = w / 2
  if not style.hatched then
    local lw = est(style, get(spec.label) or "", style.size.small - 2) + 20
    ui.reparent(ui.Rect { x = cx - lw / 2, y = 0, width = lw, height = 20, radius = 10, color = style.raised }, node)
    ui.reparent(txt(style, { x = cx - lw / 2, y = 0, width = lw, height = 20, horizontal_alignment = "center",
      vertical_alignment = "center", text = spec.label, font_size = style.size.small - 2, font_weight = 600 }), node)
    ui.reparent(ui.Rect { x = cx - s / 2, y = top, width = s, height = s, radius = s / 2, color = "transparent",
      border_width = 2, border_color = color, opacity = 0,
      loop = { scale = { from = .7, to = 1.5, duration = 1600, easing = "out_cubic" },
        opacity = { from = .8, to = 0, duration = 1600, easing = "out_cubic" } } }, node)
    ui.reparent(style.kit.shape { x = cx - s / 2, y = top, width = s, height = s, shape = "gem", color = color,
      loop = { scale = { from = 1, to = 1.06, duration = 800, easing = "in_out_sine", alternate = true } } }, node)
    ui.reparent(style.icon(spec.icon or "flag", math.floor(s * .45), style.on_accent,
      { x = cx - s / 2, y = top, width = s, height = s, horizontal_alignment = "center", vertical_alignment = "center", fill = true }), node)
  else
    ui.reparent(cap(style, { x = 0, y = 2, width = w, horizontal_alignment = "center", text = spec.label, color = style.ink }), node)
    local d = s * .72
    ui.reparent(ui.Rect { x = cx - d / 2, y = top + (s - d) / 2, width = d, height = d, rotation = 45,
      color = function() return color():alpha(.14) end, border_width = 1, border_color = color }, node)
    ui.reparent(ui.Rect { x = cx - d / 2 - 5, y = top + (s - d) / 2 - 5, width = d + 10, height = d + 10, rotation = 45,
      color = "transparent", border_width = 1, border_color = function() return color():alpha(.4) end,
      loop = { opacity = { from = 1, to = .2, duration = 700, alternate = true } } }, node)
    ui.reparent(style.icon(spec.icon or "flag", math.floor(s * .4), color,
      { x = cx - s / 2, y = top, width = s, height = s, horizontal_alignment = "center", vertical_alignment = "center" }), node)
  end
  ui.reparent(txt(style, { x = 0, y = top + s + 4, width = w, height = 20, horizontal_alignment = "center",
    font_size = style.size.small, font_weight = 700, font_family = style.mono_font, color = color,
    text = function()
      local d = num(spec.distance)
      return d >= 1000 and ("%.1f km"):format(d / 1000) or ("%d m"):format(math.floor(d))
    end }), node)
  return node
end

-- -------------------------------------------------------------- racing --

--- A lap board: `lap`, `laps`, `position`, `racers`, `current` (fn,
--- seconds into this lap), `last`, `best` (seconds), `width` (240).
function M.lap_tracker(spec, style)
  local w = spec.width or 240
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 150 }), "group", spec.label or "Lap"))
  local half = (w - 12) / 2
  local function block(x, caption, value, total)
    local b = ui.Item { x = x, width = half, height = 52 }
    if not style.hatched then
      ui.reparent(ui.Rect { anchors = { fill = true }, radius = 16, color = style.raised }, b)
    else
      ui.reparent(hair({ anchors = { fill = true } }, style.line), b)
      ui.reparent(ui.Rect { width = 6, height = 6, color = style.accent }, b)
    end
    ui.reparent(cap(style, { x = 12, y = 5, text = caption, color = style.ink_lo }), b)
    ui.reparent(ui.Row { x = 12, y = 20, gap = 3, align = "end",
      txt(style, { text = function() return tostring(math.floor(num(value))) end, font_size = style.size.large + 4, font_weight = 700,
        font_family = style.mono_font, color = style.accent, height = 28 }),
      txt(style, { text = function() return "/" .. tostring(math.floor(num(total))) end, font_size = style.size.small - 1,
        font_family = style.mono_font, color = style.ink_lo, height = 24 }) }, b)
    return b
  end
  ui.reparent(block(0, "Lap", spec.lap, spec.laps), node)
  ui.reparent(block(half + 12, "Position", spec.position, spec.racers), node)
  local laps = math.max(1, num(spec.laps))
  ui.reparent(style.kit.segmented_progress { y = 62, width = w, height = 6, segments = math.min(laps, 20),
    value = function() return clamp01((num(spec.lap) - 1) / laps) end }, node)
  local function line(y, caption, value, delta)
    local r = ui.Item { y = y, width = w, height = 20 }
    ui.reparent(cap(style, { y = 3, text = caption, color = style.ink_lo }), r)
    ui.reparent(txt(style, { x = w - 110 - (delta and 66 or 0), width = 110, height = 20, horizontal_alignment = "right", text = function() return fmt_time(get(value)) end, font_size = style.size.small,
      font_family = style.mono_font, font_weight = 600 }), r)
    if delta then
      ui.reparent(txt(style, { x = w - 62, width = 62, height = 20, horizontal_alignment = "right",
        font_size = style.size.small - 2, font_family = style.mono_font, font_weight = 600,
        text = function()
          local d = delta()
          if not d then return "" end
          return ("%+.2f"):format(d)
        end,
        color = function() local d = delta() return d and d <= 0 and get(style.ok) or get(style.alert) end }), r)
    end
    return r
  end
  ui.reparent(line(78, "Current", spec.current), node)
  ui.reparent(line(100, "Last", spec.last, function()
    local l, b = tonumber(get(spec.last)), tonumber(get(spec.best))
    return l and b and (l - b) or nil
  end), node)
  ui.reparent(line(122, "Best", spec.best), node)
  ui.reparent(ui.Rect { y = 99, width = w, height = 1, color = style.line, opacity = .6 }, node)
  return node
end

--- A car's cluster: `speed` (fn), `max_speed` (320), `rpm` (fn 0..1),
--- `redline` (.85), `gear` (fn: a number, "N" or "R"), `unit` ("km/h"),
--- `size` (170). A 240° rev arc, red past the line; the speed and the gear
--- in the middle.
function M.racing_hud(spec, style)
  local s = spec.size or 170
  local c = s / 2
  local rpm = reader(spec.rpm)
  local red = spec.redline or .85
  local t = style.hatched and 9 or 11
  local over = function() return rpm() >= red end
  local node = ui.Item(a11y(U.place(spec, { width = s, height = s }), "meter", spec.label or "Speed",
    function() return math.floor(num(spec.speed)) end, 0, spec.max_speed or 320))
  -- The redline zone, the rev arc over it.
  ui.reparent(arc(style, { size = s, thickness = t, from = -120 + 240 * red, sweep = 240 * (1 - red),
    value = function() return 1 end, color = U.alpha(style.alert, .3), track = false, steps = 6 }), node)
  ui.reparent(arc(style, { size = s, thickness = t, from = -120, sweep = 240 * red, value = function() return 1 end,
    color = style.track, track = false, steps = 34 }), node)
  ui.reparent(arc(style, { size = s, thickness = t, from = -120, sweep = 240, value = rpm, track = false, steps = 40,
    color = function() return over() and get(style.alert) or get(style.accent) end }), node)
  -- Ticks for each thousand.
  local ri = c - t - 6
  ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, s, s }, fill_color = "transparent",
    d = morf.geometry.ticks(c, c, ri - 5, ri, { from = -120, sweep = 240, count = 9 }),
    stroke_color = style.hatched and style.stroke_of("mark") or U.alpha(style.ink_lo, .6), stroke_width = 1.5 }, node)
  local big = math.floor(s * .25)
  ui.reparent(txt(style, { x = 0, y = c - big * .75, width = s, height = math.ceil(big * 1.2), horizontal_alignment = "center",
    vertical_alignment = "center", font_size = big, font_weight = style.hatched and 500 or 700, font_family = style.mono_font,
    text = function() return tostring(math.floor(num(spec.speed) + .5)) end }), node)
  ui.reparent(cap(style, { x = 0, y = c + big * .5, width = s, horizontal_alignment = "center", text = spec.unit or "km/h",
    color = style.ink_lo }), node)
  local gs = 30
  local gy = s - gs - 6
  local gear = function() return tostring(get(spec.gear) or "N") end
  if not style.hatched then
    ui.reparent(ui.Rect { x = c - gs / 2, y = gy, width = gs, height = gs,
      radius = function() return over() and gs / 2 or 9 end,
      color = function() return over() and get(style.alert) or get(style.accent) end,
      behavior = { radius = style.spring(320, 16), color = { duration = 160 } } }, node)
    ui.reparent(txt(style, { x = c - gs / 2, y = gy, width = gs, height = gs, horizontal_alignment = "center",
      vertical_alignment = "center", text = gear, font_size = style.size.large, font_weight = 800, color = style.on_accent }), node)
  else
    ui.reparent(hair({ x = c - gs / 2, y = gy, width = gs, height = gs, color = style.surface },
      function() return over() and get(style.alert) or get(style.accent) end), node)
    ui.reparent(txt(style, { x = c - gs / 2, y = gy, width = gs, height = gs, horizontal_alignment = "center",
      vertical_alignment = "center", text = gear, font_size = style.size.large, font_weight = 600,
      color = function() return over() and get(style.alert) or get(style.accent) end }), node)
  end
  return node
end

-- ------------------------------------------------------------ prompts --

--- A key to press: `key` ("E"), `action` (its words), `hold` (fn 0..1:
--- a hold's progress; none for a tap), `caption` ("Hold" / "Press").
function M.interaction_prompt(spec, style)
  local ks = 46
  local hold = spec.hold and reader(spec.hold) or nil
  local w = spec.width or 220
  local node = ui.Item(U.place(spec, { width = w, height = ks + 4, accessible_role = hold and "progress" or "status",
    accessible_name = function() return tostring(get(spec.key) or "E") .. ": " .. tostring(get(spec.action) or "") end }))
  if hold then node.accessible = function() return { value = math.floor(hold() * 100), minimum = 0, maximum = 100 } end end
  local key = ui.Item { y = 2, width = ks, height = ks }
  local done = function() return hold and hold() >= .999 end
  if not style.hatched then
    local inner = ks - 12
    ui.reparent(ui.Rect { x = 6, y = 6, width = inner, height = inner,
      radius = function() return done() and inner / 2 or 10 end,
      color = function() return done() and get(style.accent) or get(style.raised) end,
      behavior = { radius = style.spring(300, 16), color = { duration = 200 } } }, key)
    if hold then ui.reparent(arc(style, { size = ks, thickness = 4, value = hold, color = style.accent }), key) end
    ui.reparent(txt(style, { x = 6, y = 6, width = inner, height = inner, horizontal_alignment = "center",
      vertical_alignment = "center", text = spec.key or "E", font_size = style.size.large, font_weight = 700,
      color = function() return done() and get(style.on_accent) or get(style.ink) end }), key)
  else
    local inner = ks - 10
    ui.reparent(hair({ x = 5, y = 5, width = inner, height = inner, color = style.surface }, style.ink_lo), key)
    if hold then
      local d = ("M%g 0.75 H%g V%g H0.75 V0.75 H%g"):format(ks / 2, ks - .75, ks - .75, ks / 2)
      ui.reparent(hair({ anchors = { fill = true } }, style.line), key)
      ui.reparent(ui.Path { anchors = { fill = true }, view_box = { 0, 0, ks, ks }, d = d, fill_color = "transparent",
        stroke_color = style.accent, stroke_width = 1.5, trim_end = function() return math.floor(hold() * 20 + .5) / 20 end,
        behavior = { trim_end = style.spring() } }, key)
    end
    ui.reparent(txt(style, { x = 5, y = 5, width = inner, height = inner, horizontal_alignment = "center",
      vertical_alignment = "center", text = spec.key or "E", font_size = style.size.large, font_weight = 600,
      color = style.accent }), key)
  end
  ui.reparent(key, node)
  local tw = w - ks - 12
  ui.reparent(ui.Column { x = ks + 12, y = 6, width = tw, gap = 0,
    cap(style, { text = spec.caption or (hold and "Hold" or "Press"), color = style.ink_lo, width = tw }),
    txt(style, { text = spec.action or "Interact", width = tw, elide = "right", font_size = style.size.normal, font_weight = 600 }),
  }, node)
  return node
end

-- ----------------------------------------------------------- inventory --

--- A grid of slots: `items` (a list, or fn, by slot, of { icon, count,
--- rarity }), `columns` (5), `rows` (3), `selected` (fn -> a slot),
--- `slot` (44), `gap` (6). Rarity tones the slot; the selected one swells.
function M.inventory_grid(spec, style)
  local cols, rows = spec.columns or 5, spec.rows or 3
  local s, gap = spec.slot or 44, spec.gap or 6
  local w, h = cols * s + (cols - 1) * gap, rows * s + (rows - 1) * gap
  local node = ui.Item(a11y(U.place(spec, { width = w, height = h }), "grid", spec.label or "Inventory"))
  for i = 1, cols * rows do
    local function item() return (get(spec.items) or {})[i] end
    local function on() return num(spec.selected) == i end
    local function rc() local it = item() return it and kind_color(style, it.rarity or "common", "ink_lo") or get(style.line) end
    local x, y = ((i - 1) % cols) * (s + gap), math.floor((i - 1) / cols) * (s + gap)
    local cell = ui.Item { x = x, y = y, width = s, height = s, accessible_role = "grid_cell",
      accessible_name = function() local it = item() return it and (it.label or it.icon or "item") or "Empty" end,
      accessible = function() return { selected = on() } end,
      scale = function() return (on() and not style.hatched) and 1.08 or 1 end, behavior = { scale = style.spring(400, 18) } }
    if not style.hatched then
      ui.reparent(ui.Rect { anchors = { fill = true }, radius = function() return on() and 18 or 10 end,
        color = function() local it = item() return it and rc():alpha(.16) or get(style.raised) end,
        border_width = function() return on() and 2 or 0 end, border_color = style.accent,
        behavior = { radius = style.spring(320, 18) } }, cell)
      ui.reparent(ui.Rect { x = s * .3, y = s - 6, width = s * .4, height = 3, radius = 1.5, color = rc,
        visible = function() return item() ~= nil end }, cell)
    else
      ui.reparent(hair({ anchors = { fill = true }, color = function() local it = item() return it and rc():alpha(.08) or "transparent" end },
        function() return on() and get(style.accent) or get(style.line) end), cell)
      ui.reparent(ui.Rect { x = 0, y = 0, width = 5, height = 5, color = rc, visible = function() return item() ~= nil end }, cell)
      ui.reparent(ui.Item { anchors = { fill = true }, visible = on, style.marks() }, cell)
    end
    ui.reparent(style.icon(function() local it = item() return it and it.icon or "" end, math.floor(s * .5),
      function() return item() and rc() or get(style.ink_lo) end, { anchors = { center_in = true } }), cell)
    ui.reparent(txt(style, { x = 3, y = 1, width = s - 7, height = 15,
      horizontal_alignment = "right", font_size = style.size.small - 3, font_weight = 700, font_family = style.mono_font,
      text = function() local it = item() return (it and it.count and it.count > 1) and tostring(it.count) or "" end }), cell)
    ui.reparent(cell, node)
  end
  return node
end

--- Teams and their players: `teams` (a list of { name, score, kind
--- (a tone), players = { { name, score } or { name, kills, deaths } } }),
--- `width` (260). The leading team is marked.
function M.scoreboard(spec, style)
  local w = spec.width or 260
  local teams = get(spec.teams) or {}
  local hh, rh = 26, 18
  local node = ui.Item(a11y(U.place(spec, { width = w, height = 10 }), "table", spec.label or "Scoreboard"))
  local function lead()
    local best, at = -math.huge, 0
    for i, t in ipairs(get(spec.teams) or {}) do if num(t.score) > best then best, at = num(t.score), i end end
    return at
  end
  local y = 0
  for i, team in ipairs(teams) do
    local function t() return (get(spec.teams) or {})[i] or {} end
    local color = tone_fn(style, team.kind or (i == 1 and "info" or "alert"), "accent")
    local head = ui.Item { y = y, width = w, height = hh, accessible_role = "row",
      accessible_name = function() return ("%s %d"):format(tostring(t().name or ""), num(t().score)) end }
    if not style.hatched then
      ui.reparent(ui.Rect { anchors = { fill = true }, radius = hh / 2, color = function() return color():alpha(.2) end }, head)
      ui.reparent(ui.Rect { x = 8, y = 8, width = 10, height = 10, radius = 5, color = color }, head)
    else
      ui.reparent(ui.Rect { anchors = { fill = true }, color = function() return color():alpha(.08) end }, head)
      ui.reparent(ui.Rect { width = 3, height = hh, color = color }, head)
      ui.reparent(ui.Rect { y = hh - 1, width = w, height = 1, color = color }, head)
    end
    ui.reparent(txt(style, { x = 26, width = w - 90, height = hh, vertical_alignment = "center", elide = "right",
      text = function() local n = tostring(t().name or "") return style.hatched and n:upper() or n end,
      font_size = style.size.small, font_weight = 700, color = color }), head)
    ui.reparent(style.icon("crown", 16, style.warn, { x = w - 76, y = 5,
      visible = function() return lead() == i end }), head)
    ui.reparent(txt(style, { x = w - 54, width = 44, height = hh, horizontal_alignment = "right",
      vertical_alignment = "center", text = function() return tostring(math.floor(num(t().score))) end,
      font_size = style.size.large, font_weight = 700, font_family = style.mono_font, color = style.ink }), head)
    ui.reparent(head, node)
    y = y + hh + 2
    for j, p in ipairs(team.players or {}) do
      local row = ui.Item { y = y, width = w, height = rh, accessible_role = "row",
        accessible_name = tostring(p.name or p[1] or "") }
      ui.reparent(txt(style, { x = 26, width = w - 110, height = rh, vertical_alignment = "center", elide = "right",
        text = tostring(p.name or p[1] or ""), font_size = style.size.small - 2,
        color = p.self and style.accent or style.ink }), row)
      local stat = p.kills and ("%d / %d"):format(p.kills, p.deaths or 0) or tostring(p.score or p[2] or "")
      ui.reparent(txt(style, { x = w - 80, width = 70, height = rh, horizontal_alignment = "right",
        vertical_alignment = "center", text = stat, font_size = style.size.small - 2, font_family = style.mono_font,
        color = style.ink_lo }), row)
      if style.hatched and j < #team.players then
        ui.reparent(ui.Rect { x = 26, y = rh - 1, width = w - 36, height = 1, color = style.line, opacity = .5 }, row)
      end
      ui.reparent(row, node)
      y = y + rh
    end
    y = y + 8
  end
  node.height = y - 8
  return node
end

return M
