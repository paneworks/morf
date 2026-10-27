-- Selections (the Selection archetype): toggle groups, view switchers,
-- carousel indicators, and the grids and lists composites choose in (a
-- calendar's days, a colour chooser's swatches, an emoji chooser's grid, a
-- sidebar's rows). The archetype keeps the current item and the selected
-- set and answers the keys; this lays out an item per entry of
-- `spec.items` with the skin's `item` delegate and keeps the current
-- item's box in the live state, so a skin's `indicator` can travel to it.
--
--     selection.make("toggle_group", {
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
-- choose it. A table entry's `accessible_role` overrides the role its
-- area takes (a padding cell's "generic"). The live state adds `current_x`,
-- `current_y`, `current_width`, `current_height`: the current entry's box
-- within the control.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- What each widget sets unless its spec says otherwise. (Entry sizes are a
-- fallback for a configuration that names none.)
local DEFAULTS = {
  toggle_group = { orientation = "horizontal", item_width = 80, item_height = 34 },
  view_switcher = { orientation = "horizontal", item_width = 96, item_height = 52 },
  inline_view_switcher = { orientation = "horizontal", item_width = 96, item_height = 34 },
  view_switcher_bar = { orientation = "horizontal", item_width = 96, item_height = 52 },
  stack_switcher = { orientation = "horizontal", item_width = 96, item_height = 34 },
  carousel_indicator_dots = { orientation = "horizontal", item_width = 16, item_height = 16 },
  carousel_indicator_lines = { orientation = "horizontal", item_width = 24, item_height = 12 },
  sidebar_list = { orientation = "vertical", item_width = 220, item_height = 40 },
  day_grid = { orientation = "grid", columns = 7, item_width = 40, item_height = 36 },
  swatch_grid = { orientation = "grid", item_width = 32, item_height = 32 },
  emoji_grid = { orientation = "grid", item_width = 40, item_height = 40 },
}

local function label_of(item)
  if type(item) == "table" then return tostring(item.label or item.name or item.text or item.caption or item.title or "") end
  return tostring(item)
end

function M.make(widget, spec)
  local full = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do full[k] = v end
  for k, v in pairs(spec or {}) do full[k] = v end
  spec = full
  local function items()
    local v = spec.items
    if type(v) == "function" then v = v() end
    return v or {}
  end
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
    -- Each entry is a tab, an option, a cell to a screen reader.
    local _, item_role = control.roles("Selection", widget)
    for index, value in ipairs(items()) do
      local area
      local s = {
        current = function() return live.current == index end,
        selected = function() return control.has(live.selected, index) end,
        hovered = function() return area and area.hovered or false end,
        down = function() return area and area.pressed or false end,
      }
      -- A reorderable entry (a tab bar's tab) drags along the row (a
      -- headless Drag): past a neighbour, it asks to swap with it.
      local dragging
      if spec.reorderable then
        local axis = orientation == "vertical" and "y" or "x"
        dragging = control.headless("Drag", { mode = "reorder", axis = axis,
          extent = axis == "y" and (spec.item_height or 36) or (spec.item_width or 80),
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
        drag_axis = dragging and (orientation == "vertical" and "y" or "x") or nil,
        on_released = dragging and function() dragging.send("released", 0, 0) end or nil,
        on_double_clicked = function() deliver("item_activated", index) end,
        accessible_role = (type(value) == "table" and value.accessible_role) or item_role or "list_box_option",
        accessible_name = label_of(value),
        accessible = function() return { selected = s.current() or s.selected() } end,
        -- A screen reader's press is the pointer's.
        on_accessible_action = function(action)
          if action ~= "click" and action ~= "focus" then return false end
          if root then morf.focus.set(root, true) end
          deliver("item_pressed", index, "")
          if action == "click" then deliver("item_activated", index) end
          return true
        end,
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
  local state = { current_x = 0, current_y = 0, current_width = 0, current_height = 0 }
  local node, t, ctl = control.make("Selection", widget, full, {
    builders = { item = true, place = true, container = true },
    state = state,
    on_rebuild = function(builders, t, send) build_items(builders, t, send, true) end,
  })
  root = node
  ui.reparent(container, root)
  -- A new list of entries: new delegates.
  local seen
  morf.effect("kit.selection.items." .. ctl.id, function()
    local list = items()
    local key = #list .. ":" .. table.concat(full.labels(), "\0")
    if seen ~= nil and key ~= seen then
      build_items(ctl.builders(), t, ctl.send)
    end
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
