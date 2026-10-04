-- Application shells (the Shell archetype): a window's header bar, sidebar,
-- content, inspector, bottom bar, banner and toasts, arranged for the
-- window's width and rearranged as it narrows.
--
--     local node, app = shell.make("window_layout", {
--       width = function() return win.width end, height = function() return win.height end,
--       header_bar = header, sidebar = list, content = page, inspector = details,
--       bottom_bar = tabs, banner = notice, toasts = stack,
--       sidebar_width = 280, inspector_width = 300, breakpoints = { 600, 900 },
--       on_breakpoint = function(layout) end,
--     })
--     app.toggle_sidebar()
--
-- Wide, the sidebar stands beside the content and the inspector beside it
-- on the other side; under the first breakpoint the sidebar becomes a
-- drawer over the content with a scrim (Escape or a press outside shuts
-- it) and the bottom bar takes its place; under the last the inspector
-- hides. F9 and Ctrl+B toggle the sidebar, F6 and Shift+F6 move between
-- the regions. The regions are landmarks: navigation, main and
-- complementary to a screen reader. The skin draws `background` and may
-- decorate `sidebar`'s edge; the parts are the configuration's.
local ui = require("morf.ui")
local morf = require("morf")
local control = require("lib.kit.control")

local M = {}

-- An ease that overshoots a hair and settles: a drawer's spring.
local SPRINGY = { spline = { 0.2, 0.9, 0.3, 1.06, 0.6, 1.03, 0.78, 1.01, 0.9, 1.0, 1, 1 } }

local function get(v) if type(v) == "function" then return v() end return v end

