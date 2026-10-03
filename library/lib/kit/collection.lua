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
--- A node marked `lazy` has children it has not loaded yet: it can open,
--- and opened shows a loading row until they come (`load`, below).
local function flatten(nodes, expanded, depth, out, load)
  for _, node in ipairs(nodes or {}) do
    local key = tostring(node.key or node.label)
    local children = type(node.children) == "table" and #node.children > 0
    local waiting = node.lazy and type(node.children) ~= "table"
    local row = {}
    for k, v in pairs(node) do if k ~= "children" then row[k] = v end end
    row.key, row.depth, row.expandable, row.expanded = key, depth, (children or waiting) or false, expanded[key] == true
    out[#out + 1] = row
    if children and expanded[key] then flatten(node.children, expanded, depth + 1, out, load) end
    if waiting and expanded[key] then
      out[#out + 1] = { key = key .. "::loading", depth = depth + 1, expandable = false, expanded = false,
        loading = true, label = "Loading" }
      if load then load(node) end
    end
  end
  return out
end

-- What each widget is unless its spec says otherwise: its layout and the
-- size of its rows. A file list given no delegate of its own is a table of
-- name, size and date; a tree table is a tree whose rows are a table's.
local DEFAULTS = {
  grid_view = { layout = "grid" },
  flow_box = { layout = "flow", cell_width = 128, cell_height = 44 },
  data_table = { layout = "table" },
  tree_view = { layout = "tree" },
  tree_table = { layout = "tree" },
  file_list = { row_height = 36 },
  boxed_list = { row_height = 50 },
  list_box = { row_height = 52 },
  timeline = { row_height = 56 },
  feed = { row_height = 92 },
  kanban_column = { row_height = 64 },
  chat_log = { row_height = 48, size_field = "height" },
}
local FILE_COLUMNS = {
  { key = "name", title = "Name", width = 0.56, sortable = true },
  { key = "size", title = "Size", width = 0.2, sortable = true },
  { key = "modified", title = "Modified", width = 0.24, sortable = true },
}

--- A chat message's height: its lines at the bubble's width, and room
--- round them (rows that say how tall they are keep theirs).
local function bubble_height(row, width)
  local text = type(row) == "table" and tostring(row.text or row.label or "") or tostring(row)
  local per_line = math.max(8, math.floor((width * 0.72 - 28) / 7.4))
  local lines = 0
  for part in (text .. "\n"):gmatch("([^\n]*)\n") do lines = lines + math.max(1, math.ceil(#part / per_line)) end
  return 20 * math.max(1, lines) + 40
end

function M.make(widget, spec)
  spec = spec or {}
  do
    local merged = {}
    for k, v in pairs(DEFAULTS[widget] or {}) do merged[k] = v end
    for k, v in pairs(spec) do merged[k] = v end
    if widget == "file_list" and merged.delegate == nil and merged.columns == nil then
      merged.layout = merged.layout or "table"
      local W0 = merged.width or 300
      local columns = {}
      for i, c in ipairs(FILE_COLUMNS) do
        columns[i] = { key = c.key, title = c.title, sortable = c.sortable, width = math.floor(W0 * c.width) }
      end
      merged.columns = columns
    end
    spec = merged
  end
  local layout = spec.layout or widget
  if layout ~= "grid" and layout ~= "table" and layout ~= "tree" and layout ~= "flow" then layout = "list" end
  -- A tree with columns (a tree table) draws its rows as a table's.
  local tabular = layout == "table" or (layout == "tree" and spec.columns ~= nil)
  -- A chat log's messages are as tall as their text.
  if widget == "chat_log" and spec.size_field == "height" and type(spec.rows) == "table" then
    for _, row in ipairs(spec.rows) do
      if type(row) == "table" and row.height == nil then row.height = bubble_height(row, spec.width or 300) end
    end
  end
  local model = spec.rows
  if type(model) ~= "userdata" then model = morf.list_model(model or {}) end
  -- A tree's rows are its nodes as far as they are expanded.
  -- (`expanded`: the keys of the nodes open from the start.)
  local expanded = {}
  for _, key in ipairs(type(spec.expanded) == "table" and spec.expanded or {}) do expanded[tostring(key)] = true end
  local reflatten
  -- A lazy node's children, asked for once: `load_children(node, give)`,
  -- `give(children)` when they are there.
  local asked = {}
  local function load(node)
    if asked[node] or not spec.load_children then return end
    asked[node] = true
    morf.timer(1, function()
      spec.load_children(node, function(children)
        node.children = children or {}
        node.lazy = nil
        reflatten()
      end)
    end, false)
  end
  reflatten = function()
    local rows = flatten(spec.tree, expanded, 0, {}, load)
    model:replace(rows, "key")
  end
  if layout == "tree" then reflatten() end
  local columns = spec.columns or {}
  local W, H = spec.width or 300, spec.height or 300
  local header_h = tabular and (spec.header_height or 32) or 0
  -- Each column's width as it is now: a drag at a header's edge changes it
  -- live, and the header and the cells follow.
  local widths = {}
  for c, column in ipairs(columns) do
    widths[c] = morf.signal("kit.collection.width." .. tostring(model) .. "." .. tostring(spec.id) .. "." .. c,
      column.width or 120)
  end
  local function width_of(c) return function() return widths[c]:get() end end
  local function left_of(c)
    return function() local x = 0 for i = 1, c - 1 do x = x + widths[i]:get() end return x end
  end
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
  if tabular then
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
  if layout == "grid" or layout == "flow" then
    full.columns = spec.grid_columns or math.max(1, math.floor(W / (spec.cell_width or 96)))
  end
  -- A table no one else sorts sorts its own rows: by the column's values,
  -- numbers as numbers, folders before files, the order kept among equals.
  if layout == "table" and spec.on_sort_changed == nil then
    full.on_sort_changed = setmetatable({}, { __call = function(_, key, ascending)
      local rows, keyed = {}, true
      for i = 1, model:len() do
        local r = model:get(i)
        rows[i] = { r, i }
        keyed = keyed and type(r) == "table" and r.key ~= nil
      end
      local function folder(r) return type(r) == "table" and (r.is_dir or r.kind == "folder" or r.type == "folder") and 0 or 1 end
      local function value(r)
        local v = type(r) == "table" and (r[key .. "_value"] or r[key]) or r
        if type(v) == "number" then return v end
        return tostring(v or ""):lower()
      end
      table.sort(rows, function(a, b)
        local fa, fb = folder(a[1]), folder(b[1])
        if fa ~= fb then return fa < fb end
        local va, vb = value(a[1]), value(b[1])
        if type(va) ~= type(vb) then va, vb = tostring(va), tostring(vb) end
        if va == vb then return a[2] < b[2] end
        if ascending then return va < vb end
        return va > vb
      end)
      local out = {}
      for i, pair in ipairs(rows) do out[i] = pair[1] end
      if keyed then model:replace(out, "key") else model:replace(out) end
    end })
  end
  local root, t, ctl
  local view
  local offset = morf.signal("kit.collection.offset." .. tostring(model) .. tostring(spec.id), 0)
  -- One row's state, as it is bound now: a recycled delegate rebinds it.
  local function row_state(index_signal, area)
    local s = { area = area, widget = widget, layout = layout, tree = layout == "tree" }
    function s.index() return index_signal:get() end
    function s.row() return model:get(index_signal:get()) end
    function s.count() return model:len() end
    function s.width() return area.width or W end
    function s.current() return t and t.current == index_signal:get() end
    function s.selected() return t and control.has(t.selected, index_signal:get()) end
    function s.hovered() return area.hovered end
    function s.down() return area.pressed end
    function s.depth() local r = s.row() return r and r.depth or 0 end
    function s.expandable() local r = s.row() return r and r.expandable or false end
    function s.expanded() local r = s.row() return r and r.expanded or false end
    function s.loading() local r = s.row() return type(r) == "table" and r.loading == true end
    function s.focused() return t and t.visual_focus end
    function s.toggle() if ctl then ctl.send("toggle", index_signal:get()) end end
    return s
  end
  local serial = 0
  local built = 0
  -- Every delegate and the row it is bound to now, for a row that is to
  -- come or go with the skin's motion (`handle.insert`, `handle.remove`).
  local bound = setmetatable({}, { __mode = "k" })
  local function delegate(row, index)
    built = built + 1
    serial = serial + 1
    local index_signal = morf.signal("kit.collection.row." .. serial, index)
    local area
    local cell_w, cell_h = spec.cell_width or 96, spec.cell_height or 96
    local builders = ctl and ctl.builders() or {}
    -- How the skin has a row come and go (inserted, removed): a row
    -- builder given as a table with `motion = { enter =, exit = }` and a
    -- `__call`.
    local motion = not spec.delegate and type(builders.row) == "table" and builders.row.motion or {}
    area = ui.MouseArea { enter = motion.enter, exit = motion.exit,
      width = (layout == "grid" or layout == "flow") and cell_w or W,
      height = (layout == "grid" or layout == "flow") and cell_h or (spec.size_field and row[spec.size_field] or row_h),
      cursor = "pointer",
      on_pressed = function(_, _, _, _, _, modifiers)
        if ctl then ctl.send("item_pressed", index_signal:get(), modifiers or "") end
      end,
      on_double_clicked = function() if ctl then ctl.send("item_activated", index_signal:get()) end end }
    local s = row_state(index_signal, area)
    bound[area] = index_signal
    local look, update
    if spec.delegate then
      look, update = spec.delegate(row, s)
    elseif tabular and builders.cell then
      -- The row's cells, side by side at their columns' widths -- as they
      -- are now: a column dragged wider moves the ones after it.
      local cells, updaters = {}, {}
      for c, column in ipairs(columns) do
        if column.index == nil then column.index = c end
        local cell, cell_update = builders.cell(row, column, s)
        -- (The first column's cell is not clipped: a skin may lay the
        -- row's ground -- a stripe, the chosen wash -- across the whole
        -- row from it, under the cells after it.)
        cells[#cells + 1] = ui.Item { x = left_of(c), width = width_of(c), height = function() return area.height end,
          clip = c > 1, cell }
        updaters[c] = cell_update
      end
      cells.width, cells.height = W, function() return area.height end
      look = ui.Item(cells)
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
    if layout == "grid" or layout == "flow" then
      view_props.cell_width, view_props.cell_height = spec.cell_width or 96, spec.cell_height or 96
      view_props.columns = full.columns
      return ui.GridView(view_props)
    end
    view_props.item_extent = row_h
    view_props.size_field, view_props.kind_field = spec.size_field, spec.kind_field
    return ui.ListView(view_props)
  end
  local header
  local header_cells = {}
  local children = {}
  if tabular then
    header = ui.Item { width = W, height = header_h }
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
      for _, cell in ipairs(header_cells) do ui.destroy(cell, true) end
      header_cells = {}
      if header and builders.header then
        -- The header's cells: a press sorts, a drag at the edge resizes.
        for c, column in ipairs(columns) do
          if column.index == nil then column.index = c end
          local cell = builders.header(column, t)
          local from, base = 0, 0
          local holder = ui.Item { x = left_of(c), width = width_of(c), height = header_h,
            ui.MouseArea { anchors = { fill = true }, cursor = column.sortable and "pointer" or nil,
              on_clicked = function() if ctl then ctl.send("sort", column.key) end end, cell },
            ui.MouseArea { anchors = { right = true, top = true, bottom = true }, width = 8, cursor = "col_resize",
              on_pressed = function(sx) from, base = sx, widths[c]:get() end,
              on_dragged = function(sx)
                local w = math.max(spec.min_column_width or 48, math.floor(base + (sx - from)))
                widths[c]:set(w)
                column.width = w
                if ctl then ctl.send("resize", column.key, w) end
              end } }
          header_cells[#header_cells + 1] = holder
          ui.reparent(holder, header)
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
  --- Sorts by a column, as a press on its header does.
  function handle.sort(key) ctl.send("sort", key) end
  -- The delegate bound to row `i` now, and how the skin moves rows.
  local function delegate_at(i)
    for area, index_signal in pairs(bound) do
      if index_signal:get() == i and area.visible ~= false then return area end
    end
  end
  local function motion()
    local builders = ctl.builders()
    return not spec.delegate and type(builders.row) == "table" and builders.row.motion or {}
  end
  --- Inserts `row` at `i`, which comes in as the skin has rows come (a
  --- recycled delegate as well as a new one).
  function handle.insert(i, row)
    model:insert(i, row)
    local enter, area = motion().enter, delegate_at(i)
    if not (enter and area) then return end
    local steps = {}
    for property, from in pairs(enter) do
      if property ~= "duration" and property ~= "easing" and property ~= "delay" then
        local to = (property == "opacity" or property:match("^scale")) and 1 or 0
        steps[#steps + 1] = { node = area, property = property, from = from, to = to,
          duration = enter.duration or 220, easing = enter.easing or "out_cubic" }
      end
    end
    morf.animation.play { { parallel = steps } }
  end
  --- Removes row `i`: it leaves as the skin has rows go, then the rows
  --- under it close up. `done()` after.
  function handle.remove(i, done)
    local exit, area = motion().exit, delegate_at(i)
    local function finish()
      model:remove(i)
      if area then
        for property in pairs(exit or {}) do
          if property ~= "duration" and property ~= "easing" and property ~= "delay" then
            area[property] = (property == "opacity" or property:match("^scale")) and 1 or 0
          end
        end
      end
      if done then done() end
    end
    if not (exit and area) then return finish() end
    local steps = {}
    for property, to in pairs(exit) do
      if property ~= "duration" and property ~= "easing" and property ~= "delay" then
        steps[#steps + 1] = { node = area, property = property, to = to, duration = exit.duration or 200,
          easing = exit.easing or "in_cubic" }
      end
    end
    morf.animation.play { { parallel = steps }, on_finished = function() finish() end }
  end
  -- The wheel: a notch is three rows.
  local wheel_area = ui.MouseArea { anchors = { fill = true }, z = -1,
    on_wheel = function(_, _, _, _, _, steps) handle.scroll_by((steps or 0) * row_h * 3) end }
  ui.reparent(wheel_area, root)
  return root, handle
end

return M
