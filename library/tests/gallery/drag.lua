-- Gallery samples for the Drag archetype's widgets (lib.kit.drag): each a
-- small working layout -- panes that split, a panel that resizes, rows and
-- tabs that reorder, tiles that sort, cards that swipe away, a row whose
-- actions slide out, a list that pulls to refresh, a sheet, a window, a
-- chip to drag out and a place to drop it. The layouts are the same in
-- every theme; the skins draw them.
local morf = require("morf")
local ui = require("morf.ui")
local drag = require("lib.kit.drag")

local M = {}

local serial = 0
local function uid(name) serial = serial + 1 return "kit.sample.drag." .. name .. "." .. serial end

-- The spring the layouts move their controls on: the one lib.kit.drag
-- follows the finger with, so a row moved under the finger stays there.
local function spring() return ui.spring { stiffness = 700, damping = 53 } end

-- An order of keys, kept in a signal: `pos(key)`, `move(key, to)`.
local function order_of(name, keys)
  local order = morf.signal(uid(name), keys)
  local o = {}
  function o.pos(key)
    for i, k in ipairs(order:get()) do if k == key then return i end end
    return 1
  end
  function o.move(key, to)
    local list = {}
    for i, k in ipairs(order:get()) do list[i] = k end
    local from = o.pos(key)
    to = math.max(1, math.min(#list, to))
    if to == from then return end
    table.remove(list, from)
    table.insert(list, to, key)
    order:set(list)
  end
  function o.keys() return order:get() end
  return o
end

-- A pane: a card with a title and a few lines under it.
local function pane(kit, title, lines)
  local node = ui.Item { anchors = { fill = true, margins = 2 },
    kit.card { anchors = { fill = true } },
    kit.text { x = 16, y = 14, text = title, font_weight = 700 } }
  for i, line in ipairs(lines or {}) do
    ui.reparent(kit.text { x = 16, y = 18 + i * 26, text = line, opacity = 0.72, elide = "right",
      width = function() return math.max(0, (node.layout_width or 0) - 24) end }, node)
  end
  return node
end

M.span = { split_pane = { 2, 1 }, reorderable_rows = { 1, 1 } }

function M.split_pane(kit)
  local node = drag.split { width = 600, height = 220, ratio = 0.36, minimum = 0.2, maximum = 0.8, divider = 12,
    first = pane(kit, "Folders", { "Inbox", "Drafts", "Archive" }),
    second = pane(kit, "Message", { "Drag the divider to give either side more room.", "Arrows move it too." }) }
  return node
end

function M.resizable_panel(kit, widgets)
  local width = morf.signal(uid("panel"), 170)
  local panel = ui.Item { width = function() return width:get() end, height = 220, clip = true,
    pane(kit, "Details", { "Size", "Kind", "Modified" }) }
  local grip = widgets.resizable_panel { x = function() return width:get() - 6 end, width = 12, height = 220,
    minimum = 120, maximum = 260, value = function() return width:get() end,
    on_moved = function(v) width:set(math.floor(v)) end, accessible_name = "Panel width" }
  return ui.Item { width = 280, height = 220,
    kit.text { x = function() return width:get() + 14 end, y = 96, text = "Content", opacity = 0.6 },
    panel, grip }
end

function M.resize_grip(kit, widgets)
  local w, h = morf.signal(uid("grip.w"), 200), morf.signal(uid("grip.h"), 150)
  local start = { 0, 0 }
  local grip, t
  grip, t = widgets.resize_grip { width = 24, height = 24, accessible_name = "Resize",
    x = function() return w:get() - 24 end, y = function() return h:get() - 24 end,
    on_drag_started = function() start = { w:get(), h:get() } end }
  local box = ui.Item { width = 280, height = 220,
    ui.Item { width = function() return w:get() end, height = function() return h:get() end, clip = true,
      pane(kit, "Note", { "Pull the corner." }) },
    grip }
  morf.effect(uid("grip.follow"), function()
    if not t.active then return end
    w:set(math.max(140, math.min(280, start[1] + t.delta_x)))
    h:set(math.max(100, math.min(220, start[2] + t.delta_y)))
  end, { owner = box })
  return box
end

function M.reorderable_rows(_, widgets)
  local labels = { a = "Wake up", b = "Coffee", c = "Answer mail", d = "Walk" }
  local icons = { a = "alarm", b = "coffee", c = "mail", d = "directions_walk" }
  local order = order_of("rows", { "a", "b", "c", "d" })
  local box = ui.Item { width = 280, height = 220 }
  for _, key in ipairs { "a", "b", "c", "d" } do
    ui.reparent(widgets.reorderable_rows { width = 280, height = 48, extent = 54, label = labels[key], icon = icons[key],
      accessible_name = labels[key], y = function() return 4 + (order.pos(key) - 1) * 54 end,
      behavior = { y = spring() },
      on_reorder = function(step) order.move(key, order.pos(key) + step) end }, box)
  end
  return box
end

function M.reorderable_tabs(kit, widgets)
  local names = { a = "Home", b = "Music", c = "Photos" }
  local order = order_of("tabs", { "a", "b", "c" })
  local current = morf.signal(uid("tabs.current"), "a")
  local box = ui.Item { width = 280, height = 220,
    ui.Item { y = 52, width = 280, height = 168, pane(kit, "", {}),
      kit.text { x = 16, y = 16, font_weight = 700, text = function() return names[current:get()] end },
      kit.text { x = 16, y = 46, opacity = 0.72, text = "Drag a tab along the strip." } } }
  for _, key in ipairs { "a", "b", "c" } do
    ui.reparent(widgets.reorderable_tabs { width = 88, height = 40, extent = 93, label = names[key],
      accessible_name = names[key], x = function() return (order.pos(key) - 1) * 93 end, y = 4,
      highlighted = function() return current:get() == key end,
      behavior = { x = spring() },
      on_pressed = function() current:set(key) end,
      on_reorder = function(step) order.move(key, order.pos(key) + step) end }, box)
  end
  return box
end

function M.sortable_grid(_, widgets)
  local tiles = { { "a", "image", "Photos" }, { "b", "music_note", "Music" }, { "c", "movie", "Videos" },
    { "d", "description", "Files" }, { "e", "map", "Maps" }, { "f", "settings", "Settings" } }
  local keys = {}
  for i, tile in ipairs(tiles) do keys[i] = tile[1] end
  local order = order_of("grid", keys)
  local TW, TH, GX, GY = 88, 100, 96, 110
  local box = ui.Item { width = 280, height = 220 }
  for _, tile in ipairs(tiles) do
    local key = tile[1]
    local start = 1
    local node, t
    node, t = widgets.sortable_grid { width = TW, height = TH, icon = tile[2], label = tile[3], accessible_name = tile[3],
      x = function() return ((order.pos(key) - 1) % 3) * GX end,
      y = function() return 4 + math.floor((order.pos(key) - 1) / 3) * GY end,
      behavior = { x = spring(), y = spring() },
      on_drag_started = function() start = order.pos(key) end }
    -- Over another tile's place, the tile takes it and the rest close up.
    morf.effect(uid("grid.sort"), function()
      if not t.active then return end
      local col = ((start - 1) % 3) + math.floor(t.delta_x / GX + 0.5)
      local row = math.floor((start - 1) / 3) + math.floor(t.delta_y / GY + 0.5)
      col, row = math.max(0, math.min(2, col)), math.max(0, math.min(1, row))
      order.move(key, row * 3 + col + 1)
    end, { owner = node })
    ui.reparent(node, box)
  end
  return box
end

function M.swipe_dismiss(_, widgets)
  local cards = { { "a", "Battery low", "Plug in soon" }, { "b", "Update ready", "Restart to finish" },
    { "c", "New message", "From Ana" } }
  local order = order_of("swipe", { "a", "b", "c" })
  local box = ui.Item { width = 280, height = 220, clip = true }
  for _, card in ipairs(cards) do
    local key = card[1]
    local node, t
    node, t = widgets.swipe_dismiss { width = 280, height = 64, label = card[2], detail = card[3],
      accessible_name = card[2], y = function() return 4 + (order.pos(key) - 1) * 72 end,
      behavior = { y = spring() },
      -- Gone, the others close the gap; a moment later it comes back last.
      on_swiped = function()
        morf.timer(240, function() pcall(order.move, key, #order.keys()) end, false)
        morf.timer(1400, function() pcall(function() t.gone = "" end) end, false)
      end }
    ui.reparent(node, box)
  end
  return box
end

function M.swipe_actions(_, widgets)
  local rows = { { "a", "Weekly report" }, { "b", "Flight to Lisbon" }, { "c", "Dinner on Friday" } }
  local box = ui.Item { width = 280, height = 220, clip = true }
  local actions = { { icon = "archive", label = "Archive", tone = "accent" },
    { icon = "delete", label = "Delete", tone = "destructive" } }
  for i, row in ipairs(rows) do
    local y = 4 + (i - 1) * 64
    local node, t
    node, t = widgets.swipe_actions { y = y, width = 280, height = 56, label = row[2], accessible_name = row[2],
      reveal = 128, actions = actions }
    -- Where the actions are pressed, under the row's trailing edge.
    for a, action in ipairs(actions) do
      ui.reparent(widgets.area { x = 280 - 128 + (a - 1) * 64, y = y, width = 64, height = 56,
        accessible_name = action.label, enabled = function() return t.open end,
        on_clicked = function() t.open = false end }, box)
    end
    ui.reparent(node, box)
    -- The first stands open, to show what is under it.
    if i == 1 then t.open = true end
  end
  return box
end

function M.pull_to_refresh(kit, widgets)
  local items = { "Morning run", "Groceries", "Call the bank", "Book tickets", "Water the plants" }
  local rows = ui.Item { anchors = { fill = true } }
  for i, label in ipairs(items) do
    ui.reparent(ui.Item { y = (i - 1) * 44, width = 280, height = 40, kit.card { anchors = { fill = true } },
      kit.text { x = 14, anchors = { vertical_center = true }, text = label } }, rows)
  end
  local node = widgets.pull_to_refresh { width = 280, height = 220, accessible_name = "Pull to refresh",
    on_refresh = function(done) morf.timer(1400, done, false) end, rows }
  return ui.Item { width = 280, height = 220, clip = true, node }
end

function M.sheet_handle(kit, widgets)
  local top = morf.signal(uid("sheet"), 96)
  local sheet = ui.Item { y = function() return top:get() end, width = 280, height = 220,
    behavior = { y = spring() },
    kit.card { anchors = { fill = true } },
    kit.text { x = 16, y = 34, text = "Now playing", font_weight = 700 },
    kit.text { x = 16, y = 62, text = "Drag the handle up or down", opacity = 0.72 },
    (widgets.sheet_handle { width = 280, height = 28, accessible_name = "Sheet",
      minimum = 24, maximum = 168, detents = { 24, 96, 168 }, value = function() return top:get() end,
      on_moved = function(v) top:set(v) end }) }
  return ui.Item { width = 280, height = 220, clip = true,
    kit.text { x = 16, y = 16, text = "Library", font_weight = 700, opacity = 0.6 },
    sheet }
end

function M.window_move(kit, widgets)
  local pos = morf.signal(uid("window"), { 40, 40 })
  local start = { 0, 0 }
  local strip, t
  strip, t = widgets.window_move { width = 200, height = 34, label = "Notes", accessible_name = "Move window",
    on_drag_started = function() start = { pos:get()[1], pos:get()[2] } end }
  local window = ui.Item { width = 200, height = 130,
    x = function() return pos:get()[1] end, y = function() return pos:get()[2] end,
    kit.card { anchors = { fill = true } },
    kit.text { x = 14, y = 50, text = "Drag the title", opacity = 0.72 },
    strip }
  local box = ui.Item { width = 280, height = 220, window }
  morf.effect(uid("window.follow"), function()
    if not t.active then return end
    pos:set { math.max(0, math.min(80, start[1] + t.delta_x)), math.max(0, math.min(90, start[2] + t.delta_y)) }
  end, { owner = box })
  return box
end

function M.drag_source(kit, widgets)
  return ui.Item { width = 280, height = 220,
    widgets.drag_source { x = 60, y = 74, width = 160, height = 44, label = "notes.txt", icon = "description",
      accessible_name = "notes.txt", payload = { text = "notes.txt" } },
    kit.text { x = 0, y = 136, width = 280, horizontal_alignment = "center", text = "Drag it out to share",
      opacity = 0.6 } }
end

function M.drop_zone(_, widgets)
  local idle = widgets.drop_zone { width = 280, height = 104, label = "Drop files here", icon = "upload",
    keys = { "files", "text" } }
  -- A second zone shown as it is while a drag it takes is over it.
  local hot, t = widgets.drop_zone { y = 116, width = 280, height = 104, label = "Release to add", icon = "upload",
    keys = { "files", "text" } }
  t.accepting = true
  return ui.Item { width = 280, height = 220, idle, hot }
end

function M.slide_to_confirm(kit, widgets)
  return ui.Item { width = 280, height = 220,
    widgets.slide_to_confirm { x = 0, y = 60, width = 280, height = 56, label = "Slide to power off",
      icon = "power_settings_new", accessible_name = "Power off" },
    kit.text { x = 0, y = 136, width = 280, horizontal_alignment = "center", text = "Return confirms",
      opacity = 0.6 } }
end

return M
