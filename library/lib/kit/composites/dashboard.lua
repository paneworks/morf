-- A dashboard (composite: a grid Collection of tiles + Drag to rearrange).
--
--     local node, dash = composites.dashboard {
--       id = "home", width = 520, height = 340,
--       tiles = {
--         { key = "cpu", title = "CPU", icon = "memory", value = function() return "12%" end, level = 0.12 },
--         { key = "net", title = "Network", icon = "wifi", value = "48 Mb/s",
--           build = function(w, h) return node end },        -- a tile's own body
--       },
--       size = "medium",                                       -- small, medium, large
--       on_reordered = function(keys) end, on_size = function(size) end,
--     }
--     dash.move("cpu", 3) ; dash.order() --> { "net", ... } ; dash.set_size("large")
--
-- The tiles are a kit `grid_view` (a Collection: the arrows walk the grid,
-- typing jumps to a tile, Return or a double press calls `on_activated`).
-- A tile dragged past its threshold (a Drag transfer) leaves a ghost
-- under the pointer and lands where it is let go, the others making
-- room; Alt and an arrow moves the current tile. The size switch (a kit
-- `segmented` Selection) sets every tile's size: small tiles show the
-- figure, larger ones its level as a bar too and a `build`er's body.
-- Ids: `<id>-grid`, `<id>-tile-<key>`, `<id>-ghost`, `<id>-size`,
-- `<id>-size-<size>`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

