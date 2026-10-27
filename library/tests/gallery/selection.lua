-- Gallery samples for the Selection archetype's widgets (samples/init.lua):
-- each the widget as a configuration would make it, drawn by the theme's
-- skin. Colours a sample hands over (swatches) are the theme's accent
-- turned round the wheel, so every look keeps its own palette.

local S = {}

local function state(name, value) return morf.signal("kit.samples.selection." .. name, value) end

-- A selection kept in a signal: `current` follows it, a choice sets it.
local function chosen(name, first)
  local sig = state(name, first)
  return function() return sig:get() end, function(i) sig:set(i) end
end

function S.tabs(_, widgets)
  local current, set = chosen("tabs", 1)
  return widgets.tabs { id = "sample-tabs", width = 280, height = 64, accessible_name = "Sections",
    items = { { name = "Home", icon = "home" }, { name = "Media", icon = "music_note" },
      { name = "Weather", icon = "partly_cloudy_day" } },
    current = current, on_current_changed = set }
end

function S.segmented(_, widgets)
  local current, set = chosen("segmented", 2)
  return widgets.segmented { id = "sample-segmented", accessible_name = "Range", items = { "Day", "Week", "Month" },
    item_width = 84, item_height = 36, current = current, on_current_changed = set }
end

function S.view_switcher(_, widgets)
  local current, set = chosen("view_switcher", 1)
  return widgets.view_switcher { id = "sample-view-switcher", accessible_name = "Views",
    items = { { label = "Library", icon = "library_music" }, { label = "Albums", icon = "album" },
      { label = "Artists", icon = "person" } },
    item_width = 88, item_height = 56, current = current, on_current_changed = set }
end

function S.inline_view_switcher(_, widgets)
  local current, set = chosen("inline_view_switcher", 1)
  return widgets.inline_view_switcher { id = "sample-inline-switcher", accessible_name = "Layout",
    items = { { label = "Grid", icon = "grid_view" }, { label = "List", icon = "view_list" } },
    item_width = 110, item_height = 36, current = current, on_current_changed = set }
end

function S.radio_group(_, widgets)
  local current, set = chosen("radio_group", 2)
  return widgets.radio_group { id = "sample-radio-group", accessible_name = "Quality",
    items = { "Low", "Balanced", "High" }, item_width = 220, item_height = 40,
    current = current, on_current_changed = set }
end

function S.toggle_group(_, widgets)
  return widgets.toggle_group { id = "sample-toggle-group", accessible_name = "Style",
    items = { { label = "", icon = "format_bold" }, { label = "", icon = "format_italic" },
      { label = "", icon = "format_underlined" }, { label = "", icon = "strikethrough_s" } },
    item_width = 48, item_height = 40, current = 1, selected = { 1, 3 } }
end

function S.list_selection(_, widgets)
  local current, set = chosen("list_selection", 2)
  return widgets.list_selection { id = "sample-list-selection", accessible_name = "Applications",
    items = { "Firefox", "Files", "Terminal", "Text Editor" }, item_width = 240, item_height = 40,
    current = current, on_current_changed = set }
end

function S.grid_selection(_, widgets)
  local current, set = chosen("grid_selection", 5)
  local items = {}
  for i = 1, 8 do items[i] = { label = tostring(i) } end
  return widgets.grid_selection { id = "sample-grid-selection", accessible_name = "Workspace", columns = 4, gap = 6,
    items = items, item_width = 56, item_height = 56, current = current, on_current_changed = set }
end

function S.carousel_dots(_, widgets)
  local current, set = chosen("carousel_dots", 2)
  return widgets.carousel_dots { id = "sample-carousel-dots", accessible_name = "Slide",
    items = { "One", "Two", "Three", "Four", "Five" }, item_width = 24, item_height = 24,
    current = current, on_current_changed = set }
end

function S.pagination(_, widgets)
  local current, set = chosen("pagination", 3)
  return widgets.pagination { id = "sample-pagination", accessible_name = "Page",
    items = { "1", "2", "3", "4", "5", "6" }, item_width = 38, item_height = 38, gap = 4,
    current = current, on_current_changed = set }
end

function S.stepper_header(_, widgets)
  local current, set = chosen("stepper_header", 2)
  return widgets.stepper_header { id = "sample-stepper", accessible_name = "Steps",
    items = { "Account", "Network", "Finish" }, item_width = 92, item_height = 64,
    current = current, on_current_changed = set }
end

function S.sidebar_list(_, widgets)
  local current, set = chosen("sidebar_list", 1)
  return widgets.sidebar_list { id = "sample-sidebar-list", accessible_name = "Places",
    items = { { label = "Home", icon = "home" }, { label = "Documents", icon = "description" },
      { label = "Downloads", icon = "download" }, { label = "Trash", icon = "delete" } },
    item_width = 220, item_height = 40, gap = 2, current = current, on_current_changed = set }
end

function S.breadcrumbs(_, widgets)
  local items = { "Home", "Projects", "morf" }
  local current, set = chosen("breadcrumbs", 3)
  return widgets.breadcrumbs { id = "sample-breadcrumbs", accessible_name = "Path", items = items,
    item_height = 36, gap = 0, current = current, on_current_changed = set }
