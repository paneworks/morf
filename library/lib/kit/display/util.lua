-- What the display widgets share: reading a spec's values, placing a
-- widget, and the few drawing moves every style makes its own way.
local ui = require("morf.ui")

local U = {}

--- A spec value: the value, or what its function returns.
function U.get(v) if type(v) == "function" then return v() end return v end

function U.clamp01(v) v = tonumber(v) or 0 return v < 0 and 0 or (v > 1 and 1 or v) end

--- The placement fields of `spec` (`id`, `x`, `y`, `anchors`, `visible`,
--- `opacity`, `z`) onto `props`, which is returned.
function U.place(spec, props)
  for _, key in ipairs { "id", "x", "y", "anchors", "visible", "opacity", "z" } do
    if spec[key] ~= nil and props[key] == nil then props[key] = spec[key] end
  end
  return props
end

--- A colour from the spec (`color`, a colour or a function), else the style's `fallback` ("accent").
function U.color(spec, style, fallback)
  local c = spec.color
  if c ~= nil then
    if type(c) == "string" and style[c] then return style[c] end
    return c
  end
  return style[fallback or "accent"]
end

--- A colour function with alpha `a`.
function U.alpha(color, a)
  return function() return U.get(color):alpha(a) end
end

--- A filled box in the style: Material a tonal rounded rect; Tsugumori a
--- faint wash with `/` stripes across it and a hairline edge. `props`:
--- `x`, `y`, `width`, `height` (numbers or functions), `color`, `radius`,
--- `strong` (a heavier fill), and any node props.
function U.fill(style, props)
  local color = props.color or style.accent
  local strong = props.strong
  props.strong = nil
  if not style.hatched then
    props.color = function() return U.get(color) end
    props.radius = props.radius or style.radius(U.get(props.height) or 0)
    return ui.Rect(props)
  end
  local w, h = props.width, props.height
  local box = ui.Item(U.place(props, { width = w, height = h, clip = true }))
  ui.reparent(ui.Rect { anchors = { fill = true }, color = U.alpha(color, strong and 0.22 or 0.12),
    border_width = 1, border_color = U.alpha(color, strong and 0.9 or 0.6) }, box)
  if type(w) == "number" and type(h) == "number" and w >= 6 and h >= 6 then
    ui.reparent(style.stripes.box { width = w, height = h, gap = 5, weight = 1, color = U.alpha(color, strong and 0.7 or 0.45) }, box)
  end
  return box
end

--- A container in the style, `width` by `height`, with its children:
--- Material a raised tonal card with the style's corners; Tsugumori a
--- hairline frame with registration marks.
function U.box(style, spec, children)
  local props = U.place(spec, { width = spec.width, height = spec.height })
  local node = ui.Item(props)
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1, border_color = style.line }, node)
    local m = style.marks()
    if m then ui.reparent(m, node) end
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.raised, radius = style.radius(math.min(U.get(spec.width) or 0, U.get(spec.height) or 0) / 3) }, node)
  end
  for _, child in ipairs(children or {}) do ui.reparent(child, node) end
  return node
end

--- A caption in the style: Material sentence case, Tsugumori upper case
--- mono -- never smaller than the style's small size less two.
function U.caption(style, props)
  local text = props.text
  if style.hatched and type(text) == "string" then props.text = text:upper()
  elseif style.hatched and type(text) == "function" then props.text = function() return tostring(text() or ""):upper() end end
  props.font_size = math.max(props.font_size or (style.size.small - 2), style.size.small - 3)
  return style.label(props)
end

return U