local SIZES = { "small", "medium", "large" }
local CELLS = { small = { 122, 84 }, medium = { 170, 116 }, large = { 256, 140 } }

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 340
  local BAR = 40
  local GH = H - BAR
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local st = morf.state { size = spec.size or "medium", drag_key = "", ghost_x = 0, ghost_y = 0, drop_at = 0,
    current = 0, revision = 0 }

  local tiles, by_key = {}, {}
  for i, tile in ipairs(spec.tiles or {}) do
    local key = tostring(tile.key or tile.title or i)
    by_key[key] = tile
    tiles[i] = { key = key }
  end
  local model = morf.list_model(tiles)
  local function columns() local c = CELLS[st.size] return math.max(1, math.floor(W / c[1])) end
  local function cell() local c = CELLS[st.size] return math.floor(W / columns()), c[2] end
  local function find(key)
    for i = 1, model:len() do if model:get(i).key == key then return i end end
  end
  local function order()
    local out = {}
    for i = 1, model:len() do out[i] = model:get(i).key end
    return out
  end
  local function move(key, to)
    local at = find(key)
    if not at then return false end
    to = math.max(1, math.min(model:len(), to))
    if to == at then return false end
    model:move(at, to)
    st.current = to
    st.revision = st.revision + 1
    if spec.on_reordered then spec.on_reordered(order()) end
    return true
  end

  local holder = ui.Item { y = BAR, width = W, height = GH }
  local root
  local grid
  -- Where a point on the surface falls among the cells.
  local function drop_index(sx, sy)
    local cw, ch = cell()
    local gx, gy = sx - (holder.layout_x or 0), sy - (holder.layout_y or 0)
    local col = math.max(0, math.min(columns() - 1, math.floor(gx / cw)))
    local row = math.max(0, math.floor(gy / ch))
    return math.max(1, math.min(model:len(), row * columns() + col + 1))
  end
  local function delegate(row, s)
    -- (By key: a tile moved keeps its delegate, and the delegate's index.)
    local me = morf.state { key = row.key }
    local function tile() return by_key[me.key] or {} end
    local function index() local _ = st.revision return find(me.key) or 0 end
    local cw, ch = cell()
    local grab = { x = 0, y = 0 }
    local area
    local behaviour = control.headless("Drag", { mode = "transfer", axis = "both", threshold = 6,
      on_drag_started = function() st.drag_key = me.key end,
      on_dropped = function()
        local key, to = st.drag_key, st.drop_at
        st.drag_key = ""
        if key ~= "" and to > 0 then move(key, to) end
        if grid then morf.focus.set(grid, true) end
      end })
    local large = st.size ~= "small"
    local body = ui.Item { anchors = { fill = true, margins = 6 }, clip = true,
      kit.icon(tile().icon or "widgets", large and 22 or 18, kit.ink("accent"), { x = 12, y = 12 }),
      kit.label { x = large and 42 or 36, y = 14, width = cw - 72, elide = "right", text = function() return tile().title or "" end },
      kit.text { x = 12, y = large and 44 or 38, width = cw - 36, elide = "right",
        font_size = st.size == "large" and 28 or (st.size == "medium" and 24 or 18), font_weight = 600,
        text = function() return tostring(get(tile().value) or "") end } }
    if large and by_key[row.key] and by_key[row.key].level ~= nil then
      ui.reparent(kit.surface { x = 12, y = ch - 30, width = cw - 36, height = 6, radius = kit.round(3),
        color = kit.stroke("faint"),
        kit.surface { height = 6, radius = kit.round(3), color = kit.signal("accent"),
          width = function() return math.max(0, math.min(1, tonumber(get(tile().level)) or 0)) * (cw - 36) end } }, body)
    end
    if st.size == "large" and by_key[row.key] and by_key[row.key].build then
      local own = by_key[row.key].build(cw - 24, ch - 80)
      if own then own.x, own.y = cw - 24 - (own.width or 0) - 10, 12 ui.reparent(own, body) end
    end
    area = ui.MouseArea { id = sid("tile-" .. row.key), width = cw, height = ch, cursor = "grab",
      accessible_role = "list_item", accessible_name = function() return tile().title or "" end,
      opacity = function() return st.drag_key == me.key and 0.3 or 1 end,
      on_pressed = function(sx, sy, x, y)
        grab.x, grab.y = x, y
        st.current = index()
        if grid then morf.focus.set(grid, true) end
        behaviour.send("pressed", sx, sy)
      end,
      on_dragged = function(sx, sy)
        behaviour.send("dragged", sx, sy)
        if behaviour.t.active then
          st.ghost_x = sx - (root.layout_x or 0) - grab.x
          st.ghost_y = sy - (root.layout_y or 0) - grab.y
          st.drop_at = drop_index(sx, sy)
        end
      end,
      on_released = function() behaviour.send("released", 0, 0) end,
      on_destroyed = function() behaviour.drop() end,
      kit.card { anchors = { fill = true, margins = 6 } },
      kit.surface { anchors = { fill = true, margins = 6 }, radius = kit.round(12),
        color = function() return kit.ink("hi")():alpha(0.05) end },
      kit.surface { anchors = { fill = true, margins = 6 }, radius = kit.round(12),
        border_width = function()
          if st.drag_key ~= "" and st.drop_at == index() and st.drag_key ~= me.key then return 2 end
          return st.current == index() and 2 or 0
        end,
        border_color = kit.signal("accent"),
        color = function() return kit.signal("accent")():alpha(area and area.hovered and 0.06 or 0) end },
      body }
    return area, function(next_row)
      me.key = next_row.key
      if id then area.id = id .. "-tile-" .. next_row.key end
    end
  end
  local function alt(step)
    return function()
      local row = st.current >= 1 and st.current <= model:len() and model:get(st.current) or nil
      if not row then return false end
      local by = (step == "up" and -columns()) or (step == "down" and columns()) or (step == "left" and -1) or 1
      move(row.key, st.current + by)
      return true
    end
  end
  local function build_grid()
    if grid then ui.destroy(grid, true) end
    local cw, ch = cell()
    grid = widgets.grid_view { id = sid("grid"), accessible_name = spec.accessible_name or "Tiles", width = W,
      height = GH, rows = model, cell_width = cw, cell_height = ch, grid_columns = columns(),
      current = function() return st.current end,
      on_current_changed = function(i) st.current = i end,
      on_activated = function(i) local row = i >= 1 and i <= model:len() and model:get(i) or nil if row and spec.on_activated then spec.on_activated(row.key) end end,
      delegate = delegate }
    ui.reparent(grid, holder)
  end
  build_grid()
  holder.shortcuts = { ["alt+Left"] = alt("left"), ["alt+Right"] = alt("right"), ["alt+Up"] = alt("up"),
    ["alt+Down"] = alt("down") }

  local function set_size(size)
    if not CELLS[size] or size == st.size then return end
    st.size = size
    build_grid()
    if spec.on_size then spec.on_size(size) end
  end
  local size_items = { { label = "S", key = "small" }, { label = "M", key = "medium" }, { label = "L", key = "large" } }
  local switch = widgets.segmented { id = sid("size"), accessible_name = "Tile size", items = size_items,
    item_width = 40, item_height = 32, gap = 2, x = W - 3 * 42, y = 4,
    item_id = function(_, item) return sid("size-" .. item.key) end,
    current = function() for i, s in ipairs(SIZES) do if s == st.size then return i end end return 0 end,
    on_current_changed = function(i) set_size(SIZES[i]) end }

  local ghost = ui.Item { id = sid("ghost"), z = 10, opacity = 0.9, rotation = 2,
    width = function() local _ = st.size return (cell()) end, height = function() local _, ch = cell() return ch end,
    x = function() return st.ghost_x end, y = function() return st.ghost_y end,
    visible = function() return st.drag_key ~= "" end,
    kit.card { anchors = { fill = true, margins = 6 } },
    kit.surface { anchors = { fill = true, margins = 6 }, radius = kit.round(12), border_width = 2,
      border_color = kit.signal("accent"), color = function() return kit.signal("accent")():alpha(0.12) end },
    kit.label { x = 18, y = 20, text = function() local t = by_key[st.drag_key] return t and t.title or "" end } }

  root = ui.Item { id = id, width = W, height = H,
    kit.heading { x = 4, y = 8, height = 24, text = spec.title or "Dashboard" },
    switch, holder, ghost }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, model = model }
  function handle.move(key, to) return move(tostring(key), to) end
  function handle.order() local _ = st.revision return order() end
  function handle.set_size(size) set_size(size) end
  function handle.size() return st.size end
  function handle.current() return st.current end
  return root, handle
end
