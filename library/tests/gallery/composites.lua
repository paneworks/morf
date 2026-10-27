-- Gallery samples for the composites: each as an application has it --
-- rows in their boxed list, choosers inline, a dialog behind the button
-- that opens it.
local ui = require("morf.ui")

local M = { span = { file_chooser = { 2, 2 }, color_chooser = { 1, 2 }, emoji_chooser = { 2, 2 },
  font_chooser = { 2, 2 }, preferences_page = { 2, 2 }, calendar = { 1, 2 }, sidebar = { 1, 2 },
  shortcuts_dialog = { 2, 2 }, media_controls = { 2, 1 }, tab_overview = { 2, 1 } } }

local function c() return require("lib.kit.composites") end

-- A row in a boxed list of its own, as a group holds it.
local function boxed(w, node) return w.box { variant = "boxed_list", width = 280, node } end

local function opener(w, label, open)
  local node
  node = w.button { label = label, width = 170, on_clicked = function() open(node) end }
  return node
end

function M.action_row(_, w)
  return ui.Column { gap = 12,
    boxed(w, (c().action_row { width = 280, title = "About", subtitle = "Version 0.6", icon = "info",
      on_activated = function() end })),
    boxed(w, (c().action_row { width = 280, title = "Model", subtitle = "ThinkPad X1", variant = "property" })) }
end
function M.switch_row(_, w) return boxed(w, (c().switch_row { width = 280, title = "Dark style", active = true })) end
function M.combo_row(_, w)
  return boxed(w, (c().combo_row { width = 280, title = "Scale", items = { "100%", "125%", "150%" }, selected = 2 }))
end
function M.entry_row(_, w) return boxed(w, (c().entry_row { width = 280, title = "Name", text = "Ada" })) end
function M.password_entry_row(_, w)
  return boxed(w, (c().password_entry_row { width = 280, title = "Password", text = "hunter2" }))
end
function M.spin_row(_, w)
  return boxed(w, (c().spin_row { width = 280, title = "Font size", value = 11, from = 6, to = 48 }))
end
function M.expander_row(_, w)
  return boxed(w, (c().expander_row { width = 280, title = "Proxy", subtitle = "Automatic", expanded = true,
    rows = { { kind = "action_row", title = "Host", subtitle = "proxy.local" } } }))
end
function M.button_row(_, w)
  return boxed(w, (c().button_row { width = 280, title = "Add account", start_icon = "add", on_activated = function() end }))
end
function M.preferences_group(_, w)
  return (c().preferences_group { width = 280, title = "Display", description = "How it looks",
    rows = { { kind = "switch_row", title = "Night light" }, { kind = "action_row", title = "Resolution", subtitle = "2560 × 1440" } } })
end
function M.preferences_page(_, w)
  return (c().preferences_page { width = 600, height = 480, groups = {
    { title = "Appearance", rows = { { kind = "switch_row", title = "Dark style", active = true },
      { kind = "combo_row", title = "Accent", items = { "Blue", "Teal", "Pink" }, selected = 1 } } },
    { title = "Behaviour", rows = { { kind = "spin_row", title = "Delay", value = 3, from = 0, to = 10 } } } } })
end
function M.preferences_dialog(_, w)
  local _, prefs = c().preferences_dialog { title = "Preferences", pages = {
    { name = "General", icon = "tune", groups = { { title = "Look", rows = { { kind = "switch_row", title = "Dark style" } } } } },
    { name = "Sound", icon = "volume_up", groups = { { title = "Output", rows = { { kind = "action_row", title = "Speakers" } } } } } } }
  return opener(w, "Preferences…", function() prefs.open() end)
end
function M.about_dialog(_, w)
  local _, about = c().about_dialog { app_name = "Morf", version = "0.6.0", comments = "A UI engine for shells and apps.",
    developers = { "Ada", "Linus" }, license = "MIT" }
  return opener(w, "About…", function() about.open() end)
end
function M.shortcuts_dialog(_, w)
  return (c().shortcuts_dialog { inline = true, width = 600, height = 480, groups = {
    { title = "General", shortcuts = { { keys = "Ctrl+Q", description = "Quit" }, { keys = "Ctrl+,", description = "Preferences" } } },
    { title = "Editing", shortcuts = { { keys = "Ctrl+Z", description = "Undo" }, { keys = "Ctrl+Shift+Z", description = "Redo" } } } } })
end
function M.calendar(_, w) return (c().calendar { width = 280, value = "2026-10-27" }) end
function M.color_chooser(_, w) return (c().color_chooser { width = 280, height = 460, value = "#3584e4" }) end
function M.drop_down(_, w)
  return ui.Column { gap = 12, (c().drop_down { width = 200, items = { "Small", "Medium", "Large" }, selected = 2 }),
    (c().drop_down { width = 200, items = { "Amsterdam", "Berlin", "Lisbon" }, search = true, selected = 1 }) }
end
function M.emoji_chooser(_, w) return (c().emoji_chooser { width = 560, height = 440 }) end
function M.file_chooser(_, w) return (c().file_chooser { width = 600, height = 480, root = "/", path = "/" }) end
function M.font_chooser(_, w)
  return (c().font_chooser { width = 560, height = 440, fonts = { "Adwaita Sans", "Cantarell", "Inter", "monospace" } })
end
function M.media_controls(_, w)
  return (c().media_controls { width = 560, playing = true, position = 83, length = 214, volume = 0.6,
    on_volume = function() end })
end
function M.menu_button(_, w)
  return (c().menu_button { label = "File", width = 140, items = { { label = "New" }, { label = "Open…" } } })
end
function M.split_button(_, w)
  return (c().split_button { label = "Save", width = 150, variant = "suggested", items = { { label = "Save As…" } } })
end
function M.search_bar(_, w) return (c().search_bar { width = 280, title = "Files", toggle = true, revealed = true }) end
function M.sidebar(_, w)
  return (c().sidebar { width = 240, height = 460, title = "Mail", sections = {
    { title = "Library", items = { { key = "inbox", label = "Inbox", icon = "inbox", badge = 4 },
      { key = "sent", label = "Sent", icon = "send" } } },
    { title = "Labels", items = { { key = "work", label = "Work", icon = "label" } } } } })
end
function M.banner(_, w)
  return (c().banner { width = 280, title = "You are offline", button_label = "Retry", revealed = true })
end
function M.toast_overlay(_, w)
  local holder = ui.Item { width = 280, height = 220 }
  local _, toasts = c().toast_overlay { root = holder }
  ui.reparent(opener(w, "Copy", function() toasts.notify { title = "Copied", actions = { { label = "Undo" } } } end), holder)
  return holder
end
function M.tab_bar(_, w)
  local pages, view = w.tab_view { width = 280, height = 160, tabs = { { title = "Notes" }, { title = "Todo" } },
    on_add = function(n) return { title = "Tab " .. n } end }
  pages.y = 40
  return ui.Item { width = 280, height = 200, (c().tab_bar { view = view, width = 280 }), pages }
end
function M.tab_overview(_, w)
  local pages, view = w.tab_view { width = 560, height = 200,
    tabs = { { title = "Notes", icon = "description" }, { title = "Todo", icon = "check_box" }, { title = "Mail", icon = "mail" } } }
  local overview, handle = c().tab_overview { view = view, width = 560, height = 200 }
  handle.open()
  return ui.Item { width = 560, height = 200, pages, overview }
end

return M
