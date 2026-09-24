-- The desk: widgets on the wallpaper, and the board they are arranged on.
--
-- Port of Desktop.qml. The original's desk was a layer surface between the
-- wallpaper and the windows, raised to the top layer while arranging. A
-- morf layer surface keeps its layer for life, so the desk is split in
-- three instead:
--
--   `M.rest(w, h)`     the widgets and the spectra, drawn into the
--                      wallpaper's own background surface, so every window
--                      covers them. Its input region is its MouseAreas: the
--                      widgets' own controls, and the right button anywhere
--                      on the wallpaper for the desk's menu.
--   `M.arranging(w, h)` the board while arranging, on a surface of its own
--                      above the windows: the wallpaper under a grid, every
--                      widget with its handles, the card of modules, and a
--                      widget's inspector or a photo's picker. Built when
--                      arranging starts and dropped when it ends.
--   `M.menu(w, h)`     the right-click menu, on a surface of its own above
--                      the windows, open only while the menu is.
--
-- `services/auto/desktop.lua` puts the last two on their surfaces; in a
-- nested compositor without layer shell (IMPASTO_INLINE_WALLPAPER) init.lua
-- draws all three into the bar's surface instead.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local widget = require("desktop.widget")
local wallpaper = require("services.wallpaper")

local C = theme.color
local M = {}

local ESCAPE = 0xff1b

local function optional(name)
  local ok, mod = pcall(require, name)
  if ok then return mod end
  morf.log("error", "impasto: " .. name .. " did not load: " .. tostring(mod))
  return nil
end

-- The spectra along the screen's edges, in screen coordinates.
local function edge_spectra(arranging)
  local edge = optional("desktop.edge_spectrum")
  if not edge then return ui.Item {} end
  return ui.Repeater {
    model = desk.spectrum_keys,
    delegate = function(r) return edge.build(r.key, arranging) end,
  }
end

--- The widgets at rest, drawn with the wallpaper.
function M.rest(width, height)
  local board = ui.Item {
    x = function() return desk.board().x end,
    y = function() return desk.board().y end,
    width = function() return desk.board().width end,
    height = function() return desk.board().height end,
    visible = function() return not desk.editing:get() end,
    ui.Repeater {
      model = desk.keys,
      delegate = function(r) return widget.build(r.key, false) end,
    },
  }
  return ui.Item {
    width = width, height = height,
    -- The right button anywhere on the wallpaper: the desk's menu.
    ui.MouseArea {
      anchors = { fill = true }, accepted_buttons = "right",
      on_clicked = function(sx, sy)
        local b = desk.board()
        desk.open_menu("", sx - b.x, sy - b.y)
      end,
    },
    ui.Item {
      anchors = { fill = true },
      visible = function() return not desk.editing:get() end,
      edge_spectra(false),
    },
    board,
  }
end

-- ---------------------------------------------------------------- arranging --

local function lattice()
  local g = desk.grid()
  local cells = {}
  for r = 0, g.rows - 1 do
    for c = 0, g.columns - 1 do
      local box = desk.box(c, r, "2x2")
      local x, y = desk.offset_x(c), desk.offset_y(r)
      cells[#cells + 1] = ui.Rect {
        x = x, y = y,
        width = desk.offset_x(c + 1) - theme.desktop_gutter - x,
        height = desk.offset_y(r + 1) - theme.desktop_gutter - y,
        radius = theme.radius_small,
        color = morf.color("transparent"),
        border_color = C.hairline, border_width = 1,
      }
      local _ = box
    end
  end
  return ui.Item { table.unpack(cells) }
end

-- The cell the held widget would drop into.
local function landing()
  local function box()
    local family = desk.landing_family:get()
    if family == "" then return nil end
    return desk.box(desk.landing_col:get(), desk.landing_row:get(), family)
  end
  return ui.Rect {
    visible = function() return box() ~= nil end,
    x = function() local b = box() return b and b.x or 0 end,
    y = function() local b = box() return b and b.y or 0 end,
    width = function() local b = box() return b and b.width or 1 end,
    height = function() local b = box() return b and b.height or 1 end,
    behavior = {
      x = theme.behave("fast"), y = theme.behave("fast"),
      width = theme.behave("fast"), height = theme.behave("fast"),
    },
    radius = theme.desktop_radius,
    color = function() return C.accent():alpha(0.14) end,
    border_color = C.accent, border_width = 2,
  }
end

