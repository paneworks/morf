-- The dock: a capsule on a screen edge with the pinned and open
-- applications.
--
-- Port of Dock.qml. The original's surface covered the whole screen, so a
-- menu taller than the dock could be drawn in it; here the dock's surface
-- is a band along its edge, as deep as the capsule, its margin and the name
-- label, and the menu is a second surface on the layer above that exists
-- only while it is open. The compositor has less to blend every frame, and
-- the menu still gets the whole screen to catch a click outside it.
--
-- The input region is the MouseAreas: the capsule (and, while it hides,
-- the gap to the edge and a thin reveal strip), never the empty band
-- around it, so clicks beside the dock reach the window behind.
--
-- Geometry is worked out in screen coordinates (`capsule_box`) and each
-- surface subtracts its own origin, so the dock, its label and its menu all
-- use one answer.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local dock = require("services.dock")
local item = require("dock.item")
local menu = require("dock.menu")
local kit = require("components.kit")

local C = theme.color
local s = dock.signals
local M = {}

-- Room beside the capsule for the name label: above it on the bottom edge,
-- beside it on the sides.
M.LABEL_DEPTH = 44
M.LABEL_REACH = 240
M.LABEL_ROW = 30

-- --------------------------------------------------------------- geometry --

--- The capsule's rectangle on a `width` x `height` screen, laid out.
function M.capsule_box(width, height)
  local edge = dock.edge()
  local length = dock.length()
  local thickness = theme.dock_thickness()
  local margin = theme.dock_margin
  local alignment = dock.alignment()
  local function place(from, along)
    if alignment == "start" then return from + margin end
    if alignment == "end" then return from + along - length - margin end
    return from + (along - length) / 2
  end
  if edge == "bottom" then
    return place(0, width), height - margin - thickness, length, thickness
  end
  -- The top band is the bar's.
  local top = theme.bar_reserve()
  local x = edge == "left" and margin or width - margin - thickness
  return x, place(top, height - top), thickness, length
end

--- The dock surface's size and where it sits on the screen.
function M.surface_box(width, height, edge)
  local depth = theme.dock_margin + theme.dock_thickness()
  if edge == "bottom" then
    local h = depth + M.LABEL_DEPTH
    return 0, height - h, width, h
  end
  local w = depth + M.LABEL_REACH
  return edge == "left" and 0 or width - w, 0, w, height
end

--- Where a hidden dock goes: fully off its edge, shadow and all.
local function hidden_offset()
  local retreat = theme.dock_thickness() + theme.dock_margin
  local edge = dock.edge()
  if edge == "left" then return -retreat, 0 end
  if edge == "right" then return retreat, 0 end
  return 0, retreat
end

--- Whether the capsule is out: always unless it hides, and then while the
--- pointer is on it, a drag is running or its menu is open.
function M.out()
  return not dock.autohide() or s.peeked:get() or dock.dragging() ~= ""
    or s.menu:get() ~= ""
end

-- ------------------------------------------------------------------ hover --

-- Leaving one of the dock's areas and entering the next arrive together, so
-- a leave only retracts if nothing was entered in the next 350 ms (long
-- enough to cross the gap from the reveal strip).
local hover_generation = 0
function M.on_hover(over)
  hover_generation = hover_generation + 1
  local mine = hover_generation
  if over then
    s.peeked:set(true)
    return
  end
  morf.timer(350, function()
    if mine == hover_generation then s.peeked:set(false) end
  end, false)
end

-- The label keeps the last name through its fade-out; an empty one would
-- collapse the plate and slide it away.
local shown_key = morf.signal("impasto.dock.label", "")
morf.effect("impasto.dock.label", function()
  local key = s.hovered:get()
  if key ~= "" then shown_key:set(key) end
end)

-- Offset along the capsule of the centre of `key`'s icon.
local function centre_of(key)
  if key == dock.LAUNCHER then return dock.launcher_offset() + theme.dock_icon() / 2 end
  local index = dock.index_of(key)
  if index < 1 then return 0 end
  return dock.offset_of(dock.shifted(index)) + theme.dock_icon() / 2
end

-- ------------------------------------------------------------------ parts --

local function separator(along, visible)
  return ui.Rect {
    visible = visible,
    color = C.hairline,
    width = function() return dock.vertical() and math.floor(theme.dock_icon() * 0.5 + 0.5) or 1 end,
    height = function() return dock.vertical() and 1 or math.floor(theme.dock_icon() * 0.5 + 0.5) end,
    x = function()
      if dock.vertical() then return (theme.dock_thickness() - math.floor(theme.dock_icon() * 0.5 + 0.5)) / 2 end
      return along()
    end,
    y = function()
      if dock.vertical() then return along() end
      return (theme.dock_thickness() - math.floor(theme.dock_icon() * 0.5 + 0.5)) / 2
    end,
  }
