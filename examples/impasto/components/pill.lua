-- A pill button and a segmented control: impasto's PillButton.qml and
-- SegmentedControl.qml.
--
-- A pill is a glyph and a word on the island's surface, lit on hover and
-- filled with the accent while `active`. A segmented control is a row of
-- such words in one pill, the current one filled.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color
local pill = {}

local function value(v) if type(v) == "function" then return v() end return v end

--- `text`, `icon`, `active` (value or binding), `on_click`, `height`,
--- `padding`, and any node property.
function pill.button(values)
  local hovered = kit.hover_signal("pill")
  local active = values.active or false
  local pad = values.padding or 10
  local height = values.height or 28
  local row = ui.Row {
    gap = 6, align = "center",
    anchors = { center_in = true },
    kit.glyph {
      text = values.icon or "", size = 12,
      visible = (values.icon or "") ~= "",
      color = function() return value(active) and C.accentText() or C.accent() end,
    },
    kit.text {
      text = values.text or "", size = theme.size.small,
      visible = function() return value(values.text or "") ~= "" end,
      weight = 400,
      font_weight = function() return value(active) and 600 or 400 end,
      color = function() return value(active) and C.accentText() or C.text() end,
    },
  }
  return ui.Rect {
    x = values.x, y = values.y, anchors = values.anchors, visible = values.visible,
    layout = values.layout,
    width = function() return (row.layout_width or 0) + 2 * pad end,
    height = height,
    radius = height / 2,
    color = function()
      if value(active) then return hovered:get() and C.accentHover() or C.accent() end
      return hovered:get() and C.islandSurfaceHover or C.islandSurface
    end,
    border_width = 1,
    border_color = function()
      if value(active) or hovered:get() then return C.accent() end
      return C.islandBorder
    end,
    behavior = { color = theme.behave("fast"), border_color = theme.behave("fast") },
    row,
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() if values.on_click then values.on_click() end end,
    },
  }
end

--- `options` = { { id, label } }, `current()` the id shown filled,
--- `on_select(id)`.
function pill.segmented(values)
  local children = {}
  for _, option in ipairs(values.options) do
    local hovered = kit.hover_signal("segment")
    local on = function() return values.current() == option.id end
    local label = kit.text {
      anchors = { center_in = true },
      text = option.label, size = theme.size.small,
      font_weight = function() return on() and 600 or 400 end,
      color = function()
        if on() then return C.accentText() end
        return hovered:get() and C.text() or C.textMuted()
      end,
    }
    children[#children + 1] = ui.Rect {
      width = function() return (label.layout_width or 0) + 30 end,
      height = 22, radius = 11,
      color = function() return on() and C.accent() or "#00000000" end,
      behavior = { color = theme.behave("fast") },
      label,
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() values.on_select(option.id) end,
      },
    }
  end
  local row = ui.Row { gap = 2, align = "center", anchors = { center_in = true }, table.unpack(children) }
  return ui.Rect {
    width = function() return (row.layout_width or 0) + 6 end,
    height = 28, radius = 14,
    color = C.islandSurface, border_width = 1, border_color = C.islandBorder,
    row,
  }
end

return pill
