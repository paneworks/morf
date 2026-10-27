-- AdwComboRow: a row with a drop-down at its end; `items`, `selected`,
-- `on_changed(index, item)`, `search`, `drop_width`.
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local DW = spec.drop_width or math.min(200, math.floor((spec.width or 360) * 0.45))
  local drop_node, drop = require("lib.kit.composites").drop_down {
    id = spec.id and (spec.id .. "-drop") or nil, width = DW, height = 34, variant = "flat",
    items = spec.items, selected = spec.selected, on_changed = spec.on_changed, search = spec.search,
    accessible_name = spec.title, item_id = spec.item_id, placeholder = spec.placeholder,
  }
  local node = rows.frame(spec, { suffix = drop_node, suffix_w = DW })
  return node, { node = node, drop = drop }
end
