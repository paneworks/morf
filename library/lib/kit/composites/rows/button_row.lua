-- AdwButtonRow: a row that is one centred press, its `title` in the
-- accent; `start_icon`, `end_icon`, `on_activated`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local s = {}
  for k, v in pairs(spec) do s[k] = v end
  s.height = spec.height or 52
  s.icon = nil
  local label = ui.Row { anchors = { center_in = true }, gap = 6, align = "center",
    spec.start_icon and widgets.image { icon = spec.start_icon, variant = "accent" } or nil,
    widgets.label { text = spec.title or "", variant = "accent heading" },
    spec.end_icon and widgets.image { icon = spec.end_icon, variant = "accent" } or nil }
  local node = rows.frame(s, { no_titles = true, extra = { label },
    on_clicked = function() if spec.on_activated then spec.on_activated() end end })
  return node, { node = node }
end
