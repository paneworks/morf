-- Gallery samples for composites (surfaces): name -> function(kit, composites)
-- returning a node that fits a 520x340 area
-- (examples/shells/caelestia/tests/composites_gallery_spec.lua). The popup
-- ones are shown opened, inline.
local ui = require("morf.ui")

local function caption(kit, text, y, x)
  return kit.label { text = text, x = x, y = y, width = 240, elide = "right" }
end

local COMMANDS = {
  { title = "Open file…", subtitle = "Browse the disk", icon = "folder_open", shortcut = "Ctrl+O", group = "File" },
  { title = "Save", icon = "save", shortcut = "Ctrl+S", group = "File" },
  { title = "Toggle sidebar", icon = "side_navigation", shortcut = "Ctrl+B", group = "View" },
  { title = "Zoom in", icon = "zoom_in", shortcut = "Ctrl++", group = "View" },
  { title = "Zoom out", icon = "zoom_out", shortcut = "Ctrl+-", group = "View" },
  { title = "Preferences", subtitle = "Theme, fonts and keys", icon = "settings", group = "Application" },
}

return {
  command_palette = function(kit, composites)
    return (composites.command_palette { id = "sample-palette", inline = true, width = 500, list_height = 240,
      commands = COMMANDS })
  end,

  notification_stack = function(kit, composites)
    local node = composites.notification_stack { id = "sample-notes", inline = true, width = 500, max_visible = 3,
      timeout = 0, notifications = {
        { title = "Battery low", body = "12% left -- plug in soon.", icon = "battery_alert", app = "Power",
          time = "now", urgency = "critical" },
        { title = "Ada Lovelace", body = "Are we still on for the review this afternoon?", icon = "chat",
          app = "Messages", time = "2 min" },
        { title = "Update ready", body = "Restart to finish installing morf 0.15.", icon = "system_update",
          app = "Software", time = "1 h", actions = { { label = "Restart" } } },
      } }
    -- (The first made is drawn last: the newest on top.)
    return ui.Item { width = 520, height = 340, node }
  end,

  tour = function(kit, composites)
    local target = kit.card { x = 20, y = 18, width = 150, height = 40 }
    local label = kit.text { x = 34, y = 28, text = "Search" }
    local ring = ui.Item { x = 14, y = 12, width = 162, height = 52,
      kit.surface { anchors = { fill = true }, radius = kit.round and kit.round(14) or 14, color = "transparent",
        border_width = 2, border_color = kit.signal("accent") } }
    local card = composites.tour { id = "sample-tour", inline = true, x = 20, y = 80, width = 320, steps = {
      { title = "Search from anywhere", body = "Type to find apps, files and commands. Ctrl+K opens it." },
      { title = "Pin what you use", body = "Drag an app to the rail to keep it there." },
      { title = "Make it yours", body = "Themes, fonts and keys live in Preferences." },
    } }
    return ui.Item { width = 520, height = 340, target, label, ring, card }
  end,

  about_dialog = function(kit, composites)
    return (composites.about_dialog { id = "sample-about", inline = true, width = 500, height = 330,
      app_name = "Morf", version = "0.15", icon = "deployed_code",
      comments = "A reactive UI engine for desktop shells and applications.",
      links = { { label = "Website", url = "https://morf.dev", icon = "language" },
        { label = "Issues", url = "https://morf.dev/issues", icon = "bug_report" } },
      developers = { "Trim Bresilla" }, credits = { { title = "Thanks", names = { "The caelestia authors" } } },
      copyright = "© 2026 The Morf authors", license = "MIT",
      legal = "This program comes with absolutely no warranty." })
  end,

  shortcuts_window = function(kit, composites)
    return (composites.shortcuts_window { id = "sample-keys", inline = true, width = 500, height = 330, groups = {
      { title = "General", shortcuts = {
        { keys = "Ctrl+K", description = "Command palette" },
        { keys = "Ctrl+,", description = "Preferences" },
        { keys = { "F1", "Ctrl+?" }, description = "Keyboard shortcuts" } } },
      { title = "Editing", shortcuts = {
        { keys = "Ctrl+Z", description = "Undo" },
        { keys = "Ctrl+Shift+Z", description = "Redo" },
        { keys = "Ctrl+K Ctrl+S", description = "Save all" } } },
    } })
  end,

  toolbar = function(kit, composites)
    local function items()
      return {
        { icon = "add", label = "New" },
        { icon = "folder_open", label = "Open" },
        { separator = true },
        { icon = "format_bold", tooltip = "Bold", checked = true },
        { icon = "format_italic", tooltip = "Italic", checked = false },
        { icon = "format_underlined", tooltip = "Underline", checked = false },
        { separator = true },
        { icon = "share", tooltip = "Share" },
        { icon = "print", tooltip = "Print" },
      }
    end
    local wide = composites.toolbar { id = "sample-toolbar", y = 24, width = 500, items = items() }
    local narrow = composites.toolbar { id = "sample-toolbar-narrow", y = 110, width = 260, items = items() }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Toolbar", 0), wide,
      caption(kit, "Narrow: the rest overflow", 86), narrow }
  end,

  status_bar = function(kit, composites)
    local bar = composites.status_bar { id = "sample-status", y = 24, width = 500,
      left = { { icon = "check_circle", text = "Ready", tone = "ok" }, { icon = "commit", text = "main", on_clicked = function() end } },
      center = { { kind = "progress", value = 0.62, width = 110 } },
      right = { { text = "Ln 12, Col 4", on_clicked = function() end }, { kind = "badge", count = 3 },
        { icon = "notifications", tooltip = "Notifications", on_clicked = function() end } } }
    local quiet = composites.status_bar { id = "sample-status-quiet", y = 110, width = 500, ground = false,
      left = { { kind = "dot", tone = "ok" }, { kind = "label", text = "Connected" } },
      right = { { kind = "label", text = "UTF-8" }, { kind = "separator" }, { kind = "label", text = "Lua" } } }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Status bar", 0), bar,
      caption(kit, "Without its ground", 86), quiet }
  end,
  header_bar = function(kit, composites)
    local widgets = require("lib.kit.widgets")
    local ui = require("morf.ui")
    local back = widgets.icon { id = "gallery-header-back", width = 32, height = 32, icon_on = "arrow_back",
      icon_off = "arrow_back", on = function() return false end, accessible_name = "Back" }
    local menu = widgets.icon { id = "gallery-header-menu", width = 32, height = 32, icon_on = "menu",
      icon_off = "menu", on = function() return false end, accessible_name = "Menu" }
    local node = composites.header_bar { id = "gallery-header", width = 520, title = "Documents",
      subtitle = "12 items", start = { back }, ["end"] = { menu } }
    return ui.Item { width = 520, height = 60, node }
  end,
}
