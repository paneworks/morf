-- One application on the dock: its icon, its running dot and its drag.
--
-- Port of DockItem.qml. The position comes from the dock service, so the
-- name label and the menu use the same arithmetic. Everything is read by
-- key: the model is rebuilt whenever a window opens or closes, and a
-- delegate that kept a row would keep a stale one.
--
-- Hovering lifts the icon slightly (`dock_lift`) instead of magnifying it:
-- magnification shifts the neighbours and moves the target.

local ui = require("morf.ui")
local theme = require("theme")
local dock = require("services.dock")
local kit = require("components.kit")

local C = theme.color
local M = {}

-- Icon name -> file, or false when the theme has none. Looked up once.
local icon_cache = {}
function M.icon_file(name)
  if not name or name == "" then return false end
  local cached = icon_cache[name]
  if cached ~= nil then return cached end
  local file = false
  if name:sub(1, 1) == "/" then
    file = morf.fs.exists and morf.fs.exists(name) and name or false
  else
    local ok, path = pcall(morf.icon_path, name, nil, 96)
    file = ok and path or false
  end
  icon_cache[name] = file
  return file
end

--- An application's picture from the icon theme, the launcher mark when
--- there is none. `size` is a number or a function.
function M.picture(icon, size)
  local dim = type(size) == "function" and size or function() return size end
  return ui.Item {
    width = dim, height = dim,
    ui.Image {
      anchors = { fill = true },
      fill_mode = "preserve_aspect_fit",
      source_width = 96, source_height = 96,
      visible = function() return M.icon_file(icon()) ~= false end,
      source = function() return M.icon_file(icon()) or "" end,
    },
    kit.glyph {
      anchors = { center_in = true },
      glyph = "󰀻",
      size = function() return math.floor(dim() * 0.6 + 0.5) end,
      color = C.textMuted,
      visible = function() return M.icon_file(icon()) == false end,
    },
  }
end

--- The dock item for `key`. `on_hover(bool)` keeps the autohide peek open.
function M.build(key, on_hover)
  local function item() return dock.item(key) end
  local function index() return dock.index_of(key) end
  local held = function() return dock.dragging() == key end
  local hovered = function() return dock.signals.hovered:get() == key end
  local vertical, edge = dock.vertical, dock.edge
  local icon = theme.dock_icon
  local node

  local function along()
    local i = index()
    if i < 1 then return 0 end
    return dock.offset_of(dock.shifted(i))
  end

  local slot = ui.Item {
    width = icon, height = icon,
    -- Against the far side of the box; the dot takes the side nearest the
    -- screen edge.
    x = function()
      if not vertical() then return 0 end
      return edge() == "right" and 0 or theme.dock_dot_lane
    end,
    y = 0,
    scale = function() return (hovered() or held()) and theme.dock_lift or 1 end,
    behavior = { scale = theme.behave("fast") },
    -- Hover highlight, kept inside the gap between icons.
    ui.Rect {
      anchors = { fill = true, margins = -3 },
      radius = theme.radius_medium,
      color = C.islandSurfaceHover,
      opacity = function() return (hovered() or held()) and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
    },
    M.picture(function()
      local it = item()
      return it and it.icon or ""
    end, icon),
  }

  -- One window is a dot, several a longer one: the accent for the focused
  -- application, muted when it runs unfocused.
  local dot = ui.Rect {
    width = function()
      local it = item()
      local reach = (it and #it.windows > 1) and theme.dock_dot * 2.8 or theme.dock_dot
      return vertical() and theme.dock_dot or reach
    end,
    height = function()
      local it = item()
      local reach = (it and #it.windows > 1) and theme.dock_dot * 2.8 or theme.dock_dot
      return vertical() and reach or theme.dock_dot
    end,
    radius = theme.dock_dot / 2,
    color = function()
      local it = item()
      if it and it.active then return C.accent() end
      return C.textMuted()
    end,
    x = function()
      if edge() == "right" then return theme.dock_depth() - theme.dock_dot end
      if edge() == "left" then return 0 end
      local it = item()
      local reach = (it and #it.windows > 1) and theme.dock_dot * 2.8 or theme.dock_dot
      return (icon() - reach) / 2
    end,
    y = function()
      if edge() == "bottom" then return theme.dock_depth() - theme.dock_dot end
      local it = item()
      local reach = (it and #it.windows > 1) and theme.dock_dot * 2.8 or theme.dock_dot
      return (icon() - reach) / 2
    end,
    opacity = function()
      local it = item()
      return (it and it.running) and 1 or 0
    end,
    behavior = {
      opacity = theme.behave("medium"),
      width = theme.behave("fast"),
      height = theme.behave("fast"),
      color = theme.behave("fast"),
    },
  }

  -- Drag state for this press: where along the capsule it started.
  local grabbed = false
  local area = ui.MouseArea {
    anchors = { fill = true },
    cursor = function() return held() and "grabbing" or "pointer" end,
    accepted_buttons = { "left", "right", "middle" },
    on_entered = function()
      dock.signals.hovered:set(key)
      on_hover(true)
    end,
    on_exited = function()
      -- The next item can be entered before this one is left, so only
      -- clear the hover if it still points here.
      if dock.signals.hovered:get() == key then dock.signals.hovered:set("") end
      on_hover(false)
    end,
    -- A drag takes the press, so a press that moved never arrives as a click.
    on_clicked = function(_, _, _, _, button)
      local it = item()
      if not it then return end
      if button == "right" then
        dock.open_menu(key)
      elseif button == "middle" then
        dock.close_menu()
        dock.launch(it)
      else
        dock.close_menu()
        dock.activate(it)
      end
    end,
    -- Only pinned applications reorder, and only among themselves.
    on_drag_started = function()
      local it = item()
      if not it or not it.pinned or dock.pinned_count() < 2 then return end
      grabbed = true
      dock.close_menu()
      morf.animation.set_enabled(node, "translate_x", false)
      morf.animation.set_enabled(node, "translate_y", false)
      dock.begin_drag(key, it.index)
    end,
    on_dragged = function(_, _, dx, dy)
      if not grabbed then return end
      local base = dock.offset_of(dock.signals.drag_from:get())
      local lo = theme.dock_padding + dock.lead()
      local hi = dock.offset_of(dock.pinned_count())
      local delta = vertical() and dy or dx
      local at = math.max(lo, math.min(hi, base + delta))
      if vertical() then node.translate_y = at - base else node.translate_x = at - base end
      dock.set_drop(dock.slot_at(at + icon() / 2))
    end,
    on_drag_finished = function()
      if not grabbed then return end
      grabbed = false
      -- The icon leaves where it was dropped: its slot moves to the new
      -- place while the offset it was dragged by runs out, both animated.
      morf.animation.set_enabled(node, "translate_x", true)
      morf.animation.set_enabled(node, "translate_y", true)
      dock.end_drag()
      node.translate_x = 0
      node.translate_y = 0
    end,
  }

  node = ui.Item {
    width = function() return vertical() and theme.dock_depth() or icon() end,
    height = function() return vertical() and icon() or theme.dock_depth() end,
    x = function() return vertical() and theme.dock_padding or along() end,
    y = function() return vertical() and along() or theme.dock_padding end,
    z = function() return held() and 2 or (hovered() and 1 or 0) end,
    translate_x = 0, translate_y = 0,
    behavior = {
      x = theme.behave("medium"),
      y = theme.behave("medium"),
      translate_x = theme.behave("medium"),
      translate_y = theme.behave("medium"),
    },
    slot, dot, area,
  }
  return node
end

return M
