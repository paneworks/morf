-- Navigations (the Navigation archetype): AdwNavigationView's push and
-- pop, AdwViewStack, GtkStack, AdwCarousel and AdwTabView.
--
--     local node, nav = navigation.make("navigation_view", {
--       width = 400, height = 600, current = "settings",
--       pages = { settings = build_settings, sound = build_sound },   -- builders or nodes
--     })
--     nav.push("sound") ; nav.pop() ; nav.go("settings")
--
-- Alt+Left and a mouse's back button pop, wherever focus is inside it (its
-- shortcuts); Ctrl+Tab cycles a switcher; a carousel's arrows walk it. A
-- page is built when first shown and kept -- its scroll and focus with it
-- -- while it is in the stack. The skin's `transition(from, to, direction)`
-- moves the pages; without one the new page slides in from its side.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- How each widget changes its pages: a stack pushes and pops, a switcher
-- shows one by name, a carousel walks them in order.
local MODE = { navigation_view = "stack", view_stack = "switcher", stack = "switcher", tab_view = "switcher",
  carousel = "carousel" }

function M.make(widget, spec)
  spec = spec or {}
  local W, H = spec.width, spec.height
  local builders = spec.pages or {}
  local names = {}
  for name in pairs(builders) do names[#names + 1] = name end
  table.sort(names)
  local built = {}
  local area = ui.Item { width = W, height = H, clip = true }
  local active=morf.signal("kit.navigation.active."..tostring(area),"")
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  if full.mode == nil then full.mode = MODE[widget] end
  full.pages = spec.order or names
  local root, t, ctl
  local shown
  local function page(name)
    if not built[name] then
      local b = builders[name]
      local node = type(b) == "function" and b() or b
      if not node then return nil end
      -- Outgoing pages still draw during their transition, but must stop
      -- accepting clicks and focus immediately. Keep this gate on a wrapper
      -- so the page's own enabled binding remains intact when it returns.
      local layer=ui.Item {width=W or function() return node.layout_width end,
        height=H or function() return node.layout_height end,visible=false,
        enabled=function() return active:get()==name end,node}
      ui.reparent(layer, area)
      built[name] = layer
    end
    return built[name]
  end
  local function show(name, direction)
    local next_node = page(name)
    if not next_node or next_node == shown then return end
    local from = shown
    shown = next_node
    active:set(name)
    next_node.visible = true
    local transition = ctl and ctl.builders().transition
    if transition then
      transition(from, next_node, direction)
    elseif from then
      -- The new page in from its side, the old out the other way.
      local width = W or 400
      next_node.translate_x = direction * width
      morf.animation.play { { parallel = {
        { node = next_node, property = "translate_x", to = 0, duration = 260, easing = "out_cubic" },
        { node = from, property = "translate_x", to = -direction * width * 0.3, duration = 260, easing = "out_cubic" },
      } }, on_finished = function() if from ~= shown then from.visible = false from.translate_x = 0 end end }
    end
  end
  local given = spec.on_current_changed
  full.on_current_changed = function(name, direction)
    show(name, direction or 1)
    if given then given(name, direction) end
  end
  local props = { width = W, height = H, focus_policy = spec.focus_policy or "none" }
  for _, k in ipairs { "id", "x", "y", "anchors", "visible", "z" } do props[k] = spec[k] end
  props.shortcuts = {
    -- Nothing to go back to: the key goes on to what is around.
    ["alt+Left"] = function() if not (t and t.can_go_back) then return false end ctl.send("pop") end,
    ["back"] = function() if not (t and t.can_go_back) then return false end ctl.send("pop") end,
    ["ctrl+Tab"] = function() if t and t.mode == "switcher" then ctl.send("next") return true end return false end,
    ["ctrl+shift+Tab"] = function() if t and t.mode == "switcher" then ctl.send("previous") return true end return false end,
  }
  root, t, ctl = control.make("Navigation", widget, full, { children = { area }, props = props,
    builders = { transition = true } })
  show(t.current, 1)
  local handle = { node = root, t = t }
  function handle.push(name) ctl.send("push", name) end
  function handle.pop() ctl.send("pop") end
  function handle.go(name) ctl.send("go", name) end
  function handle.next() ctl.send("next") end
  function handle.previous() ctl.send("previous") end
  return root, handle
end


--- AdwCarousel: pages side by side, one shown, a swipe across it turning
--- to the next or the previous, arrows at its sides and an indicator under
--- it (`indicator`: "dots", the default, "lines", or false).
---
---     local node, car = navigation.carousel {
---       id = "tips", width = 520, height = 300,
---       pages = { first_node, function(w, h) return second_node end },
---       current = 1, wrap = false, on_changed = function(index) end,
---     }
---     car.next() ; car.previous() ; car.go(3) ; car.current()
---
--- The pages are a Navigation in carousel mode (each built when first
--- shown; Left and Right walk it once it has focus). Ids: `<id>-slides`,
--- `<id>-swipe`, `<id>-previous`, `<id>-next`, `<id>-indicator`,
--- `<id>-dot-<i>`.
function M.carousel(spec)
  spec = spec or {}
  local press = require("lib.kit.archetypes.press")
  local drag = require("lib.kit.archetypes.drag")
  local selection = require("lib.kit.archetypes.selection")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 300
  local indicator = spec.indicator
  if indicator == nil then indicator = "dots" end
  local BAR = indicator and 28 or 0
  local SH = H - BAR
  local list = spec.pages or {}
  local n = #list
  local wrap = spec.wrap == true
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local st = morf.state { current = math.max(1, math.min(n, spec.current or 1)) }
  local names, pages, index_of = {}, {}, {}
  for i, page in ipairs(list) do
    local name = "p" .. i
    names[i], index_of[name] = name, i
    pages[name] = function()
      local node = type(page) == "function" and page(W, SH) or page
      return ui.Item { width = W, height = SH, node }
    end
  end
  local nav_node, nav = M.make("carousel", { id = sid("slides"), accessible_name = spec.accessible_name or "Slides",
    width = W, height = SH, order = names, pages = pages, wrap = wrap,
    current = names[st.current], focus_policy = "strong",
    on_current_changed = function(name)
      local i = index_of[name]
      if i and i ~= st.current then
        st.current = i
        if spec.on_changed then spec.on_changed(i) end
      end
    end })
  local function go(i)
    if i < 1 or i > n then
      if not wrap or n == 0 then return end
      i = (i - 1) % n + 1
    end
    nav.go(names[i])
  end
  local function step(by) if by > 0 then nav.next() else nav.previous() end end
  -- The swipe: over the page, the page following it.
  local swipe = ui.MouseArea { id = sid("swipe"), width = W, height = SH, z = 1, cursor = "grab",
    accessible_hidden = true }
  local behaviour = drag.swipe(swipe, { axis = "x", distance = spec.swipe_distance or math.min(120, W / 5),
    on_pressed = function() morf.focus.set(nav_node, true) end,
    on_swiped = function(direction) step(direction == "left" and 1 or -1) end })
  swipe.translate_x = 0
  nav_node.translate_x = function() return behaviour.t.delta_x * 0.6 end
  local function arrow(dir)
    local can = function() return wrap or (dir < 0 and st.current > 1) or (dir > 0 and st.current < n) end
    return press.make("button", { id = sid(dir < 0 and "previous" or "next"), variant = "circular osd",
      accessible_name = dir < 0 and "Previous page" or "Next page", width = 36, height = 36,
      icon = dir < 0 and "chevron_left" or "chevron_right", z = 2,
      x = dir < 0 and 8 or W - 44, y = SH / 2 - 18,
      enabled = can, opacity = function() return can() and 1 or 0.35 end,
      on_clicked = function() step(dir) end })
  end
  local root = ui.Item { id = id, width = W, height = H,
    ui.Item { width = W, height = SH, clip = true, nav_node, swipe, arrow(-1), arrow(1) } }
  if indicator then
    local items, size = {}, indicator == "lines" and 24 or 16
    for i = 1, n do items[i] = { label = "Page " .. i } end
    ui.reparent(selection.make("carousel_indicator_" .. indicator, { id = sid("indicator"), accessible_name = "Page",
      items = items, item_width = size, item_height = 20,
      x = math.floor((W - n * size) / 2), y = SH + (BAR - 20) / 2,
      current = function() return st.current end,
      item_id = function(i) return sid("dot-" .. i) end,
      on_current_changed = function(i) go(i) end }), root)
  end
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, nav = nav }
  function handle.next() step(1) end
  function handle.previous() step(-1) end
  function handle.go(i) go(i) end
  function handle.current() return st.current end
  return root, handle
