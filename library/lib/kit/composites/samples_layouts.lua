-- Gallery samples for composites (layouts): name -> function(kit, composites)
-- returning a node that fits a 520x340 area
-- (examples/shells/caelestia/tests/composites_gallery_spec.lua).
local ui = require("morf.ui")

local function page(kit, title, body)
  return function(w)
    return ui.Item { width = w, height = 200,
      kit.heading { x = 16, y = 18, text = title },
      kit.text { x = 16, y = 56, width = w - 32, wrap = true, text = body, color = kit.ink("lo") } }
  end
end

return {
  tab_view = function(kit, composites)
    return (composites.tab_view { id = "sample-tabs", width = 520, height = 340, tab_width = 128,
      tabs = {
        { title = "Notes", icon = "description", content = page(kit, "Notes", "Three tabs, each a page of its own. Drag a tab along the strip to move it; the grid press shows them all.") },
        { title = "Mail", icon = "mail", content = page(kit, "Mail", "Nothing new.") },
        { title = "Music", icon = "music_note", content = page(kit, "Music", "Queue empty.") },
      } })
  end,

  sidebar = function(kit, composites)
    local sections = {
      { title = "Library", items = { { key = "inbox", label = "Inbox", icon = "inbox", badge = 4 },
        { key = "starred", label = "Starred", icon = "star" }, { key = "sent", label = "Sent", icon = "send" } } },
      { title = "Labels", items = { { key = "work", label = "Work", icon = "work", badge = "new" },
        { key = "home", label = "Home", icon = "home" } } },
      { title = "Archive", expanded = false, items = { { key = "2025", label = "2025", icon = "inventory_2" } } },
    }
    return ui.Item { width = 520, height = 340,
      (composites.sidebar { id = "sample-sidebar", width = 240, height = 340, title = "Mail", sections = sections,
        current = "inbox" }),
      ui.Item { x = 270, (composites.sidebar { id = "sample-sidebar-rail", width = 240, height = 340,
        sections = sections, current = "work", collapsed = true }) } }
  end,

  carousel = function(kit, composites)
    return (composites.carousel { id = "sample-carousel", width = 520, height = 340, current = 2,
      slides = {
        { title = "Welcome", subtitle = "Swipe, use the arrows or press a dot.", icon = "waving_hand" },
        { title = "Stay in sync", subtitle = "Your settings follow you to every device.", icon = "sync" },
        { title = "All set", subtitle = "Enjoy.", icon = "check_circle", kind = "ok" },
      } })
  end,

  wizard = function(kit, composites)
    local widgets = require("lib.kit.widgets")
    -- The entry in the theme's face.
    local probe = { text = "" }
    ui.destroy(kit.text(probe), true)
    return (composites.wizard { id = "sample-wizard", width = 520, height = 340, current = 1,
      on_cancel = function() end,
      steps = {
        { title = "Account", content = function(w)
            return ui.Item { width = w, height = 160,
              kit.label { x = 20, y = 24, text = "Name" },
              kit.field { x = 20, y = 48, width = 280, height = 40,
                (widgets.entry { id = "sample-wizard-name", x = 6, width = 268, height = 40,
                  inset = { 4, 0, 4, 0 }, placeholder = "Ada Lovelace", vertical_alignment = "center",
                  color = kit.ink("hi"), placeholder_color = kit.ink("lo"), caret_color = kit.signal("accent"),
                  font_family = probe.font_family, font_source = probe.font_source, font_size = probe.font_size }) } }
          end },
        { title = "Theme" }, { title = "Finish" },
      } })
  end,

  transfer_list = function(_, composites)
    return (composites.transfer_list { id = "sample-transfer", width = 520, height = 300,
      titles = { "Available", "Shown" },
      items = { "Name", "Size", "Type", "Modified", "Owner", "Permissions", "Location" },
      chosen = { "Name", "Size", "Modified" } })
  end,

  kanban = function(_, composites)
    return (composites.kanban { id = "sample-kanban", width = 520, height = 340,
      columns = {
        { key = "todo", title = "To do", cards = { { key = "a", title = "Write docs", tag = "docs" },
          { key = "b", title = "Fix the swipe", tag = "core" }, { key = "c", title = "Icons", tag = "design" } } },
        { key = "doing", title = "Doing", cards = { { key = "d", title = "Review PR", tag = "core" } } },
        { key = "done", title = "Done", cards = { { key = "e", title = "Release 0.4", tag = "ops" } } },
      } })
  end,

  dashboard = function(_, composites)
    return (composites.dashboard { id = "sample-dashboard", width = 520, height = 340, title = "Today",
      tiles = {
        { key = "cpu", title = "CPU", icon = "memory", value = "12%", level = 0.12 },
        { key = "ram", title = "Memory", icon = "memory_alt", value = "4.1 GB", level = 0.52 },
        { key = "net", title = "Network", icon = "wifi", value = "48 Mb/s" },
        { key = "disk", title = "Disk", icon = "hard_drive", value = "61%", level = 0.61 },
        { key = "bat", title = "Battery", icon = "battery_full", value = "88%", level = 0.88 },
        { key = "temp", title = "Temp", icon = "thermostat", value = "54°" },
      } })
  end,

  file_chooser = function(_, composites)
    local root = morf.fs.is_dir("/usr/share") and "/usr/share" or "/"
    return (composites.file_chooser { id = "sample-files", width = 520, height = 340, root = root, path = root,
      mode = "open", filters = { { name = "All files" }, { name = "Images", patterns = { "png", "svg", "jpg" } } },
      places = { { label = "Shared", path = root, icon = "folder_shared" } } })
  end,
}
