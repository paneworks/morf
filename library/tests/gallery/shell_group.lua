-- Gallery samples for the shell group: a notification stack drawn in
-- place, the button that shows an osd, a tray item.
local M = { span = { notification = { 2, 1 } } }

function M.notification(_, w)
  local node, stack = w.notification { inline = true, width = 360, timeout = 0 }
  stack.notify { title = "Mail", body = "Three new messages", icon = "mail", app = "Mail", time = "now",
    actions = { { label = "Open" } } }
  return node
end
function M.osd(_, w)
  local _, osd = w.osd { icon = "volume_up", value = 0.6 }
  return w.button { label = "Show the osd", width = 160, on_clicked = function() osd.show() end }
end
function M.tray_item(_, w) return (w.tray_item { icon = "wifi", title = "Network" }) end

return M
