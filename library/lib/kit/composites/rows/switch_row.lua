-- AdwSwitchRow: a row with a switch at its end; `active` (a value or a
-- binding), `on_toggled(on)`. A press on the row toggles it too.
local widgets = require("lib.kit.widgets")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local value, set = rows.flag(spec, "switch")
  local switch = widgets.switch { id = spec.id and (spec.id .. "-switch") or nil, checked = value,
    accessible_name = spec.title, on_toggled = function(on) set(on) end }
  local node = rows.frame(spec, { suffix = switch, suffix_w = 52, on_clicked = function() set(not value()) end })
  return node, { node = node, active = value, set = set }
end
