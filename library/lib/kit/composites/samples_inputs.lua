-- Gallery samples for composites (inputs): name -> function(kit, composites)
-- returning a node that fits a 520x340 area
-- (examples/shells/caelestia/tests/composites_gallery_spec.lua).
local ui = require("morf.ui")

local function caption(kit, text, y)
  return kit.label { text = text, y = y, width = 240, elide = "right" }
end

return {
  combo_box = function(kit, composites)
    local sizes = composites.combo_box { id = "sample-combo", y = 24, width = 240,
      items = { "Small", "Medium", "Large", "Extra large" }, current = 2 }
    local fruit = composites.combo_box { id = "sample-combo-search", x = 270, y = 24, width = 240, search = true,
      items = { "Apple", "Apricot", "Banana", "Cherry", "Date", "Fig", "Grape" }, current = 4 }
    local plain = composites.combo_box { id = "sample-combo-select", y = 110, width = 240, variant = "select",
      items = { "Automatic", "Always", "Never" }, current = 1, icon = "tune" }
    local empty = composites.combo_box { id = "sample-combo-empty", x = 270, y = 110, width = 240,
      items = { "Wi-Fi", "Ethernet" }, placeholder = "Choose a network…" }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Dropdown", 0), sizes,
      ui.Item { x = 270, width = 240, height = 20, caption(kit, "Searchable", 0) }, fruit,
      caption(kit, "Select", 86), plain,
      ui.Item { x = 270, y = 86, width = 240, height = 20, caption(kit, "Placeholder", 0) }, empty }
  end,

  menu_button = function(kit, composites)
    local items = { { label = "New file", icon = "add" }, { label = "Open…", icon = "folder_open" },
      { label = "Word wrap", checked = true } }
    local file = composites.menu_button { id = "sample-menu", y = 24, width = 160, label = "File", items = items }
    local split = composites.menu_button { id = "sample-split", x = 270, y = 24, width = 180, label = "Save",
      icon = "save", split = true, items = { { label = "Save as…" }, { label = "Save all" } } }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Menu button", 0), file,
      ui.Item { x = 270, width = 240, height = 20, caption(kit, "Split button", 0) }, split }
  end,

  picker = function(kit, composites)
    local icons = {}
    for i, name in ipairs { "home", "star", "favorite", "bolt", "cloud", "music_note", "pets", "eco", "palette",
      "rocket_launch" } do icons[i] = { icon = name, label = name:gsub("_", " ") } end
    local icon = composites.picker { id = "sample-picker", y = 24, width = 220, items = icons, current = 2, columns = 5 }
    local list = composites.picker { id = "sample-picker-list", x = 270, y = 24, width = 240, layout = "list",
      items = { "Left", "Centre", "Right", "Justify" }, current = 1 }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Icon", 0), icon,
      ui.Item { x = 270, width = 240, height = 20, caption(kit, "Alignment", 0) }, list }
  end,

  search_bar = function(kit, composites)
    local bar = composites.search_bar { id = "sample-search", width = 500, title = "Files", toggle = true,
      revealed = true, placeholder = "Search files" }
    local hidden = composites.search_bar { id = "sample-search-hidden", y = 140, width = 500, title = "Ctrl+F reveals",
      toggle = true, scope = "local" }
    return ui.Item { width = 520, height = 340, bar, hidden }
  end,

  tag_input = function(kit, composites)
    local tags = composites.tag_input { id = "sample-tags", y = 24, width = 500, tags = { "lua", "rust", "morf", "ui" },
      placeholder = "Add a tag" }
    return ui.Item { width = 520, height = 340, caption(kit, "Tags -- Return or a comma adds", 0), tags }
  end,

  input_group = function(kit, composites)
    local url = composites.input_group { id = "sample-url", y = 24, width = 500, prefix = "https://",
      suffix = { label = "Go", on_clicked = function() end }, placeholder = "example.org" }
    local count = composites.input_group { id = "sample-count", y = 110, width = 200, widget = "numeric_entry",
      text = "3", horizontal_alignment = "center",
      prefix = { icon = "remove", on_clicked = function() end }, suffix = { icon = "add", on_clicked = function() end } }
    local mail = composites.input_group { id = "sample-mail", x = 230, y = 110, width = 270, icon = "mail",
      suffix = "@morf.dev", placeholder = "name" }
    return ui.Item { width = 520, height = 340,
      caption(kit, "Address", 0), url, caption(kit, "Stepper", 86), count,
      ui.Item { x = 230, y = 86, width = 240, height = 20, caption(kit, "Mail", 0) }, mail }
  end,

  rows = function(kit, composites)
    local rows = require("lib.kit.composites.rows")
    return rows.preferences_page { id = "sample-prefs", width = 520, height = 340, groups = {
      { title = "Appearance", description = "How the shell looks", rows = {
        { kind = "switch_row", id = "sample-row-dark", title = "Dark mode", subtitle = "Follow the wallpaper", active = true },
        { kind = "combo_row", id = "sample-row-scale", title = "Scale", items = { "100%", "125%", "150%" }, current = 1 },
        { kind = "spin_row", id = "sample-row-size", title = "Font size", value = 11, from = 6, to = 48 },
        { kind = "check_row", id = "sample-row-blur", title = "Blur behind panels" },
      } },
      { title = "Account", rows = {
        { kind = "entry_row", id = "sample-row-name", title = "Name", text = "Ada" },
        { kind = "property_row", id = "sample-row-host", title = "Host", value = "morf.local" },
        { kind = "action_row", id = "sample-row-about", title = "About", icon = "info", on_activated = function() end },
        { kind = "expander_row", id = "sample-row-more", title = "Advanced", rows = {
          { kind = "switch_row", title = "Debug overlay" } } },
        { kind = "button_row", id = "sample-row-reset", title = "Reset to defaults", on_activated = function() end },
      } },
    } }
  end,

  media_controls = function(kit, composites)
    local state = morf.signal("kit.samples.media", { playing = true, position = 83, volume = 0.6 })
    return composites.media_controls { id = "sample-media", y = 20, width = 500,
      playing = function() return state:get().playing end,
      position = function() return state:get().position end, length = 214,
      volume = function() return state:get().volume end,
      on_play_pause = function(on) local s = state:get() state:set { playing = on, position = s.position, volume = s.volume } end,
      on_volume = function(v) local s = state:get() state:set { playing = s.playing, position = s.position, volume = v } end }
  end,
}
