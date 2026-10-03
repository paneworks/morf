-- Settings: an example application on the widget kit (PLAN Stage 16).
--
--     morf app examples/apps/settings/app.lua
--
-- A window with a header bar, a sidebar of sections and their pages of
-- preference rows, an alert dialog and an about dialog -- no theme of its
-- own: it draws with whatever `kit` is installed, else the default look.
-- Wide, the sidebar stands beside the pages; narrow (a phone), it becomes a
-- drawer the header's button and F9 open. Every control is reachable by
-- Tab and named for a screen reader; F6 moves between sidebar and page.
local morf = require("morf")
local ui = require("morf.ui")
local app = require("lib.kit.app")
local kit = app.kit()
local composites = require("lib.kit.composites")
local rows = require("lib.kit.composites.rows")
local shell = require("lib.kit.shell")
local navigation = require("lib.kit.navigation")
local popup = require("lib.kit.popup")
local widgets = require("lib.kit.widgets")

-- What the settings hold: in a real application, a settings store.
local state = morf.state {
  page = "appearance", dark = false, reduce_motion = false, scale = 2, font_size = 11,
  wifi = true, airplane = false, hostname = "workstation",
  volume = 70, mute = false, output = 1,
  dim = true, suspend = 3, battery_saver = false,
}

local SECTIONS = {
  { title = "Personal", items = {
    { key = "appearance", label = "Appearance", icon = "palette" },
    { key = "sound", label = "Sound", icon = "volume_up" },
  } },
  { title = "System", items = {
    { key = "network", label = "Network", icon = "wifi" },
    { key = "power", label = "Power", icon = "battery_charging_full" },
    { key = "about", label = "About", icon = "info" },
  } },
}
local NAMES = {}
for _, section in ipairs(SECTIONS) do for _, item in ipairs(section.items) do NAMES[item.key] = item.label end end

local about_dialog, reset_dialog

-- The page area's size, as the window and the layout leave it: a signal
-- so pages built before the layout follow it once it is there.
local area = morf.state { width = 980, height = 600 }

-- Each page: a scrolled column of preference groups, as wide as the page
-- allows and never wider than reads well, centred. Its rows are rebuilt
-- when the width moves by a step (a resize, the sidebar folding), not on
-- every pixel.
local function page(groups)
  return function()
    local holder = ui.Item { width = function() return area.width end, height = function() return area.height end }
    local built, built_w
    local function inner() return math.max(280, math.min(640, area.width - 48)) end
    morf.effect("settings.page." .. tostring(holder), function()
      local w = math.floor(inner() / 16) * 16
      local h = area.height - 24
      if w == built_w and built then return end
      if built then ui.destroy(built, true) end
      built_w = w
      local node = rows.preferences_page { width = w, height = h, groups = groups }
      built = ui.Item { y = 12, x = function() return math.max(0, (area.width - w) / 2) end,
        height = function() return area.height - 24 end, width = w, node }
      ui.reparent(built, holder)
    end, { owner = holder })
    return holder
  end
end

local PAGES = {
  appearance = page {
    { title = "Style", rows = {
      { kind = "switch_row", id = "dark", title = "Dark style", subtitle = "Follow it at night",
        active = function() return state.dark end, on_toggled = function(on) state.dark = on end },
      { kind = "combo_row", id = "scale", title = "Interface scale", items = { "75%", "100%", "125%", "150%" },
        current = function() return state.scale end, on_changed = function(i) state.scale = i end },
      { kind = "spin_row", id = "font-size", title = "Text size", value = function() return state.font_size end,
        from = 8, to = 24, on_changed = function(v) state.font_size = v end },
    } },
    { title = "Accessibility", rows = {
      { kind = "check_row", id = "reduce-motion", title = "Reduce motion",
        active = function() return state.reduce_motion end, on_toggled = function(on) state.reduce_motion = on end },
    } },
  },
  sound = page {
    { title = "Output", rows = {
      { kind = "spin_row", id = "volume", title = "Volume", value = function() return state.volume end,
        from = 0, to = 100, step = 5, on_changed = function(v) state.volume = v end },
      { kind = "switch_row", id = "mute", title = "Mute", active = function() return state.mute end,
        on_toggled = function(on) state.mute = on end },
      { kind = "combo_row", id = "output", title = "Device", items = { "Speakers", "Headphones", "HDMI" },
        current = function() return state.output end, on_changed = function(i) state.output = i end },
    } },
  },
  network = page {
    { title = "Wireless", rows = {
      { kind = "switch_row", id = "wifi", title = "Wi-Fi", active = function() return state.wifi end,
        on_toggled = function(on) state.wifi = on end },
      { kind = "switch_row", id = "airplane", title = "Airplane mode", subtitle = "Turn off every radio",
        active = function() return state.airplane end, on_toggled = function(on) state.airplane = on end },
    } },
    { title = "This computer", rows = {
      { kind = "entry_row", id = "hostname", title = "Device name", text = function() return state.hostname end,
        on_accepted = function(text) state.hostname = text end },
    } },
  },
  power = page {
    { title = "Saving", rows = {
      { kind = "switch_row", id = "dim", title = "Dim the screen", active = function() return state.dim end,
        on_toggled = function(on) state.dim = on end },
      { kind = "combo_row", id = "suspend", title = "Suspend after",
        items = { "5 minutes", "15 minutes", "30 minutes", "Never" },
        current = function() return state.suspend end, on_changed = function(i) state.suspend = i end },
      { kind = "switch_row", id = "battery-saver", title = "Battery saver",
        active = function() return state.battery_saver end, on_toggled = function(on) state.battery_saver = on end },
    } },
    { rows = {
      { kind = "button_row", id = "reset", title = "Reset to defaults…", on_activated = function() reset_dialog.open() end },
    } },
  },
  about = page {
    { title = "This system", rows = {
      { kind = "property_row", id = "os", title = "Operating system", value = "morf example" },
      { kind = "property_row", id = "device", title = "Device name", value = function() return state.hostname end },
      { kind = "action_row", id = "about-app", title = "About Settings", on_activated = function() about_dialog.open() end },
    } },
  },
}

