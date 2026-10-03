-- Gallery samples for the Popup archetype's widgets (lib.kit.samples): each
-- gives the button that opens its popup, built with what a real one holds.
-- While the gallery draws the Popup archetype (KIT_WIDGETS names it) each
-- also opens a preview of its popup under the button -- the same spec, but
-- neither modal nor dimming, so every look shows in its own cell at once.
local ui = require("morf.ui")

local M = {}

local CELL_W = 280

local function previewing()
  local wanted = morf.env("KIT_WIDGETS")
  return type(wanted) == "string" and (" " .. wanted .. " "):find(" Popup ", 1, true) ~= nil
end

-- The ink a theme writes in on this popup's ground: its own when the ground
-- is not the usual one (a dark tooltip), else the kit's.
local function ink(kit, widget, level)
  local own = kit.popup_ink and kit.popup_ink(widget, level)
  return own or kit.ink(level)
end

local function size(kit, key, fallback)
  local theme = kit.theme
  if not theme then
    local ok, t = pcall(require, "theme")
    theme = ok and type(t) == "table" and t or nil
  end
  return theme and theme.size and theme.size[key] or fallback
end

local function text(kit, widget, props)
  props.color = props.color or ink(kit, widget, props.level or "hi")
  props.level = nil
  return kit.text(props)
end

local function title(kit, widget, s, width)
  return text(kit, widget, { text = s, font_size = size(kit, "larger", 17), font_weight = 700, width = width,
    elide = "right" })
end

local function body(kit, widget, s, width)
  return text(kit, widget, { text = s, font_size = size(kit, "small", 13), width = width, wrap = true,
    level = "lo" })
end

-- A hairline the theme draws its separators in.
local function rule(kit, width)
  return ui.Rect { width = width, height = 1, color = kit.stroke and kit.stroke("faint") or kit.ink("lo") }
end

--- The opener and its popup: `label` (the button's), `spec` (a function of
--- (kit, widgets) giving the popup's spec), `preview` (settings that keep
--- the preview in the cell: a placement, a gap).
local function sample(name, label, make, preview)
  return function(kit, widgets)
    local handle
    local press
    press = widgets.push { id = "sample-" .. name .. "-open", label = label, width = 170, height = 32,
      on_clicked = function()
        if not handle then handle = widgets[name](make(kit, widgets)) end
        handle.toggle(press)
      end }
    local node = ui.Item { width = CELL_W, height = 220, press }
    if previewing() then
      -- (Its content is made when it opens: until then it would hang from
      -- nothing.)
      morf.timer(120, function()
        local spec = make(kit, widgets)
        spec.id = "sample-" .. name
        spec.modal, spec.dim, spec.close_policy, spec.focus_on_open = false, false, "none", false
        spec.placement = (preview and preview.placement) or "bottom-start"
        spec.gap = (preview and preview.gap) or spec.gap or 6
        -- At rest from its first frame: a still of the look, not of its motion.
        spec.behavior = {}
        widgets[name](spec).open(press)
      end)
    end
    return node
  end
end

-- ------------------------------------------------------------- menus --

M.menu = sample("menu", "Edit", function()
  return { width = 200, padding = 6, gap = 10, items = {
    { label = "Cut", icon = "content_cut" }, { label = "Copy", icon = "content_copy" },
    { label = "Paste", icon = "content_paste" }, { label = "Select all", icon = "select_all" } } }
end, { gap = 10 })

M.context_menu = sample("context_menu", "Right-click me", function()
  return { width = 200, padding = 6, items = {
    { label = "Open", icon = "open_in_new" }, { label = "Rename", icon = "edit" },
    { label = "Move to trash", icon = "delete" } } }
end, { gap = 2 })

M.menu_bar_menu = sample("menu_bar_menu", "File", function()
  return { width = 210, padding = 6, gap = 0, items = {
    { label = "New window", icon = "add" }, { label = "Open…", icon = "folder_open" },
    { label = "Save", icon = "save" }, { label = "Quit", icon = "logout" } } }
end, { gap = 0 })

M.submenu = sample("submenu", "Text size", function()
  return { width = 180, padding = 6, items = {
    { label = "Small", checked = false, group = "size" }, { label = "Medium", checked = true, group = "size" },
    { label = "Large", checked = false, group = "size" } } }
end)

