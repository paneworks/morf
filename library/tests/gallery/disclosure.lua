-- Gallery samples for the Disclosure widgets.
local ui = require("morf.ui")

local M = {}

function M.expander(_, w)
  return (w.expander { title = "Advanced", width = 260, expanded = true,
    content = ui.Column { x = 12, gap = 6, w.label { text = "Proxy: automatic" }, w.label { text = "Port: 8080" } } })
end
function M.revealer(_, w)
  local shown = morf.signal("gallery.revealer", true)
  return ui.Column { gap = 8,
    w.button { label = "Toggle", width = 100, on_clicked = function() shown:set(not shown:get()) end },
    (w.revealer { width = 240, expanded = function() return shown:get() end,
      content = w.box { variant = "card", width = 240, height = 60,
        w.label { text = "Revealed content", anchors = { center_in = true } } } }) }
end

return M
