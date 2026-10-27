-- AdwSidebar (composite: Selection items + Disclosure sections).
--
--     local node, side = composites.sidebar {
--       id = "nav", width = 240, height = 340,
--       sections = {
--         { title = "Library", items = {
--           { key = "inbox", label = "Inbox", icon = "inbox", badge = 4 },   -- badge: a count, a word or a binding
--           { key = "sent", label = "Sent", icon = "send" } } },
--         { title = "Labels", expanded = false, items = { ... } },
--       },
--       current = "inbox",                     -- a key, or a binding
--       on_changed = function(key, item) end,
--       collapsed = false,                     -- icon-only; a binding, or the toggle's
--       on_collapsed = function(collapsed) end,
--     }
--     side.select("sent") ; side.collapse(true) ; side.current() ; side.expanded(2)
--
-- Each section is an `expander` (a Disclosure: a press,
-- Space or Return folds it, Left and Right close and open) holding a
-- `sidebar_list` (a Selection: the arrows walk it, typing jumps, a press
-- chooses). An item is its icon, label and badge. The toggle at the top
-- folds the whole sidebar to its icons -- one list of every item -- and
-- back, the width easing
-- between. Ids: `<id>-toggle`, `<id>-section-<s>`, `<id>-list-<s>`,
-- `<id>-item-<key>`, `<id>-rail`, `<id>-rail-<key>`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local selection = require("lib.kit.archetypes.selection")
local tokens = require("lib.kit.tokens")

local function get(v) if type(v) == "function" then return v() end return v end

