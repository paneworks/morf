-- Right-click menu for a dock icon: the application's windows by title,
-- the focused one marked and each one's workspace beside it, then what can
-- be done to the application.
--
-- Port of DockMenu.qml. No keyboard focus, so no Escape: right-click, click
-- outside, or choose a row. Fixed width: fitting the content would make the
-- rows' width and the plate's depend on each other, and window titles need
-- eliding anyway.

local ui = require("morf.ui")
local theme = require("theme")
local dock = require("services.dock")
local kit = require("components.kit")

local C = theme.color
local M = {}

local ROW_WIDTH = theme.dock_menu_width - 2 * theme.dock_menu_padding

-- The rows of the open menu, rebuilt when its item changes. Keyed, so a
-- window that stays keeps its row and only its title is rewritten.
M.rows = morf.list_model({})

local function workspace_label(n)
  n = tonumber(n) or 0
  if n == 0 then return "" end
  return string.format("%d", math.floor(n))
end

local function rows_for(item)
  local rows = {}
  if not item then return rows end
  for _, w in ipairs(item.windows) do
    rows[#rows + 1] = {
      id = "w:" .. w.handle, kind = "window", handle = w.handle,
      label = w.title, workspace = w.workspace or 0, front = w.front == true,
    }
  end
  local known = item.id ~= ""
  local actions = {}
  if known then
    actions[#actions + 1] = { id = "a:launch", label = item.running and "New window" or "Open" }
    actions[#actions + 1] = { id = "a:pin", label = item.pinned and "Remove from the dock" or "Keep in the dock" }
  end
  if item.running then
    actions[#actions + 1] = {
      id = "a:close", warn = true,
      label = #item.windows > 1 and "Close all windows" or "Close",
    }
  end
  if item.running and #actions > 0 then
    rows[#rows + 1] = { id = "divider", kind = "divider" }
  end
  for _, action in ipairs(actions) do
    action.kind = "action"
    rows[#rows + 1] = action
  end
  return rows
end

--- The plate's height, worked out from its rows rather than measured, so it
--- is placed right on the frame it opens.
function M.height()
  local h = 2 * theme.dock_menu_padding
  for _, row in ipairs(rows_for(dock.menu_item())) do
    h = h + (row.kind == "divider" and 11 or theme.dock_menu_row)
  end
  return h
end

morf.effect("impasto.dock.menu.rows", function()
  M.rows:replace(rows_for(dock.menu_item()), "id")
end)

local function choose(row)
  local item = dock.menu_item()
  if row.kind == "window" then
    dock.focus_window(row.handle)
  elseif row.id == "a:launch" then
    dock.launch(item)
  elseif row.id == "a:pin" and item then
    dock.toggle_pin(item.id)
  elseif row.id == "a:close" then
    dock.close_all(item)
  end
  dock.close_menu()
end

local function row_node(row)
  if row.kind == "divider" then
    return ui.Item {
      width = ROW_WIDTH, height = 11,
      ui.Rect {
        anchors = { left = true, right = true, top = true, top_margin = 5 },
        height = 1, color = C.hairline,
      },
    }
  end
  local current = row
  local hovered = kit.hover_signal("dock.menu")
  local window = row.kind == "window"
  local title = kit.text {
    text = row.label, size = theme.size.small,
    elide = "right",
    width = window and (ROW_WIDTH - 16 - theme.dock_dot - 8 - 18 - 8) or (ROW_WIDTH - 24 - theme.dock_dot),
    height = theme.dock_menu_row,
    vertical_alignment = "center",
    x = window and 0 or 16 + theme.dock_dot,
    color = function()
      if current.warn and hovered:get() then return C.red() end
      return C.text()
    end,
  }
  local content
  local workspace, mark
  if window then
    workspace = kit.text {
      text = workspace_label(row.workspace), size = theme.size.label, mono = true,
      width = 18, height = theme.dock_menu_row,
      horizontal_alignment = "right", vertical_alignment = "center",
      color = C.textMuted,
    }
    -- Marks the focused window.
    mark = ui.Rect {
      width = theme.dock_dot, height = theme.dock_dot, radius = theme.dock_dot / 2,
      color = C.accent, opacity = row.front and 1 or 0,
    }
    content = ui.Row {
      x = 8, height = theme.dock_menu_row,
      gap = 8, align = "center",
      mark, title, workspace,
    }
  else
    -- Aligned with the window titles, not the dots.
    content = title
  end
  local node = ui.Rect {
    width = ROW_WIDTH, height = theme.dock_menu_row,
    radius = theme.radius_small,
    color = function() return hovered:get() and C.islandSurfaceHover or morf.color("#00000000") end,
    behavior = { color = theme.behave("fast") },
    content,
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() choose(current) end,
    },
  }
  return node, function(next)
    current = next
    title.text = next.label or ""
    if workspace then workspace.text = workspace_label(next.workspace) end
    if mark then mark.opacity = next.front and 1 or 0 end
  end
end

--- The menu plate, as tall as its rows.
function M.build()
  return ui.Rect {
    width = theme.dock_menu_width,
    height = M.height,
    radius = theme.radius_medium,
    color = C.island,
    border_width = 1,
    border_color = C.islandBorder,
    ui.Inset {
      margin = theme.dock_menu_padding,
      ui.Repeater { as = "column", model = M.rows, delegate = row_node },
    },
  }
end

return M
