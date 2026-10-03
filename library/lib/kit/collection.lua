-- Collections (the Collection archetype): lists, grids, tables and trees
-- over a model, through the engine's list view -- only the rows in sight
-- are built, and a row scrolled out is rebound to the row scrolled in.
--
--     collection.make("list", {
--       rows = model,                 -- a morf.list_model (or a plain array)
--       width = 300, height = 400, row_height = 36,
--       size_field = "height",        -- rows that say how tall they are
--       kind_field = "kind",          -- delegates only reused within a kind
--       on_activated = function(i) end, on_end_reached = load_more,
--     })
--     collection.make("table", { rows = files, columns = {
--       { key = "name", title = "Name", width = 220, sortable = true },
--       { key = "size", title = "Size", width = 90, sortable = true } },
--       on_sort_changed = function(key, ascending) end })
--     collection.make("tree", { tree = { { key = "a", label = "A", children = { ... } } } })
--
-- The skin draws each row with its `row(row, s)` builder -- returning a node
-- and an updater `function(row)` that rebinds it -- where `s` reads the
-- row's state as it is now bound: `s.index()`, `s.current()`,
-- `s.selected()`, `s.hovered()`, `s.down()`, and in a tree `s.depth()`,
-- `s.expandable()`, `s.expanded()`; a table's cells come from the skin's
-- `cell(row, column, s)` and its header from `header(column, t)`. A spec's
-- own `delegate(row, s)` (returning node and updater too) draws the rows
-- instead.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local function label_of(row)
  if type(row) == "table" then return tostring(row.label or row.name or row.title or row.key or "") end
  return tostring(row)
end

