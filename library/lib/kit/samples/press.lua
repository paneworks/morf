-- Gallery samples for the Press archetype's widgets (see samples/init.lua):
-- each as an application would use it -- a label and an icon where one
-- belongs, a group where the widget only makes sense in one.
local ui = require("morf.ui")

local M = {}

local function row(gap, list)
  local r = ui.Row { gap = gap, align = "center" }
  for _, node in ipairs(list) do ui.reparent(node, r) end
  return r
end

function M.push(_, w) return w.push { label = "Save", width = 120, height = 40 } end
function M.suggested(_, w) return w.suggested { label = "Continue", width = 140, height = 40 } end
function M.destructive(_, w) return w.destructive { label = "Delete", icon = "delete", width = 140, height = 40 } end
function M.flat(_, w) return w.flat { label = "Cancel", width = 120, height = 40 } end
function M.raised(_, w) return w.raised { label = "Options", icon = "tune", width = 140, height = 40 } end
function M.outlined(_, w) return w.outlined { label = "Learn more", width = 140, height = 40 } end
function M.text(_, w) return w.text { label = "Skip", width = 100, height = 40 } end
function M.tonal(_, w) return w.tonal { label = "Add to list", icon = "playlist_add", width = 170, height = 40 } end
function M.elevated(_, w) return w.elevated { label = "Share", icon = "share", width = 130, height = 40 } end
function M.pill(_, w) return w.pill { label = "Get started", width = 170, height = 48 } end
function M.circular(_, w) return w.circular { icon = "add", accessible_name = "Add", width = 48, height = 48 } end
function M.icon(_, w)
  return w.icon { icon_off = "volume_up", icon_on = "volume_off", accessible_name = "Mute", width = 40, height = 40 }
end
function M.link(_, w) return w.link { label = "Read the docs", width = 160, height = 32 } end
function M.close(_, w) return w.close { accessible_name = "Close", width = 40, height = 40 } end
function M.copy(_, w) return w.copy { label = "Copy", icon = "content_copy", width = 130, height = 40 } end
function M.loading(_, w) return w.loading { label = "Saving", width = 140, height = 40 } end
function M.toggle(_, w) return w.toggle { label = "Bold", icon = "format_bold", checked = true, width = 120, height = 40 } end
function M.toggle_group_member(_, w)
  local out = {}
  for i, entry in ipairs { { "Left", "format_align_left", "first" }, { "Centre", "format_align_center", "middle" },
    { "Right", "format_align_right", "last" } } do
    out[i] = w.toggle_group_member { icon = entry[2], accessible_name = entry[1], position = entry[3],
      group = "sample-align", checked = i == 1, width = 56, height = 40 }
  end
  return row(0, out)
end
function M.switch(_, w) return w.switch { checked = true, accessible_name = "Wi-Fi" } end
function M.checkbox(_, w) return w.checkbox { checked = true, accessible_name = "Remember me" } end
function M.radio(_, w) return w.radio { checked = true, group = "sample-radio", accessible_name = "Daily" } end
function M.chip_assist(_, w) return w.chip_assist { label = "Directions", icon = "directions", width = 150, height = 32 } end
function M.chip_filter(_, w) return w.chip_filter { label = "Vegetarian", checked = true, width = 150, height = 32 } end
function M.chip_input(_, w) return w.chip_input { label = "Ada Lovelace", icon = "person", width = 170, height = 32 } end
function M.chip_suggestion(_, w) return w.chip_suggestion { label = "Sounds good", width = 140, height = 32 } end
function M.tag(_, w) return w.tag { label = "Beta", width = 72, height = 26 } end
function M.tile(_, w)
  return w.tile { label = "Wi-Fi", subtitle = "Home", icon = "wifi", checkable = true, checked = true,
    width = 200, height = 64 }
end
function M.card_action(_, w)
  return w.card_action { label = "Storage", subtitle = "48 GB free", icon = "storage", width = 240, height = 80 }
end
function M.row_activation(_, w)
  return w.row_activation { label = "Notifications", subtitle = "Banners and sounds", icon = "notifications",
    width = 260, height = 56 }
end
function M.menu_item(_, w) return w.menu_item { label = "Open", icon = "folder_open", width = 220, height = 40 } end
function M.check_menu_item(_, w)
  return w.check_menu_item { label = "Show hidden files", checked = true, width = 240, height = 40 }
end
function M.radio_menu_item(_, w)
  return w.radio_menu_item { label = "Sort by name", checked = true, group = "sample-sort", width = 240, height = 40 }
end
function M.keycap(_, w)
  return row(6, { w.keycap { label = "Ctrl", width = 56, height = 34 }, w.keycap { label = "K", width = 38, height = 34 } })
end
function M.fab(_, w) return w.fab { icon = "edit", accessible_name = "Compose", width = 56, height = 56 } end
function M.extended_fab(_, w) return w.extended_fab { icon = "edit", label = "Compose", width = 160, height = 56 } end
function M.speed_dial_item(_, w)
  return w.speed_dial_item { icon = "photo_camera", label = "Camera", width = 170, height = 48 }
end
function M.segment(_, w)
  local out = {}
  for i, entry in ipairs { { "Day", "first" }, { "Week", "middle" }, { "Month", "last" } } do
    out[i] = w.segment { label = entry[1], position = entry[2], group = "sample-range", checked = i == 2,
      width = 80, height = 40 }
  end
  return row(0, out)
end
function M.rating_star(_, w)
  local out = {}
  for i = 1, 5 do out[i] = w.rating_star { checked = i <= 3, accessible_name = i .. " stars", width = 36, height = 36 } end
  return row(2, out)
end
function M.help(_, w) return w.help { accessible_name = "Help", width = 40, height = 40 } end
function M.disclosure_button(_, w) return w.disclosure_button { label = "Advanced", width = 150, height = 40 } end
function M.repeat_button(_, w)
  return row(8, { w.repeat_button { icon = "remove", accessible_name = "Less", width = 40, height = 40 },
    w.repeat_button { icon = "add", accessible_name = "More", width = 40, height = 40 } })
end
function M.back(_, w) return w.back { accessible_name = "Back", width = 40, height = 40 } end
function M.forward(_, w) return w.forward { accessible_name = "Forward", width = 40, height = 40 } end

M.span = {}

return M
