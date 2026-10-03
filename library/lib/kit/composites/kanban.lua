-- A kanban board (composite: a Collection per column + Drag to move cards).
--
--     local node, board = composites.kanban {
--       id = "work", width = 520, height = 340,
--       columns = {
--         { key = "todo", title = "To do", cards = { { key = "a", title = "Write docs", tag = "docs" }, ... } },
--         { key = "doing", title = "Doing", cards = { ... } },
--         { key = "done", title = "Done", cards = {} },
--       },
--       on_moved = function(card_key, from_column, to_column, index) end,
--     }
--     board.move("a", "done") ; board.cards("todo") --> { "b", "c" }
--
-- Each column is a kit `kanban_column` (a Collection: the arrows walk its
-- cards, typing jumps to one). A card is dragged (a Drag transfer: past
-- its threshold a ghost of it follows the pointer) onto another column
-- or another place in its own, the drop line showing where it lands. The
-- keyboard moves the current card: Alt+Left and Alt+Right to the column
-- beside, Alt+Up and Alt+Down within its column. Ids: `<id>-column-<key>`,
-- `<id>-list-<key>`, `<id>-card-<card key>`, `<id>-ghost`, `<id>-count-<key>`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local widgets = require("lib.kit.widgets")

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 340
  local columns = spec.columns or {}
  local NC = math.max(1, #columns)
  local GAP = 8
  local CW = math.floor((W - GAP * (NC - 1)) / NC)
  local HEAD = 34
  local CARD = spec.card_height or 60
  local LH = H - HEAD - 8
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end

  local models, lists, current, col_index = {}, {}, {}, {}
  local st = morf.state { drag_key = "", ghost_x = 0, ghost_y = 0, drop_col = 0, drop_at = 0, revision = 0 }
  local cards_by_key = {}
  for c, column in ipairs(columns) do
    local key = tostring(column.key or column.title or c)
    column.key = key
    col_index[key] = c
    local rows = {}
    for _, card in ipairs(column.cards or {}) do
      local row = { key = tostring(card.key or card.title), title = card.title or tostring(card.key), tag = card.tag,
        kind = card.kind }
      rows[#rows + 1] = row
      cards_by_key[row.key] = row
    end
    models[c] = morf.list_model(rows)
    current[c] = morf.signal(("kit.kanban.%s.%d.%s"):format(tostring(id), c, tostring(models[c])), 0)
  end

  local function find(card_key)
    for c, model in ipairs(models) do
      for i = 1, model:len() do if model:get(i).key == card_key then return c, i end end
    end
  end
  local function focus_column(c)
    if lists[c] then morf.focus.set(lists[c], true) end
  end
  -- Moves a card to column `to` at `index` (the end when nil).
  local function move(card_key, to, index)
    local from, at = find(card_key)
    if not from or not models[to] then return false end
    local row = models[from]:get(at)
    local n = models[to]:len()
    if from == to then
      index = math.max(1, math.min(n, index or n))
      if index == at then return false end
      models[from]:move(at, index)
    else
      index = math.max(1, math.min(n + 1, index or n + 1))
      models[from]:remove(at)
      models[to]:insert(index, row)
      if current[from]:get() > models[from]:len() then current[from]:set(models[from]:len()) end
    end
    current[to]:set(index)
    st.revision = st.revision + 1
    if spec.on_moved then spec.on_moved(card_key, columns[from].key, columns[to].key, index) end
    return true
  end

  local root
  -- Where a point on the surface falls: the column and the place in it.
  local function drop_target(sx, sy)
    local rx, ry = sx - (root.layout_x or 0), sy - (root.layout_y or 0)
    local c = math.floor(rx / (CW + GAP)) + 1
    if c < 1 or c > NC then return nil end
    local list = lists[c]
    local top = list and (list.layout_y or 0) - (root.layout_y or 0) or HEAD
    local at = math.floor((ry - top) / CARD + 0.5) + 1
    return c, math.max(1, math.min(models[c]:len() + 1, at))
  end

  local function card_delegate(c)
    return function(row, s)
      -- (By key: a card moved within its column keeps its delegate, and
      -- the delegate's index with it.)
      local me = morf.state { key = row.key }
      local function now() return cards_by_key[me.key] or row end
      local function is_current()
        local at = current[c]:get()
        local r = at >= 1 and at <= models[c]:len() and models[c]:get(at) or nil
        return r ~= nil and r.key == me.key
      end
      local grab = { x = 0, y = 0 }
      local behaviour
      local area
      behaviour = control.headless("Drag", { mode = "transfer", axis = "both", threshold = 6,
        on_drag_started = function() st.drag_key = now().key end,
        on_dropped = function()
          local key = st.drag_key
          local to, at = st.drop_col, st.drop_at
          st.drag_key, st.drop_col = "", 0
          if key == "" or to == 0 then return end
          local from, here = find(key)
          -- Its own column: the place counts without the card itself.
          if from == to and at > here then at = at - 1 end
          if move(key, to, at) then focus_column(to) end
        end })
      area = ui.MouseArea { id = sid("card-" .. row.key), width = CW, height = CARD, cursor = "grab",
        accessible_role = "list_item", accessible_name = function() return now().title end,
        opacity = function() return st.drag_key == now().key and 0.35 or 1 end,
        on_pressed = function(sx, sy, x, y)
          grab.x, grab.y = x, y
          local _, at = find(me.key)
          current[c]:set(at or 0)
          focus_column(c)
          behaviour.send("pressed", sx, sy)
        end,
        on_dragged = function(sx, sy)
          behaviour.send("dragged", sx, sy)
          if behaviour.t.active then
            st.ghost_x = sx - (root.layout_x or 0) - grab.x
            st.ghost_y = sy - (root.layout_y or 0) - grab.y
            local to, at = drop_target(sx, sy)
            st.drop_col, st.drop_at = to or 0, at or 0
          end
        end,
        on_released = function() behaviour.send("released", 0, 0) end,
        on_destroyed = function() behaviour.drop() end,
        kit.card { anchors = { fill = true, margins = 4 } },
        kit.surface { anchors = { fill = true, margins = 4 }, radius = kit.round(10),
          color = function() return kit.ink("hi")():alpha(0.05) end },
        kit.surface { anchors = { fill = true, margins = 4 }, radius = kit.round(10),
          border_width = function() return is_current() and 2 or 0 end, border_color = kit.signal("accent"),
          color = function() return kit.signal("accent")():alpha(area and area.hovered and 0.06 or 0) end },
        kit.text { x = 14, y = 12, width = CW - 28, elide = "right", font_weight = 500,
          text = function() return now().title end },
        kit.label { x = 14, y = CARD - 26, width = CW - 28, elide = "right",
          text = function() return now().tag and ("#" .. now().tag) or "" end } }
      return area, function(next_row)
        me.key = next_row.key
        if id then area.id = id .. "-card-" .. next_row.key end
      end
    end
  end

  local parts = {}
  for c, column in ipairs(columns) do
    local function move_key(dc, di)
      return function()
        local at = current[c]:get()
        local row = at >= 1 and at <= models[c]:len() and models[c]:get(at) or nil
        if not row then return false end
        if dc ~= 0 then
          local to = c + dc
          if to < 1 or to > NC then return true end
          if move(row.key, to, math.min(at, models[to]:len() + 1)) then focus_column(to) end
        else
          move(row.key, c, at + di)
        end
        return true
      end
    end
    local list = widgets.kanban_column { id = sid("list-" .. column.key), accessible_name = column.title,
      y = HEAD, width = CW, height = LH, rows = models[c], row_height = CARD,
      current = function() return current[c]:get() end,
      on_current_changed = function(i) current[c]:set(i) end,
      delegate = card_delegate(c) }
    lists[c] = list
    local x = (c - 1) * (CW + GAP)
    parts[#parts + 1] = ui.Item { id = sid("column-" .. column.key), x = x, width = CW, height = H,
      shortcuts = { ["alt+Left"] = move_key(-1, 0), ["alt+Right"] = move_key(1, 0),
        ["alt+Up"] = move_key(0, -1), ["alt+Down"] = move_key(0, 1) },
      kit.surface { anchors = { fill = true }, radius = kit.round(14), color = kit.stroke("faint"),
        border_width = function() return st.drag_key ~= "" and st.drop_col == c and 2 or 0 end,
        border_color = kit.signal("accent") },
      kit.text { x = 12, y = 9, width = CW - 60, elide = "right", font_weight = 600, text = column.title or column.key },
      kit.badge { id = sid("count-" .. column.key), count = function() return models[c]:len() end, size = 18,
        kind = "info", x = CW - 34, y = 8, label = (column.title or column.key) .. " cards" },
      list,
      -- Where a dragged card would land.
      kit.surface { x = 8, width = CW - 16, height = 3, radius = kit.round(1.5), color = kit.signal("accent"),
        y = function() return HEAD + (st.drop_at - 1) * CARD - 1 end,
        visible = function() return st.drag_key ~= "" and st.drop_col == c end } }
  end
  -- The ghost under the pointer.
  local ghost = ui.Item { id = sid("ghost"), z = 10, width = CW, height = CARD, opacity = 0.92,
    x = function() return st.ghost_x end, y = function() return st.ghost_y end,
    visible = function() return st.drag_key ~= "" end, rotation = 2,
    kit.card { anchors = { fill = true, margins = 4 } },
    kit.surface { anchors = { fill = true, margins = 4 }, radius = kit.round(10), border_width = 2,
      border_color = kit.signal("accent"), color = function() return kit.signal("accent")():alpha(0.12) end },
    kit.text { x = 14, y = 12, width = CW - 28, elide = "right", font_weight = 500,
      text = function() local row = cards_by_key[st.drag_key] return row and row.title or "" end } }
  parts[#parts + 1] = ghost
  parts.id, parts.width, parts.height = id, W, H
  root = ui.Item(parts)
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, models = models }
  function handle.move(card_key, column_key, index)
    local to = col_index[tostring(column_key)]
    return to and move(tostring(card_key), to, index) or false
  end
  function handle.cards(column_key)
    local c = col_index[tostring(column_key)]
    local out = {}
    if not c then return out end
    local _ = st.revision
    for i = 1, models[c]:len() do out[i] = models[c]:get(i).key end
    return out
  end
  return root, handle
end