M.dropdown_list = sample("dropdown_list", "Sort by", function()
  return { width = 170, padding = 4, gap = 2, items = {
    { label = "Name", checked = true, group = "sort" }, { label = "Size", checked = false, group = "sort" },
    { label = "Modified", checked = false, group = "sort" }, { label = "Kind", checked = false, group = "sort" } } }
end, { gap = 2 })

M.autocomplete_list = sample("autocomplete_list", "Ber", function()
  return { width = 220, padding = 4, gap = 2, items = {
    { label = "Berlin", icon = "location_on" }, { label = "Bergen", icon = "location_on" },
    { label = "Bern", icon = "history" } } }
end, { gap = 2 })

-- ---------------------------------------------------------- floating --

M.popover = sample("popover", "Brightness", function(kit, widgets)
  local w = 212
  return { padding = 14, gap = 12, content = ui.Column { gap = 10, width = w,
    title(kit, "popover", "Brightness", w),
    widgets.slider { width = w, height = 40, value = 0.62, accessible_name = "Brightness" },
    body(kit, "popover", "Night light is on until 7:00", w) } }
end, { placement = "bottom", gap = 12 })

M.tooltip = sample("tooltip", "Mute", function()
  return { text = "Mute microphone", padding = 8 }
end, { placement = "bottom", gap = 8 })

M.rich_tooltip = sample("rich_tooltip", "Autosave", function(kit, widgets)
  local w = 220
  return { padding = 14, content = ui.Column { gap = 6, width = w,
    text(kit, "rich_tooltip", { text = "Autosave", font_size = size(kit, "normal", 15), font_weight = 700 }),
    body(kit, "rich_tooltip", "Changes are kept every minute while you work.", w),
    widgets.flat { label = "Learn more", width = 110, height = 30 } } }
end, { gap = 8 })

M.hover_card = sample("hover_card", "@ana", function(kit, widgets)
  local w = 236
  local avatar = ui.Rect { width = 44, height = 44, radius = 22, color = kit.ink("accent"),
    text(kit, "hover_card", { text = "A", anchors = { center_in = true }, font_size = 20, font_weight = 700,
      color = function() return kit.ink("accent")():text_color() end }) }
  return { padding = 14, content = ui.Column { gap = 8, width = w,
    ui.Row { gap = 10, align = "center", avatar,
      ui.Column { gap = 0,
        text(kit, "hover_card", { text = "Ana Lindqvist", font_weight = 700 }),
        body(kit, "hover_card", "@ana · Stockholm", 160) } },
    body(kit, "hover_card", "Type designer. Draws letters for small screens.", w),
    widgets.suggested { label = "Follow", width = 96, height = 30 } } }
end)

-- ----------------------------------------------------------- dialogs --

M.command_palette = sample("command_palette", "Commands", function(kit)
  local w = 270
  local function command(icon, label, keys)
    return ui.Item { width = w, height = 34,
      kit.icon(icon, 18, kit.ink("lo"), { x = 4, anchors = { vertical_center = true } }),
      text(kit, "command_palette", { text = label, x = 32, anchors = { vertical_center = true } }),
      kit.keycap { text = keys, anchors = { right = true, vertical_center = true, right_margin = 4 } } }
  end
  return { padding = 10, content = ui.Column { gap = 6, width = w,
    ui.Item { width = w, height = 32,
      kit.icon("search", 20, kit.ink("accent"), { x = 4, anchors = { vertical_center = true } }),
      text(kit, "command_palette", { text = "open set", x = 32, anchors = { vertical_center = true },
        font_size = size(kit, "normal", 15) }) },
    rule(kit, w),
    command("settings", "Open settings", "Ctrl ,"),
    command("palette", "Change theme", "Ctrl T"),
    command("splitscreen", "Split editor", "Ctrl \\") } }
end)

M.dialog = sample("dialog", "Discard…", function()
  return { width = 280, title = "Discard changes?", body = "Unsaved edits to this note will be lost.",
    buttons = { { label = "Cancel", width = 90 }, { label = "Discard", destructive = true, width = 90 } } }
end)

