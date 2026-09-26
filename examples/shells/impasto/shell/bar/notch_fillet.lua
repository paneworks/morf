-- The concave corner where the attached island meets the screen edge.
--
-- Port of NotchFillet.qml. A Rect's radius only rounds a corner inwards, so
-- this is drawn: the square less a disc centred on its far corner, flaring
-- away from the island rather than cutting into it. Mirrored for the
-- island's left side. The bar and the lock screen's island both use it.
--
--     notch_fillet { x = ..., color = ..., mirrored = true, visible = ... }

local ui = require("morf.ui")
local theme = require("theme")

return function(values)
  local size = 2 * theme.radius_notch
  local d = values.mirrored
    and string.format("M%d 0 L0 0 A%d %d 0 0 1 %d %d Z", size, size, size, size, size)
    or string.format("M0 0 L%d 0 A%d %d 0 0 0 0 %d Z", size, size, size, size)
  return ui.Path {
    x = values.x, y = values.y or 0,
    width = size, height = size,
    view_box = { 0, 0, size, size },
    d = d,
    fill_color = values.color or theme.color.island,
    visible = values.visible,
    behavior = values.behavior,
  }
end
