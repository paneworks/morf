-- A transfer list (composite: two Collections + Press move buttons).
--
--     local node, transfer = composites.transfer_list {
--       id = "columns", width = 520, height = 300,
--       items = { "Name", "Size", "Type", "Modified" },   -- strings or { key, label, icon }
--       chosen = { "Name" },                             -- keys (or labels) already on the right
--       titles = { "Available", "Shown" },
--       on_changed = function(chosen_keys) end,
--     }
--     transfer.move_right() ; transfer.move_left() ; transfer.chosen() --> { "Name", ... }
--
-- Each side is a kit `transfer_list` (a Collection in multi mode: a press
-- toggles an item into the selection, Space too, Ctrl+A takes them all,
-- the arrows walk it). Between them, presses move the selected items
-- across (or all of them); a double press or Return moves the current
-- one. A side's title counts what it holds and what is selected. Ids:
-- `<id>-left`, `<id>-right`, `<id>-left-<key>`, `<id>-right-<key>`,
-- `<id>-add`, `<id>-add-all`, `<id>-remove`, `<id>-remove-all`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function key_of(item) return tostring(type(item) == "table" and (item.key or item.label) or item) end
local function label_of(item) return tostring(type(item) == "table" and (item.label or item.key) or item) end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 300
  local MID = 56
  local SIDE = math.floor((W - MID) / 2)
  local TITLE = 30
  local ROW = spec.row_height or 36
  local titles = spec.titles or { "Available", "Chosen" }
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end

  local order, by_key = {}, {}
  for i, item in ipairs(spec.items or {}) do
    local key = key_of(item)
    order[key] = i
    by_key[key] = { key = key, label = label_of(item), icon = type(item) == "table" and item.icon or nil }
  end
  local chosen = {}
  for _, k in ipairs(spec.chosen or {}) do
    local key = tostring(k)
    if not by_key[key] then
      for kk, row in pairs(by_key) do if row.label == key then key = kk end end
    end
    chosen[key] = by_key[key] and true or nil
  end
  local models = { left = morf.list_model({}), right = morf.list_model({}) }
  -- Each side's selected indices, kept here: the lists follow them.
  local st = morf.state { left = 0, right = 0, revision = 0 }
  local picked = { left = {}, right = {} }
  -- Every move rebinds every row (a fresh `slot` each time): a row the
  -- list only shifted would keep the index it was bound with.
  local generation = 0
  local function refill()
    generation = generation + 1
    local left, right = {}, {}
    for _, item in ipairs(spec.items or {}) do
      local base = by_key[key_of(item)]
      local row = { key = base.key, label = base.label, icon = base.icon, slot = base.key .. "#" .. generation }
      if chosen[base.key] then right[#right + 1] = row else left[#left + 1] = row end
    end
    models.left:replace(left, "slot")
    models.right:replace(right, "slot")
  end
  refill()
  local function clear(side)
    picked[side] = {}
    st[side] = 0
    st.revision = st.revision + 1
  end
  local function report()
    if spec.on_changed then
      local out = {}
      for _, item in ipairs(spec.items or {}) do if chosen[key_of(item)] then out[#out + 1] = key_of(item) end end
      spec.on_changed(out)
    end
  end
  local function move(side, keys)
    if #keys == 0 then return end
    for _, key in ipairs(keys) do chosen[key] = (side == "right") or nil end
    refill()
    clear("left") clear("right")
    report()
  end
  local function selected_keys(side)
    local out = {}
    for _, i in ipairs(picked[side]) do
      local row = i >= 1 and i <= models[side]:len() and models[side]:get(i) or nil
      if row then out[#out + 1] = row.key end
    end
    return out
  end
  local function all_keys(side)
    local out = {}
    for i = 1, models[side]:len() do out[i] = models[side]:get(i).key end
    return out
  end

  local lists = {}
  local function side_view(side, x)
    local model = models[side]
    local LH = H - TITLE
    local node = widgets.transfer_list { id = sid(side), accessible_name = titles[side == "left" and 1 or 2],
      y = TITLE, width = SIDE, height = LH, rows = model, row_height = ROW, mode = "multi",
      selected = function() local _ = st.revision return picked[side] end,
      on_selection_changed = function(list)
        local out = {}
        for i, v in ipairs(list or {}) do out[i] = math.floor(tonumber(v) or 0) end
        picked[side] = out
        st[side] = #out
      end,
      on_activated = function(i)
        local row = i >= 1 and i <= model:len() and model:get(i) or nil
        if row then move(side == "left" and "right" or "left", { row.key }) end
      end,
      delegate = function(row, s)
        local function now() return s.row() or row end
        local look
        look = ui.Item { id = sid(side .. "-" .. row.key), width = SIDE, height = ROW,
          kit.surface { anchors = { fill = true, margins = 2 }, radius = kit.round(8),
            color = function()
              local c = kit.signal("accent")()
              if s.selected() then return c:alpha(0.2) end
              return c:alpha(s.hovered() and 0.07 or 0)
            end,
            border_width = function() return s.current() and 1 or 0 end,
            border_color = kit.stroke("mark") },
          kit.icon(function() return s.selected() and "check_box" or "check_box_outline_blank" end, 18,
            function() return (s.selected() and kit.ink("accent") or kit.ink("lo"))() end,
            { x = 10, anchors = { vertical_center = true } }),
          kit.text { x = 38, anchors = { vertical_center = true }, width = SIDE - 48, elide = "right",
            text = function() return now().label end } }
        return look, function(next_row) if id then look.id = id .. "-" .. side .. "-" .. next_row.key end end
      end }
    lists[side] = node
    local title = titles[side == "left" and 1 or 2]
    return ui.Item { x = x, width = SIDE, height = H,
      kit.card { y = TITLE, width = SIDE, height = LH },
      kit.surface { y = TITLE, width = SIDE, height = LH, radius = kit.round(12),
        color = function() return kit.ink("hi")():alpha(0.04) end },
      kit.label { x = 4, y = 6, width = SIDE - 8, elide = "right",
        text = function()
          local n, k = model:len(), st[side]
          return k > 0 and ("%s · %d of %d"):format(title, k, n) or ("%s · %d"):format(title, n)
        end },
      node,
      kit.label { anchors = { center_in = true }, y = TITLE, text = "Nothing here",
        visible = function() return model:len() == 0 end } or nil }
  end

  local function button(suffix, icon, name, enabled, action)
    return widgets.icon { id = sid(suffix), accessible_name = name, width = 40, height = 36, size = 20,
      icon_off = icon, enabled = enabled, opacity = function() return enabled() and 1 or 0.35 end,
      on_clicked = function() if enabled() then action() end end }
  end
  local buttons = ui.Column { x = SIDE + (MID - 40) / 2, y = TITLE + math.max(0, (H - TITLE - 4 * 36 - 3 * 6) / 2),
    gap = 6,
    button("add", "chevron_right", "Move selected across", function() return st.left > 0 end,
      function() move("right", selected_keys("left")) end),
    button("add-all", "keyboard_double_arrow_right", "Move all across",
      function() return models.left:len() > 0 end, function() move("right", all_keys("left")) end),
    button("remove", "chevron_left", "Move selected back", function() return st.right > 0 end,
      function() move("left", selected_keys("right")) end),
    button("remove-all", "keyboard_double_arrow_left", "Move all back",
      function() return models.right:len() > 0 end, function() move("left", all_keys("right")) end) }

  local root = ui.Item { id = id, width = W, height = H, side_view("left", 0), buttons,
    side_view("right", SIDE + MID) }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, left = models.left, right = models.right }
  function handle.move_right() move("right", selected_keys("left")) end
  function handle.move_left() move("left", selected_keys("right")) end
  function handle.chosen()
    local out = {}
    for _, item in ipairs(spec.items or {}) do if chosen[key_of(item)] then out[#out + 1] = key_of(item) end end
    return out
  end
  function handle.selected(side) return selected_keys(side or "left") end
  return root, handle
end