M.alert_dialog = sample("alert_dialog", "Delete…", function(kit, widgets)
  local w = 240
  local alert = kit.signal and kit.signal("alert") or kit.ink("accent")
  return { padding = 20, content = ui.Column { gap = 8, width = w, align = "center",
    kit.icon("delete_forever", 28, alert),
    text(kit, "alert_dialog", { text = "Delete 3 files?", font_size = size(kit, "larger", 17), font_weight = 700,
      width = w, horizontal_alignment = "center" }),
    text(kit, "alert_dialog", { text = "They will be gone for good.", font_size = size(kit, "small", 13), width = w,
      horizontal_alignment = "center", level = "lo" }),
    ui.Item { width = w, height = 6 },
    ui.Row { gap = 8,
      widgets.push { label = "Cancel", width = (w - 8) / 2, height = 34 },
      widgets.destructive { label = "Delete", width = (w - 8) / 2, height = 34 } } } }
end)

M.message_dialog = sample("message_dialog", "Update…", function()
  return { width = 280, title = "Update ready", body = "Restart to finish installing the new version.",
    buttons = { { label = "Later", width = 90 }, { label = "Restart", suggested = true, width = 90 } } }
end)

M.preferences_dialog = sample("preferences_dialog", "Preferences", function(kit, widgets)
  local w = 248
  local function row(label, on)
    return ui.Item { width = w, height = 36,
      text(kit, "preferences_dialog", { text = label, anchors = { vertical_center = true } }),
      widgets.switch { checked = on, accessible_name = label, anchors = { right = true, vertical_center = true } } }
  end
  return { padding = 16, content = ui.Column { gap = 4, width = w,
    title(kit, "preferences_dialog", "Preferences", w),
    ui.Item { width = w, height = 4 },
    row("Dark style", true), rule(kit, w), row("Reduce motion", false), rule(kit, w), row("Large text", false) } }
end)

M.about_dialog = sample("about_dialog", "About", function(kit, widgets)
  local w = 220
  local glyph = ui.Rect { width = 56, height = 56, radius = 16, color = kit.ink("accent"),
    kit.icon("draw", 32, function() return kit.ink("accent")():text_color() end, { anchors = { center_in = true } }) }
  return { padding = 18, content = ui.Column { gap = 6, width = w, align = "center",
    glyph,
    text(kit, "about_dialog", { text = "Morf", font_size = size(kit, "large", 20), font_weight = 800 }),
    text(kit, "about_dialog", { text = "Version 1.0 · MIT", font_size = size(kit, "small", 13), level = "lo" }),
    ui.Item { width = w, height = 4 },
    widgets.flat { label = "Credits", width = 100, height = 30 } } }
end)

M.shortcuts_dialog = sample("shortcuts_dialog", "Shortcuts", function(kit)
  local w = 248
  local function row(label, keys)
    return ui.Item { width = w, height = 30,
      text(kit, "shortcuts_dialog", { text = label, anchors = { vertical_center = true } }),
      kit.keycap { text = keys, anchors = { right = true, vertical_center = true } } }
  end
  return { padding = 16, content = ui.Column { gap = 4, width = w,
    title(kit, "shortcuts_dialog", "Shortcuts", w),
    ui.Item { width = w, height = 2 },
    row("Search", "Ctrl K"), row("New tab", "Ctrl T"), row("Close tab", "Ctrl W") } }
end)

-- ------------------------------------------------------------ sheets --

