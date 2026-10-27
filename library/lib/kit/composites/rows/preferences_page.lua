-- AdwPreferencesPage: `groups` (nodes, or group specs) scrolled in a
-- clamped column; `gap` (24) between them.
local ui = require("morf.ui")
local scroll = require("lib.kit.archetypes.scroll")

return function(spec)
  spec = spec or {}
  local W, H = spec.width or 400, spec.height or 400
  local inner = W - 16
  local col = { gap = spec.gap or 24, width = inner, x = 0 }
  for _, group in ipairs(spec.groups or {}) do
    if type(group) == "table" and group.kind == nil and (group.rows or group.title) then
      local s = {}
      for k, v in pairs(group) do s[k] = v end
      s.width = s.width or inner
      group = (require("lib.kit.composites").preferences_group(s))
    end
    col[#col + 1] = group
  end
  local node, flick = scroll.make("scrolled_window", { id = spec.id, x = spec.x, y = spec.y, width = W, height = H,
    clip = true, focus_policy = "none", ui.Column(col) })
  return node, { node = node, flick = flick }
end
