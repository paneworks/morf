-- The layout every Analogue face shares: the object (a dial, a gauge, a
-- record) takes a square, and the text goes beside or under it.
--
-- Port of Instrument.qml.
--
--   2x2   the object, centred, with one small line under it
--   4x2   the object on the left; the reading and the note to its right, and
--         `extra` under them
--   8x2   as 4x2, wider, with a larger reading
--   4x4   the 4x2 layout on top of `body`
--
-- `instrument.build(ctx, values)`: `line`, `reading`, `note`, `tint`
-- (strings or functions), `filled` (the text moves up to make room for
-- `extra`), and `object`, `extra`, `body`, each a function(w, h) returning
-- the node for its box. Returns the face node and the object's box.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")

local read = common.read
local M = {}

--- The object's box for a family and face size: x, y, w, h.
function M.object_box(ctx, has_line)
  local square = ctx.family == "2x2"
  local large = ctx.family == "4x4"
  local pad = square and 18 or 22
  local upper = large and ctx.height / 2 or ctx.height
  local side = upper - 2 * pad
  if square then
    return pad, pad, ctx.width - 2 * pad, ctx.height - 2 * pad - (has_line and 26 or 0)
  end
  return pad, pad, side, side
end

function M.build(ctx, values)
  local w, h = ctx.width, ctx.height
  local ink = ctx.ink
  local square = ctx.family == "2x2"
  local large = ctx.family == "4x4"
  local band = ctx.family == "8x2"
  local pad = square and 18 or 22
  local upper = large and h / 2 or h
  local has_line = square and values.line ~= nil
  local ox, oy, ow, oh = M.object_box(ctx, has_line)
  local children = {}

  if values.object then
    children[#children + 1] = ui.Item { x = ox, y = oy, width = ow, height = oh, values.object(ow, oh) }
  end

  if square then
    if values.line then
      children[#children + 1] = kit.text {
        x = pad, y = h - (pad - 4) - 16, width = w - 2 * pad, height = 16,
        horizontal_alignment = "center", elide = "right",
        text = function() return tostring(read(values.line) or "") end,
        size = theme.size.small, color = ink.muted,
      }
    end
  else
    local left = ox + ow + 16
    local tw = w - pad - left
    local size = band and math.floor(theme.size.display * 0.65) or theme.size.widget
    local words_h = math.floor(size * 1.25) + 2 + 17
    local wy = values.filled and pad + 6 or math.floor(upper / 2 - words_h / 2)
    local reading = function() return tostring(read(values.reading) or "") end
    local note = function() return tostring(read(values.note) or "") end
    children[#children + 1] = ui.Column {
      x = left, y = wy, width = tw, gap = 2,
      kit.text {
        width = tw, elide = "right", text = reading, weight = 600,
        size = function() return common.fit_size(reading(), tw, size) end,
        color = function() return read(values.tint) or ink.text() end,
      },
      kit.text {
        width = tw, elide = "right", text = note, size = theme.size.regular, color = ink.muted,
        visible = function() return note() ~= "" end,
      },
    }
    if values.extra then
      local ey = wy + words_h + 10
      local eh = math.max(0, upper - pad - ey)
      children[#children + 1] = ui.Item { x = left, y = ey, width = tw, height = eh, values.extra(tw, eh) }
    end
  end

  if large and values.body then
    children[#children + 1] = ui.Item {
      x = pad, y = upper, width = w - 2 * pad, height = upper - pad,
      values.body(w - 2 * pad, upper - pad),
    }
  end

  return ui.Item { width = w, height = h, table.unpack(children) }
end

return M