end

--- AdwTabView: tabs and their pages, one page shown -- what a tab bar
--- and a tab overview (lib.kit.composites) draw.
---
---     local node, view = navigation.tab_view {
---       id = "docs", width = 520, height = 300,
---       tabs = { { title = "Notes", icon = "description", content = build_notes }, ... },
---       current = 1,
---       on_add = function(n) return { title = "Tab " .. n } end,   -- the new tab (nil: none)
---       on_changed = function(index, tab) end,
---       on_closed = function(tab) end, on_reordered = function(titles) end,
---     }
---     view.add { title = "More" } ; view.close(2) ; view.select(1) ; view.move(1, 1)
---
--- A tab's `content` is a node or a builder `function(width, height)`; its
--- page is built when first shown and kept, and slides in from its side.
--- `view.tabs()` (a binding follows it) lists the tabs, each `{ key, title,
--- icon, closable }`; `view.current()`, `view.count()`, `view.titles()`.
--- Ctrl+T adds a tab and Ctrl+W closes the current one when the view has
--- focus. Ids: `<id>`, `<id>-page-<i>` (the page of the tab at `i` when it
--- was built).
function M.tab_view(spec)
  spec = spec or {}
  local W, H = spec.width or 520, spec.height or 300
  local id = spec.id
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
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
  local st = morf.state { revision = 0, current = math.max(0, math.min(#tabs, spec.current or 1)) }
  local function touch() st.revision = st.revision + 1 end
  local function list() local _ = st.revision return tabs end
  local pages, nav = {}, nil
  local function page_of(tab, index)
    pages[tab.key] = function()
      local content = tab.content
      if type(content) == "function" then content = content(W, H) end
      local holder = ui.Item { id = sid("page-" .. index), width = W, height = H }
      if content then ui.reparent(content, holder) end
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
    if spec.on_changed then spec.on_changed(st.current, tabs[st.current]) end
  end
  local function select(index)
    if index < 1 or index > #tabs then return end
    local moved = index ~= st.current
    st.current = index
    if nav then nav.go(tabs[index].key) end
    if moved then changed() end
  end
  local view = {}
  function view.tabs() return list() end
  function view.add(tab)
    local made = adopt(tab or (spec.on_add and spec.on_add(#tabs + 1)) or { title = "New tab" })
    tabs[#tabs + 1] = made
    page_of(made, #tabs)
    touch()
    select(#tabs)
    return made
  end
  function view.close(index)
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
    if index <= current then changed() end
  end
  function view.move(index, step)
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
  view.select = select
  function view.current() return st.current end
  function view.count() return #list() end
  function view.titles()
    local out = {}
    for i, tab in ipairs(list()) do out[i] = tab.title end
    return table.concat(out, ",")
  end
  local node
  node, nav = M.make("tab_view", { id = id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H,
    order = keys, pages = pages, current = tabs[st.current] and tabs[st.current].key or "",
    visible = spec.visible })
  node.shortcuts = {
    ["ctrl+t"] = function() if not spec.on_add then return false end view.add() end,
    ["ctrl+w"] = function() if #tabs == 0 then return false end view.close(st.current) end,
  }
  view.node, view.nav = node, nav
  return node, view
end

return M
