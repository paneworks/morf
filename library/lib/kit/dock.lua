-- Docks (the Dock archetype): panels in a tree of splits and tab stacks
-- that the user rearranges -- a tab dragged onto a stack's middle joins
-- it, onto an edge splits beside it, out of the dock floats -- and
-- maximises, closes, and walks by keys.
--
--     local node, dock = lib.kit.dock.make("dock_area", {
--       width = 1200, height = 800,
--       panels = {
--         files = { title = "Files", icon = "folder", content = file_tree },
--         editor = { title = "main.lua", content = editor, closable = false },
--         log = { title = "Log", content = function() return log_view() end },  -- built when first shown
--       },
--       layout = { orientation = "horizontal", ratios = { 0.2, 0.8 }, children = {
--         { panels = { "files" } },
--         { orientation = "vertical", ratios = { 0.7, 0.3 }, children = {
--           { panels = { "editor" } }, { panels = { "log" } } } } } },
--       on_layout_changed = function(tree, floating) save(tree, floating) end,
--       on_closed = function(panel) end,
--     })
--     dock.activate("log")  dock.close("log")  dock.maximize("editor")  dock.float("files", x, y, w, h)
--     dock.dock("files", stack_id, "left")  dock.layout()
--
-- A panel's content is made once and kept: a stack shows its current and
-- parks the rest, so a panel moved, hidden or floated keeps its state.
-- The skin draws `tab` (a builder: one tab, told `s.title`, `s.icon`,
-- `s.current()`, `s.focused()`, `s.hovered()`, `s.closable`, `s.close()`),
-- `stack` (a builder: the frame round a stack, told `s.focused()`),
-- `divider` (a builder: the handle between two parts, told
-- `s.orientation`, `s.hovered()`, `s.dragging()`), `floating` (a builder:
-- a floating panel's frame, told `s.title`; its top `tab_height` is the
-- bar it is moved by) and `drop_indicator` (where a dragged tab would
-- land).
local ui = require("morf.ui")
local morf = require("morf")
local control = require("lib.kit.control")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end