return function(spec)
  spec = spec or {}
  local id = spec.id
  local W, H = spec.width or 240, spec.height or 340
  -- The height may be a binding (a window's, as it resizes).
  local Hn = function() return get(H) end
  local RAIL = spec.rail_width or 56
  local ROW = spec.item_height or 38
  local HEAD = spec.header_height or 32
  local TOP = 44
  local sections = spec.sections or {}
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local function key_of(item) return tostring(item.key or item.label) end

  local first
  for _, section in ipairs(sections) do
    for _, item in ipairs(section.items or {}) do first = first or key_of(item) end
  end
  local st = morf.state { current = get(spec.current) or "", collapsed = get(spec.collapsed) == true }
  local function choose(key)
    if key == st.current then return end
    st.current = key
    if spec.on_changed then
      for _, section in ipairs(sections) do
        for _, item in ipairs(section.items or {}) do
          if key_of(item) == key then spec.on_changed(key, item) return end
        end
      end
    end
  end
  local function collapse(on)
    on = on == true
    if on == st.collapsed then return end
    st.collapsed = on
    if spec.on_collapsed then spec.on_collapsed(on) end
  end

  -- One entry's look: icon, label, badge.
  local function entry(item, s, rail)
    local width = rail and RAIL or W - 8
    local look = ui.Item { anchors = { fill = true },
      ui.Rect { anchors = { fill = true, margins = 2 }, radius = 10,
        color = function() return tokens.color("accent_bg")():alpha(s.hovered() and not s.current() and 0.07 or 0) end } }
    local ink = function() return (s.current() and tokens.color("accent") or tokens.color("window_fg", 0.55))() end
    if item.icon then
      ui.reparent(widgets.image { icon = item.icon, pixel_size = 20, color = ink,
        anchors = rail and { center_in = true } or { left = true, left_margin = 12, vertical_center = true } }, look)
    end
    local badge = item.badge
    local function has_badge()
      local v = get(badge)
      return v ~= nil and v ~= false and v ~= 0 and v ~= ""
    end
    if not rail then
      local x = item.icon and 44 or 14
      ui.reparent(widgets.label { anchors = { left = true, left_margin = x, vertical_center = true },
        width = width - x - 44, elide = "right", text = item.label or key_of(item),
        color = function() return (s.current() and tokens.color("window_fg") or tokens.color("window_fg", 0.55))() end },
        look)
    end
    if badge ~= nil and rail then
      -- Folded, a badge is a dot on the icon's shoulder.
      ui.reparent(ui.Rect { x = RAIL / 2 + 2, y = 4, width = 7, height = 7, radius = 3.5,
        color = tokens.color("error_bg"), visible = has_badge }, look)
    elseif badge ~= nil then
      local count = widgets.label { anchors = { center_in = true }, variant = "caption_heading",
        color = tokens.color("accent_fg"), text = function() return tostring(get(badge) or "") end }
      ui.reparent(ui.Rect { anchors = { right = true, right_margin = 10, vertical_center = true }, height = 18,
        radius = 9, color = tokens.color("accent_bg"), visible = has_badge,
        width = function() return math.max(18, (count.layout_width or 0) + 10) end, count }, look)
    end
    return look
  end

  -- Expanded: a folding section each.
  local column = ui.Column { gap = 4, width = W }
  local folds = {}
  for s_index, section in ipairs(sections) do
    local items = section.items or {}
    local list = selection.make("sidebar_list", { id = sid("list-" .. s_index), accessible_name = section.title,
      x = 4, items = items, item_width = W - 8, item_height = ROW, gap = 2, orientation = "vertical",
      item_id = function(_, item) return sid("item-" .. key_of(item)) end,
      current = function()
        for i, item in ipairs(items) do if key_of(item) == st.current then return i end end
        return 0
      end,
      on_current_changed = function(i) if items[i] then choose(key_of(items[i])) end end,
      on_activated = function(i) if items[i] then choose(key_of(items[i])) end end,
      delegate = function(_, item, s) return entry(item, s, false) end })
    local holder = ui.Item { width = W, height = #items * (ROW + 2) + 4, list }
    local fold, t = widgets.expander { id = sid("section-" .. s_index), title = section.title or "",
      width = W, header_height = HEAD, expanded = section.expanded ~= false, content = holder,
      on_toggled = section.on_toggled }
    folds[s_index] = t
    ui.reparent(fold, column)
  end
  local scroller = widgets.scrolled_window { id = sid("scroll"), y = TOP, width = W, height = function() return Hn() - TOP end, clip = true,
    scroll_policy_x = "never", column }
  local expanded_view = ui.Item { width = W, height = H, clip = true,
    opacity = function() return st.collapsed and 0 or 1 end,
    visible = function() return not st.collapsed end,
    behavior = { opacity = { duration = 160 } },
    scroller }

  -- Collapsed: the icons only, one list.
  local all = {}
  for _, section in ipairs(sections) do
    for _, item in ipairs(section.items or {}) do all[#all + 1] = item end
  end
  local rail = selection.make("sidebar_list", { id = sid("rail"), accessible_name = spec.accessible_name or "Sections",
    y = TOP, items = all, item_width = RAIL, item_height = ROW, gap = 2, orientation = "vertical",
    item_id = function(_, item) return sid("rail-" .. key_of(item)) end,
    current = function()
      for i, item in ipairs(all) do if key_of(item) == st.current then return i end end
      return 0
    end,
    on_current_changed = function(i) if all[i] then choose(key_of(all[i])) end end,
    delegate = function(_, item, s) return entry(item, s, true) end })
  local rail_view = ui.Item { width = RAIL, height = H, clip = true,
    visible = function() return st.collapsed end, rail }

  local toggle = widgets.button { id = sid("toggle"), accessible_name = "Collapse sidebar", width = 34, height = 34,
    variant = "flat", icon = "side_navigation",
    -- At the start: (RAIL - 36) / 2 is its margin in the rail as well.
    anchors = { left = true, left_margin = math.floor((RAIL - 36) / 2) }, y = 4,
    on_clicked = function() collapse(not st.collapsed) end }
  local title = spec.title and widgets.label { anchors = { left = true, left_margin = 54 }, y = 10, height = 24,
    variant = "heading",
    text = spec.title,
    visible = function() return not st.collapsed end } or nil

  local root = ui.Item { id = id, height = H, clip = true,
    width = function() return st.collapsed and RAIL or W end,
    behavior = { width = { duration = 220, easing = "out_cubic" } },
    accessible_role = "navigation", accessible_name = spec.accessible_name or "Sidebar",
    ui.Rect { anchors = { fill = true }, radius = 14, color = tokens.color("sidebar_bg") },
    expanded_view, rail_view, toggle, title }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  if st.current == "" and spec.current == nil and first then st.current = first end
  if type(spec.current) == "function" then
    morf.effect("kit.sidebar.current." .. tostring(root), function()
      local v = spec.current()
      if v ~= nil then st.current = tostring(v) end
    end, { owner = root })
  end
  if type(spec.collapsed) == "function" then
    morf.effect("kit.sidebar.collapsed." .. tostring(root), function() st.collapsed = spec.collapsed() == true end,
      { owner = root })
  end
  local handle = { node = root }
  function handle.select(key) choose(tostring(key)) end
  function handle.current() return st.current end
  function handle.collapse(on) collapse(on) end
  function handle.collapsed() return st.collapsed end
  function handle.expanded(index) return folds[index] and folds[index].expanded or false end
  return root, handle
end
