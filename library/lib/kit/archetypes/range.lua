-- The Range widgets (contract.lua): `scale` (a slider: `orientation`,
-- `marks`, `has_origin`, `digits`, and the range's settings -- `step` and
-- `snap` make it discrete, `logarithmic` a frequency, `live = false` a
-- seek bar that waits for the release), `spin_button` (its buttons and the
-- arrows step it; a drag up or down scrubs it) and `scrollbar` (the wheel
-- goes past it to the view it scrolls).
--
--     local range = require("lib.kit.archetypes.range")
--     local node = range.make("scale", { value = function() return level:get() end, on_moved = set_level })
local control = require("lib.kit.control")

local M = {}

local DEFAULTS = {
  spin_button = { step = 1, snap = "always", from = 0, to = 100, drag_mode = "vertical" },
  scrollbar = { wheel = false },
}

--- A range of `widget` built from `spec`.
function M.make(widget, spec)
  local merged = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do merged[k] = v end
  for k, v in pairs(spec or {}) do merged[k] = v end
  merged.widget = widget
  if merged.has_origin == nil then merged.has_origin = true end
  return (control.make("Range", widget, merged))
end

return M
