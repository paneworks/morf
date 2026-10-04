-- Sheets (the Sheet archetype): a grid the keyboard walks one cell at a
-- time -- a spreadsheet, a data grid that edits in place, a step
-- sequencer, a seat map (WAI-ARIA grid).
--
--     local node, sheet = lib.kit.sheet.make("spreadsheet", {
--       width = 600, height = 400, rows = 1000, columns = 8,
--       cell = function(row, col) return data[row][col] end,   -- a value or a string; may read signals
--       on_edited = function(row, col, text) data[row][col] = text end,
--       on_paste = function(row, col, grid) end,     -- grid: rows of strings, from TSV
--       on_cleared = function(r0, c0, r1, c1) end,
--     })
--     sheet.refresh()   -- cell() is read again
--
-- Spec: `rows`, `columns` (numbers or bindings), `cell(row, col)`,
-- `headers` (column titles; A, B, C ... by default; false for none),
-- `row_headers` (a list or `function(row)`; 1..n by default; false for
-- none), `column_widths` (a list or one number), `row_height`,
-- `header_height`, `row_header_width`, `disabled(row, col)` (a cell that
-- takes no toggle or edit: a taken seat), `playhead` (a binding: the
-- column a step sequencer plays), and the archetype's settings
-- (`editable`, `toggle`, `read_only`, `page_rows`, `wrap`). Only the rows
-- in sight are built; columns are not virtual. The header row and the row
-- headers stay put while the cells scroll under them.
--
-- The skin fills `cell` (a builder: `cell(s)` -> node, told `s.column`,
-- `s.row()`, `s.value()`, `s.text()`, `s.disabled()`, `s.playing()`,
-- `s.zebra`, `s.widget`), `header` (a builder: `header(h)` -> node, told
-- `h.kind` -- "column", "row" or "corner" --, `h.index()`, `h.title()`,
-- `h.current()`, `h.selected()`), `range` and `cursor` (nodes laid over
-- the cells in their own coordinates: `spec.cursor_box(t)` and
-- `spec.range_box(t)` give x, y, w, h, and `spec.cell_box(r0, c0, r1,
-- c1)` any box; `spec.column_box(c)` a column's) and `editor` (a builder:
-- `editor(props)` -> node, input -- a `ui.TextInput` made with `props`).
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end

-- What each widget is unless its spec says otherwise.
local DEFAULTS = {
  spreadsheet = { column_width = 88, row_height = 30, row_header_width = 44 },
  data_grid = { column_width = 110, row_height = 36, row_headers = false, zebra = true },
  step_sequencer = { toggle = true, editable = false, square = true, row_height = 30, row_header_width = 96,
    header_height = 26 },
  seat_map = { toggle = true, editable = false, square = true, row_height = 30, row_header_width = 30,
    header_height = 26, row_letters = true },
  cell_grid = { column_width = 60, row_height = 32, headers = false, row_headers = false },
}

--- A spreadsheet's column name: A ... Z, AA ...
local function letters(n)
  local out = ""
  while n > 0 do
    local r = (n - 1) % 26
    out = string.char(65 + r) .. out
    n = (n - 1 - r) // 26
  end
  return out
end
M.letters = letters

--- A cell's value as text.
local function as_text(v)
  if v == nil or type(v) == "boolean" then return "" end
  if type(v) == "number" then
    if v == math.floor(v) and math.abs(v) < 1e15 then return ("%d"):format(v) end
    return tostring(v)
  end
  if type(v) == "table" then return tostring(v.text or v.label or v.value or "") end
  return tostring(v)
end
M.as_text = as_text

