-- AdwExpanderRow: a row that opens on `rows` (nodes, or row specs with
-- `kind`) under it; `title`, `subtitle`, `expanded`, `on_expanded`,
-- `on_collapsed`.
local ui = require("morf.ui")
local disclosure = require("lib.kit.archetypes.disclosure")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local W = spec.width or 360
  local list = { gap = 0, width = W }
  for _, row in ipairs(spec.rows or {}) do list[#list + 1] = rows.build(row, W) end
  local node, t = disclosure.make("expander_row", { id = spec.id, width = W, header_height = rows.height_of(spec),
    title = spec.title, subtitle = spec.subtitle, expanded = spec.expanded, accessible_name = spec.title,
    on_expanded = spec.on_expanded, on_collapsed = spec.on_collapsed, content = ui.Column(list) })
  return node, { node = node, t = t }
end
