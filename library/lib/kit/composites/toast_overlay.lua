-- AdwToastOverlay: toasts at the bottom centre of a surface, a few seconds
-- each, its title on the dark pill and its first action a button (Popup +
-- Press). The notification stack in its toast mode
-- (lib.kit.composites.notification): its fields, and `notify` takes a
-- toast -- `{ title, icon, timeout, actions = { { label, on_clicked } } }`.
--
--     local node, toasts = composites.toast_overlay { id = "toasts", root = surface_root }
--     toasts.notify { title = "Copied", actions = { { label = "Undo", on_clicked = undo } } }
local notification = require("lib.kit.composites.notification")

return function(spec)
  local s = {}
  for k, v in pairs(spec or {}) do s[k] = v end
  s.mode = "toast"
  return notification(s)
end