--- Rows of strings from tab-separated text.
local function parse_tsv(text)
  local grid = {}
  text = tostring(text or ""):gsub("\r\n", "\n"):gsub("\r", "\n")
  if text:sub(-1) == "\n" then text = text:sub(1, -2) end
  for line in (text .. "\n"):gmatch("([^\n]*)\n") do
    local row = {}
    for field in (line .. "\t"):gmatch("([^\t]*)\t") do row[#row + 1] = field end
    grid[#grid + 1] = row
  end
  return grid
end
M.parse_tsv = parse_tsv

-- What was copied last (from any sheet), and the newest clipboard offer,
-- for a paste: the compositor's clipboard when there is one, the sheet's
-- own copy when there is not.
local last_copied
local latest_offer
local watching = false
local function watch_clipboard()
  if watching then return end
  watching = true
  local clip = morf.clipboard
  if clip and clip.watch then
    pcall(clip.watch, function(offer) latest_offer = offer end)
  end
end

local serial = 0

function M.make(widget, spec)
  spec = spec or {}
  serial = serial + 1
  local key = "kit.sheet." .. serial
  do
    local merged = {}
    for k, v in pairs(DEFAULTS[widget] or {}) do merged[k] = v end
    for k, v in pairs(spec) do merged[k] = v end
    spec = merged
  end
  local W, H = spec.width or 400, spec.height or 300
  local rows_of = function() return math.max(1, math.floor(tonumber(get(spec.rows)) or 1)) end
  local columns = math.max(1, math.floor(tonumber(get(spec.columns)) or 1))
  local RH = spec.row_height or 30
  local HH = spec.headers == false and 0 or (spec.header_height or 30)
  local RW = spec.row_headers == false and 0 or (spec.row_header_width or 44)
  -- Columns: as given, or as many as fit the sheet's width (square cells
  -- for pads and seats).
  local widths = {}
  do
    local given = spec.column_widths
    local room = (tonumber(get(W)) or 400) - RW
    local fit = math.floor(room / columns)
    if spec.square and type(given) ~= "table" and type(given) ~= "number" then
      local rows_room = math.floor(((tonumber(get(H)) or 300) - HH) / rows_of())
      -- (Each way as far as it fits, within a pad's sizes: a seat may be
      -- a little wider than it is tall.)
      RH = math.max(22, math.min(48, rows_room))
      given = math.max(22, math.min(48, fit, math.floor(RH * 1.25)))
    end
    for c = 1, columns do
      if type(given) == "table" then widths[c] = given[c] or spec.column_width or 88
      elseif type(given) == "number" then widths[c] = given
      else widths[c] = math.max(spec.column_width or 60, fit) end
    end
    -- Columns that leave room over share it out, so the grid fills its box.
    local sum = 0
    for c = 1, columns do sum = sum + widths[c] end
    if not spec.square and spec.fill ~= false and sum < room and sum > 0 then
      local extra, given_out = room - sum, 0
      for c = 1, columns do
        local add = c == columns and (extra - given_out) or math.floor(extra * widths[c] / sum)
        widths[c] = widths[c] + add
        given_out = given_out + add
      end
    end
  end
  local lefts, total = {}, 0
  for c = 1, columns do lefts[c] = total total = total + widths[c] end

  local version = morf.signal(key .. ".version", 0)
  local ox, oy = morf.signal(key .. ".ox", 0), morf.signal(key .. ".oy", 0)
  -- (Plain copies of the offsets: following the cursor reads these, so a
  -- wheel turn does not pull the view back to it.)
  local ox_now, oy_now = 0, 0
  local function view_w() return math.max(0, (tonumber(get(W)) or 0) - RW) end
  local function view_h() return math.max(0, (tonumber(get(H)) or 0) - HH) end

  local function value_at(r, c)
    version:get()
    if spec.cell then return spec.cell(r, c) end
    return nil
  end
  local function disabled_at(r, c)
    if spec.disabled and spec.disabled(r, c) then return true end
    for _, ro in ipairs(type(spec.read_only) == "table" and spec.read_only or {}) do
      if ro == c and not spec.toggle then return true end
    end
    return false
  end

  local t, ctl, root, send
  local editor_holder, editor_input
  local editing_at
  local function bump() version:set(version:get() + 1) end

  local function cell_box(r0, c0, r1, c1)
    r1, c1 = r1 or r0, c1 or c0
    c0, c1 = math.max(1, math.min(columns, c0)), math.max(1, math.min(columns, c1))
    return lefts[c0], (r0 - 1) * RH, lefts[c1] + widths[c1] - lefts[c0], (r1 - r0 + 1) * RH
  end
  local function range_of(state)
    local r0, c0, r1, c1 = tostring(state.range or ""):match("(%d+),(%d+),(%d+),(%d+)")
    if not r0 then return state.row or 1, state.column or 1, state.row or 1, state.column or 1 end
    return tonumber(r0), tonumber(c0), tonumber(r1), tonumber(c1)
  end

  -- The editor: put over the cell, filled, focused; gone when the edit ends.
  local function open_editor(r, c, text)
    if not editor_holder then return end
    local x, y, w, h = cell_box(r, c)
    editor_holder.x, editor_holder.y, editor_holder.width, editor_holder.height = x, y, w, h
    if text == nil or text == "" then text = as_text(value_at(r, c)) end
    editing_at = { r, c }
    editor_holder.visible = true
    if editor_input then
      editor_input.text = text
      editor_input.cursor_position = #text
      editor_input.focus = true
    end
  end
  local function close_editor()
    editing_at = nil
    if editor_holder then editor_holder.visible = false end
    if editor_input then editor_input.focus = false end
    if root then morf.focus.set(root, true) end
  end

  local function tsv(r0, c0, r1, c1)
    local lines = {}
    for r = r0, r1 do
      local fields = {}
      for c = c0, c1 do fields[#fields + 1] = (as_text(value_at(r, c)):gsub("[\t\n]", " ")) end
      lines[#lines + 1] = table.concat(fields, "\t")
    end
    return table.concat(lines, "\n")
  end
  local function copy(r0, c0, r1, c1)
    local text = tsv(r0, c0, r1, c1)
    last_copied = text
    local clip = morf.clipboard
    if clip and clip.set then pcall(clip.set, text) end
    return text
  end

  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.columns = columns
  full.rows = spec.rows or 1
  -- (Not the node's: the sheet's own.)
  full.headers, full.row_headers, full.cell, full.disabled = nil, nil, nil, nil
  full.cell_box = cell_box
  function full.cursor_box(state) return cell_box(state.row or 1, state.column or 1) end
  function full.range_box(state) return cell_box(range_of(state)) end
  function full.column_box(c) return lefts[c] or 0, 0, widths[c] or 0, rows_of() * RH end
  full.content_width, full.row_height = total, RH
  full.rows_count = rows_of
  full.playhead_column = function() return spec.playhead and get(spec.playhead) or 0 end
  -- What the archetype raises, done here before the configuration hears it.
  local function wrap(name, before)
    local given = spec[name]
    full[name] = function(...)
      local skip = before and before(...)
      if not skip and given then given(...) end
    end
  end
  wrap("on_edit_started", function(r, c, text) open_editor(r, c, text) end)
  wrap("on_edited", function() close_editor() end)
  full.on_edited = (function(inner)
    return function(...) inner(...) bump() end
  end)(full.on_edited)
  wrap("on_edit_canceled", function() close_editor() end)
  wrap("on_toggled", function(r, c) return disabled_at(r, c) end)
  full.on_toggled = (function(inner) return function(...) inner(...) bump() end end)(full.on_toggled)
  wrap("on_copy", function(r0, c0, r1, c1) copy(r0, c0, r1, c1) end)
  full.on_cut = function(r0, c0, r1, c1)
    copy(r0, c0, r1, c1)
    if spec.on_cut then spec.on_cut(r0, c0, r1, c1)
    elseif spec.on_cleared then spec.on_cleared(r0, c0, r1, c1) end
    bump()
  end
  full.on_cleared = function(...)
    if spec.on_cleared then spec.on_cleared(...) end
    bump()
  end
  full.on_paste = function(r, c)
    local function give(text)
      if text == nil then return end
      local grid = parse_tsv(text)
      if spec.on_paste then spec.on_paste(r, c, grid) end
      bump()
    end
    local offer = latest_offer
    if offer and offer.read then
      local ok = pcall(offer.read, offer, "text", function(bytes, err)
        if bytes and not err then give(tostring(bytes)) else give(last_copied) end
      end)
      if ok then return end
    end
    give(last_copied)
  end
  watch_clipboard()

  -- The parts: headers over the cells, the rows, and a layer over the
  -- rows for the range, the cursor and the editor -- all moving with the
  -- offsets.
  local header_strip = ui.Item { x = RW, y = 0, width = function() return view_w() end, height = HH, clip = true, z = 3 }
  local header_row = ui.Item { x = function() return -ox:get() end, width = total, height = HH }
  ui.reparent(header_row, header_strip)
  local corner = ui.Item { x = 0, y = 0, width = RW, height = HH, z = 4 }
  local body = ui.Item { x = 0, y = HH, width = function() return get(W) end, height = function() return view_h() end,
    clip = true }
  local over = ui.Item { x = RW, y = 0, width = function() return view_w() end, height = function() return view_h() end,
    clip = true, z = 2 }
  local layer = ui.Item { x = function() return -ox:get() end, y = function() return -oy:get() end,
    width = total, height = function() return rows_of() * RH end }
  ui.reparent(layer, over)
  ui.reparent(over, body)

  local model = morf.list_model({})
  local function fill_model()
    local n = rows_of()
    local list = {}
    for i = 1, n do list[i] = i end
    model:replace(list)
  end
  fill_model()

  local built = 0
  local view
  local builders = {}
  local row_serial = 0
  local function cell_state(index_signal, c)
    local s = { column = c, widget = widget, kind = "cell", zebra = spec.zebra == true,
      width = widths[c], height = RH, first = c == 1, last = c == columns }
    function s.row() return index_signal:get() end
    function s.value() return value_at(index_signal:get(), c) end
    function s.text() return as_text(value_at(index_signal:get(), c)) end
    function s.disabled() version:get() return disabled_at(index_signal:get(), c) end
    function s.playing() return spec.playhead ~= nil and get(spec.playhead) == c end
    function s.toggle() return spec.toggle == true end
    return s
  end
  local function row_title(r)
    local given = spec.row_headers
    if type(given) == "function" then return tostring(given(r) or "") end
    if type(given) == "table" then return tostring(given[r] or "") end
    if spec.row_letters then return letters(r) end
    return tostring(r)
  end
  local function column_title(c)
    local given = spec.headers
    if type(given) == "table" then return tostring(given[c] or "") end
    if type(given) == "function" then return tostring(given(c) or "") end
    if spec.toggle then return tostring(c) end
    return letters(c)
  end
  local function delegate(_, index)
    built = built + 1
    row_serial = row_serial + 1
    local index_signal = morf.signal(key .. ".row." .. row_serial, index)
    local row = ui.Item { width = RW + total, height = RH }
    for c = 1, columns do
      local look = builders.cell and builders.cell(cell_state(index_signal, c))
      if not look then
        look = ui.Text { anchors = { fill = true, left_margin = 6, right_margin = 6 }, vertical_alignment = "center",
          elide = "right", text = function() return as_text(value_at(index_signal:get(), c)) end }
      end
      ui.reparent(ui.Item { x = RW + lefts[c], width = widths[c], height = RH, look }, row)
    end
    if RW > 0 and builders.header then
      local h = { kind = "row", widget = widget, width = RW, height = RH }
      function h.index() return index_signal:get() end
      function h.title() return row_title(index_signal:get()) end
      function h.current() return t ~= nil and t.row == index_signal:get() end
      function h.selected()
        if not t then return false end
        local r0, _, r1 = range_of(t)
        local i = index_signal:get()
        return i >= r0 and i <= r1
      end
      local look = builders.header(h)
      -- (Kept at the sheet's left edge while the cells scroll sideways.)
      if look then
        ui.reparent(ui.Item { x = function() return ox:get() end, width = RW, height = RH, z = 1, look }, row)
      end
    end
    return row, function(_, next_index) index_signal:set(next_index) end
  end
  local function make_view()
    return ui.ListView { model = model, delegate = delegate, x = function() return -ox:get() end, y = 0,
      width = RW + total,
      -- (A number when it is one: the view sizes its pool of rows from the
      -- height it is made with.)
      height = type(H) == "function" and function() return view_h() end or view_h(), item_extent = RH, overscan = spec.overscan or 2,
      content_y = oy_now }
  end

  local header_cells = {}
  local function make_headers()
    for _, node in ipairs(header_cells) do ui.destroy(node, true) end
    header_cells = {}
    if not builders.header then return end
    if HH > 0 then
      for c = 1, columns do
        local h = { kind = "column", widget = widget, width = widths[c], height = HH }
        function h.index() return c end
        function h.title() return column_title(c) end
        function h.current() return t ~= nil and t.column == c end
        function h.selected()
          if not t then return false end
          local _, c0, _, c1 = range_of(t)
          return c >= c0 and c <= c1
        end
        function h.playing() return spec.playhead ~= nil and get(spec.playhead) == c end
        local look = builders.header(h)
        if look then
          local holder = ui.Item { x = lefts[c], width = widths[c], height = HH, look }
          ui.reparent(holder, header_row)
          header_cells[#header_cells + 1] = holder
        end
      end
    end
    if HH > 0 and RW > 0 then
      local h = { kind = "corner", widget = widget, width = RW, height = HH }
      function h.index() return 0 end
      function h.title() return "" end
      function h.current() return false end
      function h.selected() return false end
      local look = builders.header(h)
      if look then
        local holder = ui.Item { width = RW, height = HH, look }
        ui.reparent(holder, corner)
        header_cells[#header_cells + 1] = holder
      end
    end
  end

  local function make_editor()
    if editor_holder then ui.destroy(editor_holder, true) end
    editor_holder, editor_input = nil, nil
    if not builders.editor then return end
    local input
    local props = {
      anchors = { fill = true }, tab_navigation = false,
      on_accepted = function(text) if send then send("commit", text) end end,
      on_escape = function() if send then send("cancel") end end,
      on_key_pressed = function(_, _, modifiers, _, name)
        if name == "Tab" or name == "ISO_Left_Tab" then
          -- Tab commits and goes on across, as a spreadsheet does.
          if send then
            send("commit", input and input.text or "", false)
            send("key", name, modifiers or "", "", 0)
          end
          return true
        end
        return false
      end,
    }
    local node
    node, input = builders.editor(props)
    if not node then return end
    editor_input = input
    editor_holder = ui.Item { visible = false, z = 5, node }
    ui.reparent(editor_holder, layer)
  end

  local ready = false
  local function rebuild()
    local slots = ctl.slots()
    builders = ctl.builders() or {}
    if view then ui.destroy(view, true) end
    built = 0
    view = make_view()
    ui.reparent(view, body)
    make_headers()
    for _, name in ipairs { "range", "cursor" } do
      local node = slots[name]
      if node then
        node.z = name == "cursor" and 2 or 1
        ui.reparent(node, layer)
      end
    end
    make_editor()
  end

  local props = { width = W, height = H, clip = true }
  for _, k in ipairs { "id", "x", "y", "anchors", "visible", "z" } do props[k] = spec[k] end
  root, t, ctl = control.make("Sheet", widget, full, {
    props = props, children = { body, header_strip, corner },
    builders = { cell = true, header = true, editor = true },
    on_rebuild = function() if ready then rebuild() end end,
  })
  send = ctl.send
  ready = true
  rebuild()

  -- Rows given as a binding: the rows' list follows.
  if type(spec.rows) == "function" then
    morf.effect(key .. ".rows", function()
      local n = rows_of()
      if n ~= model:len() then fill_model() end
    end, { owner = root })
  end

  -- Scrolling, kept within the content; the current cell kept in sight.
  local function scroll_to(x, y)
    local max_x = math.max(0, total - view_w())
    local max_y = math.max(0, rows_of() * RH - view_h())
    x = math.max(0, math.min(x, max_x))
    y = math.max(0, math.min(y, max_y))
    if x ~= ox_now then ox_now = x ox:set(x) end
    if y ~= oy_now then
      oy_now = y
      oy:set(y)
      if view then morf.sync_view(view, y) end
    end
  end
  morf.effect(key .. ".follow", function()
    local r, c = t.row or 1, t.column or 1
    local x, y, w, h = cell_box(r, c)
    local vw, vh = view_w(), view_h()
    local nx, ny = ox_now, oy_now
    if x < nx then nx = x elseif x + w > nx + vw then nx = x + w - vw end
    if y < ny then ny = y elseif y + h > ny + vh then ny = y + h - vh end
    scroll_to(nx, ny)
  end, { owner = root })

  -- The pointer: which cell is under it (a header picks its whole column
  -- or row).
  local function cell_at(x, y)
    local cx = x - RW + ox_now
    local c = columns
    for i = 1, columns do
      if cx < lefts[i] + widths[i] then c = i break end
    end
    if cx < 0 then c = 1 end
    local r = math.floor((y - HH + oy_now) / RH) + 1
    return math.max(1, math.min(rows_of(), r)), c
  end
  local last_drag
  root.on_pressed = function(sx, sy, x, y, button, modifiers)
    if button and button ~= "left" then return end
    local r, c = cell_at(x, y)
    -- A press elsewhere while editing commits what was typed first.
    if t.editing and editing_at and (editing_at[1] ~= r or editing_at[2] ~= c) and editor_input then
      send("commit", editor_input.text or "", false)
    end
    if y < HH and x >= RW then
      send("pressed", 1, c, modifiers or "")
      send("dragged", rows_of(), c)
      last_drag = nil
    elseif x < RW and y >= HH then
      send("pressed", r, 1, modifiers or "")
      send("dragged", r, columns)
      last_drag = nil
    else
      send("pressed", r, c, modifiers or "")
      last_drag = r .. ":" .. c
    end
    if spec.on_pressed then spec.on_pressed(sx, sy, x, y, button, modifiers) end
  end
  root.on_dragged = function(_, _, _, _, x, y)
    if not last_drag then return end
    local r, c = cell_at(x, y)
    local k = r .. ":" .. c
    if k ~= last_drag then
      last_drag = k
      send("dragged", r, c)
    end
  end
  root.on_double_clicked = function(_, _, x, y)
    if y < HH or x < RW then return end
    local r, c = cell_at(x, y)
    send("double_clicked", r, c)
  end
  root.on_wheel = function(_, _, _, _, step_x, step_y, _, _, modifiers)
    local dx, dy = step_x or 0, step_y or 0
    if modifiers and modifiers:find("shift", 1, true) then dx, dy = dy, 0 end
    scroll_to(ox_now + dx * 60, oy_now + dy * RH * 3)
  end

  local handle = { node = root, t = t, send = send, id = ctl.id }
  --- Reads `cell()` again (after the data changed outside a signal).
  function handle.refresh() bump() end
  --- Moves to a cell.
  function handle.go(r, c)
    ctl.configure("row", r)
    if c then ctl.configure("column", c) end
  end
  function handle.scroll_to(x, y) scroll_to(x, y) end
  function handle.offset() return ox_now, oy_now end
  function handle.rows_built() return built end
  function handle.editor() return editor_input end
  function handle.editing() return t.editing == true end
  function handle.range() return range_of(t) end
  function handle.copy_text() return tsv(range_of(t)) end
  return root, handle
end

return M
