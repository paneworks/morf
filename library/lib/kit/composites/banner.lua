-- AdwBanner: a strip across the top of a page with a `title` and, when
-- given, a button; it slides into view while `revealed` (a binding) says
-- so (Disclosure + Press).
--
--     local node = composites.banner { id = "offline", width = 480, title = "You are offline",
--       button_label = "Retry", on_button = retry, revealed = function() return offline:get() end }
--
-- Ids: `<id>`, `<id>-button`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local tokens = require("lib.kit.tokens")

return function(spec)
  spec = spec or {}
  local W, H = spec.width or 480, spec.height or 48
  local button = spec.button_label and widgets.button { id = spec.id and (spec.id .. "-button") or nil,
    label = spec.button_label, variant = "pill", height = 30,
    anchors = { right = true, right_margin = 12, vertical_center = true },
    on_clicked = function() if spec.on_button then spec.on_button() end end } or nil
  local strip = ui.Rect { width = W, height = H, color = tokens.color("accent_bg", 0.16),
    widgets.label { anchors = { center_in = true }, text = spec.title or "", variant = "heading", elide = "right",
      width = W - (button and 160 or 32), horizontal_alignment = "center" },
    button }
  local revealed = spec.revealed
  if revealed == nil then revealed = true end
  local node = widgets.revealer { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W,
    expanded = revealed, content = strip, accessible_role = "alert", accessible_name = spec.title }
  return node, { node = node }
end
