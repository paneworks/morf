-- Overflows (the Overflow archetype): items sharing a line keep the ones
-- that fit and put the rest behind a "more" button (priority+) -- a
-- toolbar's actions, a tab strip, a breadcrumb trail, a row of chips, a
-- site's navigation.
--
--     local node, bar = overflow.make("overflow_toolbar", {
--       id = "actions", width = function() return room:get() end,
--       items = {
--         { icon = "content_cut", label = "Cut", on_activated = cut, priority = 2 },
--         { icon = "share", label = "Share", on_activated = share, pinned = true },
--         { node = some_control, label = "Zoom", on_activated = zoom },   -- a node as it is
--       },
--     })
--     bar.open_menu() bar.close_menu() bar.shown() bar.hidden()
--
-- Each item's laid-out width is measured and the archetype decides what
-- fits the control's width: higher `priority` stays longer, `pinned`
-- never goes, equals go from the end (breadcrumbs: from the middle, so the
-- first and the last stay). Shown items sit in a row at running x and
-- spring to their places as the line changes; hidden ones are hidden. The
-- "more" button (the skin's `more(s)` look on a press; "…" between a
-- breadcrumb trail's ends) opens a menu listing the hidden items, and a
-- press there runs the item's `on_activated` (or `on_clicked`).
--
-- A tab strip and a navigation take `current` (a value or a binding) and
-- `on_current_changed(i)`: the current item is chosen and given the
-- highest priority, so it never goes. Other fields: `mode` ("end",
-- "start", "middle", "priority"), `gap`, `height`, `menu_width`, `x`,
-- `y`, `anchors`, `on_changed(shown, hidden)`. Ids: `<id>-item-<n>` (or
-- the item's own `id`), `<id>-more`, `<id>-menu`, `<id>-menu-<n>`.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- The press each widget's described items are drawn as.
local ITEM = { overflow_toolbar = "flat", overflow_tabs = "flat", overflow_breadcrumbs = "flat",
  chip_overflow = "chip_assist", priority_nav = "flat" }
-- Layout shared across themes: gap between items, the line's height.
local LAYOUT = {
  overflow_toolbar = { gap = 4, height = 40 },
  overflow_tabs = { gap = 2, height = 40 },
  overflow_breadcrumbs = { gap = 0, height = 36 },
  chip_overflow = { gap = 8, height = 32 },
  priority_nav = { gap = 4, height = 40 },
}
-- The "more" button's width, where it is not square.
local MORE_W = { overflow_breadcrumbs = 48, chip_overflow = 52 }
local CHOOSES = { overflow_tabs = true, priority_nav = true }

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

--- A described press's width when it shows a label: the label's own,
--- measured by a probe the press carries unseen, and room round it.
local function label_width(label, icon, room)
  local ok, kit = pcall(require, "kit")
  if not (label and ok and type(kit) == "table" and kit.text) then return nil, nil end
  local probe = kit.text { text = tostring(label), font_weight = 600, opacity = 0, accessible_hidden = true }
  return function() return math.ceil(probe.layout_width or 0) + (icon and 26 or 0) + room end, probe
end

local serial = 0

function M.make(widget, spec)
  spec = spec or {}
  serial = serial + 1
  local key = "kit.overflow." .. widget .. "." .. serial
  local kit = K()
  local layout = LAYOUT[widget] or LAYOUT.overflow_toolbar
  local id = spec.id
  local H = spec.height or layout.height
  local GAP = spec.gap or layout.gap
  local crumbs = widget == "overflow_breadcrumbs"
  local mode = spec.mode or (crumbs and "middle" or "end")
  local chooses = CHOOSES[widget] or spec.current ~= nil

  local root, t, ctl
  local descs, holders, nodes = {}, {}, {}
  local n = 0

  -- The chosen item (a tab strip's, a navigation's).
  local chosen = morf.signal(key .. ".current", type(spec.current) == "number" and spec.current or 1)
  local function current()
    if type(spec.current) == "function" then return spec.current() or 0 end
    return chosen:get()
  end
  local function choose(i)
    chosen:set(i)
    if spec.on_current_changed then spec.on_current_changed(i) end
  end
  local function activate(i)
    local d = descs[i]
    if chooses then choose(i) end
    local run = d.on_activated or d.on_clicked
    if run then run(i) end
  end

  local function label_of(d, i) return d.label or d.tooltip or d.accessible_name or d.icon or ("Item " .. i) end

  local function build(d, i)
    if d.node then return d.node end
    local press = d.widget or spec.item_widget or ITEM[widget] or "flat"
    local s = { widget = press, id = d.id or (id and (id .. "-item-" .. i)) or nil, label = d.label, icon = d.icon,
      icon_off = d.icon, icon_on = d.icon, height = d.height or (press:match("^chip") and 32 or H - 4),
      size = d.size or 20, width = d.width or ((d.icon and not d.label) and (H - 4) or nil), enabled = d.enabled, accessible_name = label_of(d, i), on_clicked = function() activate(i) end }
    if chooses then
      s.checkable = true
      s.checked = function() return current() == i end
      s.on_clicked = nil
      s.on_toggled = function() activate(i) end
    end
    local probe
    if not s.width and s.label and press ~= "icon" then
      s.width, probe = label_width(s.label, s.icon, press:match("^chip") and 34 or 28)
    end
    local node = control.make("Press", press, s, probe and { children = { probe } } or nil)
    if d.tooltip and not d.label then
      local popup = require("lib.kit.popup")
      if popup.tooltip then popup.tooltip(node, d.tooltip) end
    end
    if crumbs and i > 1 and kit.icon then
      -- A trail's step: the chevron that leads to it, then the crumb.
      return ui.Row { gap = 0, align = "center",
        ui.Item { width = 18, height = 18, kit.icon("chevron_right", 16, kit.ink and kit.ink("lo") or nil,
          { anchors = { center_in = true } }) }, node }
    end
    return node
  end

  for _, entry in ipairs(spec.items or {}) do
    n = n + 1
    local d = type(entry) == "userdata" and { node = entry } or entry
    descs[n] = d
  end

  local function priorities()
    local out = {}
    local cur = chooses and current() or 0
    for i = 1, n do out[i] = (i == cur) and 1e6 or (tonumber(descs[i].priority) or 0) end
    return out
  end
  local pinned = {}
  for i = 1, n do if descs[i].pinned then pinned[#pinned + 1] = i end end

  local stage = ui.Item { anchors = { fill = true } }
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.items = nil
  full.current = nil
  full.mode = mode
  full.gap = GAP
  full.pinned = pinned
  full.priorities = priorities
  full.width = nil
  full.height = nil
  local props = {
    focus_policy = "none",
    width = spec.width or (spec.anchors == nil and 400 or nil),
    height = spec.height or H,
    accessible_role = spec.accessible_role or (crumbs and "navigation" or "toolbar"),
  }
  if type(spec.width) == "function" then props.width = spec.width end

  local more, more_t, more_look, menu
  local function more_state()
    return {
      mode = mode, height = H, width = MORE_W[widget] or H,
      count = function() local c = 0 for _ in tostring(t.hidden or ""):gmatch("%d+") do c = c + 1 end return c end,
      hovered = function() return more_t ~= nil and more_t.hovered == true end,
      down = function() return more_t ~= nil and more_t.down == true end,
      open = function() return t.menu_open == true end,
      focused = function() return more_t ~= nil and more_t.visual_focus == true end,
    }
  end
  local function dress_more(builders)
    if not more then return end
    if more_look then ui.destroy(more_look, true) end
    local look = builders and builders.more
    more_look = type(look) == "function" and look(more_state()) or nil
    if not more_look and kit.icon then
      more_look = ui.Item { anchors = { fill = true },
        kit.icon(crumbs and "more_horiz" or "more_vert", 20, kit.ink and kit.ink("hi") or nil, { anchors = { center_in = true } }) }
    end
    if more_look then ui.reparent(more_look, more) end
  end

  root, t, ctl = control.make("Overflow", widget, full, { children = { stage }, props = props,
    builders = { more = true, menu = true },
    state = { cur_x = 0, cur_w = 0, cur_h = 0, current = 0 },
    on_rebuild = function(builders) dress_more(builders) end })

  for i = 1, n do
    nodes[i] = build(descs[i], i)
    local node = nodes[i]
    local holder = ui.Item { anchors = { vertical_center = true }, x = 0,
      width = function() return node.layout_width or 0 end, height = function() return node.layout_height or 0 end,
      visible = function() return not control.has(t.hidden, i) end, node }
    holders[i] = holder
    ui.reparent(holder, stage)
  end

  -- ------------------------------------------------------- the menu --
  local function close_menu() if menu and menu.is_open() then menu.close("closed") end end
  local function rows()
    local widgets = require("lib.kit.widgets")
    local opts = type(ctl.builders().menu) == "function" and ctl.builders().menu(spec) or {}
    local MW = spec.menu_width or opts.width or 220
    local column = { gap = 0 }
    for i = 1, n do
      local d = descs[i]
      column[#column + 1] = widgets.menu_item { id = id and (id .. "-menu-" .. i) or nil, label = label_of(d, i),
        icon = d.icon, width = MW - 8, height = opts.item_height or 36,
        visible = function() return control.has(t.hidden, i) end,
        on_clicked = function()
          if menu then menu.close("activated") end
          activate(i)
        end }
    end
    return ui.Column(column), MW, opts
  end
  local function open_menu()
    if not menu then
      local content, MW, opts = rows()
      menu = require("lib.kit.popup").make("menu", { id = id and (id .. "-menu") or nil, content = content,
        width = MW, padding = 4, placement = opts.placement or (crumbs and "bottom-start" or "bottom-end"),
        close_policy = "escape+outside",
        on_closed = function() if t.menu_open then ctl.send("close_menu") end end })
    end
    if not t.menu_open then ctl.send("toggle_menu") end
    if t.menu_open and not menu.is_open() then menu.open(more) end
  end
  local function toggle_menu()
    if menu and menu.is_open() then close_menu() else open_menu() end
  end
  more, more_t = control.make("Press", "area", { widget = "area", id = id and (id .. "-more") or nil,
    width = MORE_W[widget] or H, height = H, cursor = "pointer",
    accessible_name = crumbs and "Show the hidden path" or "More", accessible_role = "button",
    visible = function() return t.overflowing == true end,
    on_clicked = toggle_menu })
  dress_more(ctl.builders())
  local more_holder = ui.Item { anchors = { vertical_center = true }, x = 0,
    width = function() return more.layout_width or 0 end, height = H,
    visible = function() return t.overflowing == true end, more }
  ui.reparent(more_holder, stage)
  local popup = require("lib.kit.popup")
  if popup.tooltip and not crumbs then popup.tooltip(more, "More") end

  -- -------------------------------------------------------- measuring --
  for i = 1, n do
    local node = nodes[i]
    morf.effect(key .. ".measure." .. i, function()
      local w = node.layout_width or 0
      if w > 0 then ctl.send("measure", i, w) end
    end, { owner = root })
  end
  morf.effect(key .. ".more", function()
    local w = more.layout_width or 0
    if w > 0 then ctl.configure("more_width", w) end
  end, { owner = root })
  morf.effect(key .. ".resize", function()
    local w = root.layout_width or 0
    if w > 0 then ctl.send("resize", w) end
  end, { owner = root })
  -- Nothing left to list: the menu goes.
  morf.effect(key .. ".menu", function()
    if not t.menu_open and menu and menu.is_open() then menu.close("closed") end
  end, { owner = root })

  -- -------------------------------------------------------- placing --
  -- Moves are transforms: the node jumps to its new x and its offset
  -- settles from where it was drawn to nothing, on a spring-like curve.
  local SETTLE = { spline = { 0.2, 0.9, 0.3, 1.06, 0.6, 1.02, 0.8, 1.0, 0.9, 1.0, 1, 1 } }
  local reduced = kit.theme and kit.theme.reduced
  local last, running = {}, {}
  local function move(node, to, appearing)
    local from = last[node]
    last[node] = to
    if from == nil or reduced then node.x = to return end
    if from == to and not appearing then return end
    local offset = (node.translate_x or 0) + (from - to)
    if appearing then offset = math.min(12, math.max(-12, offset)) end
    node.x = to
    if running[node] then running[node]:stop() end
    local steps = { { node = node, property = "translate_x", from = offset, to = 0, duration = 420, easing = SETTLE } }
    if appearing then steps[2] = { node = node, property = "opacity", from = 0, to = 1, duration = 180, easing = "out_cubic" } end
    running[node] = morf.animation.play { { parallel = steps } }
  end
  local was_hidden = {}
  morf.effect(key .. ".place", function()
    local hidden = t.hidden
    local x, more_at = 0, nil
    local first_hidden
    local at = {}
    for i = 1, n do
      if control.has(hidden, i) then
        first_hidden = first_hidden or i
        if mode == "middle" and not more_at then
          more_at = x
          x = x + (more.layout_width or 0) + GAP
        end
      else
        at[i] = x
        x = x + (nodes[i].layout_width or 0) + GAP
      end
    end
    if first_hidden and not more_at then
      if mode == "start" then
        more_at = 0
        local shift = (more.layout_width or 0) + GAP
        for i in pairs(at) do at[i] = at[i] + shift end
      else
        more_at = x
      end
    end
    for i, ax in pairs(at) do move(holders[i], ax, was_hidden[i]) end
    for i = 1, n do was_hidden[i] = at[i] == nil end
    move(more_holder, more_at or x, false)
    -- The chosen item's box, for a skin's underline.
    if chooses then
      local c = current()
      t.current = c
      if at[c] then t.cur_x, t.cur_w, t.cur_h = at[c], nodes[c].layout_width or 0, nodes[c].layout_height or 0 end
    end
  end, { owner = root })
  local handle = { node = root, t = t, items = nodes }
  function handle.shown() return t.shown end
  function handle.hidden() return t.hidden end
  function handle.overflowing() return t.overflowing == true end
  handle.open_menu = open_menu
  handle.close_menu = close_menu
  function handle.menu_open() return menu ~= nil and menu.is_open() end
  function handle.current() return current() end
  handle.choose = choose
  return root, handle
end

return M
