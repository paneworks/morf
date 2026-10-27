-- Gallery samples for the Shell widgets: windows as applications lay them
-- out -- a mail client, a settings window, a notes drawer, a photo library
-- -- each at a width that shows how its layout answers.
local ui = require("morf.ui")

local M = {}

M.span = {
  application_window = { 2, 2 }, navigation_split_view = { 2, 1 }, multi_layout_view = { 2, 1 },
  breakpoint_bin = { 2, 1 }, clamp = { 2, 1 },
}

local function icon(w, glyph, name, on_clicked)
  return w.button { icon = glyph, variant = "flat", accessible_name = name, on_clicked = on_clicked }
end

local function header(w, width, title, starts, ends, subtitle)
  return (w.header_bar { width = width, height = 42, title = title, subtitle = subtitle, controls = false,
    variant = "flat", start = starts or {}, ["end"] = ends or {} })
end

local function page(w, width, title, lines)
  local column = { x = 16, y = 14, gap = 6, width = width - 32,
    w.label { text = title, variant = "title_4", width = width - 32, elide = "right" } }
  for _, line in ipairs(lines or {}) do
    column[#column + 1] = w.label { text = line, width = width - 32, elide = "right" }
  end
  return ui.Item { width = width, height = 400, ui.Column(column) }
end

local function sidebar(w, width, items, title)
  local top = title and 40 or 6
  local children = { width = width, height = 400 }
  if title then
    children[#children + 1] = w.label { x = 14, y = 10, text = title, variant = "heading", width = width - 28 }
  end
  children[#children + 1] = ui.Item { x = 6, y = top, width = width - 12, height = #items * 34,
    (require("lib.kit.archetypes.selection").make("sidebar_list", { items = items, current = 1, width = width - 12,
      item_width = width - 12, item_height = 34 })) }
  return ui.Item(children)
end

local SECTIONS = { "Inbox", "Starred", "Sent", "Archive" }

function M.application_window(_, w)
  local W, H = 600, 480
  return (w.application_window { width = W, height = H, breakpoints = { 360, 520 },
    sidebar_width = 160, inspector_width = 160, header_height = 42,
    header_bar = header(w, W, "Mail", { icon(w, "menu", "Sections") },
      { icon(w, "search", "Search"), icon(w, "more_vert", "Menu") }),
    sidebar = sidebar(w, 160, SECTIONS),
    content = page(w, 280, "Inbox", { "Weekly sync notes", "Your order shipped", "Build passed" }),
    inspector = page(w, 160, "Details", { "From Ana", "Today, 9:41" }) })
end

function M.header_bar(_, w)
  return header(w, 280, "Inbox", { icon(w, "arrow_back", "Back") }, { icon(w, "more_vert", "Menu") }, "3 unread")
end

function M.toolbar_view(_, w)
  local W, H = 280, 220
  local tools = ui.Item { width = W, height = 44,
    ui.Row { anchors = { center_in = true }, gap = 18,
      icon(w, "reply", "Reply"), icon(w, "forward", "Forward"), icon(w, "archive", "Archive") } }
  return (w.toolbar_view { width = W, height = H, header_height = 42, toolbar_bottom_height = 44,
    header_bar = header(w, W, "Message", { icon(w, "arrow_back", "Back") }),
    toolbar_bottom = tools,
    content = page(w, W, "Weekly sync", { "Notes from Monday", "and next steps." }) })
end

function M.overlay_split_view(_, w)
  local W, H = 280, 220
  local node, app
  node, app = w.overlay_split_view { width = W, height = H, header_height = 42, sidebar_width = 180,
    header_bar = header(w, W, "Notes", { icon(w, "menu", "Sections", function()
      if app then app.toggle_sidebar() end end) }),
    sidebar = sidebar(w, 180, { "All notes", "Pinned", "Shared" }),
    content = page(w, W, "Groceries", { "Oat milk, rye bread", "Lemons, basil" }) }
  -- Narrow, the sidebar is a drawer: shown open, over its scrim.
  app.toggle_sidebar()
  return node
end

function M.navigation_split_view(_, w)
  local W, H = 600, 220
  local detail = ui.Item { width = 380, height = 400,
    header(w, 380, "Wi-Fi", {}, { icon(w, "more_vert", "Menu") }),
    ui.Item { y = 42, width = 380, height = 300, page(w, 380, "Home network", { "Connected, secured" }) } }
  local side = ui.Item { width = 220, height = 400,
    header(w, 220, "Settings"),
    ui.Item { y = 42, width = 220, height = 300, sidebar(w, 220, { "Wi-Fi", "Bluetooth", "Display" }) } }
  return (w.navigation_split_view { width = W, height = H, breakpoints = { 400, 760 }, sidebar_width = 220,
    sidebar = side, content = detail })
end

function M.multi_layout_view(_, w)
  local W, H = 600, 220
  return (w.multi_layout_view { width = W, height = H, breakpoints = { 300, 500 }, sidebar_width = 150,
    inspector_width = 170,
    sidebar = sidebar(w, 150, { "Photos", "Albums", "People" }, "Library"),
    content = page(w, 280, "Albums", { "Summer, 214 photos", "Hiking, 88 photos" }),
    inspector = page(w, 170, "Summer", { "214 photos", "June to August" }) })
end

function M.breakpoint_bin(_, w)
  local W, H = 600, 220
  local app
  local made = morf.signal("gallery.shell.breakpoint_bin", false)
  local function layout() return made:get() and app.t.layout or "" end
  local content = ui.Item { width = W, height = H,
    ui.Column { x = 16, y = 16, gap = 10,
      w.label { text = "Breakpoints 400 and 760", variant = "title_4" },
      w.label { text = function() return ("At %d px this bin is %s"):format(W, layout()) end } } }
  local node
  node, app = w.breakpoint_bin { width = W, height = H, breakpoints = { 400, 760 }, content = content }
  made:set(true)
  return node
end

function M.clamp(_, w)
  local W, H = 600, 220
  local CW = 320
  local list = { variant = "boxed_list", width = CW }
  for _, label in ipairs { "Appearance", "Notifications", "Privacy" } do
    list[#list + 1] = ui.Item { width = CW, height = 44, w.label { x = 14, anchors = { vertical_center = true }, text = label } }
  end
  local content = ui.Item { width = W, height = H,
    ui.Column { x = (W - CW) / 2, y = 18, width = CW, gap = 10,
      w.label { text = "Settings", variant = "title_4" }, w.box(list) } }
  return (w.clamp { width = W, height = H, maximum_size = CW, content = content })
end

return M