function M.make(widget, spec)
  spec = spec or {}
  local W, H = spec.width, spec.height
  local TAB = spec.tab_height or 34
  local DIV = spec.divider or 6
  local t, ctl, root, send
  local tree = morf.signal("kit.dock.tree." .. tostring(spec), nil)
  local floating = morf.signal("kit.dock.floating." .. tostring(spec), {})
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.layout, full.floating, full.panels = nil, nil, nil
  -- The archetype's tree, as it says it changed.
  local given_layout = spec.on_layout_changed
  full.on_layout_changed = setmetatable({}, { __call = function(_, new_tree, new_floating)
    tree:set(new_tree)
    floating:set(new_floating or {})
    if given_layout then given_layout(new_tree, new_floating) end
  end })
  for _, name in ipairs { "on_activated", "on_closed", "on_maximized", "on_focus_changed" } do
    local handler = full[name]
    if type(handler) == "function" then
      full[name] = setmetatable({}, { __call = function(_, ...) return handler(...) end })
    end
  end
  local fixed = {}
  for id, panel in pairs(spec.panels or {}) do if panel.closable == false then fixed[#fixed + 1] = id end end
  full.fixed = fixed
  local props = { width = W, height = H, clip = true, focus_policy = "none" }
  local settle
  root, t, ctl = control.make("Dock", widget, full, {
    props = props,
    builders = { tab = true, stack = true, divider = true, floating = true },
    on_rebuild = function() if settle then settle() end end,
  })
  send = ctl.send
  -- The keys, while focus is anywhere in the dock.
  local function shortcut(name, modifiers)
    return function() send("key", name, modifiers or "", "", 0) end
  end
  root.shortcuts = {
    ["ctrl+Page_Down"] = shortcut("Page_Down", "ctrl"), ["ctrl+Page_Up"] = shortcut("Page_Up", "ctrl"),
    ["ctrl+w"] = shortcut("w", "ctrl"), ["ctrl+shift+m"] = shortcut("m", "ctrl+shift"),
    ["F6"] = shortcut("F6"), ["shift+F6"] = shortcut("F6", "shift"),
    ["Escape"] = function()
      if t.dragging == "" and t.maximized == "" then return false end
      send("key", "Escape", "", "", 0)
    end,
  }

  -- Panels: their content, made once, parked while not shown.
  local parking = ui.Item { visible = false, width = 0, height = 0 }
  ui.reparent(parking, root)
  local made = {}
  -- (Given as nodes, they are parked at once: a node outside any surface
  -- is drawn nowhere and warned of.)
  for id, panel in pairs(spec.panels or {}) do
    if panel.content ~= nil and type(panel.content) ~= "function" then
      made[id] = panel.content
      ui.reparent(panel.content, parking)
    end
  end
  local function content_of(id)
    if made[id] ~= nil then return made[id] end
    local panel = (spec.panels or {})[id] or {}
    local node = panel.content
    if type(node) == "function" then node = node() end
    made[id] = node or false
    return made[id]
  end
  local function info(id)
    local panel = (spec.panels or {})[id] or {}
    return panel.title or id, panel.icon
  end

  -- Each stack's box, for a dragged tab to find where it is.
  local stacks = {}
  local layer
  local function box_of(node)
    return (node.layout_x or 0) - (root.layout_x or 0), (node.layout_y or 0) - (root.layout_y or 0),
      node.layout_width or 0, node.layout_height or 0
  end
  local function over(sx, sy)
    local x, y = sx - (root.layout_x or 0), sy - (root.layout_y or 0)
    for id, holder in pairs(stacks) do
      local bx, by, bw, bh = box_of(holder)
      if x >= bx and x <= bx + bw and y >= by and y <= by + bh then return id, x - bx, y - by, bw, bh end
    end
  end
  local function inside(sx, sy)
    local x, y = sx - (root.layout_x or 0), sy - (root.layout_y or 0)
    return x >= 0 and y >= 0 and x <= (root.layout_width or 0) and y <= (root.layout_height or 0)
  end

  -- (A tab is made with the tree it belongs to, which is made anew as
  -- its current changes, so whether it is current is known as it is made.)
  local function tab_node(panel, stack_id, is_current, x, w)
    local title, icon = info(panel)
    local area
    local s = {
      panel = panel, title = title, icon = icon,
      current = function() return is_current end,
      focused = function() return t.focused == stack_id end,
      hovered = function() return area ~= nil and area.hovered end,
      dragging = function() return t.dragging == panel end,
    }
    s.closable = ((spec.panels or {})[panel] or {}).closable ~= false
    function s.close() send("close", panel) end
    local build = (ctl.builders() or {}).tab
    local look = build and build(s)
    -- (Focusable, so the dock's keys work once a tab is pressed.)
    area = ui.MouseArea { x = x, y = 0, width = w, height = TAB, accepted_buttons = { "left", "middle" },
      focus_policy = "strong",
      accessible_role = "tab", accessible_name = title, cursor = "pointer",
      on_pressed = function() send("focus_stack", stack_id) end,
      on_clicked = function(_, _, _, _, button)
        if button == "middle" then if s.closable then send("close", panel) end else send("activate", panel) end
      end,
      on_drag_started = function() send("drag_start", panel) end,
      on_dragged = function(sx, sy)
        if t.dragging ~= panel then return end
        local id, lx, ly, bw, bh = over(sx, sy)
        if id then send("drag_over", id, lx, ly, bw, bh)
        elseif not inside(sx, sy) or spec.float ~= false then send("drag_outside") end
      end,
      on_released = function(sx, sy)
        if t.dragging ~= panel then return end
        local x, y = sx - (root.layout_x or 0), sy - (root.layout_y or 0)
        send("drop", x - 40, y - TAB / 2, spec.float_width or 360, spec.float_height or 260)
      end }
    if look then ui.reparent(look, area) end
    return area
  end

  -- One stack: its tabs over its current panel.
  local function stack_node(node, bx, by, bw, bh)
    local holder = ui.Item { x = bx, y = by, width = bw, height = bh, clip = true }
    holder.accessible_role, holder.accessible_name = "group", node.id
    stacks[node.id] = holder
    local build = (ctl.builders() or {}).stack
    local frame = build and build({ id = node.id, focused = function() return t.focused == node.id end })
    if frame then frame.z = -1 ui.reparent(frame, holder) end
    local strip = ui.Item { width = bw, height = TAB }
    strip.accessible_role = "tab_list"
    local panels = node.panels or {}
    local tab_w = function() return math.min(spec.tab_width or 180, math.max(64, get(bw) / math.max(1, #panels))) end
    for i, panel in ipairs(panels) do
      ui.reparent(tab_node(panel, node.id, panel == node.current, function() return (i - 1) * tab_w() end, tab_w), strip)
    end
    ui.reparent(strip, holder)
    local body = ui.Item { y = TAB, width = bw, height = function() return math.max(0, get(bh) - TAB) end, clip = true }
    body.accessible_role = "tab_panel"
    local shown = content_of(node.current)
    if shown then ui.reparent(shown, body) end
    ui.reparent(body, holder)
    return holder
  end

  -- A split: its parts at their ratios, a divider between each two.
  local build_node
  local function split_node(node, bx, by, bw, bh)
    local holder = ui.Item { x = bx, y = by, width = bw, height = bh }
    local vertical = node.orientation == "vertical"
    local ratios = node.ratios or {}
    local n = #(node.children or {})
    local function length() return vertical and get(bh) or get(bw) end
    local function start(i)
      local sum = 0
      for k = 1, i - 1 do sum = sum + (ratios[k] or 0) end
      return sum
    end
    for i, child in ipairs(node.children or {}) do
      local function from() return math.floor(start(i) * length()) + (i > 1 and DIV / 2 or 0) end
      local function size()
        local whole = (ratios[i] or 0) * length()
        return math.max(0, math.floor(whole) - ((i > 1 and DIV / 2 or 0) + (i < n and DIV / 2 or 0)))
      end
      local cx = vertical and 0 or from
      local cy = vertical and from or 0
      local cw = vertical and bw or size
      local ch = vertical and size or bh
      ui.reparent(build_node(child, cx, cy, cw, ch), holder)
      if i < n then
        local dragging = false
        local area
        local function at() return math.floor(start(i + 1) * length()) - DIV / 2 end
        area = ui.MouseArea { x = vertical and 0 or at, y = vertical and at or 0,
          width = vertical and bw or DIV, height = vertical and DIV or bh, z = 2,
          cursor = vertical and "row_resize" or "col_resize", accessible_role = "splitter",
          on_pressed = function() dragging = true end,
          on_released = function() dragging = false end,
          on_dragged = function(sx, sy)
            local hx, hy = box_of(holder)
            local local_pos = vertical and (sy - (root.layout_y or 0) - hy) or (sx - (root.layout_x or 0) - hx)
            send("resize", node.id, i - 1, local_pos / math.max(1, length()))
          end }
        local build = (ctl.builders() or {}).divider
        local look = build and build({ orientation = vertical and "vertical" or "horizontal",
          hovered = function() return area.hovered end, dragging = function() return area.pressed end })
        if look then ui.reparent(look, area) end
        ui.reparent(area, holder)
      end
    end
    return holder
  end

  function build_node(node, bx, by, bw, bh)
    if node.kind == "split" then return split_node(node, bx, by, bw, bh) end
    return stack_node(node, bx, by, bw, bh)
  end

  -- The whole tree, rebuilt when it changes; the panels moved, not made.
  local function find_stack(node, panel)
    if not node then return nil end
    if node.kind == "stack" then
      for _, p in ipairs(node.panels or {}) do if p == panel then return node end end
      return nil
    end
    for _, c in ipairs(node.children or {}) do local r = find_stack(c, panel) if r then return r end end
  end
  morf.effect("kit.dock.layout." .. ctl.id, function()
    local tr = tree:get()
    local maximized = t.maximized
    for _, node in pairs(made) do if node then ui.reparent(node, parking) end end
    if layer then ui.destroy(layer, true) end
    stacks = {}
    layer = ui.Item { width = W, height = H }
    if tr then
      if maximized ~= "" then
        local home = find_stack(tr, maximized)
        tr = { kind = "stack", id = home and home.id or "maximized", panels = { maximized }, current = maximized }
      end
      ui.reparent(build_node(tr, 0, 0, W, H), layer)
    end
    -- Floating panels, over the dock.
    for _, f in ipairs(floating:get() or {}) do
      local title = info(f.panel)
      local frame = ui.Item { x = f.x, y = f.y, width = f.w, height = f.h, z = 30 }
      local build = (ctl.builders() or {}).floating
      local look = build and build({ title = title, panel = f.panel })
      if look then look.z = -1 ui.reparent(look, frame) end
      local origin
      ui.reparent(ui.MouseArea { width = f.w, height = TAB, cursor = "move", accessible_role = "grip",
        accessible_name = title,
        on_pressed = function(sx, sy) origin = { sx, sy, f.x, f.y } end,
        on_dragged = function(sx, sy)
          if not origin then return end
          frame.x, frame.y = origin[3] + sx - origin[1], origin[4] + sy - origin[2]
        end,
        on_released = function()
          if origin then send("move_floating", f.panel, frame.x, frame.y, f.w, f.h) end
          origin = nil
        end,
        on_double_clicked = function() send("dock", f.panel, t.focused, "center") end }, frame)
      local body = ui.Item { y = TAB, width = f.w, height = math.max(0, f.h - TAB), clip = true }
      local shown = content_of(f.panel)
      if shown then ui.reparent(shown, body) end
      ui.reparent(body, frame)
      ui.reparent(frame, layer)
    end
    ui.reparent(layer, root)
  end, { owner = root })

  -- Where a dragged tab would land, over the stack it is on.
  local indicator = ui.Item { z = 40,
    visible = function() return t.dragging ~= "" and t.drop_target ~= "" and t.drop_target ~= "float" end }
  local function zone_box()
    local holder = stacks[t.drop_target]
    if not holder then return 0, 0, 0, 0 end
    local x, y, w, h = box_of(holder)
    local z = t.drop_zone
    if z == "left" then return x, y, w / 2, h
    elseif z == "right" then return x + w / 2, y, w / 2, h
    elseif z == "top" then return x, y, w, h / 2
    elseif z == "bottom" then return x, y + h / 2, w, h / 2 end
    return x, y, w, h
  end
  indicator.x = function() local x = zone_box() return x end
  indicator.y = function() local _, y = zone_box() return y end
  indicator.width = function() local _, _, w = zone_box() return w end
  indicator.height = function() local _, _, _, h = zone_box() return h end
  ui.reparent(indicator, root)
  function settle()
    local s = ctl and ctl.slots and ctl.slots() or {}
    if s.drop_indicator then
      s.drop_indicator.anchors = { fill = true }
      ui.reparent(s.drop_indicator, indicator)
    end
  end
  settle()

  ctl.configure("layout", spec.layout or { panels = {} })
  if spec.floating then ctl.configure("floating", spec.floating) end

  local dock = { node = root, t = t }
  function dock.activate(panel) send("activate", panel) end
  function dock.close(panel) send("close", panel) end
  function dock.maximize(panel) send("maximize", panel) end
  function dock.float(panel, x, y, w, h) send("float", panel, x or 40, y or 40, w or 360, h or 260) end
  function dock.dock(panel, stack, zone) send("dock", panel, stack or t.focused, zone or "center") end
  function dock.layout() return tree:get(), floating:get() end
  function dock.set_layout(layout, floats)
    ctl.configure("layout", layout)
    if floats then ctl.configure("floating", floats) end
  end
  return root, dock
end

return M
