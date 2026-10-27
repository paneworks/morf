-- The Plane: a point moved over two ranges at once. It is a part of
-- `color_chooser`, its saturation-value plane, and of nothing else.
--
--     local plane = require("lib.kit.archetypes.plane")
--     local node = plane.make("color_plane", { x = 0.5, y = 0.5, hue = function() return 210 end,
--       on_moved = function(s, v) end })
local control = require("lib.kit.control")

local M = {}

--- A plane of `widget` built from `spec`.
function M.make(widget, spec)
  local merged = {}
  for k, v in pairs(spec or {}) do merged[k] = v end
  merged.widget = widget
  return (control.make("Plane", widget, merged))
end

return M
