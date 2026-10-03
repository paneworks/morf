-- Display widgets: structure. See lib.kit.display.
--
-- What holds and divides the others: separators and spacers, a group box
-- with a title, a divider with words in it, a frame and an inset. They
-- take children in their array part (`spec[1..n]`), placed by their own
-- `x`, `y` and anchors inside the content area.
local ui = require("morf.ui")
local U = require("lib.kit.display.util")

local M = {}
local get = U.get

local function children(spec, node)
  for _, child in ipairs(spec) do ui.reparent(child, node) end
  return node
end

-- The width Tsugumori's caps caption takes: its face is mono.
local function caps_width(text, size, spacing)
  local n = utf8.len(tostring(text or "")) or 0
  return math.ceil(n * (size * 0.6 + (spacing or 0)))
end

--- A dividing line: `width` (or `length`), `vertical` (then `height` or
--- `length`), `color`. Material a hairline in the outline tone; Tsugumori
--- a hairline with ticks at its ends.
function M.separator(spec, style)
  local vertical = spec.vertical
  local len = spec.length or (vertical and spec.height or spec.width) or 200
  local color = spec.color and U.color(spec, style) or style.line
  local thick = style.hatched and 7 or 1
  local node = ui.Item(U.place(spec, { width = vertical and thick or len, height = vertical and len or thick }))
  local mid = math.floor(thick / 2)
  if vertical then
    ui.reparent(ui.Rect { x = mid, width = 1, height = len, color = color }, node)
  else
    ui.reparent(ui.Rect { y = mid, width = len, height = 1, color = color }, node)
  end
  if style.hatched then
    for _, at in ipairs { 0, len - 1 } do
      ui.reparent(ui.Rect { x = vertical and 0 or at, y = vertical and at or 0,
        width = vertical and thick or 1, height = vertical and 1 or thick, color = style.ink_lo }, node)
    end
  end
  node.accessible_role = "separator"
  node.accessible = { orientation = vertical and "vertical" or "horizontal" }
  return node
end

--- Empty room: `width`, `height`. Draws nothing.
function M.spacer(spec, _)
  return ui.Item(U.place(spec, { width = spec.width or 1, height = spec.height or 1, accessible_hidden = true }))
end

--- A titled box of related things: `title`, `width` (260), `height`
--- (150), `padding` (14), children. Material: the title above a tonal
--- container; Tsugumori: a hairline frame, the caps title set in a break
--- of its top edge, registration marks.
function M.group_box(spec, style)
  local w, h = spec.width or 260, spec.height or 150
  local pad = spec.padding or (style.hatched and 12 or 14)
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  local top
  if style.hatched then
    local size = style.size.small - 3
    local tw = spec.title and caps_width(spec.title, size, 1) or 0
    local ty = 7
    -- The frame, its top edge broken for the title.
    ui.reparent(ui.Rect { y = ty, width = 1, height = h - ty, color = style.line }, node)
    ui.reparent(ui.Rect { x = w - 1, y = ty, width = 1, height = h - ty, color = style.line }, node)
    ui.reparent(ui.Rect { y = h - 1, width = w, height = 1, color = style.line }, node)
    ui.reparent(ui.Rect { y = ty, width = 8, height = 1, color = style.line }, node)
    ui.reparent(ui.Rect { x = math.min(w, 14 + tw + 6), y = ty, width = math.max(1, w - (14 + tw + 6)), height = 1,
      color = style.line }, node)
    ui.reparent(ui.Rect { y = ty - 1, width = 4, height = 3, color = style.accent }, node)
    if spec.title then
      ui.reparent(U.caption(style, { text = spec.title, x = 14, y = 0, height = 14, font_size = size,
        letter_spacing = 1, color = style.ink_lo, width = tw + 4, elide = "right" }), node)
    end
    local m = style.marks()
    if m then ui.reparent(ui.Item { y = ty, width = w, height = h - ty, m }, node) end
    top = ty + pad
  else
    local th = spec.title and 22 or 0
    if spec.title then
      ui.reparent(style.text { text = spec.title, x = 4, y = 0, height = 18, font_size = style.size.small - 1,
        font_weight = 600, color = style.accent, width = w - 8, elide = "right" }, node)
    end
    ui.reparent(ui.Rect { y = th, width = w, height = h - th, radius = style.radius(h - th) + 4,
      color = style.raised }, node)
    top = th + pad
  end
  local content = ui.Item { x = pad, y = top, width = w - pad * 2, height = math.max(1, h - top - pad) }
  children(spec, content)
  ui.reparent(content, node)
  node.accessible_role = "group"
  if spec.title ~= nil then node.accessible_name = spec.title end
  return node
