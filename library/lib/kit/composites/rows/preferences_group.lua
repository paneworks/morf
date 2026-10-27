-- AdwPreferencesGroup: a `title` and a `description` over `rows` (nodes,
-- or row specs with `kind`) boxed on the card with hairlines between;
-- `header_suffix` a node at the title's end.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local W = spec.width or 360
  local col = { id = spec.id, width = W, gap = 6 }
  if spec.title then
    col[#col + 1] = ui.Item { width = W, height = 24,
      widgets.label { text = spec.title, x = 4, width = W - 8, elide = "right", variant = "heading",
        anchors = { vertical_center = true } },
      spec.header_suffix }
    if spec.header_suffix then spec.header_suffix.anchors = { right = true, vertical_center = true } end
  end
  if spec.description then
    col[#col + 1] = widgets.label { text = spec.description, width = W - 8, x = 4, wrap = true, variant = "dim" }
  end
  local list = { variant = "boxed_list", width = W }
  for _, row in ipairs(spec.rows or {}) do list[#list + 1] = rows.build(row, W) end
  col[#col + 1] = widgets.box(list)
  local node = ui.Column(col)
  node.accessible_role, node.accessible_name = "group", spec.title
  return node, { node = node }
end
