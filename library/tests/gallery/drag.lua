-- Gallery samples for the Drag widgets.
local ui = require("morf.ui")

local M = { span = { paned = { 2, 1 } } }

local function pane(w, text, tone)
  return ui.Rect { anchors = { fill = true }, color = function() return require("lib.kit.tokens").color(tone)() end,
    w.label { text = text, anchors = { center_in = true } } }
end

function M.paned(_, w)
  return (w.paned { width = 560, height = 200, position = 0.35,
    start_child = pane(w, "Sidebar", "sidebar_bg"), end_child = pane(w, "Content", "view_bg") })
end
function M.window_handle(_, w)
  return (w.window_handle { width = 240, height = 46,
    w.window_title { title = "Drag to move", subtitle = "a window handle" } })
end
function M.drag_source(_, w)
  return (w.drag_source { width = 160, height = 40, label = "report.pdf", icon = "description",
    payload = { ["text/plain"] = "report.pdf" } })
end
function M.drop_target(_, w)
  return (w.drop_target { width = 240, height = 140, keys = { "text/plain" }, label = "Drop a file here" })
end

return M
