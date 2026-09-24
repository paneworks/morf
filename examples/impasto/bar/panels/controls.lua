-- The control centre: a row of buttons over a grid of blocks the person
-- arranges like desktop widgets.
--
-- Port of ControlsPanel.qml and Block.qml. The session actions sit at the
-- left of the row and the doors -- buttons opening other panels -- at the
-- right. Under them, `services.controls` says which blocks sit where.
--
-- Right-click the background to arrange: drag a block to move it (it lands
-- on the lit cells, or the nearest free fit), pull its corner or scroll
-- over it to resize, and press its badge to remove it. Closing the panel
-- ends arranging, so it never reopens swallowing clicks. Tiles that lead to
-- a list (Wi-Fi, Bluetooth) open it as a panel of its own.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")
local modules = require("services.modules")
local blocks = require("bar.controls.blocks")
local power_row = require("bar.controls.power_row")

local C = theme.color
local M = {}

local function open_panel(name)
  if name == "" then modules.request_settings() return end
  island.open(name)
end

-- -------------------------------------------------------------------- block --

-- The drag in progress, in board pixels.
local drag_x = morf.signal("impasto.controls.drag.x", 0)
local drag_y = morf.signal("impasto.controls.drag.y", 0)

local function badge(values)
  return ui.Rect {
    width = 20, height = 20, radius = 10,
    x = values.x, y = values.y,
    color = C.island, border_width = function() return values.lit and values.lit() and 2 or 1 end,
    border_color = function() return values.lit and values.lit() and C.accent() or C.islandBorder end,
    opacity = values.opacity,
    behavior = { opacity = theme.behave("fast") },
    values[1], values[2],
  }
end

local function block(row)
  local key = row.key
  local entry = function() return service.entry_of(key) end
  local geometry = function()
    local b = entry()
    return b and service.geometry(b) or { x = 0, y = 0, width = 0, height = 0 }
  end
  local size = function() return service.size_of(entry()) end
  local held = function() return service.dragging:get() == key end
  local selected = function() return service.selected:get() == key end
  local editing = function() return service.editing:get() end
  local hovered = controls.signal("block.hover", false)
  local resizing = controls.signal("block.resize", false)
  local dressed = function() return editing() and not held() and (hovered:get() or selected() or resizing:get()) end
  local start = { x = 0, y = 0 }

  -- One loader per size the block offers, so a resize builds the face for
  -- its new cells while a move keeps the one it has (a list keeps its
  -- scroll, a player keeps playing).
  local b0 = service.entry_of(key)
  local faces = { anchors = { fill = true } }
  for _, offered in ipairs(service.sizes_for(b0 and b0.id or "")) do
    local cols, rows = service.parse(offered)
    local w, h = service.pixels(offered)
    faces[#faces + 1] = ui.Loader {
      x = 0, y = 0, width = w, height = h,
      active = function() return entry() ~= nil and size() == offered end,
      source = function()
        local b = entry()
        return blocks.build {
          key = key, id = b and b.id or "", size = offered, cols = cols, rows = rows,
          width = w, height = h,
          on_panel = open_panel,
          on_dismiss = function() island.close() end,
        }
      end,
    }
  end

  local function aim(x, y)
    local spot = service.nearest_free(service.cell_x(x), service.cell_y(y), size(), key)
    service.set_landing(spot, size())
  end

  return ui.Item {
    x = function() return held() and drag_x:get() or geometry().x end,
    y = function() return held() and drag_y:get() or geometry().y end,
    width = function() return geometry().width end,
    height = function() return geometry().height end,
    z = function() return held() and 2 or (selected() and 1 or 0) end,
    visible = function() return entry() ~= nil end,
    behavior = {
      width = theme.behave("medium"), height = theme.behave("medium"),
    },
    ui.Item(faces),
    -- Arranging: an outline, and an area over the face that takes the
    -- drag, so a drag cannot also press a button.
    ui.Rect {
      anchors = { fill = true, margins = -3 },
      visible = editing,
      radius = theme.radius_medium + 3, color = "#00000000",
      border_width = function() return (held() or selected()) and 2 or 1 end,
      border_color = function() return (held() or selected()) and C.accent() or C.hairline end,
    },
    ui.MouseArea {
      anchors = { fill = true },
      visible = editing,
      cursor = function() return held() and "grabbing" or "grab" end,
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        service.selected:set(selected() and "" or key)
      end,
      on_drag_started = function()
        local g = geometry()
        start.x, start.y = g.x, g.y
        drag_x:set(g.x)
        drag_y:set(g.y)
        service.dragging:set(key)
        service.selected:set("")
      end,
      on_dragged = function(_, _, dx, dy)
        if not held() then return end
        local w, h = geometry().width, geometry().height
        local x = math.max(0, math.min(service.board_width - w, start.x + dx))
        local y = math.max(0, math.min(service.board_height - h, start.y + dy))
        drag_x:set(x)
        drag_y:set(y)
        aim(x, y)
      end,
      on_drag_finished = function()
        if not held() then return end
        service.dragging:set("")
        service.set_landing(nil)
        service.place(key, service.cell_x(drag_x:get()), service.cell_y(drag_y:get()))
      end,
      -- One size per detent.
      on_wheel = function(_, _, _, _, _, steps)
        if steps and steps ~= 0 then service.cycle_size(key, steps > 0 and 1 or -1) end
      end,
    },
    -- The badge removes the block.
    badge {
      x = -7, y = -7,
      opacity = function() return dressed() and 1 or 0 end,
      ui.Rect { anchors = { center_in = true }, width = 8, height = 2, radius = 1, color = C.scrimText },
      ui.MouseArea { anchors = { fill = true }, cursor = "pointer", visible = dressed,
        on_clicked = function() service.remove(key) end },
    },
    -- The handle resizes it, snapping between the sizes it offers.
    badge {
      x = function() return geometry().width - 13 end,
      y = function() return geometry().height - 13 end,
      lit = function() return resizing:get() end,
      opacity = function() return (dressed() or resizing:get()) and 1 or 0 end,
      ui.Item {
        anchors = { center_in = true }, width = 8, height = 8,
        ui.Rect { anchors = { right = true, bottom = true }, width = 8, height = 2, radius = 1, color = C.scrimText },
        ui.Rect { anchors = { right = true, bottom = true }, width = 2, height = 8, radius = 1, color = C.scrimText },
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "nwse_resize", visible = editing,
        on_entered = function() hovered:set(true) end,
        on_drag_started = function()
          local g = geometry()
          start.x, start.y = g.width, g.height
          resizing:set(true)
        end,
        on_dragged = function(_, _, dx, dy)
          local b = entry()
          if not b then return end
          local cols = (start.x + dx + theme.centre_gutter) / theme.centre_stride_x
          local rows = (start.y + dy + theme.centre_gutter) / theme.centre_stride_y
          local next = service.size_nearest(b.id, cols, rows)
          if next ~= size() then service.set_size(key, next) end
        end,
        on_drag_finished = function() resizing:set(false) end,
      },
    },
  }