function M.make(widget, spec)
  spec = spec or {}
  local W, H = spec.width, spec.height
  local sidebar_w, inspector_w = spec.sidebar_width or 280, spec.inspector_width or 300
  local header, sidebar, content = spec.header_bar, spec.sidebar, spec.content or ui.Item {}
  local inspector, bottom, banner, toasts = spec.inspector, spec.bottom_bar, spec.banner, spec.toasts
  -- A toolbar view's bars: under the header and over the bottom, always
  -- shown (the bottom bar only stands in for a folded sidebar).
  local bar_top, bar_bottom = spec.toolbar_top, spec.toolbar_bottom
  local t, ctl
  -- The control comes first and the regions after, so their bindings read
  -- its state from the start (a binding that ran while `t` was nil would
  -- read nothing to follow) and nothing slides into place as it opens.
  local function live() return t end
  local motion = { duration = 260, easing = "out_cubic" }
  local function header_h() return header and (spec.header_height or header.layout_height or 48) or 0 end
  local function banner_h() return banner and banner.visible ~= false and (banner.layout_height or 0) or 0 end
  local function bottom_h() return (bottom and live() and t.bottom_bar) and (spec.bottom_height or bottom.layout_height or 56) or 0 end
  local function bar_top_h() return bar_top and (spec.toolbar_top_height or bar_top.layout_height or 46) or 0 end
  local function bar_bottom_h() return bar_bottom and (spec.toolbar_bottom_height or bar_bottom.layout_height or 46) or 0 end
  local function top() return header_h() + bar_top_h() + banner_h() end
  local function body_h() return math.max(0, (get(H) or 0) - top() - bottom_h() - bar_bottom_h()) end
  local function beside() return sidebar and live() and not t.collapsed and t.sidebar_open end
  local function inspecting() return inspector and live() and t.inspector_shown end
  local regions = spec.regions or {}
  if not spec.regions then
    if sidebar then regions[#regions + 1] = "sidebar" end
    regions[#regions + 1] = "content"
    if inspector then regions[#regions + 1] = "inspector" end
  end
  -- (Filled once the regions are made, below.)
  local by_region = {}
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget, full.regions, full.sidebar = widget, regions, sidebar ~= nil
  full.width = nil
  local given_region = spec.on_region
  full.on_region = function(name)
    -- Focus into the region: onto it, then on to its first control.
    local node = by_region[name]
    if node then morf.focus.set(node, true) morf.focus.next() end
    if given_region then given_region(name) end
  end
  -- The Shell's own signals are not ones lib.kit.control knows, and a
  -- handler it does not know goes to the node as a property (an unknown
  -- one, for a MouseArea): given as callable tables, they reach only the
  -- signals.
  for _, name in ipairs { "on_region", "on_breakpoint", "on_collapsed", "on_sidebar_toggled" } do
    local handler = full[name]
    if type(handler) == "function" then
      full[name] = setmetatable({}, { __call = function(_, ...) return handler(...) end })
    end
  end
  local props = { width = W, height = H, focus_policy = "none", clip = true }
  for _, k in ipairs { "id", "x", "y", "anchors", "visible", "z" } do props[k] = spec[k] end
  local function key(name, modifiers)
    return function() return ctl.send("key", name, modifiers or "", "", 0) end
  end
  props.shortcuts = {
    ["F9"] = key("F9"), ["ctrl+b"] = key("b", "ctrl"), ["F6"] = key("F6"), ["shift+F6"] = key("F6", "shift"),
    -- Escape shuts an open drawer and otherwise goes on.
    ["Escape"] = function()
      if not (t and t.collapsed and t.sidebar_open) then return false end
      ctl.send("dismiss")
    end,
  }
  local root
  -- The skin's grounds for the regions (its `sidebar`, `inspector`,
  -- `header_bar`, ... slots) go under each region's own content, so they
  -- move with it: a drawer's ground slides with the drawer. Placed once
  -- the regions are made, and again on a theme switch.
  local holders = {}
  local function place_grounds()
    if not ctl then return end
    local slots = ctl.slots()
    for _, name in ipairs { "content", "sidebar", "inspector", "header_bar", "toolbar_top", "toolbar_bottom",
      "bottom_bar", "banner" } do
      local node, holder = slots[name], holders[name]
      if node and holder then
        if (node.z or 0) == 0 then node.z = -1 end
        ui.reparent(node, holder)
      elseif node and next(holders) then
        -- A region this window does not have: its ground draws nothing.
        node.visible = false
      end
    end
  end
  root, t, ctl = control.make("Shell", widget, full, { props = props, on_rebuild = function() place_grounds() end })
  -- The window's width, as it is laid out, is what the breakpoints read --
  -- told first as given, so the first layout is already the right one.
  -- (Against the width last sent, not `t.width`: lib.kit.control keeps
  -- that one as the node's laid-out size for skins, so it already equals
  -- the new width and the archetype would never hear of it.)
  local sent = tonumber(get(W))
  if sent and sent > 0 then ctl.send("resize", sent) end
  -- Right to left the regions swap sides: a place `x` wide `w` from the
  -- start is that far from the end.
  local function flip(x, w)
    if root.effective_direction ~= "rtl" then return x end
    return (get(W) or 0) - x - w
  end
  -- The regions, as landmarks.
  local function region(node, role, name, props)
    local holder = ui.Item(props)
    holder.accessible_role, holder.accessible_name = role, name
    holder.focus_scope = true
    ui.reparent(node, holder)
    return holder
  end
  local function main_w()
    return math.max(0, (get(W) or 0) - (beside() and sidebar_w or 0) - (inspecting() and inspector_w or 0))
  end
  -- (The skin's `content` slot is the page's ground: placed in the main
  -- region, not over the window.)
  local function main_x() return flip(beside() and sidebar_w or 0, main_w()) end
  local function side_x() return flip((live() and t.sidebar_open) and 0 or -sidebar_w, sidebar_w) end
  local main = region(content, "main", spec.content_name or "Content", {
    x = main_x, y = top,
    width = main_w, height = body_h, clip = true })
  local parts = { main }
  local side, scrim
  if sidebar then
    -- Collapsed, the sidebar slides over the content from its edge.
    side = region(sidebar, "navigation", spec.sidebar_name or "Sidebar", {
      y = top, width = sidebar_w, height = body_h, z = 20, clip = true,
      x = side_x })
    holders.sidebar = side
    -- A ground under it, so as a drawer it covers the page it slides over:
    -- the configuration's colour, else the kit's sidebar tone.
    local ok, kit = pcall(require, "kit")
    local P = ok and type(kit) == "table" and kit.theme and kit.theme.P
    local ground = spec.sidebar_color or (P and function() return P().sidebar end)
    if ground then ui.reparent(ui.Rect { anchors = { fill = true }, z = -1, color = ground }, side) end
    -- The scrim fades in and out; only an open drawer's takes the press.
    local function drawer() return live() ~= nil and t.collapsed and t.sidebar_open end
    parts[#parts + 1] = ui.Rect { y = top, width = W, height = body_h, z = 19, color = "#00000052",
      opacity = function() return drawer() and 1 or 0 end,
      behavior = { opacity = { duration = 220, easing = "out_cubic" } } }
    scrim = ui.MouseArea { y = top, width = W, height = body_h, z = 19,
      visible = drawer,
      on_clicked = function() ctl.send("dismiss") end }
    parts[#parts + 1] = scrim
    parts[#parts + 1] = side
  end
  if inspector then
    holders.inspector = region(inspector, "complementary", spec.inspector_name or "Details", {
      x = function() return flip((get(W) or 0) - inspector_w, inspector_w) end, y = top, width = inspector_w,
      height = body_h,
      visible = function() return inspecting() and true or false end, clip = true })
    parts[#parts + 1] = holders.inspector
  end
  if header then
    holders.header_bar = region(header, "banner", spec.title or "Header", { width = W, height = header_h })
    parts[#parts + 1] = holders.header_bar
  end
  if bar_top then
    holders.toolbar_top = region(bar_top, "toolbar", spec.toolbar_top_name or "Toolbar",
      { y = header_h, width = W, height = bar_top_h })
    parts[#parts + 1] = holders.toolbar_top
  end
  if banner then
    holders.banner = ui.Item { y = function() return header_h() + bar_top_h() end, width = W, banner }
    parts[#parts + 1] = holders.banner
  end
  if bar_bottom then
    holders.toolbar_bottom = region(bar_bottom, "toolbar", spec.toolbar_bottom_name or "Actions", {
      y = function() return (get(H) or 0) - bottom_h() - bar_bottom_h() end, width = W, height = bar_bottom_h })
    parts[#parts + 1] = holders.toolbar_bottom
  end
  if bottom then
    holders.bottom_bar = region(bottom, "navigation", spec.bottom_name or "Sections", {
      y = function() return (get(H) or 0) - bottom_h() end, width = W, height = bottom_h,
      visible = function() return live() ~= nil and t.bottom_bar end })
    parts[#parts + 1] = holders.bottom_bar
  end
  if toasts then parts[#parts + 1] = ui.Item { anchors = { fill = true }, z = 40, toasts } end
  by_region.sidebar, by_region.content = side, main
  holders.content = main
  -- (Made with its parts, so their stacking -- the drawer over its scrim
  -- over the content -- is the one they were given.)
  local frame = { width = W, height = H }
  for i, part in ipairs(parts) do frame[i] = part end
  ui.reparent(ui.Item(frame), root)
  place_grounds()
  -- The regions slide as the sidebar comes and goes: each stands where it
  -- now belongs at once and is drawn from where it was (`translate_x`
  -- easing back to nothing), the drawer with a hair of overshoot. Armed
  -- once they stand where they open: a shell made while the screen is
  -- busy (inside an effect, say) can see its bindings' first values land
  -- late, and would slide in from nothing. (A behavior cannot be given
  -- after a node is made, so the motion is played.)
  local armed, last_side, last_main = false, nil, nil
  morf.timer(300, function() armed = true end, false)
  local function slide(node, offset, easing)
    if math.abs(offset) < 0.5 then return end
    pcall(morf.animation.play, { { node = node, property = "translate_x", from = offset, to = 0,
      duration = motion.duration + 160, easing = easing } })
  end
  -- (From the start edge, so a window resized right to left, where the
  -- regions' x follows the width, slides nothing.)
  morf.effect("kit.shell.slide." .. ctl.id, function()
    local sx = (live() and t.sidebar_open) and 0 or -sidebar_w
    local mx = beside() and sidebar_w or 0
    local dir = root.effective_direction == "rtl" and -1 or 1
    if armed and last_main then
      if side then slide(side, (last_side - sx) * dir, SPRINGY) end
      slide(main, (last_main - mx) * dir, motion.easing)
    end
    last_side, last_main = sx, mx
  end, { owner = root })
  morf.effect("kit.shell.width." .. ctl.id, function()
    local w = root.layout_width or 0
    if w > 0 and w ~= sent then sent = w ctl.send("resize", w) end
  end, { owner = root })
  local handle = { node = root, t = t }
  function handle.toggle_sidebar() ctl.send("toggle_sidebar") end
  function handle.dismiss() ctl.send("dismiss") end
  return root, handle
end

return M
