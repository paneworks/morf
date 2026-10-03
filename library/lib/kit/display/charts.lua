-- Display widgets: charts. See lib.kit.display.
--
-- Every chart is a framed figure (`title`, an optional `legend`) around a
-- plot. What streams -- bars, stacks, histograms, scatter, heatmaps,
-- calendars, waveforms, spectrograms, candles, boxes, state timelines,
-- status history, radial bars -- reads a data channel (`morf.channel`, or
-- a list or a function of one through `lib.channel.from`) drawn by a
-- `ui.Path`'s `series` and `plot`: no Lua per sample, and a multi-tone
-- kind is several paths over the same channel, one per tone, layer or
-- state, each its own colour. What is structured and changes rarely --
-- pie, donut, sunburst, treemap, sankey, funnel, gantt, flame graph -- is
-- laid out by lib.kit.display.layout when its data changes, in bindings.
--
-- The style decides the look: Material tonal fills, rounded ends and soft
-- gradients on a raised card; Tsugumori square shapes in a hairline frame
-- with registration marks, `/`-hatched fills and upper-case mono captions.
--
-- Common fields: `width`, `height` (or `size`), `title`, `label` (the
-- accessible name when there is no title), `legend` (false hides it; a
-- list of names where the chart has none), `labels`, `color`, `bare`
-- (no card or frame), and the placement fields (`id`, `x`, `y`, ...).
-- Data comes in `values`, `series` or `data` (each chart says which
-- shape), or `channel` for one already fed.
local morf = require("morf")
local ui = require("morf.ui")
local U = require("lib.kit.display.util")
local L = require("lib.kit.display.layout")
local channel = require("lib.channel")

local get = U.get
local M = {}

-- ------------------------------------------------------------- helpers --

local function reparent(child, parent) if child then ui.reparent(child, parent) end return parent end

--- A list from a spec value: a channel's numbers, a function's result, a list.
local function list_of(v)
  if channel.is(v) then return v:get() or {} end
  v = get(v)
  return type(v) == "table" and v or {}
end

local function source(spec) return spec.channel or spec.values or spec.series or spec.data end

--- A channel for `src` (as lib.channel.from), its lists put through
--- `transform` first when given; and its starter.
local function feed(src, transform, options)
  if channel.is(src) and not transform then return src, function() end end
  if transform then return channel.from(function() return transform(list_of(src)) end, options) end
  return channel.from(src or {}, options)
end

--- A function caching `f(list_of(src))` until the list is another table.
local function memo(src, f)
  local seen, out
  return function()
    local v = list_of(src)
    if out == nil or v ~= seen then seen, out = v, { f(v) } end
    return table.unpack(out)
  end
end

--- The smallest a caption is ever drawn.
local function small(style) return style.size.small - 3 end

--- A caption in the style; a width elides it rather than shrinks it.
local function cap(style, props)
  if props.width and props.elide == nil then props.elide = "right" end
  props.font_size = props.font_size or small(style)
  return U.caption(style, props)
end

--- How wide a caption of `text` is drawn, near enough to lay out by.
local function est(style, text, size)
  return utf8.len(tostring(text or "")) * (size or small(style)) * (style.hatched and 0.68 or 0.64) + 2
end

local function color_named(style, c, i)
  if c == nil then return style.series(i or 1) end
  if type(c) == "string" and style[c] then return style[c] end
  return c
end

--- A colour `c` (a value or a function of one) at alpha `a`.
local function alpha(c, a) return function() return get(c):alpha(a) end end