--- A tree's shown rows: depth first, the children of an expanded node under it.
local function flatten(nodes, expanded, depth, out)
  for _, node in ipairs(nodes or {}) do
    local key = tostring(node.key or node.label)
    local children = node.children and #node.children > 0
    local row = {}
    for k, v in pairs(node) do if k ~= "children" then row[k] = v end end
    row.key, row.depth, row.expandable, row.expanded = key, depth, children or false, expanded[key] == true
    out[#out + 1] = row
    if children and expanded[key] then flatten(node.children, expanded, depth + 1, out) end
  end
  return out
end

function M.make(widget, spec)
  spec = spec or {}
  local layout = spec.layout or (widget == "grid_view" and "grid") or (widget == "data_table" and "table")
    or (widget == "tree_view" and "tree") or widget
  if layout ~= "grid" and layout ~= "table" and layout ~= "tree" and layout ~= "flow" then layout = "list" end
  local model = spec.rows
  if type(model) ~= "userdata" then model = morf.list_model(model or {}) end
  -- A tree's rows are its nodes as far as they are expanded.
  local expanded = {}
  local function reflatten()
    local rows = flatten(spec.tree, expanded, 0, {})
    model:replace(rows, "key")
  end
  if layout == "tree" then reflatten() end
  local columns = spec.columns or {}
  local W, H = spec.width or 300, spec.height or 300
  local header_h = layout == "table" and (spec.header_height or 32) or 0
  local row_h = spec.row_height or 36
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget, full.layout = widget, layout
  full.count = function() return model:len() end
  -- Typeahead reads the rows' labels; past a thousand rows it would cost
  -- more than it finds, so a long list goes without (`typeahead = true`
  -- asks anyway).
  full.labels = function()
    local out = {}
    local n = model:len()
    if n > 1000 and not spec.typeahead then return out end
    for i = 1, n do out[i] = label_of(model:get(i)) end
    return out
  end
  -- `columns` is a table's column list here, not a grid's count.
  full.columns = nil
  if layout == "table" then
    local spec_columns = {}
    for i, c in ipairs(columns) do spec_columns[i] = { key = c.key, width = c.width or 120, sortable = c.sortable or false } end
    full.columns_spec = spec_columns
  end
  if layout == "tree" then
    full.tree_rows = function()
      local out = {}
      for i = 1, model:len() do
        local r = model:get(i)
        out[i] = { key = r.key, depth = r.depth, children = r.expandable, expanded = r.expanded }
      end
      return out
    end
    local given = spec.on_expanded_changed
    full.on_expanded_changed = function(key, on)
      expanded[key] = on or nil
      reflatten()
      if given then given(key, on) end
    end
  end
  if layout == "grid" then full.columns = spec.grid_columns or math.max(1, math.floor(W / (spec.cell_width or 96))) end
  local root, t, ctl
  local view
  local offset = morf.signal("kit.collection.offset." .. tostring(model) .. tostring(spec.id), 0)
  -- One row's state, as it is bound now: a recycled delegate rebinds it.
  local function row_state(index_signal, area)
    local s = {}
    function s.index() return index_signal:get() end
    function s.row() return model:get(index_signal:get()) end
    function s.current() return t and t.current == index_signal:get() end
    function s.selected() return t and control.has(t.selected, index_signal:get()) end
    function s.hovered() return area.hovered end
    function s.down() return area.pressed end
    function s.depth() local r = s.row() return r and r.depth or 0 end
    function s.expandable() local r = s.row() return r and r.expandable or false end
    function s.expanded() local r = s.row() return r and r.expanded or false end
    function s.toggle() if ctl then ctl.send("toggle", index_signal:get()) end end
    return s
  end
  local serial = 0
  local built = 0
  local function delegate(row, index)
    built = built + 1
    serial = serial + 1
    local index_signal = morf.signal("kit.collection.row." .. serial, index)
    local area
    area = ui.MouseArea { width = layout == "grid" and (spec.cell_width or 96) or W,
      height = layout == "grid" and (spec.cell_height or 96) or (spec.size_field and row[spec.size_field] or row_h),
      cursor = "pointer",
      on_pressed = function() if ctl then ctl.send("item_pressed", index_signal:get(), "") end end,
      on_double_clicked = function() if ctl then ctl.send("item_activated", index_signal:get()) end end }
    local s = row_state(index_signal, area)
    local builders = ctl and ctl.builders() or {}
    local look, update
    if spec.delegate then
      look, update = spec.delegate(row, s)
    elseif layout == "table" and builders.cell then
      -- The row's cells, side by side at their columns' widths.
      local cells, updaters = { gap = 0 }, {}
      for c, column in ipairs(columns) do
        local cell, cell_update = builders.cell(row, column, s)
        cells[#cells + 1] = ui.Item { width = column.width or 120,
          height = row_h, clip = true, cell }
        updaters[c] = cell_update
      end
      look = ui.Row(cells)
      update = function(next_row) for _, u in pairs(updaters) do u(next_row) end end
    elseif builders.row then
      look, update = builders.row(row, s)
    end
    if look then ui.reparent(look, area) end
    return area, function(next_row, next_index)
      index_signal:set(next_index)
      if spec.size_field then area.height = next_row[spec.size_field] or row_h end
      if update then update(next_row) end
    end
  end
  -- The rows' view: made once the control (and its skin's row builder) is
  -- there, and again on a theme switch.
  local function make_view()
    local view_props = { model = model, delegate = delegate, y = header_h, width = W, height = H - header_h,
      overscan = spec.overscan or 2, content_y = offset:get() }
    if layout == "grid" then
      view_props.cell_width, view_props.cell_height = spec.cell_width or 96, spec.cell_height or 96
      view_props.columns = full.columns
      return ui.GridView(view_props)
    end
    view_props.item_extent = row_h
    view_props.size_field, view_props.kind_field = spec.size_field, spec.kind_field
    return ui.ListView(view_props)
  end
  local header
  local children = {}
  if layout == "table" then
    header = ui.Row { gap = 0, width = W, height = header_h }
    children[#children + 1] = header
  end
  local props = { width = W, height = H, clip = true }
  for _, k in ipairs { "id", "x", "y", "anchors", "visible", "z" } do props[k] = spec[k] end
  root, t, ctl = control.make("Collection", widget, full, {
    children = children, props = props, builders = { row = true, cell = true, header = true },
    on_rebuild = function(builders)
      -- On a theme switch the rows are the new skin's.
      if view then
        ui.destroy(view, true)
        built = 0
        view = make_view()
        ui.reparent(view, root)
      end
      if header and builders.header then
        -- The header's cells: a press sorts, a drag at the edge resizes.
        for _, column in ipairs(columns) do
          local cell = builders.header(column, t)
          ui.reparent(ui.Item { width = column.width or 120, height = header_h,
            ui.MouseArea { anchors = { fill = true }, cursor = "pointer",
              on_clicked = function() if ctl then ctl.send("sort", column.key) end end, cell },
            ui.MouseArea { anchors = { right = true, top = true, bottom = true }, width = 6, cursor = "col_resize",
              on_dragged = function(_, _, dx)
                if ctl then ctl.send("resize", column.key, (column.width or 120) + dx) end
              end,
              on_drag_finished = function() column.width = t.widths and column.width end } }, header)
        end
      end
    end,
  })
  view = make_view()
  ui.reparent(view, root)
  -- The wheel scrolls by rows; keeping the current row in sight follows it.
  local function room() return math.max(0, morf.view_extent(view) - (H - header_h)) end
  local function scroll_to(y)
    y = math.max(0, math.min(y, room()))
    if y ~= offset:get() then offset:set(y) morf.sync_view(view, y) end
  end
  morf.effect("kit.collection.follow." .. ctl.id, function()
    local current = t.current
    if current < 1 or current > model:len() then return end
    local top = morf.view_item_start(view, current)
    local bottom = (current < model:len()) and morf.view_item_start(view, current + 1) or morf.view_extent(view)
    local now = offset:get()
    if top < now then scroll_to(top) elseif bottom > now + H - header_h then scroll_to(bottom - (H - header_h)) end
  end, { owner = root })
  local handle = { node = root, view = view, model = model, t = t }
  function handle.scroll_to(y) scroll_to(y) end
  function handle.scroll_by(dy) scroll_to(offset:get() + dy) end
  function handle.offset() return offset:get() end
  function handle.delegates_built() return built end
  -- The wheel: a notch is three rows.
  local wheel_area = ui.MouseArea { anchors = { fill = true }, z = -1,
    on_wheel = function(_, _, _, _, _, steps) handle.scroll_by((steps or 0) * row_h * 3) end }
  ui.reparent(wheel_area, root)
  return root, handle
end

return M