end

function S.day_grid(_, widgets)
  local current, set = chosen("day_grid", 17)
  local items, disabled = {}, {}
  -- A month starting on a Wednesday: two blanks before the 1st.
  for i = 1, 35 do
    local day = i - 2
    if day >= 1 and day <= 31 then items[i] = { label = tostring(day), day = day }
    else items[i] = { label = "", blank = true } disabled[#disabled + 1] = i end
  end
  return widgets.day_grid { id = "sample-day-grid", accessible_name = "Days", items = items, disabled = disabled,
    item_width = 40, item_height = 34, gap = 0, current = current, on_current_changed = set }
end

function S.swatch_grid(kit, widgets)
  local current, set = chosen("swatch_grid", 3)
  local items = {}
  for i = 1, 12 do
    local turn = (i - 1) * 30
    items[i] = { label = "Hue " .. turn, color = function() return kit.signal("accent")():rotate(turn) end }
  end
  return widgets.swatch_grid { id = "sample-swatch-grid", accessible_name = "Swatches", columns = 6, gap = 6,
    items = items, item_width = 36, item_height = 36, current = current, on_current_changed = set }
end

function S.emoji_grid(_, widgets)
  local current, set = chosen("emoji_grid", 6)
  local items = {}
  for i, e in ipairs { "😀", "😂", "🥰", "😎", "🤔", "😴", "🥳", "😇", "🙃", "😺", "🌱", "🔥", "⭐", "🍕", "🎧" } do
    items[i] = { label = e, name = e }
  end
  return widgets.emoji_grid { id = "sample-emoji-grid", accessible_name = "Emoji", columns = 5, gap = 2,
    items = items, item_width = 44, item_height = 44, current = current, on_current_changed = set }
end

function S.icon_chooser(_, widgets)
  local current, set = chosen("icon_chooser", 2)
  local items = {}
  for i, name in ipairs { "home", "star", "favorite", "bolt", "cloud", "pets", "rocket_launch", "palette",
    "terminal", "headphones" } do
    items[i] = { label = "", icon = name, name = name }
  end
  return widgets.icon_chooser { id = "sample-icon-chooser", accessible_name = "Icon", columns = 5, gap = 4,
    items = items, item_width = 48, item_height = 48, current = current, on_current_changed = set }
end

function S.transfer_side(_, widgets)
  return widgets.transfer_side { id = "sample-transfer-side", accessible_name = "Available",
    items = { "Calendar", "Clock", "Contacts", "Maps", "Weather" }, item_width = 220, item_height = 36,
    current = 2, selected = { 2, 3 } }
end

function S.rating_items(_, widgets)
  local current, set = chosen("rating_items", 4)
  return widgets.rating_items { id = "sample-rating-items", accessible_name = "Rating",
    items = { "1", "2", "3", "4", "5" }, item_width = 40, item_height = 40, gap = 2,
    current = current, on_current_changed = set }
end

function S.radial_menu(_, widgets)
  local current, set = chosen("radial_menu", 2)
  return widgets.radial_menu { id = "sample-radial-menu", accessible_name = "Edit", size = 210,
    items = { { label = "Copy", icon = "content_copy" }, { label = "Paste", icon = "content_paste" },
      { label = "Share", icon = "share" }, { label = "Delete", icon = "delete" }, { label = "Cut", icon = "content_cut" },
      { label = "Rename", icon = "edit" } },
    item_width = 44, item_height = 44, current = current, on_current_changed = set }
end

-- Shown where it would open: a configuration opens it round the pointer
-- (`handle.open_at`, `handle.attach`).
function S.pie_menu(_, widgets)
  local current, set = chosen("pie_menu", 1)
  local node = widgets.pie_menu { id = "sample-pie-menu", accessible_name = "Media", size = 220,
    items = { { label = "Play", icon = "play_arrow" }, { label = "Next", icon = "skip_next" },
      { label = "Queue", icon = "queue_music" }, { label = "Back", icon = "skip_previous" } },
    item_width = 64, item_height = 48, current = current, on_current_changed = set }
  return node
end

-- A time picker: hours and minutes on two drums.
function S.tumbler(kit, widgets)
  local ui = require("morf.ui")
  local hours, minutes = {}, {}
  for i = 0, 23 do hours[#hours + 1] = ("%02d"):format(i) end
  for i = 0, 59, 5 do minutes[#minutes + 1] = ("%02d"):format(i) end
  local hour, set_hour = chosen("tumbler_hour", 10)
  local minute, set_minute = chosen("tumbler_minute", 7)
  return ui.Row { gap = 8, align = "center",
    widgets.tumbler { id = "sample-tumbler-hours", accessible_name = "Hours", items = hours, item_width = 72,
      item_height = 36, current = hour, on_current_changed = set_hour },
    kit.text and kit.text { text = ":", font_size = 22, font_weight = 700 } or ui.Text { text = ":", font_size = 22 },
    widgets.tumbler { id = "sample-tumbler-minutes", accessible_name = "Minutes", items = minutes, item_width = 72,
      item_height = 36, current = minute, on_current_changed = set_minute } }
end

return S
