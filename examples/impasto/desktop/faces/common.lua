-- The pieces every desk face is made of.
--
-- A face is `build(ctx)`, returning a node exactly `ctx.width` by
-- `ctx.height`. The context carries the module id, `family`, `theme`, the
-- widget's `ink` (a table of colour functions: ground, border, text, muted,
-- accent, accentText, raised, dim, red), `row` (a function returning the
-- desk row, nil on the card's tiles) and `key`. A face is rebuilt whenever
-- the family or the theme changes, so it may lay itself out in plain
-- numbers.
--
-- `common.widget_face` is WidgetFace.qml, the layout every Modern face
-- shares: the mark top left, the label top right, the reading along the
-- bottom with a caption under it; a wide face gives part of its width to
-- `extra`, a large one puts `body` in the middle.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local draw = require("pets.draw")

local C = theme.color
local common = {}

function common.read(v)
  if type(v) == "function" then return v() end
  return v
end
local read = common.read

local function length(s)
  local ok, n = pcall(utf8.len, s)
  return ok and n or #s
end

--- Text that shrinks to fit `width` on one line, down to `floor`.
function common.fit_size(text, width, size, floor)
  local n = math.max(1, length(text))
  local fitted = math.floor(width / (n * 0.62))
  return math.max(floor or theme.size.large, math.min(size, fitted))
end

--- A glyph in the mono face, centred in `box` (a number or { w, h }).
function common.glyph(values)
  local box = values.box or 40
  local out = {
    glyph = values.glyph, size = values.size or 30,
    color = values.color, width = values.width or box, height = values.height or box,
    vertical_alignment = "center", x = values.x, y = values.y, anchors = values.anchors,
    visible = values.visible, opacity = values.opacity,
  }
  return kit.glyph(out)
end

--- WidgetFace. `values`: `label`, `reading`, `note`, `tint` (functions or
--- strings), `reading_size`, `extra_share`, `mark` (a node for the 40px
--- box), `extra` (a function(w, h) returning a node), `body` (the same).
function common.widget_face(ctx, values)
  local w, h = ctx.width, ctx.height
  local ink = ctx.ink
  local pad = w > 300 and 22 or 18
  local share = values.extra_share or 0
  local text_width = w - 2 * pad - w * share
  local reading_size = values.reading_size or theme.size.widget
  local reading = function() return tostring(read(values.reading) or "") end
  local note = function() return tostring(read(values.note) or "") end
  local children = {}

  if values.mark then
    children[#children + 1] = ui.Item { x = pad, y = pad, width = 40, height = 40, values.mark }
  end
  children[#children + 1] = kit.text {
    x = pad + 48, y = pad + 4, width = w - 2 * pad - 48,
    text = function() return tostring(read(values.label) or "") end,
    horizontal_alignment = "right", elide = "right",
    size = theme.size.small, weight = 600, color = ink.muted,
  }
  local note_height = 16
  local reading_node = kit.text {
    width = text_width, elide = "right",
    text = reading,
    size = function() return common.fit_size(reading(), text_width, reading_size) end,
    weight = 600,
    color = function() return read(values.tint) or ink.text() end,
  }
  local note_node = kit.text {
    width = text_width, elide = "right", text = note,
    size = theme.size.small, color = ink.muted,
    visible = function() return note() ~= "" end,
  }
  children[#children + 1] = ui.Item {
    x = pad, y = 0, width = text_width, height = h - pad,
    ui.Column {
      anchors = { bottom = true, left = true },
      gap = 2,
      reading_node, note_node,
    },
  }
  if values.body then
    local top = pad + 40 + 12
    local bottom = h - pad - reading_size * 1.25 - note_height - 12
    children[#children + 1] = ui.Item {
      x = pad, y = top, width = w - 2 * pad, height = math.max(0, bottom - top),
      values.body(w - 2 * pad, math.max(0, bottom - top)),
    }
  end
  if values.extra and share > 0 then
    local ew = math.max(0, w * share - pad)
    local top = pad + 4 + 14 + 10
    local eh = h - top - pad
    children[#children + 1] = ui.Item {
      x = w - pad - ew, y = top, width = ew, height = eh,
      values.extra(ew, eh),
    }
  end
  return ui.Item { width = w, height = h, table.unpack(children) }
end

--- A small text in the face's type: `size`, `weight`, `color`, `mono`.
function common.text(values) return kit.text(values) end

--- A circular gauge (RingIndicator) filling `box` with children centred.
function common.ring(values)
  local ring = require("components.ring_indicator")
  return ring {
    size = values.size or 40, thickness = values.thickness or 3,
    progress = values.progress, track_color = values.track_color, fill_color = values.fill_color,
    x = values.x, y = values.y, anchors = values.anchors,
    table.unpack(values),
  }
end

--- A thin bar (UsageBar).
function common.bar(values)
  return require("components.usage_bar")(values)
end

--- A borderless round button with a glyph, in the face's text colour.
function common.icon_button(values)
  local d = values.diameter or 30
  local hovered = kit.hover_signal("desk.button")
  return ui.Rect {
    width = d, height = d, radius = d / 2,
    color = function()
      if hovered:get() then return values.ink.raised() end
      return morf.color("transparent")
    end,
    behavior = { color = theme.behave("fast") },
    opacity = values.opacity, visible = values.visible,
    kit.glyph {
      anchors = { center_in = true }, size = values.glyph_size or 16,
      glyph = values.glyph, color = values.color or values.ink.text,
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() if values.on_click then values.on_click() end end,
    },
  }
end

--- A capsule with a label, the accent when active (PillButton).
function common.pill(values)
  return require("components.pill_button")(values)
end

--- A picture clipped to a rounded square, a glyph standing in when there is
--- none: album art, a photo's frame.
function common.picture(values)
  local size = values.size
  local source = values.source
  return ui.ClipRect {
    x = values.x, y = values.y, anchors = values.anchors,
    width = values.width or size, height = values.height or size,
    radius = values.radius or (size or 40) * theme.picture_corner,
    color = values.ink.raised,
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source = function() return read(source) or "" end,
      visible = function() return (read(source) or "") ~= "" end,
    },
    kit.glyph {
      anchors = { center_in = true }, size = values.glyph_size or 18,
      glyph = values.glyph or "󰎇", color = values.ink.muted,
      visible = function() return (read(source) or "") == "" end,
    },
  }
end

--- A line through `values` (0..1 each, oldest first), as one SVG image
--- `width` by `height` (Sparkline.qml).
function common.sparkline(values)
  local w, h = values.width, values.height
  return ui.Image {
    x = values.x, y = values.y, width = w, height = h, anchors = values.anchors,
    source = function()
      local list = read(values.values) or {}
      local stroke = draw.hex(read(values.stroke) or C.accent())
      if #list < 2 then
        return string.format('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d"/>', w, h)
      end
      local pts = {}
      for i, v in ipairs(list) do
        local x = (i - 1) / (#list - 1) * (w - 4) + 2
        local y = h - 2 - math.max(0, math.min(1, tonumber(v) or 0)) * (h - 4)
        pts[#pts + 1] = draw.n(x) .. "," .. draw.n(y)
      end
      return string.format(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d"><polyline points="%s" fill="none" stroke="%s" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/><circle cx="%s" cy="%s" r="2.2" fill="%s"/></svg>',
        w, h, table.concat(pts, " "), stroke,
        pts[#pts]:match("^([^,]+)"), pts[#pts]:match(",(.+)$"), stroke)
    end,
  }
end

--- The Claude mark: a sunburst of eight rounded rays, in `color`.
function common.claude_mark(values)
  local size = values.size or 32
  return ui.Image {
    x = values.x, y = values.y, anchors = values.anchors, width = size, height = size,
    source = function()
      local col = draw.hex(read(values.color) or C.text())
      local rays = {}
      for i = 0, 11 do
        local a = math.rad(i * 30)
        local long = i % 2 == 0 and 46 or 34
        rays[#rays + 1] = string.format('<line x1="50" y1="50" x2="%s" y2="%s"/>',
          draw.n(50 + math.cos(a) * long), draw.n(50 + math.sin(a) * long))
      end
      return string.format(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><g stroke="%s" stroke-width="10" stroke-linecap="round">%s</g></svg>',
        col, table.concat(rays))
    end,
  }
end

--- A layer that shadows its content, for faces drawn on the wallpaper.
common.shadow_layer = function()
  return { enabled = true, shadow_color = morf.color("#000000"):alpha(0.6), shadow_blur = 8, shadow_offset_y = 2 }
end

return common
