-- Selections (the Selection archetype): tabs, segmented choices, list and
-- grid selection, swatch grids. The archetype keeps the current item and
-- the selected set and answers the keys; this lays out an item per entry
-- of `spec.items` with the skin's `item` delegate and keeps the current
-- item's box in the live state, so a skin's `indicator` can travel to it.
--
--     selection.make("tabs", {
--       items = { "Overview", "Media", "Weather" },     -- or a function
--       current = function() return tab:get() end,
--       on_current_changed = function(i) tab:set(i) end,
--       orientation = "horizontal",                     -- vertical, grid
--       columns = 4, gap = 0, item_width = 80, item_height = 36,
--     })
--
-- A skin's `item(index, value, s)` builds one entry; `s` reads its state:
-- `s.current()`, `s.selected()`, `s.hovered()`, `s.down()`, and `s.area`
-- (the entry's MouseArea). A skin may also give `place(index, value)`, the
-- entry area's own properties (`x`, `width`, `layout`, ...), and
-- `container()`, the node the entries go in (a Row, a Column or a Grid by
-- `orientation` otherwise). `spec.item_id(index, value)` names each
-- entry's area; `spec.delegate(index, value, s)`, when given, draws the
-- entries instead of the skin (a layout's own swatches); and
-- `spec.press_activates` makes one press activate an entry, not only
-- choose it. The live state adds `current_x`, `current_y`,
-- `current_width`, `current_height`: the current entry's box within the
-- control.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local function label_of(item)
  if type(item) == "table" then return tostring(item.label or item.name or item.text or "") end
  return tostring(item)
end

function M.make(widget, spec)
  spec = spec or {}
  local function items()
    local v = spec.items
    if type(v) == "function" then v = v() end
    return v or {}
  end
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.count = function() return #items() end
  full.labels = function()
    local out = {}
    for i, item in ipairs(items()) do out[i] = label_of(item) end
    return out
  end
  local orientation = spec.orientation or "horizontal"
  local function default_container()
    if orientation == "vertical" then return ui.Column { gap = spec.gap or 0 } end
    if orientation == "grid" then return ui.Grid { columns = spec.columns or 4, gap = spec.gap or 0 } end
    return ui.Row { gap = spec.gap or 0 }
  end
  local container
  local areas, root = {}, nil
  local live, deliver = nil, nil
  local function build_items(builders, t, send, fresh)
    live, deliver = t, send
    for _, area in ipairs(areas) do ui.destroy(area, true) end
    areas = {}
    -- On a theme switch the entries' container is the new skin's too.
    if fresh or not container then
      if container then ui.destroy(container, true) end
      container = builders.container and builders.container() or default_container()
      if root then ui.reparent(container, root) end
    end
    local make_item = spec.delegate or builders.item
    for index, value in ipairs(items()) do
      local area
      local s = {
        current = function() return live.current == index end,
        selected = function() return control.has(live.selected, index) end,
        hovered = function() return area and area.hovered or false end,
        down = function() return area and area.pressed or false end,
      }
      -- A reorderable entry drags along the row (a headless Drag): past a
      -- neighbour, it asks to swap with it.
      local dragging
      if spec.reorderable then
        dragging = control.headless("Drag", { mode = "reorder",
          axis = (spec.orientation == "vertical") and "y" or "x",
          extent = (spec.orientation == "vertical") and (spec.item_height or 36) or (spec.item_width or 80),
          on_reorder = function(step) if spec.on_reorder then spec.on_reorder(index, step) end end })
      end
      local props = { width = spec.item_width, height = spec.item_height, cursor = "pointer",
        id = spec.item_id and spec.item_id(index, value) or nil,
        on_pressed = function(sx, sy)
          deliver("item_pressed", index, "")
          if spec.press_activates then deliver("item_activated", index) end
          if dragging then dragging.send("pressed", sx, sy) end
        end,
        on_dragged = dragging and function(sx, sy) dragging.send("dragged", sx, sy) end or nil,
        on_released = dragging and function() dragging.send("released", 0, 0) end or nil,
        on_double_clicked = function() deliver("item_activated", index) end,
      }
      for k, v in pairs(builders.place and builders.place(index, value) or {}) do props[k] = v end
      area = ui.MouseArea(props)
      s.area = area
      local look = make_item and make_item(index, value, s)
      if look then ui.reparent(look, area) end
      areas[index] = area
      ui.reparent(area, container)
    end
  end
  local node, t, ctl = control.make("Selection", widget, full, {
    builders = { item = true, place = true, container = true },
    state = { current_x = 0, current_y = 0, current_width = 0, current_height = 0 },
    on_rebuild = function(builders, t, send) build_items(builders, t, send, true) end,
  })
  root = node
  ui.reparent(container, root)
  -- A new list of entries: new delegates.
  local seen
  morf.effect("kit.selection.items." .. ctl.id, function()
    local list = items()
    local key = #list .. ":" .. table.concat(full.labels(), "\0")
    if seen ~= nil and key ~= seen then build_items(ctl.builders(), t, ctl.send) end
    seen = key
  end, { owner = root })
  -- The current entry's box, for the indicator.
  morf.effect("kit.selection.current." .. ctl.id, function()
    local area = areas[t.current]
    if not area then return end
    t.current_x = (area.layout_x or 0) - (root.layout_x or 0)
    t.current_y = (area.layout_y or 0) - (root.layout_y or 0)
    t.current_width = area.layout_width or 0
    t.current_height = area.layout_height or 0
  end, { owner = root })
  return node, t, ctl
end

return M
