-- Gallery samples for the Roving archetype's widgets (samples/init.lua):
-- each a group as a configuration would make it -- a text editor's format
-- toolbar, an application's menu bar, a linked alignment group, filter
-- chips, a bar of icons -- one Tab stop the arrows walk.
local S = {}

local function state(name, value) return morf.signal("kit.samples.roving." .. name, value) end

-- A toggle kept in a signal.
local function toggle(name, first)
  local sig = state(name, first)
  return function() return sig:get() end, function(on) sig:set(on) end
end

function S.toolbar_group(_, w)
  local bold, set_bold = toggle("bold", true)
  local italic, set_italic = toggle("italic", false)
  return (w.toolbar_group { id = "sample-toolbar-group", accessible_name = "Format", items = {
    { icon = "format_bold", tooltip = "Bold", checked = bold, on_toggled = set_bold },
    { icon = "format_italic", tooltip = "Italic", checked = italic, on_toggled = set_italic },
    { icon = "format_underlined", tooltip = "Underline", enabled = false },
    { separator = true },
    { icon = "link", tooltip = "Link" },
    { icon = "image", tooltip = "Image" },
  } })
end

function S.menubar(_, w)
  return (w.menubar { id = "sample-menubar", items = {
    { label = "File", items = { { label = "New", icon = "add" }, { label = "Open…", icon = "folder_open" },
      { label = "Save", icon = "save" } } },
    { label = "Edit", items = { { label = "Undo", icon = "undo" }, { label = "Redo", icon = "redo" } } },
    { label = "View", items = { { label = "Zoom in", icon = "zoom_in" }, { label = "Zoom out", icon = "zoom_out" } } },
    { label = "Help", items = { { label = "About", icon = "info" } } },
  } })
end

function S.button_group(_, w)
  local range = state("range", 2)
  local function member(i, label)
    return { label = label, checked = function() return range:get() == i end, on_toggled = function() range:set(i) end }
  end
  return (w.button_group { id = "sample-button-group", accessible_name = "Range", items = {
    member(1, "Day"), member(2, "Week"), member(3, "Month") } })
end

function S.chip_row(_, w)
  local function chip(name, label, on)
    local get, set = toggle("chip." .. name, on)
    return { label = label, checked = get, on_toggled = set }
  end
  return (w.chip_row { id = "sample-chip-row", accessible_name = "Filters", items = {
    chip("music", "Music", true), chip("video", "Video", false), chip("photos", "Photos", false) } })
end

function S.icon_bar(_, w)
  return (w.icon_bar { id = "sample-icon-bar", accessible_name = "Places", items = {
    { icon = "home", tooltip = "Home" }, { icon = "search", tooltip = "Search" },
    { icon = "notifications", tooltip = "Notifications" }, { icon = "settings", tooltip = "Settings" },
    { icon = "person", tooltip = "Account" } } })
end

return S
