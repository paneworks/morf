-- The card of blocks shown while the grid is arranged: ControlsTray.qml.
--
-- Every block at the smallest size it offers, which is the size it is
-- added at, as its real face, packed two cells wide, tallest first. Click
-- one to add it on the first free cells, or drag it onto the grid: the
-- ghost follows the pointer and the landing cells light. A block dragged
-- from the grid and let go over the card is removed (Block.qml:150-157).
--
-- impasto floats this card on the bar's surface, under the island, where
-- it can be moved and resized. Here it stands beside the grid inside the
-- panel, which grows by it while arranging, so a drag between the two
-- stays in one coordinate space; the wheel scrolls it. Positions are in
-- board pixels, the tray starting at `service.tray_x`.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")

local C = theme.color
local M = {}

local F = service.tray_factor
local PAD = service.tray_pad

M.pulling = morf.signal("impasto.controls.tray.pulling", "")
M.ghost_x = morf.signal("impasto.controls.tray.ghost.x", 0)
M.ghost_y = morf.signal("impasto.controls.tray.ghost.y", 0)
-- A block from the grid is over the card: letting go removes it.
M.receiving = morf.signal("impasto.controls.tray.receiving", false)
local scroll = morf.signal("impasto.controls.tray.scroll", 0)

function M.smallest(id) return service.sizes_for(id)[1] or "2x2" end

-- ---------------------------------------------------------------- packing --

