-- Gallery samples for the Shell archetype: each widget as a miniature
-- application window -- a header, a sidebar, a page, an inspector, bars --
-- arranged by lib.kit.shell for the size its cell gives it. The parts are
-- kit widgets and text; the window's grounds and edges are the theme's
-- Shell skin. (Format: library/lib/kit/samples/init.lua.)
local ui = require("morf.ui")

local M = {}

M.span = {
  window_layout = { 2, 2 }, split_view = { 2, 1 }, navigation_split_view = { 2, 1 }, multi_pane = { 2, 1 },
  breakpoint_bin = { 2, 1 }, clamp = { 2, 1 },
}

-- (The Shell's widgets are lib.kit.shell's: `make(widget, spec)` gives
-- the window's node and its handle.)
local function shell(widget, spec) return require("lib.kit.shell").make(widget, spec) end

local function composite(name) return require("lib.kit.composites")[name] end

-- A flat icon press: `icon`, `name` (what a screen reader reads).
local function icon(widgets, glyph, name, on_clicked)
  return widgets.icon { width = 32, height = 32, icon_on = glyph, icon_off = glyph, on = function() return false end,
    accessible_name = name, on_clicked = on_clicked }
end

-- A header: the composite header bar, flat (the Shell skin draws its
-- ground and rule), with the window controls left out.
local function header(widgets, w, title, starts, ends, subtitle)
  return (composite("header_bar") { width = w, height = 42, title = title, subtitle = subtitle, controls = false,
    flat = true, start = starts or {}, ["end"] = ends or {} })
end

-- A page: a heading and a few lines of body text, padded.
local function page(kit, w, title, lines)
  local column = { x = 16, y = 14, gap = 6, width = w - 32 }
  column[#column + 1] = kit.heading { text = title, level = "section", width = w - 32, elide = "right" }
  for _, line in ipairs(lines or {}) do
    column[#column + 1] = kit.text { text = line, width = w - 32, elide = "right" }
  end
  return ui.Item { width = w, height = 400, ui.Column(column) }
end

-- A sidebar: a section list (a kit sidebar_list) under an optional title.
local function sidebar(kit, widgets, w, items, title)
  local top = title and 40 or 6
  local children = { width = w, height = 400 }
  if title then
    children[#children + 1] = kit.heading { x = 14, y = 10, text = title, level = "section", width = w - 28,
      elide = "right" }
  end
  children[#children + 1] = ui.Item { x = 6, y = top, width = w - 12, height = #items * 34,
    widgets.sidebar_list { items = items, current = 1, width = w - 12, item_width = w - 12, item_height = 34 } }
  return ui.Item(children)
end

local SECTIONS = { "Inbox", "Starred", "Sent", "Archive" }

M.window_layout = function(kit, widgets)
  local W, H = 600, 480
  return (shell("window_layout", { width = W, height = H, breakpoints = { 360, 520 },
    sidebar_width = 160, inspector_width = 160, header_height = 42,
    header_bar = header(widgets, W, "Mail", { icon(widgets, "menu", "Sections") },
      { icon(widgets, "search", "Search"), icon(widgets, "more_vert", "Menu") }),
    sidebar = sidebar(kit, widgets, 160, SECTIONS),
    content = page(kit, 280, "Inbox", { "Weekly sync notes", "Your order shipped", "Build passed",
      "Lunch on Friday?" }),
    inspector = page(kit, 160, "Details", { "From Ana", "Today, 9:41", "2 attachments" }) }))
end

M.header_bar = function(kit, widgets)
  local W, H = 280, 220
  return (shell("header_bar", { width = W, height = H, header_height = 42,
    header_bar = header(widgets, W, "Inbox", { icon(widgets, "arrow_back", "Back") },
      { icon(widgets, "more_vert", "Menu") }),
    content = page(kit, W, "Today", { "Weekly sync notes", "Your order shipped", "Build passed" }) }))
end

M.toolbar_view = function(kit, widgets)
  local W, H = 280, 220
  local tools = ui.Item { width = W, height = 44,
    ui.Row { anchors = { center_in = true }, gap = 18,
      icon(widgets, "reply", "Reply"), icon(widgets, "forward", "Forward"), icon(widgets, "archive", "Archive"),
      icon(widgets, "delete", "Delete") } }
  return (shell("toolbar_view", { width = W, height = H, header_height = 42, toolbar_bottom_height = 44,
    header_bar = header(widgets, W, "Message", { icon(widgets, "arrow_back", "Back") }),
    toolbar_bottom = tools,
    content = page(kit, W, "Weekly sync", { "Notes from Monday", "and next steps." }) }))
end

M.split_view = function(kit, widgets)
  local W, H = 600, 220
  return (shell("split_view", { width = W, height = H, breakpoints = { 400, 760 }, sidebar_width = 180,
    sidebar = sidebar(kit, widgets, 180, SECTIONS, "Mail"),
    content = page(kit, 420, "Inbox", { "Weekly sync notes", "Your order shipped", "Build passed" }) }))
end

M.overlay_split_view = function(kit, widgets)
  local W, H = 280, 220
  local app
  local node
  node, app = shell("overlay_split_view", { width = W, height = H, header_height = 42, sidebar_width = 180,
    header_bar = header(widgets, W, "Notes", { icon(widgets, "menu", "Sections", function()
      if app then app.toggle_sidebar() end end) }),
    sidebar = sidebar(kit, widgets, 180, { "All notes", "Pinned", "Shared" }),
    content = page(kit, W, "Groceries", { "Oat milk, rye bread", "Lemons, basil" }) })
  -- Narrow, the sidebar is a drawer: shown open, over its scrim.
  app.toggle_sidebar()
  return node
end

M.navigation_split_view = function(kit, widgets)
  local W, H = 600, 220
  local detail = ui.Item { width = 380, height = 400,
    header(widgets, 380, "Wi-Fi", {}, { icon(widgets, "more_vert", "Menu") }),
    ui.Item { y = 42, width = 380, height = 300,
      page(kit, 380, "Home network", { "Connected, secured", "Signal excellent" }) } }
  local side = ui.Item { width = 220, height = 400,
    header(widgets, 220, "Settings"),
    ui.Item { y = 42, width = 220, height = 300, sidebar(kit, widgets, 220, { "Wi-Fi", "Bluetooth", "Display" }) } }
  return (shell("navigation_split_view", { width = W, height = H, breakpoints = { 400, 760 }, sidebar_width = 220,
    sidebar = side, content = detail }))
end

M.multi_pane = function(kit, widgets)
  local W, H = 600, 220
  return (shell("multi_pane", { width = W, height = H, breakpoints = { 300, 500 }, sidebar_width = 150,
    inspector_width = 170,
    sidebar = sidebar(kit, widgets, 150, { "Photos", "Albums", "People" }, "Library"),
    content = page(kit, 280, "Albums", { "Summer, 214 photos", "Hiking, 88 photos", "Family, 512 photos" }),
    inspector = page(kit, 170, "Summer", { "214 photos", "June to August" }) }))
end

M.breakpoint_bin = function(kit, widgets)
  local W, H = 600, 220
  -- (The bin's layout, once it is made: the page is made before it.)
  local app
  local made = morf.signal("samples.shell.breakpoint_bin", false)
  local function layout() return made:get() and app.t.layout or "" end
  local LAYOUTS = { narrow = 1, medium = 2, wide = 3 }
  local content = ui.Item { width = W, height = H,
    ui.Column { x = 16, y = 16, gap = 10,
      kit.heading { text = "Breakpoints 400 and 760", level = "section" },
      kit.text { text = function() return ("At %d px this bin is %s"):format(W, layout()) end },
      widgets.segmented { items = { "Narrow", "Medium", "Wide" }, width = 300, item_width = 100, item_height = 34,
        current = function() return LAYOUTS[layout()] or 0 end } },
    -- Widths from 0 to 1000 px: the breakpoints marked, the bin's own width a dot.
    (function()
      local L, X, Y = W - 64, 32, 168
      local function at(px) return X + px / 1000 * L end
      local ruler = ui.Item { width = W, height = H,
        ui.Rect { x = X, y = Y, width = L, height = 2, radius = 1, color = function() return kit.ink("lo")():alpha(0.35) end } }
      for _, px in ipairs { 400, 760 } do
        ui.reparent(ui.Rect { x = at(px) - 1, y = Y - 7, width = 2, height = 16, color = function() return kit.ink("lo")() end }, ruler)
        ui.reparent(kit.label { x = at(px) - 14, y = Y + 14, width = 28, text = tostring(px), horizontal_alignment = "center" }, ruler)
      end
      ui.reparent(ui.Rect { x = at(W) - 7, y = Y - 6, width = 14, height = 14, radius = 7,
        color = function() return kit.signal("accent")() end }, ruler)
      ui.reparent(kit.label { x = at(W) - 30, y = Y - 30, width = 60, text = W .. " px", horizontal_alignment = "center" }, ruler)
      return ruler
    end)() }
  local node
  node, app = shell("breakpoint_bin", { width = W, height = H, breakpoints = { 400, 760 }, content = content })
  made:set(true)
  return node
end

M.clamp = function(kit, widgets)
  local W, H = 600, 220
  local CW = 320
  local rows = { "Appearance", "Notifications", "Privacy" }
  local list = { x = 0, y = 0, width = CW, gap = 0 }
  for i, label in ipairs(rows) do
    list[#list + 1] = ui.Item { width = CW, height = 44,
      kit.text { x = 14, anchors = { vertical_center = true }, text = label },
      i < #rows and ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
        color = function() return kit.ink("lo")():alpha(0.18) end } or nil }
  end
  local content = ui.Item { width = W, height = H,
    ui.Item { x = (W - CW) / 2, y = 18, width = CW, height = 180,
      kit.heading { text = "Settings", level = "section" },
      ui.Item { y = 34, width = CW, height = 44 * #rows,
        kit.card { anchors = { fill = true } },
        ui.Column(list) } } }
  return (shell("clamp", { width = W, height = H, maximum_size = CW, content = content }))
end

M.bottom_bar = function(kit, widgets)
  local W, H = 280, 220
  -- A view switcher bar: each section an icon over its name.
  local tabs = widgets.tabs { width = W, height = 60, current = 1, items = {
    { name = "Home", label = "Home", icon = "home" }, { name = "Search", label = "Search", icon = "search" },
    { name = "Library", label = "Library", icon = "library_music" } } }
  return (shell("bottom_bar", { width = W, height = H, header_height = 42, bottom_height = 60, sidebar_width = 180,
    header_bar = header(widgets, W, "Music"),
    sidebar = sidebar(kit, widgets, 180, { "Home", "Search", "Library" }),
    bottom_bar = tabs,
    content = page(kit, W, "Listen now", { "New this week", "Made for you" }) }))
end

return M