app.application {
  title = "Settings", app_id = "dev.morf.Settings",
  width = 980, height = 660, minimum_width = 340, minimum_height = 480,
  build = function(win)
    local function W() return win.width end
    local function H() return win.height end
    local content = ui.Item { width = function() return W() end, height = function() return H() - 46 end }
    local nav_node, nav
    local layout_node, layout
    local toggle = widgets.icon { id = "settings-show-sections", width = 32, height = 32,
      icon_on = "menu", icon_off = "menu", on = function() return false end, accessible_name = "Show sections",
      on_clicked = function() layout.toggle_sidebar() end }
    local header = composites.header_bar { id = "settings-header", width = W, window = win,
      title = "Settings", subtitle = function() return NAMES[state.page] end, start = { toggle } }
    local side = composites.sidebar { id = "settings-sidebar", width = 240,
      height = function() return H() - 46 end, sections = SECTIONS,
      current = function() return state.page end,
      on_changed = function(key)
        state.page = key
        if nav then nav.go(key) end
        -- On a phone the drawer gives the page back once a section is chosen.
        if layout and layout.t.collapsed then layout.dismiss() end
      end }
    local builders = {}
    for key, make in pairs(PAGES) do builders[key] = make end
    nav_node, nav = navigation.make("view_stack", { id = "settings-pages", mode = "switcher",
      width = function() return area.width end, height = function() return area.height end,
      current = "appearance", order = { "appearance", "sound", "network", "power", "about" }, pages = builders })
    layout_node, layout = shell.make("window_layout", { id = "settings-layout", width = W, height = H,
      header_bar = header, header_height = 46, sidebar = side, sidebar_width = 240, content = nav_node,
      breakpoints = { 640, 960 }, title = "Settings", sidebar_name = "Sections", content_name = "Settings page" })
    -- The page area follows the window and the sidebar folding.
    morf.effect("settings.area", function()
      area.width = W() - (layout.t.collapsed and 0 or 240)
      area.height = H() - 46
    end, { owner = layout_node })
    -- Shown only while the sections are a drawer (bound now the layout is there).
    toggle.visible = function() return layout.t.collapsed end
    -- The dialogs: the reset confirmation and the about box, over the window.
    reset_dialog = popup.make("dialog", { id = "settings-reset", root = layout_node, width = 380,
      title = "Reset every setting?", body = "Your choices on every page go back to how they were when the system was new.",
      buttons = {
        { label = "Cancel", id = "settings-reset-cancel" },
        { label = "Reset", id = "settings-reset-confirm", destructive = true, on_clicked = function()
          state.dark, state.scale, state.font_size, state.reduce_motion = false, 2, 11, false
          state.wifi, state.airplane, state.volume, state.mute, state.dim, state.suspend = true, false, 70, false, true, 3
          state.battery_saver = false
        end },
      } })
    local about_node
    about_node, about_dialog = composites.about_dialog { id = "settings-about", root = layout_node,
      app_name = "Settings", version = "1.0", icon = "settings",
      comments = "An example application on the morf widget kit.",
      developers = { "The morf authors" }, license = "MIT" }
    -- For tests and scripting: the window's size, as a compositor would set it.
    morf.ipc["settings-resize"] = function(w, h) win:size(tonumber(w), tonumber(h)) return true end
    morf.ipc["settings-collapsed"] = function() return layout.t.collapsed end
    morf.ipc["settings-drawer"] = function() return layout.t.sidebar_open end
    morf.ipc["settings-size"] = function() return { w = win.width, h = win.height, shell = layout.t.width, layout = layout.t.layout } end
    return layout_node
  end,
}
-- Exposed for tests and scripting.
morf.ipc["settings-state"] = function()
  return { page = state.page, dark = state.dark, wifi = state.wifi, scale = state.scale, volume = state.volume,
    font_size = state.font_size }
end