M.bottom_sheet = sample("bottom_sheet", "Share", function(kit, widgets)
  local w = 260
  local function target(icon, label)
    return ui.Column { gap = 4, align = "center", width = 58,
      widgets.icon { icon = icon, icon_off = icon, icon_on = icon, width = 44, height = 44, size = 22,
        accessible_name = label },
      text(kit, "bottom_sheet", { text = label, font_size = size(kit, "small", 13), level = "lo" }) }
  end
  -- (Room above for the sheet's handle.)
  return { width = w + 32, height = 156, content = ui.Item { width = w + 32, height = 156,
    ui.Column { x = 16, y = 28, gap = 12, width = w,
      title(kit, "bottom_sheet", "Share", w),
      ui.Row { gap = 9, target("mail", "Mail"), target("link", "Link"), target("print", "Print"),
        target("qr_code", "Code") } } } }
end)

M.side_sheet = sample("side_sheet", "Filters", function(kit, widgets)
  local w = 180
  local function row(label, on)
    return ui.Row { gap = 10, align = "center",
      widgets.checkbox { checked = on, accessible_name = label },
      text(kit, "side_sheet", { text = label }) }
  end
  return { padding = 16, content = ui.Column { gap = 10, width = w,
    title(kit, "side_sheet", "Filters", w),
    row("Images", true), row("Documents", true), row("Archives", false) } }
end, { placement = "bottom-start" })

M.drawer = sample("drawer", "Mail", function(kit, widgets)
  local w = 188
  local rows = { gap = 2, width = w,
    text(kit, "drawer", { text = "Mail", font_size = size(kit, "larger", 17), font_weight = 700, x = 12, height = 34,
      vertical_alignment = "center" }) }
  for _, item in ipairs { { "inbox", "Inbox" }, { "star", "Starred" }, { "send", "Sent" }, { "delete", "Trash" } } do
    rows[#rows + 1] = widgets.menu_item { label = item[2], icon = item[1], width = w, height = 34 }
  end
  return { padding = 10, content = ui.Column(rows) }
end)

-- --------------------------------------------------------- transient --

M.toast = sample("toast", "Copy", function()
  return { text = "Copied to clipboard" }
end, { placement = "bottom-start", gap = 10 })

M.snackbar = sample("snackbar", "Archive", function(kit, widgets)
  local w = 236
  return { padding = 8, content = ui.Item { width = w, height = 34,
    text(kit, "snackbar", { text = "Message archived", x = 8, anchors = { vertical_center = true } }),
    widgets.flat { label = "Undo", width = 68, height = 32, ink = ink(kit, "snackbar", "accent"),
      anchors = { right = true, vertical_center = true } } } }
end, { gap = 10 })

M.banner = sample("banner", "Go offline", function(kit, widgets)
  local w = 252
  return { padding = 12, content = ui.Item { width = w, height = 34,
    kit.icon("wifi_off", 20, ink(kit, "banner", "accent"), { anchors = { vertical_center = true } }),
    text(kit, "banner", { text = "You're offline", x = 30, anchors = { vertical_center = true } }),
    widgets.flat { label = "Retry", width = 68, height = 32, anchors = { right = true, vertical_center = true } } } }
end)

M.notification_popup = sample("notification_popup", "Notify", function(kit)
  local w = 250
  local badge = ui.Rect { width = 36, height = 36, radius = 18, color = kit.ink("accent"),
    kit.icon("chat", 20, function() return kit.ink("accent")():text_color() end, { anchors = { center_in = true } }) }
  return { padding = 14, content = ui.Row { gap = 12, width = w,
    badge,
    ui.Column { gap = 2, width = w - 48,
      text(kit, "notification_popup", { text = "Chat · now", font_size = size(kit, "small", 13), level = "lo" }),
      text(kit, "notification_popup", { text = "Ana Lindqvist", font_weight = 700 }),
      body(kit, "notification_popup", "The proofs look great, ship it!", w - 48) } } }
end)

M.lightbox = sample("lightbox", "View photo", function(kit, widgets)
  local w, h = 248, 120
  local picture = ui.Rect { width = w, height = h, radius = 6,
    gradient = function()
      local a = kit.ink("accent")()
      return { angle = 135, stops = { a, a:mix(kit.ink("hi")(), 0.55) } }
    end,
    kit.icon("landscape", 44, function() return kit.ink("accent")():text_color():alpha(0.85) end,
      { anchors = { center_in = true } }) }
  return { padding = 10, content = ui.Column { gap = 8, width = w,
    picture,
    ui.Item { width = w, height = 24,
      text(kit, "lightbox", { text = "Harbour, 06:40", anchors = { vertical_center = true },
        font_size = size(kit, "small", 13) }),
      text(kit, "lightbox", { text = "3 / 12", anchors = { right = true, vertical_center = true },
        font_size = size(kit, "small", 13), level = "lo" }) } } }
end)

M.tour_step = sample("tour_step", "Start tour", function(kit, widgets)
  local w = 230
  return { padding = 14, gap = 12, content = ui.Column { gap = 6, width = w,
    text(kit, "tour_step", { text = "Step 2 of 4", font_size = size(kit, "small", 13), level = "accent" }),
    text(kit, "tour_step", { text = "Pin your apps", font_size = size(kit, "larger", 17), font_weight = 700 }),
    body(kit, "tour_step", "Drag any app to the dock to keep it close.", w),
    ui.Item { width = w, height = 32,
      ui.Row { anchors = { right = true }, gap = 6,
        widgets.flat { label = "Back", width = 70, height = 32 },
        widgets.suggested { label = "Next", width = 70, height = 32 } } } } }
end, { placement = "bottom", gap = 12 })

return M
