-- Gallery samples for the Press widgets: each as an application would use
-- it, every variant of a button side by side.
local ui = require("morf.ui")

local M = { span = { button = { 2, 1 } } }

local function flow(width, list)
  local f = ui.Flex { direction = "row", wrap = true, gap = 10, width = width }
  for _, node in ipairs(list) do ui.reparent(node, f) end
  return f
end

function M.button(_, w)
  return flow(560, {
    w.button { label = "Save", width = 110 },
    w.button { label = "Continue", variant = "suggested", width = 120 },
    w.button { label = "Delete", icon = "delete", variant = "destructive", width = 120 },
    w.button { label = "Cancel", variant = "flat", width = 100 },
    w.button { label = "Options", icon = "tune", variant = "raised", width = 120 },
    w.button { label = "Tinted", variant = "opaque", color = function() return morf.color("#26a269") end, width = 110 },
    w.button { label = "Get started", variant = "pill suggested", width = 160, height = 44 },
    w.button { icon = "add", variant = "circular", accessible_name = "Add" },
    w.button { icon = "play_arrow", variant = "osd circular", accessible_name = "Play" },
    w.button { label = "Saving", loading = true, width = 110 },
  })
end
function M.toggle_button(_, w)
  return ui.Row { gap = 10, w.toggle_button { label = "Bold", icon = "format_bold", checked = true, width = 110 },
    w.toggle_button { icon = "format_italic", variant = "flat", accessible_name = "Italic" } }
end
function M.check_button(_, w)
  return ui.Column { gap = 8,
    w.check_button { label = "Show hidden files", checked = true },
    w.check_button { label = "Partly", partial = true },
    w.check_button { label = "Small", group = "sample-size", checked = true },
    w.check_button { label = "Large", group = "sample-size" } }
end
function M.switch(_, w)
  return ui.Row { gap = 12, w.switch { checked = true, accessible_name = "Wi-Fi" }, w.switch { accessible_name = "Bluetooth" } }
end
function M.link_button(_, w) return w.link_button { label = "Read the docs", uri = "https://gnome.org", width = 140, height = 32 } end
function M.menu_item(_, w)
  return ui.Column { gap = 0,
    w.menu_item { label = "New window", icon = "open_in_new", width = 240, height = 36 },
    w.menu_item { label = "Word wrap", kind = "check", checked = true, width = 240, height = 36 },
    w.menu_item { label = "Light", kind = "radio", group = "sample-scheme", checked = true, width = 240, height = 36 },
    w.menu_item { label = "Dark", kind = "radio", group = "sample-scheme", width = 240, height = 36 } }
end
function M.tab_button(_, w) return w.tab_button { count = 3, accessible_name = "Tabs" } end

return M