end

--- A line with words in it: `text`, `width` (260), `align` ("center";
--- Tsugumori sets its caps at the start).
function M.labelled_divider(spec, style)
  local w = spec.width or 260
  local h = 20
  local node
  if style.hatched then
    local size = style.size.small - 3
    local tw = caps_width(get(spec.text), size, 1)
    node = ui.Item(U.place(spec, { width = w, height = h,
      ui.Rect { y = h / 2 - 1, width = 10, height = 2, color = style.accent },
      U.caption(style, { text = spec.text, x = 16, y = 3, height = 14, font_size = size, letter_spacing = 1,
        color = style.ink_lo, width = tw + 4, elide = "right" }),
      ui.Rect { x = 24 + tw, y = h / 2, width = math.max(1, w - 24 - tw), height = 1, color = style.line },
      ui.Rect { x = w - 1, y = h / 2 - 3, width = 1, height = 7, color = style.ink_lo },
    }))
  else
    local line = function() return ui.Rect { height = 1, color = style.line, layout = { grow = 1, minimum_width = 0 } } end
    local label = style.text { text = spec.text, font_size = style.size.small - 2, font_weight = 500,
      color = style.ink_lo, elide = "right", layout = { shrink = 1, minimum_width = 0 } }
    local row = { direction = "row", align = "center", gap = 12, width = w, height = h }
    if spec.align ~= "start" then row[#row + 1] = line() end
    row[#row + 1] = label
    row[#row + 1] = line()
    node = ui.Flex(U.place(spec, row))
  end
  node.accessible_role = "separator"
  if spec.text ~= nil then node.accessible_name = spec.text end
  return node
end

--- A frame round some content: `width` (260), `height` (170), `title`
--- (optional), `padding`, children. Material: an outline with the style's
--- corners; Tsugumori: a hairline frame with a header strip and marks.
function M.frame(spec, style)
  local w, h = spec.width or 260, spec.height or 170
  local pad = spec.padding or 12
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  local top = pad
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 1,
      border_color = style.line }, node)
    local m = style.marks()
    if m then ui.reparent(m, node) end
    if spec.title then
      ui.reparent(ui.Rect { x = 10, y = 11, width = 5, height = 5, color = style.accent }, node)
      ui.reparent(U.caption(style, { text = spec.title, x = 21, y = 6, height = 16, font_size = style.size.small - 2,
        letter_spacing = 0.8, color = style.ink, width = w - 32, elide = "right" }), node)
      ui.reparent(ui.Rect { x = 10, y = 26, width = w - 20, height = 1, color = U.alpha(style.line, 0.7) }, node)
      ui.reparent(ui.Rect { x = 10, y = 25, width = 24, height = 2, color = style.accent }, node)
      top = 26 + pad
    end
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, color = "transparent", radius = 16, border_width = 1,
      border_color = style.line }, node)
    if spec.title then
      ui.reparent(style.text { text = spec.title, x = 16, y = 12, height = 20, font_size = style.size.normal,
        font_weight = 500, color = style.ink, width = w - 32, elide = "right" }, node)
      top = 12 + 20 + pad - 4
    end
  end
  local content = ui.Item { x = pad, y = top, width = w - pad * 2, height = math.max(1, h - top - pad) }
  children(spec, content)
  ui.reparent(content, node)
  node.accessible_role = "group"
  if spec.title ~= nil then node.accessible_name = spec.title end
  return node
end

--- Margins round its content: `margins` (a number, or `{ left, top,
--- right, bottom }`; 12), `gap` (between several children, which stack
--- in a column), children.
function M.inset(spec, _)
  local m = spec.margins or spec.margin or 12
  local props = U.place(spec, {})
  if type(m) == "table" then
    props.left_margin, props.top_margin = m.left or m[1] or 0, m.top or m[2] or 0
    props.right_margin, props.bottom_margin = m.right or m[3] or 0, m.bottom or m[4] or 0
  else
    props.margin = m
  end
  local child = spec[1]
  if #spec > 1 then
    local col = { gap = spec.gap or 8 }
    for i, c in ipairs(spec) do col[i] = c end
    child = ui.Column(col)
  end
  props[1] = child
  if spec.width then props.width = spec.width end
  if spec.height then props.height = spec.height end
  return ui.Inset(props)
end

return M