--- Faint guide lines across a `w` by `h` plot: `rows` bands, and on
--- Tsugumori `cols` columns too.
local function grid(style, w, h, rows, cols)
  local d = {}
  for k = 1, (rows or 4) - 1 do d[#d + 1] = ("M0 %.1f H%.1f "):format(math.floor(h * k / rows) + 0.5, w) end
  if style.hatched then
    for k = 1, (cols or 0) - 1 do d[#d + 1] = ("M%.1f 0 V%.1f "):format(math.floor(w * k / cols) + 0.5, h) end
  end
  if #d == 0 then return nil end
  return ui.Path { width = w, height = h, view_box = { 0, 0, w, h }, d = table.concat(d), fill_color = "transparent",
    stroke_color = style.hatched and (style.stroke_of and style.stroke_of("quiet") or alpha(style.line, .5))
      or alpha(style.line, .45), stroke_width = 1 }
end

--- Tsugumori's hairline floor under a plot.
local function baseline(style, w, y)
  if not style.hatched then return nil end
  return ui.Rect { y = y - 1, width = w, height = 1, color = style.line }
end

--- A maker of paths over one outline: a channel's `plot`, or path data `d`.
local function series_mk(ch, plot, w, h)
  return function(props)
    props.width, props.height, props.view_box = w, h, { 0, 0, w, h }
    props.series, props.plot = ch.id, plot
    props.fill_color = props.fill_color or "transparent"
    return ui.Path(props)
  end
end

local function d_mk(d, w, h)
  return function(props)
    props.width, props.height, props.view_box = w, h, { 0, 0, w, h }
    props.d = d
    props.fill_color = props.fill_color or "transparent"
    return ui.Path(props)
  end
end

--- A filled shape (what `mk` outlines) in the style: Material one tonal
--- fill (`o.alpha`); Tsugumori a faint wash, `/` stripes masked to the
--- shape and a hairline edge (`o.strong` a heavier one).
local function shape(style, w, h, color, mk, o)
  o = o or {}
  if not style.hatched then
    local fill = o.alpha and alpha(color, o.alpha) or color
    return mk { fill_color = fill, stroke_color = o.stroke, stroke_width = o.stroke and (o.stroke_width or 1.5) or nil }
  end
  local box = ui.Item { width = w, height = h }
  reparent(mk { fill_color = alpha(color, o.strong and .26 or .14) }, box)
  if o.hatch ~= false and w >= 4 and h >= 4 then
    reparent(ui.Path { width = w, height = h, view_box = { 0, 0, w, h }, d = style.stripes.hatch_d(w, h, o.gap or 5),
      fill_color = "transparent", stroke_color = alpha(color, o.strong and .85 or .6), stroke_width = 1,
      stroke_cap = "butt", mask = mk { fill_color = "#ffffff" } }, box)
  end
  reparent(mk { stroke_color = color, stroke_width = 1, stroke_join = "miter" }, box)
  return box
end

--- A row of legend entries (`{ text, color }`) `w` wide: those that fit.
local function legend_row(style, entries, w, x0, y0)
  local row = ui.Item { x = x0, y = y0, width = w, height = 16 }
  local x = 0
  for _, e in ipairs(entries) do
    local tw = est(style, e.text)
    if x + 12 + tw > w then break end
    reparent(ui.Rect { x = x, y = 4, width = 8, height = 8, radius = style.hatched and 0 or 4, color = e.color }, row)
    reparent(cap(style, { text = e.text, x = x + 12, y = 0, width = math.ceil(tw) + 2, color = style.ink_lo }), row)
    x = x + 12 + tw + 12
  end
  return row
end

--- The legend entries a spec asks for: `spec.legend` (names) or `names`,
--- coloured by `colors(i)`; none when the legend is off.
local function legend_entries(spec, names, colors)
  if spec.legend == false then return nil end
  local list = type(spec.legend) == "table" and spec.legend or names
  if type(list) ~= "table" or #list == 0 then return nil end
  local out = {}
  for i, name in ipairs(list) do out[i] = { text = tostring(name), color = colors(i) } end
  return out
end

--- The figure every chart is drawn in: a card (Material) or hairline frame
--- with marks (Tsugumori), the title on top, the legend at the foot.
--- Returns the root, the plot's box and its width and height.
local function frame(spec, style, name, legend)
  local W = get(spec.width or spec.size) or 260
  local H = get(spec.height or spec.size) or 190
  local bare = spec.bare == true
  local pad = bare and 0 or (style.hatched and 10 or 12)
  local root = ui.Item(U.place(spec, { width = W, height = H, accessible_role = "figure",
    accessible_name = spec.title or spec.label or name }))
  if not bare then
    if style.hatched then
      reparent(ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1, border_color = style.line }, root)
      reparent(style.marks(), root)
    else
      reparent(ui.Rect { anchors = { fill = true }, color = style.raised, radius = style.radius(32) + 4 }, root)
    end
  end
  local top, bottom = pad, pad
  if spec.title then
    reparent(cap(style, { text = spec.title, x = pad, y = top - 2, width = W - 2 * pad, color = style.ink,
      font_size = style.size.small - 1, font_weight = 500 }), root)
    top = top + 22
  end
  if legend then
    reparent(legend_row(style, legend, W - 2 * pad, pad, H - bottom - 16), root)
    bottom = bottom + 22
  end
  local w, h = W - 2 * pad, H - top - bottom
  local body = ui.Item { x = pad, y = top, width = w, height = h }
  reparent(body, root)
  return root, body, w, h
end

--- Category captions under `n` bars `gap` apart across `w`, every
--- `step`-th when they would crowd.
local function xlabels(style, labels, w, y, gap)
  local n = #labels
  if n == 0 then return nil end
  local bw = (w - gap * (n - 1)) / n
  local need = 0
  for _, l in ipairs(labels) do need = math.max(need, est(style, l)) end
  local step = 1
  while step < n and (bw + gap) * step < need + 4 do step = step + 1 end
  local item = ui.Item { y = y, width = w, height = 16 }
  for k = 1, n, step do
    local lw = math.min(w, (bw + gap) * step)
    local x = math.max(0, math.min(w - lw, (k - 1) * (bw + gap) + bw / 2 - lw / 2))
    reparent(cap(style, { text = labels[k], x = x, width = lw, horizontal_alignment = "center" }), item)
  end
  return item
end

--- A plot table that resolves its function-valued fields when read.
local function plot_of(t)
  return function()
    local out = {}
    for k, v in pairs(t) do out[k] = get(v) end
    return out
  end
end

--- The states a timeline or history draws: `spec.states` (`{ value,
--- label, color }`, `value` its index less one) or ok, warn, alert.
local function states_of(spec)
  local list = spec.states or { { label = "OK", color = "ok" }, { label = "Warn", color = "warn" },
    { label = "Down", color = "alert" } }
  local out = {}
  for i, s in ipairs(list) do
    if type(s) == "string" then s = { label = s } end
    out[i] = { value = s.value or (i - 1), label = s.label or tostring(i), color = s.color }
  end
  return out
end

--- Flattens a list of tuples (`{ {o,h,l,c}, ... }`) or passes a flat list.
local function flatten(list)
  if type(list[1]) ~= "table" then return list end
  local out = {}
  for _, t in ipairs(list) do for _, v in ipairs(t) do out[#out + 1] = v end end
  return out
end

-- ---------------------------------------------------------------- bars --

--- Bars, one per value (a frame channel): `values`, `labels` (under the
--- bars), `top` (the peak with headroom when not given), `bottom`, `gap`,
--- `color`.
function M.bars(spec, style)
  local root, body, w, h = frame(spec, style, "Bar chart")
  local labels = get(spec.labels)
  local ph = labels and h - 18 or h
  local gap = spec.gap or (style.hatched and 4 or 6)
  local ch, start = feed(source(spec))
  local plot = plot_of { kind = "bars", width = w, height = ph, gap = gap, radius = style.hatched and 0 or 8,
    min_bar = 2, bottom = spec.bottom or 0, top = spec.top or false, headroom = 1.12 }
  local area = ui.Item { width = w, height = ph }
  reparent(grid(style, w, ph, 4, 0), area)
  reparent(shape(style, w, ph, U.color(spec, style), series_mk(ch, plot, w, ph)), area)
  reparent(baseline(style, w, ph), area)
  reparent(area, body)
  if labels then reparent(xlabels(style, labels, w, ph + 2, gap), body) end
  start(root)
  return root
end

--- Stacked bars: `series` (one list per layer, or an interleaved channel
--- with `layers`), `legend` (the layers' names), `labels`, `top`, `gap`.
function M.stacked(spec, style, area_kind)
  local src = source(spec)
  local layers = spec.layers or (not channel.is(src) and #list_of(src)) or 1
  local names = spec.names or (type(spec.legend) == "table" and spec.legend) or nil
  local root, body, w, h = frame(spec, style, area_kind and "Stacked area chart" or "Stacked bar chart",
    legend_entries(spec, names, function(i) return style.series(i) end))
  local labels = not area_kind and get(spec.labels) or nil
  local ph = labels and h - 18 or h
  local gap = spec.gap or (style.hatched and 4 or 6)
  local ch, start = feed(src, not channel.is(src) and L.interleave or nil)
  local area = ui.Item { width = w, height = ph }
  reparent(grid(style, w, ph, 4, area_kind and 8 or 0), area)
  for k = 1, layers do
    local plot = plot_of { kind = area_kind and "stack" or "stack_bars", width = w, height = ph, layers = layers,
      layer = k - 1, gap = gap, radius = style.hatched and 0 or 8, top = spec.top or false, headroom = 1.1,
      samples = spec.samples or 0, pad_top = 2, pad_bottom = 0,
      -- Material's bands are smooth; Tsugumori's stay straight.
      smooth = area_kind and not style.hatched or nil }
    local color = style.series(k)
    if area_kind and not style.hatched then
      -- Material: each band a soft vertical fade of its tone.
      reparent(ui.Rect { width = w, height = ph, gradient = function()
        local c = get(color)
        return { angle = 180, stops = { { c:alpha(.9), 0 }, { c:alpha(.55), 1 } } }
      end, mask = series_mk(ch, plot, w, ph) { fill_color = "#ffffff" } }, area)
    else
      reparent(shape(style, w, ph, color, series_mk(ch, plot, w, ph), { gap = 4 + k }), area)
    end
  end
  reparent(baseline(style, w, ph), area)
  reparent(area, body)
  if labels then reparent(xlabels(style, labels, w, ph + 2, gap), body) end
  start(root)
  return root
end

--- Stacked areas: as `stacked`, each layer a band over those before it,
--- the newest sample at the right; `samples` across the width.
function M.stacked_area(spec, style) return M.stacked(spec, style, true) end

--- A histogram of raw `values` in `bins` (12) equal bins across `left`..
--- `right` (their range when not given); `labels` under the bins.
function M.histogram(spec, style)
  local root, body, w, h = frame(spec, style, "Histogram")
  local labels = get(spec.labels)
  local ph = labels and h - 18 or h
  local gap = spec.gap or 2
  local ch, start = feed(source(spec))
  local plot = plot_of { kind = "histogram", width = w, height = ph, bins = spec.bins or 12, gap = gap,
    radius = style.hatched and 0 or 6, min_bar = 1, left = spec.left, right = spec.right, top = spec.top or false,
    headroom = 1.1 }
  local area = ui.Item { width = w, height = ph }
  reparent(grid(style, w, ph, 4, 0), area)
  reparent(shape(style, w, ph, U.color(spec, style), series_mk(ch, plot, w, ph), { strong = true }), area)
  reparent(baseline(style, w, ph), area)
  reparent(area, body)
  if labels then reparent(xlabels(style, labels, w, ph + 2, gap), body) end
  start(root)
  return root
end

--- A scatter plot: `values` (`{ {x, y}, ... }` or flat pairs) or `series`
--- (a list of those, one colour each), `legend` (the series' names),
--- `left`, `right`, `bottom`, `top` (the data's range when not given).
function M.scatter(spec, style)
  local groups = spec.series
  if groups == nil then groups = { spec.values or spec.data or spec.channel } end
  local n = channel.is(groups) and 1 or #list_of(groups)
  if channel.is(groups) then groups = { groups } end
  local names = type(spec.legend) == "table" and spec.legend or nil
  local root, body, w, h = frame(spec, style, "Scatter plot",
    n > 1 and legend_entries(spec, names, function(i) return style.series(i) end) or nil)
  local function group(i)
    return function()
      local g = list_of(groups)[i]
      return flatten(list_of(g))
    end
  end
  -- One range for every group, a little past the data.
  local bounds = function()
    local l, r, b, t = math.huge, -math.huge, math.huge, -math.huge
    for i = 1, n do
      local flat = group(i)()
      for k = 1, #flat - 1, 2 do
        local x, y = tonumber(flat[k]) or 0, tonumber(flat[k + 1]) or 0
        l, r, b, t = math.min(l, x), math.max(r, x), math.min(b, y), math.max(t, y)
      end
    end
    if l == math.huge then return 0, 1, 0, 1 end
    local px, py = (r - l) * .06 + 1e-6, (t - b) * .08 + 1e-6
    return get(spec.left) or l - px, get(spec.right) or r + px, get(spec.bottom) or b - py, get(spec.top) or t + py
  end
  local point = spec.point or (style.hatched and 3 or 4)
  reparent(grid(style, w, h, 4, 6), body)
  local starts = {}
  for i = 1, n do
    local ch, start = channel.from(group(i))
    starts[#starts + 1] = start
    local plot = function()
      local l, r, b, t = bounds()
      return { kind = "scatter", width = w, height = h, left = l, right = r, bottom = b, top = t, point = point }
    end
    local color = n > 1 and style.series(i) or U.color(spec, style)
    local mk = series_mk(ch, plot, w, h)
    if style.hatched then
      reparent(mk { fill_color = alpha(color, .18), stroke_color = color, stroke_width = 1 }, body)
    else
      reparent(mk { fill_color = alpha(color, .78) }, body)
    end
  end
  reparent(baseline(style, w, h), body)
  for _, s in ipairs(starts) do s(root) end
  return root
end

-- ---------------------------------------------------------- pie, donut --

local function pie_like(spec, style, inner, name)
  local root, body, w, h = frame(spec, style, name)
  local src = source(spec)
  local labels = get(spec.labels) or {}
  local initial = list_of(src)
  local n = #initial
  local show_legend = spec.legend ~= false and #labels > 0
  local d = show_legend and math.min(h, math.floor(w * 0.56)) or math.min(w, h)
  local s = d
  local px = show_legend and 0 or (w - s) / 2
  local py = (h - s) / 2
  local cx, cy, R = s / 2, s / 2, s / 2 - 1
  local r0 = R * inner
  local slices = memo(src, function(v) return L.pie(v, { pad = style.hatched and 0 or 0 }) end)
  local plot = ui.Item { x = px, y = py, width = s, height = s }
  if inner > 0 then
    reparent(ui.Path { width = s, height = s, view_box = { 0, 0, s, s },
      d = morf.geometry.sector(cx, cy, r0, R, 0, 359.99), fill_color = style.track }, plot)
  end
  for k = 1, n do
    local color = color_named(style, spec.colors and spec.colors[k], k)
    local function dk()
      local sl = slices()[k]
      if not sl or sl.sweep <= 0.01 then return "M0 0" end
      return morf.geometry.sector(cx, cy, r0, R, sl.from, math.min(sl.sweep, 359.99))
    end
    local mk = d_mk(dk, s, s)
    if style.hatched then
      reparent(shape(style, s, s, color, mk, { strong = k == 1 }), plot)
    else
      -- Material: the slices parted by the card's own tone.
      reparent(mk { fill_color = color, stroke_color = spec.bare and nil or style.raised,
        stroke_width = spec.bare and nil or 2.5 }, plot)
    end
  end
  if inner > 0 then
    -- The centre: the total, or what `center` says, over `unit`.
    local text = spec.center or function()
      local t = L.total(list_of(src))
      return t == math.floor(t) and tostring(math.floor(t)) or ("%.1f"):format(t)
    end
    local big = math.max(small(style) + 2, math.min(style.size.large, math.floor(r0 * 0.55)))
    local tw = math.floor(r0 * 1.6)
    reparent(style.text { text = text, x = cx - tw / 2, y = cy - big * 0.75 - (spec.unit and 6 or 0), width = tw,
      height = math.ceil(big * 1.3), font_size = big, horizontal_alignment = "center", color = style.ink,
      font_family = style.hatched and style.mono_font or nil }, plot)
    if spec.unit then
      reparent(cap(style, { text = spec.unit, x = cx - tw / 2, y = cy + big * 0.45, width = tw,
        horizontal_alignment = "center" }), plot)
    end
  end
  reparent(plot, body)
  if show_legend then
    local lx = s + 14
    local lw = w - lx
    local rows = math.min(n, math.floor(h / 20))
    local top = (h - rows * 20) / 2
    for k = 1, rows do
      local color = color_named(style, spec.colors and spec.colors[k], k)
      local y = top + (k - 1) * 20
      reparent(ui.Rect { x = lx, y = y + 4, width = 9, height = 9, radius = style.hatched and 0 or 4.5, color = color }, body)
      local pct_w = est(style, "100%") + 2
      reparent(cap(style, { text = labels[k] or "", x = lx + 14, y = y, width = lw - 14 - pct_w, color = style.ink }), body)
      reparent(cap(style, { text = function()
        local sl = slices()[k]
        return sl and ("%d%%"):format(math.floor(sl.fraction * 100 + .5)) or ""
      end, x = w - pct_w, y = y, width = pct_w, horizontal_alignment = "right" }), body)
    end
  end
  return root
end

--- A pie: `values`, `labels` (a legend beside it with each share),
--- `colors` (style names or colours; the series palette otherwise).
function M.pie(spec, style) return pie_like(spec, style, 0, "Pie chart") end

--- A donut: as `pie`, the total (or `center`) in the hole over `unit`.
function M.donut(spec, style) return pie_like(spec, style, spec.inner or 0.62, "Donut chart") end

--- Radial bars: `values` (0..1, or up to `top`), one ring each from the
--- outside in, swept 270 degrees; `labels` beside the rings' starts.
function M.radial_bar(spec, style)
  local root, body, w, h = frame(spec, style, "Radial bar chart")
  local src = source(spec)
  local n = math.max(1, #list_of(src))
  local s = math.min(w, h)
  local px = (w - s) / 2 + (spec.labels and math.min((w - s) / 2, s * 0.12) or 0)
  local inner, gap = spec.inner or 0.24, spec.gap or (style.hatched and 3 or 4)
  local plot = ui.Item { x = px, y = (h - s) / 2, width = s, height = s }
  local function base(t)
    t.kind, t.width, t.height, t.inner, t.gap, t.sweep, t.start = "radial", s, s, inner, gap, 270, 0
    t.bottom, t.top = 0, spec.top or 1
    return plot_of(t)
  end
  local starts = {}
  local ones, start_ones = feed(src, function(v) local t = {} for i = 1, #v do t[i] = 1 end return t end)
  starts[#starts + 1] = start_ones
  -- Material's rings are round-capped strokes through each band's middle;
  -- Tsugumori's are square sectors.
  local band = ((s / 2 - s / 2 * inner) - gap * (n - 1)) / n
  if style.hatched then
    reparent(series_mk(ones, base {}, s, s) { fill_color = "transparent", stroke_color = style.stroke_of and style.stroke_of("quiet") or style.line,
      stroke_width = 1 }, plot)
  else
    reparent(series_mk(ones, base { arcs = true }, s, s) { fill_color = "transparent", stroke_color = style.track,
      stroke_width = band, stroke_cap = "round" }, plot)
  end
  for k = 1, n do
    -- Ring k alone: the others swept nothing, so drawn not at all.
    local ch, start = feed(src, function(v)
      local t = {}
      for i = 1, #v do t[i] = i == k and (tonumber(v[i]) or 0) or 0 end
      return t
    end)
    starts[#starts + 1] = start
    if style.hatched then
      reparent(shape(style, s, s, color_named(style, spec.colors and spec.colors[k], k), series_mk(ch, base {}, s, s),
        { strong = true, gap = 4 }), plot)
    else
      reparent(series_mk(ch, base { arcs = true }, s, s) { fill_color = "transparent",
        stroke_color = color_named(style, spec.colors and spec.colors[k], k), stroke_width = band, stroke_cap = "round" }, plot)
    end
  end
  reparent(plot, body)
  local labels = get(spec.labels)
  if labels then
    local outer = s / 2
    local band = ((outer - outer * inner) - gap * (n - 1)) / n
    for k = 1, math.min(n, #labels) do
      local mid = outer - (k - 1) * (band + gap) - band / 2
      local lw = px + s / 2 - 6
      reparent(cap(style, { text = labels[k], x = 0, y = (h - s) / 2 + s / 2 - mid - 8, width = lw,
        horizontal_alignment = "right", color = style.ink_lo }), body)
    end
  end
  for _, st in ipairs(starts) do st(root) end
  return root
end

-- ------------------------------------------------------------ heatmaps --

--- The colour of tone `k` of `n`: the accent from faint to full.
local function tone(style, color, k, n)
  local a = n > 1 and (k - 1) / (n - 1) or 1
  if style.hatched then return alpha(color, 0.12 + 0.88 * a * a) end
  return alpha(color, 0.14 + 0.86 * a)
end

--- Cells drawn as `levels` tone paths over one channel; `o`: rows,
--- columns, gap, radius, top, colour, tone.
local function cell_tones(style, body, ch, w, h, o)
  local n = o.levels
  for k = 1, n do
    local plot = plot_of { kind = "cells", width = w, height = h, rows = o.rows, columns = o.columns, gap = o.gap,
      radius = o.radius, lo = (k - 1) / n, hi = k / n, bottom = o.bottom or 0, top = o.top or false, headroom = 1 }
    local mk = series_mk(ch, plot, w, h)
    local color = (o.tone or function(i, m) return tone(style, o.color, i, m) end)(k, n)
    if style.hatched and k == n then
      reparent(shape(style, w, h, o.color, mk, { strong = true, gap = 4 }), body)
    elseif style.hatched and k == 1 and not o.tone then
      reparent(mk { fill_color = "transparent", stroke_color = style.stroke_of and style.stroke_of("idle") or style.line,
        stroke_width = 1 }, body)
    else
      reparent(mk { fill_color = color }, body)
    end
  end
end

--- A heatmap: `values` a matrix (a list of rows) or a flat column-major
--- run with `rows`; `levels` (5) tones; `row_labels`, `column_labels`.
function M.heatmap(spec, style)
  local root, body, w, h = frame(spec, style, "Heatmap")
  local src = source(spec)
  local first = list_of(src)
  local matrix = type(first[1]) == "table"
  local rows = spec.rows or (matrix and #first) or 1
  local cols = spec.columns or (matrix and #(first[1] or {})) or math.max(1, math.ceil(#first / rows))
  local rl, cl = get(spec.row_labels), get(spec.column_labels)
  local lw = 0
  if rl then for _, l in ipairs(rl) do lw = math.max(lw, est(style, l)) end lw = math.ceil(lw) + 6 end
  local gw, gh = w - lw, h - (cl and 18 or 0)
  local gap = spec.gap or (style.hatched and 2 or 3)
  local ch, start = feed(src, matrix and function(v) return (L.columns(v)) end or nil)
  local grid_box = ui.Item { x = lw, width = gw, height = gh }
  cell_tones(style, grid_box, ch, gw, gh, { levels = spec.levels or 5, rows = rows, columns = cols, gap = gap,
    radius = style.hatched and 0 or 4, top = spec.top, bottom = spec.bottom, color = U.color(spec, style) })
  reparent(grid_box, body)
  local ch_ = (gh - gap * (rows - 1)) / rows
  if rl then
    for r = 1, math.min(rows, #rl) do
      reparent(cap(style, { text = rl[r], x = 0, y = (r - 1) * (ch_ + gap) + ch_ / 2 - 8, width = lw - 4 }), body)
    end
  end
  if cl then
    local cw = (gw - gap * (cols - 1)) / cols
    local need = 0
    for _, l in ipairs(cl) do need = math.max(need, est(style, l)) end
    local step = math.max(1, math.ceil((need + 4) / (cw + gap)))
    for c = 1, math.min(cols, #cl), step do
      local x = lw + (c - 1) * (cw + gap) + cw / 2 - need / 2 - 2
      x = math.max(lw, math.min(w - need - 4, x))
      reparent(cap(style, { text = cl[c], x = x, y = gh + 2, width = need + 4, horizontal_alignment = "center" }), body)
    end
  end
  start(root)
  return root
end

--- A calendar heatmap: `values` one per day, oldest first, a column per
--- week (Monday on top); `weeks` (as many as the days fill), `months`
--- (`{ {week, "Jan"}, ... }` captions over the weeks they start), `days`
--- (false hides Mon/Wed/Fri), `levels` (5), `scale` (false hides the
--- less-to-more key).
function M.calendar_heatmap(spec, style)
  local root, body, w, h = frame(spec, style, "Calendar heatmap")
  local src = source(spec)
  local weeks = spec.weeks or math.max(1, math.ceil(#list_of(src) / 7))
  local months = get(spec.months)
  local lw = spec.days == false and 0 or math.ceil(est(style, "Wed")) + 6
  local th = months and 18 or 0
  local kh = spec.scale == false and 0 or 20
  local gap = spec.gap or 2
  local cs = math.floor(math.min((w - lw - gap * (weeks - 1)) / weeks, (h - th - kh - gap * 6) / 7))
  cs = math.max(2, cs)
  local gw, gh = weeks * cs + gap * (weeks - 1), 7 * cs + gap * 6
  local x0 = lw + math.floor((w - lw - gw) / 2)
  local y0 = th + math.floor((h - th - kh - gh) / 2)
  local ch, start = feed(src)
  local levels = spec.levels or 5
  local color = U.color(spec, style)
  local grid_box = ui.Item { x = x0, y = y0, width = gw, height = gh }
  cell_tones(style, grid_box, ch, gw, gh, { levels = levels, rows = 7, columns = weeks, gap = gap,
    radius = style.hatched and 0 or math.min(3, cs / 3), top = spec.top, color = color })
  reparent(grid_box, body)
  if spec.days ~= false then
    for _, d in ipairs { { 1, "Mon" }, { 3, "Wed" }, { 5, "Fri" } } do
      reparent(cap(style, { text = d[2], x = x0 - lw, y = y0 + d[1] * (cs + gap) + cs / 2 - 8, width = lw - 4 }), body)
    end
  end
  if months then
    for _, m in ipairs(months) do
      local x = x0 + (m[1] - 1) * (cs + gap)
      local mw = math.ceil(est(style, m[2])) + 2
      if x + mw <= w then reparent(cap(style, { text = m[2], x = x, y = y0 - th, width = mw }), body) end
    end
  end
  if kh > 0 then
    -- Less, a swatch per tone, more -- at the right under the grid.
    local sw = math.min(cs, 10)
    local lessw, morew = math.ceil(est(style, "Less")) + 2, math.ceil(est(style, "More")) + 2
    local kw = lessw + 4 + levels * (sw + 2) + 2 + morew
    local kx, ky = math.max(0, x0 + gw - kw), y0 + gh + 6
    reparent(cap(style, { text = "Less", x = kx, y = ky - 2, width = lessw }), body)
    for k = 1, levels do
      local c = tone(style, color, k, levels)
      reparent(ui.Rect { x = kx + lessw + 4 + (k - 1) * (sw + 2), y = ky + (14 - sw) / 2, width = sw, height = sw,
        radius = style.hatched and 0 or 2, color = c,
        border_width = style.hatched and k == 1 and 1 or 0, border_color = style.line }, body)
    end
    reparent(cap(style, { text = "More", x = kx + lessw + 6 + levels * (sw + 2), y = ky - 2, width = morew }), body)
  end
  start(root)
  return root
end

-- ------------------------------------------------------- audio, signal --

--- A waveform: `values` amplitudes (or a ring `channel` of them), drawn up
--- and down from the middle, the newest at the right; `samples` across
--- the width, `top` the full-scale amplitude (the peak when not given).
function M.waveform(spec, style)
  local root, body, w, h = frame(spec, style, "Waveform")
  local ch, start = feed(source(spec), nil, { size = spec.size_samples or 8192 })
  local color = U.color(spec, style)
  local plot = plot_of { kind = "wave", width = w, height = h, samples = spec.samples or 0, bottom = 0,
    top = spec.top or false, headroom = 1.05, pad_top = style.hatched and 2 or 4 }
  local mk = series_mk(ch, plot, w, h)
  reparent(ui.Rect { y = math.floor(h / 2), width = w, height = 1, color = style.hatched and style.line or alpha(style.line, .6) }, body)
  if style.hatched then
    reparent(grid(style, w, h, 4, 8), body)
    reparent(shape(style, w, h, color, mk, { gap = 6 }), body)
  else
    -- Material: the envelope a soft sweep across two tones.
    local second = style.series(2)
    reparent(ui.Rect { width = w, height = h, gradient = function()
      return { angle = 90, stops = { { get(second):alpha(.85), 0 }, { get(color), 1 } } }
    end, mask = mk { fill_color = "#ffffff" } }, body)
  end
  start(root)
  return root
end

--- The tones of a spectrogram, quiet to loud, through the style's signals.
local function heat(style, k, n)
  local stops = { style.info, style.accent, style.warn, style.alert }
  return function()
    local t = n > 1 and (k - 1) / (n - 1) or 1
    local seg = t * (#stops - 1)
    local i = math.min(#stops - 2, math.floor(seg))
    local f = seg - i
    local a, b = get(stops[i + 1]), get(stops[i + 2])
    local c = a:mix(b, f)
    return c:alpha(0.18 + 0.82 * math.min(1, t * 1.6))
  end
end

--- A spectrogram: a column of `rows` bins (0..1, or up to `top`) per moment,
--- the newest at the right, `columns` (48) across. `channel` a ring of
--- `rows * columns` pushed a column at a time; or `values` a function
--- returning the newest column (pushed when it changes) or a list of
--- columns; `levels` (6) tones.
function M.spectrogram(spec, style)
  local root, body, w, h = frame(spec, style, "Spectrogram")
  local src = spec.channel or spec.values or spec.data
  local columns = spec.columns or 48
  local rows = spec.rows
  local ch, start
  if channel.is(src) then
    ch, start = src, function() end
    rows = rows or 32
  else
    local probe = type(src) == "function" and src() or src or {}
    local matrix = type(probe[1]) == "table"
    rows = rows or (matrix and #probe[1]) or #probe
    rows = math.max(1, rows)
    if type(src) == "function" and not matrix then
      ch = morf.channel { size = rows * columns, mode = "ring" }
      start = function(owner)
        morf.effect("kit.spectrogram." .. ch.id, function()
          local col = src()
          if type(col) == "table" and #col > 0 then ch:push(col) end
        end, { owner = owner })
      end
    else
      -- Columns of rows, the lowest bin at the foot: flipped to top-down.
      ch, start = feed(src, function(m)
        local out = {}
        for _, col in ipairs(m) do for r = rows, 1, -1 do out[#out + 1] = tonumber(col[r]) or 0 end end
        return out
      end)
    end
  end
  local levels = spec.levels or 6
  local box = ui.Item { width = w, height = h, clip = true }
  reparent(ui.Rect { anchors = { fill = true }, color = style.hatched and alpha(style.info, .04) or style.track,
    radius = style.hatched and 0 or 6 }, box)
  cell_tones(style, box, ch, w, h, { levels = levels, rows = rows, columns = columns, gap = style.hatched and 1 or 0.5,
    radius = 0, top = spec.top or 1, color = style.accent, tone = function(k, n) return heat(style, k, n) end })
  if style.hatched then reparent(ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 1,
    border_color = style.line }, box) end
  reparent(box, body)
  start(root)
  return root
end

-- -------------------------------------------------------------- finance --

--- Candles: `values` `{ {open, high, low, close}, ... }` (or flat fours);
--- rising in the style's ok tone, falling in its alert; `labels`, `bottom`
--- and `top` (the data's range when not given).
function M.candlestick(spec, style)
  local root, body, w, h = frame(spec, style, "Candlestick chart")
  local labels = get(spec.labels)
  local ph = labels and h - 18 or h
  local gap = spec.gap or (style.hatched and 3 or 4)
  local ch, start = feed(source(spec), flatten)
  reparent(grid(style, w, ph, 4, 6), body)
  for _, dir in ipairs { "up", "down" } do
    local plot = plot_of { kind = "candles", width = w, height = ph, gap = gap, radius = style.hatched and 0 or 2.5,
      direction = dir, bottom = spec.bottom or 0, top = spec.top or false }
    local color = dir == "up" and style.ok or style.alert
    reparent(series_mk(ch, plot, w, ph) { fill_color = color }, body)
  end
  reparent(baseline(style, w, ph), body)
  if labels then reparent(xlabels(style, labels, w, ph + 2, gap), body) end
  start(root)
  return root
end

--- Box plots: `values` `{ {min, q1, median, q3, max}, ... }` (or flat
--- fives); `labels`, `bottom` (0), `top` (the peak with headroom).
function M.box_plot(spec, style)
  local root, body, w, h = frame(spec, style, "Box plot")
  local labels = get(spec.labels)
  local ph = labels and h - 18 or h
  local gap = spec.gap or (style.hatched and 10 or 14)
  local ch, start = feed(source(spec), flatten)
  local plot = plot_of { kind = "boxes", width = w, height = ph - 2, gap = gap, radius = style.hatched and 0 or 5,
    bottom = spec.bottom or 0, top = spec.top or false, headroom = 1.08 }
  local area = ui.Item { width = w, height = ph }
  reparent(grid(style, w, ph, 4, 0), area)
  reparent(reparent(shape(style, w, ph - 2, U.color(spec, style), series_mk(ch, plot, w, ph - 2), { gap = 4 }),
    ui.Item { y = 1, width = w, height = ph - 2 }), area)
  reparent(baseline(style, w, ph), area)
  reparent(area, body)
  if labels then reparent(xlabels(style, labels, w, ph + 2, gap), body) end
  start(root)
  return root
end

-- -------------------------------------------------------------- states --

--- The rows a state chart draws: `series` (`{ {label, values}, ... }` or
--- lists) or `values` (one row); their labels.
local function state_rows(spec)
  local src = spec.series or spec.data
  if src == nil then return { spec.values or spec.channel }, { spec.label } end
  local rows, names = {}, {}
  for i, r in ipairs(list_of(src)) do
    if type(r) == "table" and (r.values or r.channel) then rows[i], names[i] = r.values or r.channel, r.label
    else rows[i], names[i] = r, nil end
  end
  return rows, names
end

--- A state timeline: a lane per row, each state's runs a bar in its own
--- colour (one path per state over the lane's channel). `series` (`{
--- {label = "api", values = {0, 0, 1, ...}}, ... }`) or `values` (one
--- lane) of state ids; `states` (`{ {label, color}, ... }`, the id is the
--- index less one; ok, warn, alert by default); `samples`.
function M.state_timeline(spec, style)
  local states = states_of(spec)
  local names = {}
  for i, s in ipairs(states) do names[i] = s.label end
  local root, body, w, h = frame(spec, style, "State timeline", legend_entries(spec, names, function(i)
    return color_named(style, states[i].color, i)
  end))
  local rows, labels = state_rows(spec)
  local lw = 0
  for _, l in pairs(labels) do lw = math.max(lw, est(style, l)) end
  lw = lw > 0 and math.ceil(lw) + 8 or 0
  local n = #rows
  local lane_gap = style.hatched and 4 or 6
  local lh = math.min(28, (h - lane_gap * (n - 1)) / n)
  local y0 = math.floor((h - (lh * n + lane_gap * (n - 1))) / 2)
  local starts = {}
  for r, src in ipairs(rows) do
    local ch, start = feed(src)
    starts[#starts + 1] = start
    local y = y0 + (r - 1) * (lh + lane_gap)
    local lane = ui.Item { x = lw, y = y, width = w - lw, height = lh }
    reparent(ui.Rect { anchors = { fill = true }, color = style.hatched and alpha(style.accent, .03) or style.track,
      radius = style.hatched and 0 or math.min(6, lh / 2), border_width = style.hatched and 1 or 0,
      border_color = style.line }, lane)
    for i, s in ipairs(states) do
      local plot = plot_of { kind = "states", width = w - lw, height = lh, state = s.value, samples = spec.samples or 0,
        gap = style.hatched and 0 or 1.5, radius = style.hatched and 0 or math.min(6, lh / 2) }
      reparent(shape(style, w - lw, lh, color_named(style, s.color, i), series_mk(ch, plot, w - lw, lh),
        { strong = true, gap = 4, hatch = i > 1 }), lane)
    end
    reparent(lane, body)
    if labels[r] then
      reparent(cap(style, { text = labels[r], x = 0, y = y + lh / 2 - 8, width = lw - 6 }), body)
    end
  end
  for _, s in ipairs(starts) do s(root) end
  return root
end

--- Status history: a square per sample per row, coloured by its state
--- (one path per state), the newest column at the right. `series` (rows
--- as in `state_timeline`) or `values` (one row); `states`; `columns`.
function M.status_history(spec, style)
  local states = states_of(spec)
  local names = {}
  for i, s in ipairs(states) do names[i] = s.label end
  local root, body, w, h = frame(spec, style, "Status history", legend_entries(spec, names, function(i)
    return color_named(style, states[i].color, i)
  end))
  local rows, labels = state_rows(spec)
  local n = #rows
  local lw = 0
  for _, l in pairs(labels) do lw = math.max(lw, est(style, l)) end
  lw = lw > 0 and math.ceil(lw) + 8 or 0
  local cols = spec.columns or 0
  if cols == 0 then for _, r in ipairs(rows) do cols = math.max(cols, #list_of(r)) end end
  cols = math.max(1, cols)
  local gap = spec.gap or (style.hatched and 2 or 3)
  -- Cells as wide as the columns allow, as tall as the rows do (to 26).
  local cw = math.max(2, (w - lw - gap * (cols - 1)) / cols)
  local cs = math.max(2, math.min(26, math.floor((h - gap * (n - 1)) / n)))
  local gw, gh = w - lw, n * cs + gap * (n - 1)
  local x0, y0 = w - gw, math.floor((h - gh) / 2)
  -- The rows interleaved sample by sample: the column-major run cells read.
  local ch, start = channel.from(function()
    local lists = {}
    for i, r in ipairs(rows) do lists[i] = list_of(r) end
    return L.interleave(lists)
  end)
  local box = ui.Item { x = x0, y = y0, width = gw, height = gh }
  for i, s in ipairs(states) do
    local plot = plot_of { kind = "state_cells", width = gw, height = gh, rows = n, columns = cols, gap = gap,
      radius = style.hatched and 0 or math.min(4, cw / 3, cs / 3), state = s.value }
    reparent(shape(style, gw, gh, color_named(style, s.color, i), series_mk(ch, plot, gw, gh),
      { strong = true, gap = 4, hatch = i > 1 }), box)
  end
  reparent(box, body)
  for r = 1, n do
    if labels[r] then
      reparent(cap(style, { text = labels[r], x = math.max(0, x0 - lw), y = y0 + (r - 1) * (cs + gap) + cs / 2 - 8,
        width = lw - 6 }), body)
    end
  end
  start(root)
  return root
end

-- ------------------------------------------------- structured, layouts --

--- Path data for the rectangles `rects` (`{ x, y, w, h }`), corners `r`.
local function rects_d(rects, r, inset)
  local d = {}
  inset = inset or 0
  for _, t in ipairs(rects) do
    d[#d + 1] = L.rect_d(t.x + inset, t.y + inset, t.w - 2 * inset, t.h - 2 * inset, r)
  end
  local s = table.concat(d)
  return s ~= "" and s or "M0 0"
end

--- A caption inside a tile that shows only while the tile can hold it.
local function tile_label(style, at, fill_ink)
  return cap(style, {
    text = function() local t = at() return t and t.name or "" end,
    x = function() local t = at() return t and t.x + 5 or 0 end,
    y = function() local t = at() return t and t.y + 3 or 0 end,
    width = function() local t = at() return t and math.max(1, t.w - 10) or 1 end,
    visible = function()
      local t = at()
      return t ~= nil and t.name ~= nil and t.w >= 34 and t.h >= 18
    end,
    color = fill_ink,
  })
end

--- A gantt chart: `values` tasks `{ label, start, finish, progress (0..1),
--- color }` (numbers on one axis -- days, hours), one row each; `from`,
--- `to` (the tasks' span when not given), `now` (a marker), `ticks`
--- (`{ {at, "label"}, ... }` along the foot).
function M.gantt(spec, style)
  local root, body, w, h = frame(spec, style, "Gantt chart")
  local src = source(spec)
  local tasks0 = list_of(src)
  local n = #tasks0
  local lw = 0
  for _, t in ipairs(tasks0) do lw = math.max(lw, est(style, t.label or t.name or "")) end
  lw = math.min(math.floor(w * 0.36), math.ceil(lw) + 10)
  local ticks = get(spec.ticks)
  local pw, ph = w - lw, h - (ticks and 18 or 0)
  local layout = memo(src, function(tasks)
    return L.gantt(tasks, pw, ph, { gap = style.hatched and 8 or 9, from = get(spec.from), to = get(spec.to) })
  end)
  local plot = ui.Item { x = lw, width = pw, height = ph }
  reparent(grid(style, pw, ph, 1, 6), plot)
  if style.hatched then reparent(ui.Rect { width = 1, height = ph, color = style.line }, plot) end
  for k = 1, n do
    local color = color_named(style, tasks0[k].color, k)
    local function bar() return (layout())[k] end
    local function d()
      local b = bar()
      if not b then return "M0 0" end
      return L.rect_d(b.x, b.y, math.max(2, b.w), b.h, style.hatched and 0 or b.h / 2)
    end
    local function done()
      local b = bar()
      if not b or not b.progress or b.progress <= 0 then return "M0 0" end
      return L.rect_d(b.x, b.y, math.max(2, b.w * math.min(1, b.progress)), b.h, style.hatched and 0 or b.h / 2)
    end
    if style.hatched then
      reparent(shape(style, pw, ph, color, d_mk(d, pw, ph), { gap = 4 }), plot)
      reparent(d_mk(done, pw, ph) { fill_color = color }, plot)
    else
      reparent(d_mk(d, pw, ph) { fill_color = alpha(color, .32) }, plot)
      reparent(d_mk(done, pw, ph) { fill_color = color }, plot)
    end
    reparent(cap(style, { text = function() local b = bar() return b and b.label or "" end, x = 0,
      y = function() local b = bar() return b and b.y + b.h / 2 - 8 or 0 end, width = lw - 8, color = style.ink }), body)
  end
  if spec.now ~= nil then
    reparent(ui.Rect { y = 0, width = style.hatched and 1 or 2, height = ph, color = style.alert,
      x = function()
        local _, lo, hi = layout()
        return math.max(0, math.min(pw - 2, L.scale(get(spec.now), lo, hi, pw)))
      end }, plot)
  end
  reparent(plot, body)
  if ticks then
    for _, t in ipairs(ticks) do
      local tw = math.ceil(est(style, t[2])) + 4
      reparent(cap(style, { text = t[2], y = ph + 2, width = tw, horizontal_alignment = "center",
        x = function()
          local _, lo, hi = layout()
          return math.max(lw, math.min(w - tw, lw + L.scale(t[1], lo, hi, pw) - tw / 2))
        end }), body)
    end
  end
  return root
end

--- A treemap: `values` (or `data`) a hierarchy `{ children = { {name,
--- value | children}, ... } }` two levels deep -- tiles in proportion,
--- coloured by their group, named where they fit; `legend` the groups.
function M.treemap(spec, style)
  local src = source(spec)
  local tree0 = list_of(src)
  local groups0 = tree0.children or tree0
  local names = {}
  for i, g in ipairs(groups0) do names[i] = g.name end
  local root, body, w, h = frame(spec, style, "Treemap", spec.legend ~= nil and legend_entries(spec, names,
    function(i) return style.series(i) end) or nil)
  local gap = style.hatched and 2 or 3
  local layout = memo(src, function(tree) return L.treemap(tree, w, h, { gap = gap }) end)
  local leaves0 = layout()
  for g = 1, #groups0 do
    local color = style.series(g)
    local function d()
      local mine = {}
      for _, t in ipairs((layout())) do if t.group == g then mine[#mine + 1] = t end end
      return rects_d(mine, style.hatched and 0 or 8)
    end
    reparent(shape(style, w, h, color, d_mk(d, w, h), { alpha = .9, gap = 5 }), body)
  end
  for k = 1, #leaves0 do
    reparent(tile_label(style, function() return (layout())[k] end, style.hatched and style.ink or style.on_accent), body)
  end
  return root
end

--- A sunburst: `values` (or `data`) a hierarchy (as `treemap`, any depth),
--- rings from the centre out, each child its share of its parent's sweep,
--- coloured by its top-level group and paler further out; `labels` false
--- hides the legend of groups beside it.
function M.sunburst(spec, style)
  local root, body, w, h = frame(spec, style, "Sunburst chart")
  local src = source(spec)
  local tree0 = list_of(src)
  local groups0 = tree0.children or {}
  local show_legend = spec.legend ~= false and #groups0 > 0
  local s = show_legend and math.min(h, math.floor(w * 0.58)) or math.min(w, h)
  local px = show_legend and 0 or (w - s) / 2
  local R = s / 2 - 1
  local inner = spec.inner or 0.3
  local layout = memo(src, function(tree) return L.sunburst(tree, { inner = R * inner, outer = R }) end)
  local plot = ui.Item { x = px, y = (h - s) / 2, width = s, height = s }
  -- One path per group and ring.
  local keys, seen = {}, {}
  for _, a in ipairs(layout()) do
    local key = a.group * 100 + a.depth
    if not seen[key] then seen[key] = true keys[#keys + 1] = { group = a.group, depth = a.depth } end
  end
  table.sort(keys, function(a, b) return a.depth < b.depth or (a.depth == b.depth and a.group < b.group) end)
  for _, key in ipairs(keys) do
    local function d()
      local out = {}
      for _, a in ipairs(layout()) do
        if a.group == key.group and a.depth == key.depth and a.sweep > 0.05 then
          out[#out + 1] = morf.geometry.sector(s / 2, s / 2, a.r0, a.r1, a.from, math.min(a.sweep, 359.99))
        end
      end
      return #out > 0 and table.concat(out, " ") or "M0 0"
    end
    local color = style.series(key.group)
    local mk = d_mk(d, s, s)
    if style.hatched then
      reparent(shape(style, s, s, color, mk, { strong = key.depth == 1, hatch = key.depth == 1, gap = 4 }), plot)
    else
      local a = ({ 1, .62, .38, .24 })[math.min(4, key.depth)]
      reparent(mk { fill_color = alpha(color, a), stroke_color = spec.bare and nil or style.raised,
        stroke_width = spec.bare and nil or 2 }, plot)
    end
  end
  reparent(plot, body)
  if show_legend then
    local lx = s + 14
    local rows = math.min(#groups0, math.floor(h / 20))
    local top = (h - rows * 20) / 2
    for k = 1, rows do
      local y = top + (k - 1) * 20
      reparent(ui.Rect { x = lx, y = y + 4, width = 9, height = 9, radius = style.hatched and 0 or 4.5,
        color = style.series(k) }, body)
      reparent(cap(style, { text = groups0[k].name or "", x = lx + 14, y = y, width = w - lx - 14, color = style.ink }), body)
    end
  end
  return root
end

--- A sankey diagram: `values` (or `data`) `{ nodes = { "a", ... } or
--- { {name}, ... }, links = { {source, target, value}, ... } }` (indices or
--- names); ribbons in their source's colour, nodes named beside them.
function M.sankey(spec, style)
  local root, body, w, h = frame(spec, style, "Sankey diagram")
  local src = source(spec)
  local data0 = list_of(src)
  local nodes0, links0 = data0.nodes or {}, data0.links or {}
  local nw = style.hatched and 6 or 8
  local layout = memo(src, function(data)
    return L.sankey(data.nodes or {}, data.links or {}, w, h, { node_width = nw, gap = 8 })
  end)
  local _, lks0 = layout()
  for k = 1, #lks0 do
    local function link() local _, lks = layout() return lks[k] end
    local function d() local l = link() return l and L.ribbon_d(l) or "M0 0" end
    local function color() local l = link() return get(style.series(l and l.source or 1)) end
    if style.hatched then
      reparent(d_mk(d, w, h) { fill_color = alpha(color, .12), stroke_color = alpha(color, .45), stroke_width = 1 }, body)
    else
      reparent(d_mk(d, w, h) { fill_color = alpha(color, .34) }, body)
    end
  end
  for i = 1, #nodes0 do
    local function node() return (layout())[i] end
    local function d() local n = node() return n and L.rect_d(n.x, n.y, n.w, math.max(1, n.h), style.hatched and 0 or 3) or "M0 0" end
    reparent(d_mk(d, w, h) { fill_color = style.series(i) }, body)
    local lw = math.ceil(w * 0.3)
    reparent(cap(style, {
      text = function() local n = node() return n and n.name or "" end,
      x = function() local n = node() if not n then return 0 end return n.last and n.x - lw - 4 or n.x + n.w + 4 end,
      y = function() local n = node() return n and math.max(0, math.min(h - 16, n.y + n.h / 2 - 8)) or 0 end,
      width = lw, color = style.ink,
      horizontal_alignment = function() local n = node() return n and n.last and "right" or "left" end,
    }), body)
  end
  return root
end

--- A funnel: `values` the stages' counts, largest first; `labels` beside
--- them with each value (and its share of the first).
function M.funnel(spec, style)
  local root, body, w, h = frame(spec, style, "Funnel chart")
  local src = source(spec)
  local labels = get(spec.labels) or {}
  local n = #list_of(src)
  local lw = 0
  for _, l in ipairs(labels) do lw = math.max(lw, est(style, l)) end
  lw = math.min(math.floor(w * 0.42), math.ceil(math.max(lw, est(style, "00000 · 00%"))) + 8)
  local fw = w - lw
  local gap = style.hatched and 3 or 4
  local layout = memo(src, function(v) return L.funnel(v, fw, h, { gap = gap, min = 0.14 }) end)
  local color = U.color(spec, style)
  local plot = ui.Item { x = lw, width = fw, height = h }
  for k = 1, n do
    local function stage() return (layout())[k] end
    local function d()
      local st = stage()
      if not st then return "M0 0" end
      return L.trapezoid_d(fw, st.y, st.h, st.top, st.bottom, style.hatched and 0 or 6)
    end
    local c = style.hatched and color or alpha(color, 1 - 0.6 * (k - 1) / math.max(1, n - 1))
    reparent(shape(style, fw, h, style.hatched and color or c, d_mk(d, fw, h), { strong = k == 1, gap = 4 + k % 2 }), plot)
    local function cy() local st = stage() return st and st.y + st.h / 2 or 0 end
    reparent(cap(style, { text = labels[k] or "", x = 0, y = function() return cy() - 15 end, width = lw - 8,
      color = style.ink }), body)
    reparent(cap(style, { text = function()
      local st = stage()
      if not st then return "" end
      local v = st.value == math.floor(st.value) and tostring(math.floor(st.value)) or ("%.1f"):format(st.value)
      return k == 1 and v or ("%s · %d%%"):format(v, math.floor(st.fraction * 100 + .5))
    end, x = 0, y = function() return cy() end, width = lw - 8 }), body)
  end
  reparent(plot, body)
  return root
end

--- A flame graph: `values` (or `data`) a call tree `{ name, value,
--- children }`, the root along the foot and each callee above its caller
--- as wide as its share (`icicle` puts the root on top); frames tinted
--- warm by name, named where they fit.
function M.flame_graph(spec, style)
  local root, body, w, h = frame(spec, style, "Flame graph")
  local src = source(spec)
  local row = spec.row or 20
  local layout = memo(src, function(tree) return L.flame(tree, w, h, { row = row, gap = 2, icicle = spec.icicle }) end)
  local palette = { style.warn, style.alert, style.accent, style.extra }
  local function hue(name, depth)
    local s = depth
    for c in tostring(name or ""):gmatch(".") do s = s + c:byte() end
    return s % #palette + 1
  end
  for p = 1, #palette do
    local function d()
      local mine = {}
      for _, f in ipairs(layout()) do
        if hue(f.name, f.depth) == p then mine[#mine + 1] = { x = f.x, y = f.y, w = math.max(0.5, f.w - 1.5), h = f.h } end
      end
      return rects_d(mine, style.hatched and 0 or 4)
    end
    reparent(shape(style, w, h, palette[p], d_mk(d, w, h), { alpha = .85, gap = 4, hatch = false }), body)
  end
  local frames0 = layout()
  for k = 1, #frames0 do
    reparent(tile_label(style, function()
      local f = (layout())[k]
      return f and { x = f.x - 1, y = f.y + (row - 16) / 2 - 3, w = f.w, h = row, name = f.name } or nil
    end, style.hatched and style.ink or style.on_accent), body)
  end
  return root
end

return M
