-- Small pieces every part of the shell uses: a Material Symbols icon, text
-- in the shell's face, a rounded card.

local ui = require("morf.ui")
local theme = require("theme")

local M = {}

--- A Material Symbols Rounded icon by its ligature name (`"wifi_off"`).
--- `name` and `color` may be bindings; `props.fill` (a boolean or a
--- binding) fills it in through the face's FILL axis.
function M.icon(name, size, color, props)
  props = props or {}
  props.text = name
  props.font_family = theme.icon_font
  props.font_size = size or 18
  props.color = color or function() return theme.color.onSurface end
  if props.fill ~= nil then
    local fill = props.fill
    props.fill = nil
    props.axes = function()
      local on = fill
      if type(fill) == "function" then on = fill() end
      return { FILL = on and 1 or 0 }
    end
    props.behavior = props.behavior or {}
    props.behavior.axes = { duration = theme.duration.small, easing = theme.ease.standard }
  end
  return ui.Text(props)
end

--- Text in Rubik; `props` as a `ui.Text`'s.
function M.text(props)
  props.font_family = props.font_family or theme.font
  if theme.font_file ~= "" and props.font_source == nil then props.font_source = theme.font_file end
  props.font_size = props.font_size or theme.size.normal
  -- The reference's font builder sets `opsz` to the size in points (morf's
  -- automatic optical sizing, like CSS's, uses pixels, which reads a size
  -- larger and sets it tighter) and `ROND` 25 on every face. A face without
  -- those axes ignores them.
  if props.axes == nil and type(props.font_size) == "number" then
    props.axes = { opsz = props.font_size * 3 / 4, ROND = 25, wght = props.font_weight }
  elseif props.font_weight and props.axes == nil then
    props.axes = { wght = props.font_weight }
  end
  if props.color == nil then props.color = function() return theme.color.onSurface end end
  return ui.Text(props)
end

--- A card: a surfaceContainer box with the large rounding.
function M.card(props)
  props.radius = props.radius or theme.ROUNDING
  if props.color == nil then props.color = function() return theme.color.surfaceContainer end end
  return ui.Rect(props)
end

--- An item centred in a box of `w` x `h`.
function M.centred(w, h, child, props)
  props = props or {}
  props.width, props.height = w, h
  child.anchors = { center_in = true }
  props[#props + 1] = child
  return ui.Item(props)
end

--- Gives a MouseArea a rounded background whose colour follows its hover:
--- `color(hovered)`. (Built after the area, so the binding can read it.)
function M.hover(area, color, radius)
  local bg = ui.Rect {
    anchors = { fill = true }, z = -1, radius = radius or 0,
    color = function() return color(area.hovered) end,
    behavior = { color = { duration = theme.duration.small } },
  }
  ui.reparent(bg, area)
  return area
end

return M