--- The catalogue packed into two columns: `{ id, name, size, x, y, width,
--- height }` in tray pixels (scaled), and the height they reach.
function M.packed()
  local entries = {}
  for _, entry in ipairs(service.catalogue) do
    local size = M.smallest(entry.id)
    local cols, rows = service.parse(size)
    entries[#entries + 1] = { id = entry.id, name = entry.name, size = size, cols = cols, rows = rows }
  end
  -- Tallest first, then widest, then by name.
  table.sort(entries, function(a, b)
    if a.rows ~= b.rows then return a.rows > b.rows end
    if a.cols ~= b.cols then return a.cols > b.cols end
    return a.name < b.name
  end)
  local gap = theme.centre_gutter * F
  local unit = (theme.centre_cell_width + theme.centre_gutter) * F
  local heights = { 0, 0 }
  for _, entry in ipairs(entries) do
    local w, h = service.pixels(entry.size)
    entry.width, entry.height = w * F, h * F
    if entry.cols >= 2 then
      entry.x, entry.y = 0, math.max(heights[1], heights[2])
      heights[1] = entry.y + entry.height + gap
      heights[2] = heights[1]
    else
      local column = heights[1] <= heights[2] and 1 or 2
      entry.x, entry.y = (column - 1) * unit, heights[column]
      heights[column] = entry.y + entry.height + gap
    end
  end
  return entries, math.max(heights[1], heights[2]) - gap
end

-- ------------------------------------------------------------------ faces --

--- A block's face at its full size, scaled by `factor` about its top left.
--- `build` is `bar.controls.blocks`.build; the face takes no input.
local function face(blocks, id, size, factor)
  local cols, rows = service.parse(size)
  local w, h = service.pixels(size)
  return ui.Item {
    width = w * factor, height = h * factor,
    ui.Item {
      x = -(w - w * factor) / 2, y = -(h - h * factor) / 2,
      width = w, height = h, scale = factor,
      blocks.build {
        key = "", id = id, size = size, cols = cols, rows = rows, width = w, height = h,
        on_panel = function() end, on_dismiss = function() end,
      },
    },
  }
end

-- ------------------------------------------------------------------- aim --

--- The ghost under the pointer (board pixels), and the landing cells under
--- the ghost, or none while the pointer is over the card.
local function aim(px, py)
  local id = M.pulling:get()
  local size = M.smallest(id)
  local w, h = service.pixels(size)
  local gx, gy = px - w / 2, py - h / 2
  M.ghost_x:set(gx)
  M.ghost_y:set(gy)
  if service.over_tray(px, py) then
    service.set_landing(nil)
    return
  end
  local spot = service.nearest_free(service.cell_x(gx), service.cell_y(gy), size, "")
  service.set_landing(spot, size)
end

--- The ghost, for the board: the dragged block at its landing size.
function M.ghost(blocks)
  return ui.Loader {
    active = function() return M.pulling:get() ~= "" end,
    source = function()
      local id = M.pulling:get()
      local size = M.smallest(id)
      local w, h = service.pixels(size)
      return ui.Item {
        x = function() return M.ghost_x:get() end,
        y = function() return M.ghost_y:get() end,
        width = w, height = h, opacity = 0.9,
        ui.Rect {
          anchors = { fill = true, margins = -6 }, radius = theme.radius_large,
          color = C.island, border_width = 2, border_color = C.accent,
        },
        face(blocks, id, size, 1),
      }
    end,
  }
end

-- ------------------------------------------------------------------ card --

--- The card, `service.tray_width` by `height`.
function M.build(blocks, height)
  local entries, reach = M.packed()
  local view_h = height - 2 * PAD
  local most = math.max(0, reach - view_h)
  local function scroll_by(pixels)
    scroll:set(math.max(0, math.min(most, scroll:get() + pixels)))
  end
  scroll:set(math.min(scroll:get(), most))

  local tiles = { x = 0, y = function() return -scroll:get() end, width = service.tray_width - 2 * PAD,
    height = reach }
  for _, entry in ipairs(entries) do
    local hovered = controls.signal("tray.tile", false)
    local start = { x = 0, y = 0 }
    local pulled = function() return M.pulling:get() == entry.id end
    tiles[#tiles + 1] = ui.Item {
      x = entry.x, y = entry.y, width = entry.width, height = entry.height,
      face(blocks, entry.id, entry.size, F),
      ui.Rect {
        anchors = { fill = true }, radius = theme.radius_medium * F, color = "#00000000",
        border_width = 2,
        border_color = function()
          if pulled() then return C.accent() end
          return hovered:get() and C.islandBorder or "#00000000"
        end,
      },
      -- Over the face, so the face's own buttons take nothing.
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return pulled() and "grabbing" or "grab" end,
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() service.add(entry.id) end,
        on_drag_started = function(_, _, _, _, lx, ly)
          start.x = service.tray_x + PAD + entry.x + (lx or 0)
          start.y = PAD + entry.y - scroll:get() + (ly or 0)
          M.pulling:set(entry.id)
          aim(start.x, start.y)
        end,
        on_dragged = function(_, _, dx, dy)
          if pulled() then aim(start.x + dx, start.y + dy) end
        end,
        on_drag_finished = function()
          if not pulled() then return end
          local col, row = service.landing_col:get(), service.landing_row:get()
          M.pulling:set("")
          service.set_landing(nil)
          if col >= 0 then service.add(entry.id, col, row) end
        end,
      },
    }
  end

  return ui.Rect {
    id = "controls-tray",
    width = service.tray_width, height = height,
    radius = theme.radius_large, color = C.island, border_width = function() return M.receiving:get() and 2 or 1 end,
    border_color = function() return M.receiving:get() and C.accent() or C.islandBorder end,
    behavior = { border_color = theme.behave("fast") },
    ui.MouseArea {
      anchors = { fill = true },
      on_wheel = function(_, _, _, pixel_y, _, step_y)
        local delta = (pixel_y and pixel_y ~= 0) and pixel_y or ((step_y or 0) * 48)
        scroll_by(delta)
      end,
    },
    ui.ClipRect {
      x = PAD, y = PAD, width = service.tray_width - 2 * PAD, height = view_h, color = "#00000000",
      ui.Item(tiles),
    },
    kit.text {
      anchors = { horizontal_center = true, bottom = true, bottom_margin = -18 },
      text = "Click or drag in · drop here to remove",
      size = theme.size.label, color = C.textMuted,
    },
  }
end

return M
