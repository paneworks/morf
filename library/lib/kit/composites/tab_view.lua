-- A tab view with an overview (composite: Selection tabs + Navigation pages
-- + Drag reorder + Collection overview grid).
--
--     local node, tabs = composites.tab_view {
--       id = "docs", width = 520, height = 340,
--       tabs = { { title = "Notes", icon = "description", content = build_notes }, ... },
--       current = 1, closable = true, addable = true,
--       on_add = function(n) return { title = "Tab " .. n } end,   -- the new tab (nil: none)
--       on_changed = function(index, tab) end,
--       on_closed = function(tab) end, on_reordered = function(titles) end,
--     }
--     tabs.add { title = "More" } ; tabs.close(2) ; tabs.select(1) ; tabs.toggle_overview()
--
-- The tab strip is a kit `tabs` Selection (the arrows walk it; Alt+Left
-- and Alt+Right, or a drag along the strip, move the current tab), each
-- tab with its icon, title and a close press. The pages are a kit
-- `view_stack` (a Navigation: the new page slides in from its side; each
-- is built when first shown and kept). The overview press swaps the page
-- for a kit `grid_view` (a Collection) of the tabs as cards: a press or
-- Return opens one. Ctrl+T adds a tab, Ctrl+W closes the current one.
-- A tab's `content` is a node or a builder `function(width, height)`.
-- Ids: `<id>-tabs`, `<id>-tab-<i>`, `<id>-close-<i>`, `<id>-add`,
-- `<id>-overview`, `<id>-pages`, `<id>-page-<i>` (the page of the tab at
-- `i` when it was built), `<id>-grid`, `<id>-card-<i>`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 340
  local BAR = spec.bar_height or 40
  local BUTTONS = (spec.addable == false and 0 or 36) + (spec.overview == false and 0 or 36)
  local TAB_W = spec.tab_width or 132
  local closable_default = spec.closable ~= false
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end

  -- The tabs, each with a key its page goes by.
  local serial, tabs = 0, {}
  local function adopt(tab)
    serial = serial + 1
    local own = {}
    for k, v in pairs(type(tab) == "table" and tab or { title = tostring(tab) }) do own[k] = v end
    own.key = "tab" .. serial
    own.title = own.title or own.label or ("Tab " .. serial)
    return own
  end
  for _, tab in ipairs(spec.tabs or {}) do tabs[#tabs + 1] = adopt(tab) end
  local st = morf.state { revision = 0, current = math.max(0, math.min(#tabs, spec.current or 1)), overview = false }
  local function touch() st.revision = st.revision + 1 end
  local function list() local _ = st.revision return tabs end

  local pages, nav = {}, nil
  local function page_of(tab, index)
    pages[tab.key] = function()
      local content = tab.content
      if type(content) == "function" then content = content(W, H - BAR) end
      local holder = ui.Item { id = sid("page-" .. index), width = W, height = H - BAR }
      if content then
        ui.reparent(content, holder)
      else
        ui.reparent(kit.subtitle { anchors = { center_in = true }, text = tab.title }, holder)
      end
      return holder
    end
  end
  for i, tab in ipairs(tabs) do page_of(tab, i) end
  local function keys()
    local out = {}
    for i, tab in ipairs(list()) do out[i] = tab.key end
    return out
  end

  local function changed()
    local tab = tabs[st.current]
    if spec.on_changed then spec.on_changed(st.current, tab) end
  end
  local function select(index)
    if index < 1 or index > #tabs then return end
    local moved = index ~= st.current
    st.current = index
    if nav then nav.go(tabs[index].key) end
    if moved then changed() end
  end
  local handle = {}
  function handle.add(tab)
    local made = adopt(tab or (spec.on_add and spec.on_add(#tabs + 1)) or { title = "New tab" })
    tabs[#tabs + 1] = made
    page_of(made, #tabs)
    touch()
    select(#tabs)
    return made
  end
  function handle.close(index)
    index = index or st.current
    local tab = tabs[index]
    if not tab or tab.closable == false then return end
    table.remove(tabs, index)
    pages[tab.key] = nil
    local current = st.current
    if index < current or (index == current and current > #tabs) then current = current - 1 end
    st.current = math.max(0, current)
    touch()
    if tabs[st.current] and nav then nav.go(tabs[st.current].key) end
    if spec.on_closed then spec.on_closed(tab) end
    if index == current or index <= current then changed() end
  end
  function handle.move(index, step)
    local to = index + step
    if index < 1 or index > #tabs or to < 1 or to > #tabs then return end
    tabs[index], tabs[to] = tabs[to], tabs[index]
    if st.current == index then st.current = to elseif st.current == to then st.current = index end
    touch()
    if spec.on_reordered then
      local titles = {}
      for i, tab in ipairs(tabs) do titles[i] = tab.title end
      spec.on_reordered(titles)
    end
  end
  function handle.select(index) select(index) end
  function handle.current() return st.current end
  function handle.titles()
    local out = {}
    for i, tab in ipairs(list()) do out[i] = tab.title end
    return table.concat(out, ",")
  end
  function handle.count() return #list() end

  -- The strip.
  local strip_w = W - BUTTONS
  local strip = widgets.view_switcher { id = sid("tabs"), accessible_name = spec.accessible_name or "Tabs",
    width = function() return math.max(1, #list() * (TAB_W + 2)) end, height = BAR,
    items = function()
      local out = {}
      for i, tab in ipairs(list()) do out[i] = { label = tab.title, icon = tab.icon, key = tab.key } end
      return out
    end,
    item_width = TAB_W, item_height = BAR, gap = 2, reorderable = true,
    current = function() return st.current end,
    item_id = function(i) return sid("tab-" .. i) end,
    on_current_changed = function(i) select(i) end,
    on_reorder = function(index, step) handle.move(index, step) end,
    delegate = function(i, item, s)
      local tab = tabs[i] or {}
      local closable = (tab.closable == nil and closable_default) or tab.closable == true
      local text_x = item.icon and 36 or 12
      local look = ui.Item { anchors = { fill = true },
        kit.surface { anchors = { fill = true, margins = 3 }, radius = kit.round(10),
          color = function() return kit.signal("accent")():alpha(s.hovered() and not s.current() and 0.07 or 0) end },
        kit.text { x = text_x, anchors = { vertical_center = true }, width = TAB_W - text_x - (closable and 32 or 10),
          elide = "right", text = item.label, font_weight = 500,
          color = function() return (s.current() and kit.ink("hi") or kit.ink("lo"))() end } }
      if item.icon then
        ui.reparent(kit.icon(item.icon, 18, function() return (s.current() and kit.ink("accent") or kit.ink("lo"))() end,
          { x = 12, anchors = { vertical_center = true } }), look)
      end
      if closable then
        ui.reparent(widgets.icon { id = sid("close-" .. i), accessible_name = "Close " .. item.label,
          width = 24, height = 24, size = 14, icon_off = "close",
          anchors = { right = true, right_margin = 6, vertical_center = true },
          on_clicked = function() handle.close(i) end }, look)
      end
      return look
    end }
  local strip_holder = ui.Item { width = strip_w, height = BAR, clip = true, strip }

  local bar_buttons = ui.Row { x = strip_w, width = BUTTONS, height = BAR, gap = 0 }
  if spec.addable ~= false then
    ui.reparent(ui.Item { width = 36, height = BAR,
      widgets.icon { id = sid("add"), accessible_name = "New tab", width = 32, height = 32, size = 18,
        icon_off = "add", anchors = { center_in = true }, on_clicked = function() handle.add() end } }, bar_buttons)
  end

  -- The pages.
  local stack
  stack, nav = widgets.view_stack { id = sid("pages"), y = BAR, width = W, height = H - BAR,
    mode = "switcher", order = keys, pages = pages,
    current = tabs[st.current] and tabs[st.current].key or "",
    visible = function() return not st.overview end }

  -- The overview: the tabs as cards.
  local cards = morf.list_model({})
  local COLS = spec.overview_columns or 3
  local CW = math.floor(W / COLS)
  local CH = spec.card_height or 96
  local function refill()
    local out = {}
    for i, tab in ipairs(list()) do out[i] = { key = tab.key, slot = i, title = tab.title, icon = tab.icon } end
    cards:replace(out, "key")
  end
  refill()
  local function open_card(i)
    st.overview = false
    select(i)
  end
  local grid_node = widgets.grid_view { id = sid("grid"), accessible_name = "Open tabs", y = BAR, width = W,
    height = H - BAR, rows = cards, cell_width = CW, cell_height = CH, grid_columns = COLS,
    visible = function() return st.overview end,
    current = function() return st.current end,
    on_activated = function(i) open_card(i) end,
    delegate = function(row, s)
      -- (By key: a card moved keeps its delegate, and the delegate's index.)
      local me = morf.state { key = row.key }
      local function index()
        for i, tab in ipairs(list()) do if tab.key == me.key then return i end end
        return 0
      end
      local function now() return list()[index()] or row end
      local function is_current() return index() == st.current end
      local area
      area = ui.MouseArea { id = sid(("card-%d"):format(row.slot)), width = CW, height = CH, cursor = "pointer",
        accessible_role = "button", accessible_name = function() return now().title end,
        on_clicked = function() open_card(index()) end,
        kit.card { anchors = { fill = true, margins = 6 } },
        kit.surface { anchors = { fill = true, margins = 6 }, radius = kit.round(12),
          border_width = function() return is_current() and 2 or 0 end,
          border_color = kit.signal("accent"),
          color = function()
            local c = kit.signal("accent")()
            return c:alpha(is_current() and 0.14 or (area and area.hovered and 0.07 or 0))
          end },
        kit.icon(function() return now().icon or "tab" end, 22, kit.ink("accent"), { x = 18, y = 18 }),
        kit.text { x = 18, y = CH - 40, width = CW - 36, elide = "right", font_weight = 500,
          text = function() return now().title end } }
      return area, function(next_row)
        me.key = next_row.key
        if id then area.id = ("%s-card-%d"):format(id, next_row.slot) end
      end
    end }

  if spec.overview ~= false then
    ui.reparent(ui.Item { width = 36, height = BAR,
      widgets.icon { id = sid("overview"), accessible_name = "Overview", width = 32, height = 32, size = 18,
        icon_off = "grid_view",
        anchors = { center_in = true },
        on_clicked = function()
          st.overview = not st.overview
          if st.overview then morf.focus.set(grid_node, true) end
        end } }, bar_buttons)
  end

  local root = ui.Item { id = id, width = W, height = H, clip = true,
    shortcuts = {
      ["ctrl+t"] = function() if spec.addable == false then return false end handle.add() end,
      ["ctrl+w"] = function() if #tabs == 0 then return false end handle.close(st.current) end,
    },
    strip_holder, bar_buttons, stack, grid_node,
    kit.subtitle { anchors = { center_in = true }, visible = function() return #list() == 0 end,
      text = spec.empty_text or "No tabs open" } }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  morf.effect("kit.tab_view.cards." .. tostring(root), refill, { owner = root })
  function handle.toggle_overview() st.overview = not st.overview end
  function handle.overview() return st.overview end
  handle.node, handle.nav = root, nav
  return root, handle
end