end

-- Not a dock item: no windows, dot, pinning or drag. It asks for the
-- launcher through `on_launcher` rather than opening it.
local function launcher_button(on_launcher)
  local hovered = function() return s.hovered:get() == dock.LAUNCHER end
  return ui.Item {
    visible = dock.has_launcher,
    width = theme.dock_icon, height = theme.dock_icon,
    x = function() return dock.vertical() and theme.dock_padding + theme.dock_dot_lane / 2 or dock.launcher_offset() end,
    y = function() return dock.vertical() and dock.launcher_offset() or theme.dock_padding end,
    scale = function() return hovered() and theme.dock_lift or 1 end,
    behavior = { scale = theme.behave("fast") },
    ui.Rect {
      anchors = { fill = true, margins = -3 },
      radius = theme.radius_medium,
      color = C.islandSurfaceHover,
      opacity = function() return hovered() and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
    },
    kit.glyph {
      anchors = { center_in = true },
      glyph = "󰀻",
      size = function() return math.floor(theme.dock_icon() * 0.58 + 0.5) end,
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function()
        s.hovered:set(dock.LAUNCHER)
        M.on_hover(true)
      end,
      on_exited = function()
        if s.hovered:get() == dock.LAUNCHER then s.hovered:set("") end
        M.on_hover(false)
      end,
      on_clicked = function()
        dock.close_menu()
        if on_launcher then on_launcher() end
      end,
    },
  }
end

-- ------------------------------------------------------------------- dock --