end

-- -------------------------------------------------------------------- panel --

function M.build()
  local board_w, board_h = service.board_width, service.board_height

  -- The doors, in the order the settings keep them.
  local doors = { anchors = { right = true, vertical_center = true }, gap = 12, align = "center" }
  for _, door in ipairs(service.shown_doors()) do
    doors[#doors + 1] = controls.icon_button {
      icon = door.icon, icon_size = 14,
      on_click = function() open_panel(door.panel) end,
    }
  end

  -- The cells, only while arranging.
  local lattice = { anchors = { fill = true }, visible = function() return service.editing:get() end }
  for index = 0, service.columns * service.rows - 1 do
    lattice[#lattice + 1] = ui.Rect {
      x = service.offset_x(index % service.columns),
      y = service.offset_y(index // service.columns),
      width = theme.centre_cell_width, height = theme.centre_cell_height,
      radius = theme.radius_small, color = "#00000000",
      border_width = 1, border_color = C.hairline,
    }
  end

  local landing_shown = function()
    return service.editing:get() and service.landing_col:get() >= 0
  end
  local landing = ui.Rect {
    visible = landing_shown,
    x = function() return service.offset_x(math.max(0, service.landing_col:get())) end,
    y = function() return service.offset_y(math.max(0, service.landing_row:get())) end,
    width = function() local w = service.pixels(service.landing_size:get()) return w end,
    height = function() local _, h = service.pixels(service.landing_size:get()) return h end,
    radius = theme.radius_medium,
    color = function() return C.accent():alpha(0.14) end,
    border_width = 2, border_color = C.accent,
    behavior = { x = theme.behave("fast"), y = theme.behave("fast"),
      width = theme.behave("fast"), height = theme.behave("fast") },
  }

  return ui.Column {
    gap = service.row_gap,
    ui.Item {
      width = board_w, height = service.row_height,
      ui.Item { anchors = { left = true, vertical_center = true }, height = 28, width = 300,
        power_row.build(function() island.close() end) },
      ui.Row(doors),
    },
    ui.Item {
      width = board_w, height = board_h,
      -- Beneath everything: blocks take the left button and the right one
      -- falls through to toggle arranging. A left click on the ground
      -- closes the selection.
      ui.MouseArea {
        anchors = { fill = true }, z = -1,
        accepted_buttons = { "left", "right" },
        on_clicked = function(_, _, _, _, button)
          if button == "right" then service.edit(not service.editing:get())
          elseif service.editing:get() then service.selected:set("") end
        end,
      },
      ui.Item(lattice),
      landing,
      ui.Repeater {
        anchors = { fill = true },
        model = service.keys,
        delegate = block,
      },
      -- What arranging is, while it is on.
      kit.text {
        anchors = { horizontal_center = true, bottom = true, bottom_margin = -18 },
        visible = function() return service.editing:get() end,
        text = "Drag to move · pull a corner or scroll to resize · right-click to finish",
        size = theme.size.label, color = C.textMuted,
      },
    },
  }
end

island.register("controls", {
  size = function() return service.panel_width, service.panel_height end,
  build = M.build,
})

-- Closing the panel ends arranging.
morf.effect("impasto.controls.leave", function()
  if island.state.open_panel() ~= "controls" and service.editing:get() then
    service.edit(false)
  end
end)

morf.ipc.controls_edit = function()
  if island.state.open_panel() ~= "controls" then island.open("controls") end
  service.edit(not service.editing:get())
  return service.editing:get() and "editing" or "done"
end
morf.ipc.controls_add = function(id)
  return service.add(id or "")
end

return M
