-- Gallery samples for the Range widgets: a scale as it is used -- with
-- marks and a reading, standing up, on the osd ground -- a spin button and
-- the scroll bars.
local ui = require("morf.ui")

local M = { span = { scale = { 2, 1 } } }

function M.scale(_, w)
  local level = morf.signal("gallery.scale", 0.4)
  return ui.Row { gap = 24,
    ui.Column { gap = 16,
      w.scale { width = 300, value = function() return level:get() end, on_moved = function(v) level:set(v) end,
        icon = "volume_up", digits = 0, from = 0, to = 100, format = function(v) return ("%d"):format(math.floor(v + 0.5)) end },
      w.scale { width = 300, from = 0, to = 4, step = 1, snap = "always", value = 2,
        marks = { { value = 0, label = "0" }, { value = 2, label = "2" }, { value = 4, label = "4" } } },
      w.scale { width = 300, variant = "osd", icon = "brightness_6", value = 0.7 } },
    w.scale { orientation = "vertical", height = 180, value = 0.6 } }
end
function M.spin_button(_, w) return w.spin_button { value = 12, from = 6, to = 72, width = 150, accessible_name = "Size" } end
function M.scrollbar(_, w)
  return ui.Item { width = 240, height = 160,
    w.scrollbar { x = 0, y = 150, width = 220, height = 10, size = 0.4, value = 0.3 },
    w.scrollbar { x = 230, y = 0, width = 10, height = 140, orientation = "vertical", size = 0.25, value = 0.6 } }
end

return M
