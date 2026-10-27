-- Gallery samples for the Scroll widget: a page of rows that scrolls, the
-- undershoot along the edge it goes on past.
local ui = require("morf.ui")

local M = {}

function M.scrolled_window(_, w)
  local column = ui.Column { gap = 8, width = 240 }
  for i = 1, 14 do ui.reparent(w.label { text = ("Line %d of a long page"):format(i), width = 240 }, column) end
  return (w.scrolled_window { width = 260, height = 200, clip = true, variant = "undershoot_bottom", column })
end

return M
