-- Disclosures (the Disclosure archetype): GtkExpander, GtkRevealer, and
-- the header of an expander row.
--
--     local node, t = disclosure.make("expander", {
--       title = "Advanced", width = 320, header_height = 40,
--       content = ui.Column { ... },         -- shown while expanded
--       expanded = false, group = "settings", -- one open at a time
--       on_toggled = function(open) end,
--     })
--
-- The header is the control: a press, Space or Return toggles, Left and
-- Right close and open. The skin draws `header` (the row's look, given
-- `spec.title`), `indicator` (the chevron) and `background`; the content
-- is the spec's, clipped under the header, the whole growing and shrinking
-- with it. A revealer has no header and takes no focus: `expanded` (a
-- binding) shows its content.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

function M.make(widget, spec)
  spec = spec or {}
  local revealer = widget == "revealer"
  local header_h = spec.header_height or (revealer and 0 or 40)
  local content = spec.content or ui.Item {}
  local holder = ui.Item { y = header_h, width = spec.width, clip = true, content }
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget, full.content = widget, nil
  local t
  local props = { width = spec.width, clip = true, focus_policy = spec.focus_policy or (revealer and "none" or "tab") }
  for _, k in ipairs { "id", "x", "y", "anchors", "visible", "z" } do props[k] = spec[k] end
  props.height = header_h
  props.behavior = spec.animated == false and nil or { height = { duration = 240, easing = "out_cubic" } }
  local root
  root, t = control.make("Disclosure", widget, full, { children = { holder }, props = props })
  holder.height = function() return t.expanded and (content.layout_height or 0) or 0 end
  -- (Bound once `t` is there: a binding that ran before it read nothing to
  -- follow.)
  root.height = function() return header_h + (t.expanded and (content.layout_height or 0) or 0) end
  return root, t
end

return M
