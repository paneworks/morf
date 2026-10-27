-- AdwSpinRow: a row with a spin button at its end; `value` (a number or a
-- binding), `from`, `to`, `step`, `digits`, `on_changed(value)`,
-- `spin_width`.
local widgets = require("lib.kit.widgets")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local value = morf.signal(rows.key(spec, "spin"), tonumber(rows.get(spec.value)) or spec.from or 0)
  local function current()
    if type(spec.value) == "function" then return tonumber(spec.value()) or 0 end
    return value:get()
  end
  local SW = spec.spin_width or 150
  local spin = widgets.spin_button { id = spec.id and (spec.id .. "-value") or nil, width = SW, height = 34,
    from = spec.from or 0, to = spec.to or 100, step = spec.step or 1, digits = spec.digits,
    value = current, accessible_name = spec.title,
    on_moved = function(v)
      value:set(v)
      if spec.on_changed then spec.on_changed(v) end
    end }
  local node = rows.frame(spec, { suffix = spin, suffix_w = SW })
  return node, { node = node, value = current, set = function(v) value:set(v) end }
end