--- The board while arranging: above the windows, the whole screen.
function M.arranging(width, height)
  local tray = optional("desktop.tray")
  local inspector = optional("desktop.inspector")
  local picker = optional("desktop.picker")
  local function board_node(children)
    local node = {
      x = function() return desk.board().x end,
      y = function() return desk.board().y end,
      width = function() return desk.board().width end,
      height = function() return desk.board().height end,
    }
    for _, child in ipairs(children) do node[#node + 1] = child end
    return ui.Item(node)
  end
  local children = {
    -- The wallpaper under the grid: the windows go out of the way and the
    -- desk looks as it does empty.
    ui.ClipRect {
      x = function() return desk.board().x end,
      y = function() return desk.board().y end,
      width = function() return desk.board().width end,
      height = function() return desk.board().height end,
      color = C.background,
      ui.Image {
        x = function() return -desk.board().x end,
        y = function() return -desk.board().y end,
        width = width, height = height, fill_mode = "preserve_aspect_crop",
        source = function() return wallpaper.current:get() end,
      },
    },
    -- Anywhere on the background: a left click puts the inspector away, a
    -- right click ends arranging, and Escape does too.
    ui.MouseArea {
      anchors = { fill = true }, accepted_buttons = { "left", "right" }, focus = true,
      on_clicked = function(_, _, _, _, button)
        if button == "right" then desk.edit(false) else desk.select("") end
      end,
      on_key_pressed = function(keysym)
        if keysym == ESCAPE then desk.edit(false) end
      end,
    },
    edge_spectra(true),
    board_node {
      lattice(),
      landing(),
      ui.Repeater {
        model = desk.keys,
        delegate = function(r) return widget.build(r.key, true) end,
      },
      tray and ui.Item { z = 3, anchors = { fill = true }, tray.build() } or ui.Item {},
      inspector and ui.Item { z = 5, anchors = { fill = true }, inspector.build() } or ui.Item {},
      picker and ui.Item { z = 5, anchors = { fill = true }, picker.build() } or ui.Item {},
    },
  }
  return ui.Item { width = width, height = height, table.unpack(children) }
end

-- --------------------------------------------------------------------- menu --

local menu_rows = morf.list_model({})
local menu_count = morf.signal("impasto.desk.menu.count", 0)

local function choose(id)
  local key = desk.menu_key:get()
  desk.close_menu()
  local island_ok, island = pcall(require, "bar.island")
  local function panel(name) if island_ok then pcall(island.toggle, name) end end
  if id == "arrange" then
    desk.edit(true)
  elseif id == "note" then
    require("desktop.sources").notes.create()
  elseif id == "wallpaper" then
    panel("appearance")
  elseif id == "palette" then
    panel("palette")
  elseif id == "settings" then
    -- DesktopService.settingsRequested: the settings window, not a panel.
    local ok, window = pcall(require, "settings.window")
    if ok then window.open() end
  elseif id == "open" then
    local row = desk.entry_of(key)
    local S = require("desktop.sources")
    local note = S.notes.note_for(row)
    S.notes.open(note and note.key or "")
  elseif id == "edit" then
    desk.edit(true)
    desk.select(key)
  elseif id == "remove" then
    desk.remove(key)
  end
end

--- The rows for the menu that is opening.
function M.fill_menu()
  local key = desk.menu_key:get()
  local rows = {}
  if key == "" then
    rows = {
      { id = "arrange", label = "Arrange widgets", icon = "󰆾" },
      { id = "note", label = "New note", icon = "󰎞" },
      { id = "wallpaper", label = "Wallpaper", icon = "󰸉" },
      { id = "palette", label = "Palette", icon = "󰏘" },
      { id = "settings", label = "Settings", icon = "󰒓" },
    }
  else
    local row = desk.entry_of(key)
    if row and row.id == "notes" then rows[#rows + 1] = { id = "open", label = "Open", icon = "󰏫" } end
    rows[#rows + 1] = { id = "edit", label = "Edit", icon = "󰆾" }
    rows[#rows + 1] = { id = "remove", label = "Remove", icon = "󰆴", warn = true }
  end
  menu_rows:replace(rows, "id")
  menu_count:set(#rows)
end

local function menu_row(r)
  local hovered = kit.hover_signal("desk.menu")
  return ui.Rect {
    width = theme.dock_menu_width - 2 * theme.dock_menu_padding, height = theme.dock_menu_row,
    radius = theme.radius_small,
    color = function() return hovered:get() and C.islandSurfaceHover or morf.color("transparent") end,
    behavior = { color = theme.behave("fast") },
    kit.glyph { x = 10, y = 0, width = 16, height = theme.dock_menu_row, vertical_alignment = "center",
      glyph = r.icon or "", size = 12,
      color = function() return r.warn and hovered:get() and C.red() or C.textMuted() end },
    kit.text { x = 36, y = 0, height = theme.dock_menu_row, vertical_alignment = "center",
      width = theme.dock_menu_width - 2 * theme.dock_menu_padding - 46, elide = "right",
      text = r.label, size = theme.size.small,
      color = function() return r.warn and hovered:get() and C.red() or C.text() end },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() choose(r.id) end,
    },
  }
end

--- The right-click menu, the whole screen: a click anywhere else closes it.
function M.menu(width, height)
  local function count() return menu_count:get() end
  local menu_w = theme.dock_menu_width
  local function menu_h() return count() * theme.dock_menu_row + 2 * theme.dock_menu_padding end
  return ui.Item {
    width = width, height = height,
    visible = function() return desk.menu_open:get() end,
    ui.MouseArea {
      anchors = { fill = true }, accepted_buttons = { "left", "right" },
      on_clicked = function() desk.close_menu() end,
    },
    ui.Rect {
      x = function()
        local b = desk.board()
        return b.x + math.max(theme.desktop_gutter, math.min(b.width - theme.desktop_gutter - menu_w, desk.menu_x:get()))
      end,
      y = function()
        local b = desk.board()
        return b.y + math.max(theme.desktop_gutter, math.min(b.height - theme.desktop_gutter - menu_h(), desk.menu_y:get()))
      end,
      width = menu_w,
      height = menu_h,
      radius = theme.radius_medium,
      color = C.island, border_color = C.islandBorder, border_width = 1,
      ui.MouseArea { anchors = { fill = true }, accepted_buttons = { "left", "right" } },
      ui.Repeater {
        as = "column", x = theme.dock_menu_padding, y = theme.dock_menu_padding,
        model = menu_rows, delegate = menu_row,
      },
    },
  }
end

-- The menu's rows are chosen as it opens, in the handler that opens it.
desk.on_menu_open = M.fill_menu

return M