--- The dock's surface contents for an `edge`, on a `width` x `height`
--- screen. `options.on_launcher` opens the launcher.
function M.build(width, height, edge, options)
  options = options or {}
  local function origin()
    local ox, oy = M.surface_box(width, height, edge)
    return ox, oy
  end
  local function surface_size()
    local _, _, w, h = M.surface_box(width, height, edge)
    return w, h
  end
  local function box()
    local x, y, w, h = M.capsule_box(width, height)
    local ox, oy = origin()
    return x - ox, y - oy, w, h
  end
  local function bridge() return dock.autohide() and theme.dock_margin or 0 end

  local items = ui.Repeater {
    anchors = { fill = true },
    model = dock.model,
    delegate = function(row) return item.build(row.key, M.on_hover) end,
  }

  local shelf = ui.Item {
    x = function() local x = box() return x end,
    y = function() local _, y = box() return y end,
    width = function() local _, _, w = box() return w end,
    height = function() local _, _, _, h = box() return h end,
    translate_x = function()
      local hx = hidden_offset()
      return M.out() and 0 or hx
    end,
    translate_y = function()
      local _, hy = hidden_offset()
      return M.out() and 0 or hy
    end,
    behavior = {
      width = theme.behave("medium"),
      height = theme.behave("medium"),
      translate_x = theme.behave("morph"),
      translate_y = theme.behave("morph"),
    },
    -- Under everything: hover over the whole capsule, and while it hides,
    -- the gap to the edge, so crossing from the strip is not leaving.
    ui.MouseArea {
      z = -1,
      x = function() return edge == "left" and -bridge() or 0 end,
      y = 0,
      width = function()
        local _, _, w = box()
        return w + (dock.vertical() and bridge() or 0)
      end,
      height = function()
        local _, _, _, h = box()
        return h + (dock.vertical() and 0 or bridge())
      end,
      on_entered = function() M.on_hover(true) end,
      on_exited = function() M.on_hover(false) end,
    },
    -- The island's black; with lower opacity what is behind shows through.
    -- The border keeps its own alpha, so a translucent capsule has an edge.
    ui.Rect {
      anchors = { fill = true },
      radius = theme.dock_radius,
      color = function() return C.island:alpha(dock.opacity()) end,
      border_width = 1,
      border_color = C.islandBorder,
      shadow_color = function()
        return settings.windowShadow and morf.color("#000000"):alpha(theme.shadow.opacity) or morf.color("#00000000")
      end,
      shadow_blur = function() return settings.windowShadow and theme.shadow.range or 0 end,
    },
    launcher_button(options.on_launcher),
    items,
    -- After the launcher button, when there is anything after it.
    separator(function() return dock.launcher_offset() + theme.dock_icon() + theme.dock_gap end,
      function() return dock.has_launcher() and dock.count() > 0 end),
    -- Between pinned and unpinned applications, only when both exist.
    separator(function()
      return dock.offset_of(dock.pinned_count()) + theme.dock_icon() + theme.dock_gap
    end, dock.divides),
  }
  local children = { shelf }

  -- A thin strip on the edge a hidden dock still listens on.
  local reveal = ui.MouseArea {
    visible = function() return dock.autohide() and not M.out() end,
    x = function()
      local w = surface_size()
      return edge == "right" and w - theme.dock_reveal or 0
    end,
    y = function()
      local _, h = surface_size()
      return edge == "bottom" and h - theme.dock_reveal or 0
    end,
    width = function()
      local w = surface_size()
      return dock.vertical() and theme.dock_reveal or w
    end,
    height = function()
      local _, h = surface_size()
      return dock.vertical() and h or theme.dock_reveal
    end,
    on_entered = function() M.on_hover(true) end,
    on_exited = function() M.on_hover(false) end,
  }
  children[#children + 1] = reveal

  -- The hovered icon's name, on the inner side of the capsule. Hidden while
  -- dragging or while a menu is open.
  local name = kit.text {
    text = function()
      local key = shown_key:get()
      if key == dock.LAUNCHER then return "Applications" end
      local it = dock.item(key)
      return it and it.name or ""
    end,
    size = theme.size.small, weight = 600,
  }
  -- The plate fits its text; it is centred (or pushed against the capsule's
  -- side) by a fixed-width row, so nothing waits on a measurement.
  local plate = ui.Rect {
    radius = theme.radius_medium,
    color = C.island,
    border_width = 1,
    border_color = C.islandBorder,
    ui.Inset { left_margin = 10, right_margin = 10, top_margin = 6, bottom_margin = 6, name },
  }
  local lane = ui.Row {
    width = M.LABEL_REACH, height = M.LABEL_ROW,
    align = "center",
    justify = function()
      if not dock.vertical() then return "center" end
      return edge == "left" and "start" or "end"
    end,
    x = function()
      local bx, _, bw = box()
      if dock.vertical() then
        return edge == "left" and bx + bw + theme.dock_gap or bx - M.LABEL_REACH - theme.dock_gap
      end
      local w = surface_size()
      local want = bx + centre_of(shown_key:get()) - M.LABEL_REACH / 2
      return math.max(0, math.min(w - M.LABEL_REACH, want))
    end,
    y = function()
      local _, by = box()
      if dock.vertical() then
        local _, h = surface_size()
        local want = by + centre_of(shown_key:get()) - M.LABEL_ROW / 2
        return math.max(theme.dock_margin, math.min(h - M.LABEL_ROW - theme.dock_margin, want))
      end
      return by - M.LABEL_ROW - theme.dock_gap
    end,
    opacity = function()
      local over = s.hovered:get() ~= ""
      return (over and M.out() and dock.dragging() == "" and s.menu:get() == "") and 1 or 0
    end,
    behavior = { opacity = theme.behave("fast") },
    plate,
  }
  children[#children + 1] = lane

  local w, h = surface_size()
  local root = ui.Item {
    width = w, height = h,
    table.unpack(children),
  }
  return root
end

-- ------------------------------------------------------------------- menu --

--- The menu surface's contents: the whole screen, a click anywhere outside
--- the plate closes it.
function M.build_menu(width, height)
  local plate = menu.build()
  local placed
  placed = ui.Item {
    x = function()
      local bx, _, bw = M.capsule_box(width, height)
      local pw = theme.dock_menu_width
      local key = s.menu:get()
      if dock.vertical() then
        return dock.edge() == "left" and bx + bw + theme.dock_gap or bx - pw - theme.dock_gap
      end
      local want = bx + centre_of(key) - pw / 2
      return math.max(theme.dock_margin, math.min(width - pw - theme.dock_margin, want))
    end,
    y = function()
      local _, by = M.capsule_box(width, height)
      local ph = menu.height()
      local key = s.menu:get()
      if dock.vertical() then
        local want = by + centre_of(key) - ph / 2
        return math.max(theme.dock_margin, math.min(height - ph - theme.dock_margin, want))
      end
      return by - ph - theme.dock_gap
    end,
    plate,
  }
  return ui.Item {
    width = width, height = height,
    ui.MouseArea {
      anchors = { fill = true },
      z = -1,
      accepted_buttons = "all",
      on_pressed = function() dock.close_menu() end,
    },
    placed,
  }
end

return M
