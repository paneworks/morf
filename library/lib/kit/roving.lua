-- Rovings (the Roving archetype): one Tab stop for a group of controls the
-- arrows move between -- a toolbar's buttons, a menu bar, a linked button
-- group, a row of chips, a bar of icons (WAI-ARIA toolbar and menubar).
--
--     local node, group = roving.make("toolbar_group", {
--       id = "format", accessible_name = "Format",
--       items = {
--         { icon = "format_bold", tooltip = "Bold", checked = false, on_toggled = set_bold },
--         { icon = "format_italic", tooltip = "Italic", on_clicked = italic },
--         { separator = true },
--         some_kit_control,                          -- a node as it is
--         function(i) return build(i) end,           -- or a builder
--       },
--     })
--     group.focus() group.current() group.open(2) group.close()
--
-- A member is a node (a kit control), a builder, or a press described by
-- `icon`, `label`, `tooltip`, `on_clicked`, `checked`/`on_toggled`,
-- `enabled` (a value or a binding) and `id` -- drawn as the widget's
-- member press (a toolbar's flat button, a button group's segment, a chip,
-- an icon). `{ separator = true }` puts the skin's separator between two
-- members. The current member alone is a Tab stop ("strong"), the others
-- take a click ("click"), so Tab stops once and enters where it left; the
-- arrows (Left/Right, Up/Down, both in a grid), Home and End move it,
-- passing over disabled members.
--
-- A menu bar's members are menus: `{ label = "File", items = { ... } }`,
-- items as a popup menu's (lib.kit.popup). Down, Return and Space open the
-- current one's menu; while one is open Left and Right open the next;
-- Escape closes it and focus goes back to its title. F10, or Alt tapped
-- alone, focuses the bar; Alt tapped again goes back to what had focus.
--
-- Other fields: `orientation` ("horizontal", "vertical", "grid"),
-- `columns`, `wrap`, `current`, `gap`, `padding`, `item_height`, `x`,
-- `y`, `anchors`, `on_current_changed(i)`. The skin draws `background`,
-- `indicator` (riding the current member's box: `t.cur_x`, `t.cur_y`,
-- `t.cur_w`, `t.cur_h`; `t.within` while focus is inside, `t.keyboard`
-- when a keyboard put it there, `t.open`) and `separator(vertical)`, a
-- builder.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- The press each widget's described members are drawn as.
local MEMBER = { toolbar_group = "flat", menubar = "flat", button_group = "segment", chip_row = "chip_filter",
  icon_bar = "icon" }
-- Layout each widget shares across themes: gap between members, padding
-- round them, a member's height.
local LAYOUT = {
  toolbar_group = { gap = 2, padding = 4, height = 36 },
  menubar = { gap = 2, padding = 2, height = 32 },
  button_group = { gap = 0, padding = 0, height = 36 },
  chip_row = { gap = 8, padding = 2, height = 32 },
  icon_bar = { gap = 4, padding = 4, height = 40 },
}

local function get(v) if type(v) == "function" then return v() end return v end
local function has_alt(modifiers) return tostring(modifiers or ""):find("alt", 1, true) ~= nil end

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
  local key = "kit.roving." .. widget .. "." .. serial
  local layout = LAYOUT[widget] or LAYOUT.toolbar_group
  local menubar = widget == "menubar" or spec.menubar == true
  local orientation = spec.orientation or "horizontal"
  local vertical = orientation == "vertical"
  local gap = spec.gap or layout.gap
  local pad = spec.padding or layout.padding
  local H = spec.item_height or layout.height
  local id = spec.id

  local root, t, ctl
  local members, descs, menus = {}, {}, {}
  -- Where each separator goes: before member n.
  local separators = {}
  local switching = false

  local function send(...) if ctl then return ctl.send(...) end end
  local function focus_member(i, keyboard)
    local m = members[i]
    if m then morf.focus.set(m, keyboard ~= false) end
  end

  -- ------------------------------------------------------------ menus --
  local function close_menus(except)
    for i, menu in pairs(menus) do
      if i ~= except and menu.is_open() then menu.close("closed") end
    end
  end
  local function key_of_menu(_, _, modifiers, _, name)
    -- Left and Right in an open menu go to the bar: the next menu opens.
    if (name == "Left" or name == "Right") and not has_alt(modifiers) then
      local effects = send("key", name, modifiers or "", "", 0)
      return effects ~= nil and effects.handled == true
    end
    return false
  end
  local function menu_of(i)
    if menus[i] then return menus[i] end
    local desc = descs[i]
    if not (desc and desc.items) then return nil end
    menus[i] = require("lib.kit.popup").make("menu", { id = desc.id and (desc.id .. "-menu") or (id and (id .. "-menu-" .. i)),
      items = desc.items, width = desc.menu_width or spec.menu_width or 220, padding = 4,
      placement = vertical and "right-start" or "bottom-start", close_policy = "escape+outside",
      on_key_pressed = key_of_menu,
      on_closed = function(reason)
        -- (A menu the bar has moved on from says so late: not the open one.)
        if switching or t.open ~= i then return end
        send("menu_closed")
        -- Escape: back to the title; a press elsewhere leaves focus there.
        if reason == "escape" or reason == "activated" then focus_member(t.current, true) end
      end })
    return menus[i]
  end
  local function open_menu(i)
    local menu = menu_of(i)
    local leaving = false
    for j, other in pairs(menus) do if j ~= i and other.is_open() then leaving = true end end
    switching = true
    close_menus(i)
    switching = false
    if not (menu and not menu.is_open()) then return end
    if not leaving then menu.open(members[i]) return end
    -- The menu left goes out of the layer first (its exit a ghost under
    -- the new one), so Escape reaches the menu now open.
    morf.timer(1, function() if t.open == i and not menu.is_open() then menu.open(members[i]) end end, false)
  end

  -- ---------------------------------------------------------- members --
  local function described(desc, i)
    local press = desc.widget or spec.member_widget or MEMBER[widget] or "flat"
    local checkable = desc.checked ~= nil
    local s = {
      widget = press, id = desc.id or (id and (id .. "-item-" .. i)) or nil,
      label = desc.label, icon = desc.icon, tooltip = desc.tooltip,
      icon_off = desc.icon, icon_on = desc.icon_on or desc.icon, size = desc.size or 20,
      -- (An icon alone is square.)
      width = desc.width or ((desc.icon and not desc.label) and (desc.height or H) or nil), height = desc.height or H,
      enabled = desc.enabled, checkable = checkable, checked = desc.checked,
      on_toggled = desc.on_toggled, accessible_name = desc.accessible_name or desc.label or desc.tooltip or desc.icon,
    }
    if press == "segment" then s.position = desc.position end
    if menubar and desc.items then
      -- A title opens its menu the way Down does, so the bar knows.
      s.on_clicked = function()
        local was = t.open
        if was > 0 then switching = true close_menus() switching = false send("menu_closed") end
        if was == i then return end
        send("focus_in", i)
        send("key", "Return", "", "", 0)
      end
    else
      s.on_clicked = desc.on_clicked
    end
    local probe
    if not s.width and s.label and press ~= "icon" then
      s.width, probe = label_width(s.label, s.icon, press:match("^chip") and 34 or 28)
    end
    local node = control.make("Press", press, s, probe and { children = { probe } } or nil)
    if desc.tooltip and not desc.label then
      local popup = require("lib.kit.popup")
      if popup.tooltip then popup.tooltip(node, desc.tooltip) end
    end
    return node
  end

  local entries = spec.items or {}
  local nmembers = 0
  for _, entry in ipairs(entries) do
    if type(entry) == "table" and (entry.separator or entry.kind == "separator") then
      separators[nmembers + 1] = true
    else
      nmembers = nmembers + 1
      local node
      if type(entry) == "userdata" then node = entry
      elseif type(entry) == "function" then node = entry(nmembers)
      elseif type(entry) == "table" then descs[nmembers] = entry end
      members[nmembers] = node or false
    end
  end
  -- A linked group's ends: a segment knows where it sits.
  if (spec.member_widget or MEMBER[widget]) == "segment" then
    for i = 1, nmembers do
      if descs[i] and descs[i].position == nil then
        descs[i].position = nmembers == 1 and "only" or (i == 1 and "first" or (i == nmembers and "last" or "middle"))
      end
    end
  end
  for i = 1, nmembers do
    if not members[i] then members[i] = described(descs[i], i) end
  end

  -- What the arrows pass over: the configuration's, and members disabled.
  local given_disabled = spec.disabled
  local function disabled()
    local out = {}
    for _, i in ipairs(get(given_disabled) or {}) do out[#out + 1] = i end
    for i = 1, nmembers do
      local d = descs[i]
      if d and d.enabled ~= nil and get(d.enabled) == false then out[#out + 1] = i end
    end
    return out
  end

  local box
  if orientation == "grid" then box = ui.Grid { columns = spec.columns or 3, gap = gap }
  elseif vertical then box = ui.Column { gap = gap, align = "stretch" }
  else box = ui.Row { gap = gap, align = "center" } end
  box.x, box.y = pad, pad

  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.items = nil
  full.count = nmembers
  full.menubar = menubar
  full.disabled = disabled
  full.orientation = orientation
  local given_changed = spec.on_current_changed
  full.on_current_changed = function(i)
    -- (A menu bar with a menu open moves the menu, not focus: the menu
    -- that opens takes it.)
    if not (t and t.open > 0) then focus_member(i, true) end
    if given_changed then given_changed(i) end
  end
  full.on_open = function(i) open_menu(i) if spec.on_open then spec.on_open(i) end end
  full.on_close = function() switching = true close_menus() switching = false if spec.on_close then spec.on_close() end end

  local props = {
    focus_policy = "none",
    width = spec.width or function() return (box.layout_width or 0) + 2 * pad end,
    height = spec.height or function() return (box.layout_height or 0) + 2 * pad end,
    accessible_role = spec.accessible_role or (menubar and "menu_bar" or "toolbar"),
  }
  if menubar then
    -- (Alt tapped alone: an engine tap shortcut -- Alt held with a letter
    -- is still that letter's chord.)
    local before
    local function reach()
      if t.within then
        -- Back to where focus was before the bar took it.
        if before then pcall(morf.focus.set, before, true) else pcall(morf.focus.clear, members[t.current]) end
        before = nil
        return true
      end
      before = morf.focus.get and morf.focus.get() or nil
      focus_member(t.current, true)
      return true
    end
    props.shortcuts = { scope = "surface", F10 = reach, alt = reach }
  end
  root, t, ctl = control.make("Roving", widget, full, { children = { box }, props = props,
    builders = { separator = true },
    state = { cur_x = 0, cur_y = 0, cur_w = 0, cur_h = 0, within = false, keyboard = false } })

  -- The members and the separators the skin gives, in order.
  local function fill()
    local sep = ctl.builders().separator
    for i = 1, nmembers do
      if separators[i] and i > 1 then
        local node = type(sep) == "function" and sep(not vertical) or nil
        ui.reparent(node or ui.Item { width = vertical and 1 or 9, height = vertical and 9 or 1 }, box)
      end
      ui.reparent(members[i], box)
    end
  end
  fill()

  -- Tab stops at the current member only.
  for i = 1, nmembers do
    local m = members[i]
    m.focus_policy = function() return t.current == i and "strong" or "click" end
    -- A member taking focus, by a click or Tab, becomes the current one.
    morf.effect(key .. ".focus." .. i, function()
      -- (Not while a menu is open: focus going back to a title as the
      -- menu the bar left closes is not a choice.)
      if m.focused and (t.open or 0) == 0 then send("focus_in", i) end
    end, { owner = root })
  end
  -- Whether focus is inside, and whether a keyboard put it there.
  morf.effect(key .. ".within", function()
    local within, keyboard = false, false
    for i = 1, nmembers do
      if members[i].focused then within, keyboard = true, members[i].visual_focus == true end
    end
    t.within, t.keyboard = within, keyboard
  end, { owner = root })
  -- The box the indicator rides: the open menu's title, else the current.
  morf.effect(key .. ".box", function()
    local i = (t.open or 0) > 0 and t.open or t.current
    local m = members[i]
    if not m then return end
    local w, h = m.layout_width or 0, m.layout_height or 0
    local x, y = (m.layout_x or 0) - (root.layout_x or 0), (m.layout_y or 0) - (root.layout_y or 0)
    if w > 0 and h > 0 then t.cur_x, t.cur_y, t.cur_w, t.cur_h = x, y, w, h end
  end, { owner = root })

  -- The keys a focused member leaves come up here.
  root.on_key_pressed = function(_, text, modifiers, _, name)
    if not t.within and not (t.open > 0) then return false end
    local effects = send("key", name or "", modifiers or "", text or "", 0)
    return effects ~= nil and effects.handled == true
  end

  local handle = { node = root, t = t, members = members }
  function handle.current() return t.current end
  function handle.focus(i) focus_member(i or t.current, true) end
  function handle.set_current(i) ctl.configure("current", i) end
  function handle.open(i)
    if i then send("focus_in", i) end
    if t.open == 0 then send("key", "Return", "", "", 0) end
  end
  function handle.close() switching = true close_menus() switching = false send("menu_closed") end
  function handle.is_open() return t.open > 0 end
  function handle.menu(i) return menus[i] end
  return root, handle
end

-- ------------------------------------------------------------ the track --

local function ease_out(u, power) return 1 - (1 - u) ^ (power or 3) end
local STEPS = 18

--- For skins: an invisible track that rides the current member's box
--- (`t.cur_x`, `t.cur_y`, `t.cur_w`, `t.cur_h`, through `opts.fit(x, y,
--- w, h)`), its two edges travelling apart -- the leading one first, the
--- trailing one after -- so what rides it stretches towards the new member
--- and draws itself in there. Only the track's box moves; a field shape
--- tracking it is redrawn by the renderer. `opts`: `fit`, `duration`
--- (base ms, 240), `lead` (the share of the time the leading edge takes,
--- 0.55), `reduced` (jump), `stretch` (the track's).
function M.glide(t, opts)
  opts = opts or {}
  local fit = opts.fit or function(x, y, w, h) return x, y, w, h end
  local lead = opts.lead or 0.55
  local track = ui.Item { stretch = opts.stretch }
  local last, running
  local function travel(a, b, duration)
    local tracks = {}
    for _, axis in ipairs { { "x", "width", 1, 3 }, { "y", "height", 2, 4 } } do
      local l0, r0 = a[axis[3]], a[axis[3]] + a[axis[4]]
      local l1, r1 = b[axis[3]], b[axis[3]] + b[axis[4]]
      if l0 == l1 and r0 == r1 then
        track[axis[1]], track[axis[2]] = l1, r1 - l1
      else
        local forward = l1 >= l0
        local pos, len = {}, {}
        for i = 0, STEPS do
          local u = i / STEPS
          local function edge(from, to, leading)
            local k = leading and ease_out(math.min(1, u / lead), 3) or ease_out(math.max(0, (u - 0.2) / 0.8), 2)
            return from + (to - from) * k
          end
          local l, r = edge(l0, l1, not forward), edge(r0, r1, forward)
          pos[#pos + 1] = { at = u, value = l }
          len[#len + 1] = { at = u, value = math.max(0, r - l) }
        end
        tracks[#tracks + 1] = { node = track, property = axis[1], duration = duration, keyframes = pos }
        tracks[#tracks + 1] = { node = track, property = axis[2], duration = duration, keyframes = len }
      end
    end
    if #tracks > 0 then return morf.animation.play { { parallel = tracks } } end
  end
  morf.effect("kit.roving.glide." .. tostring(track), function()
    local x, y, w, h = t.cur_x, t.cur_y, t.cur_w, t.cur_h
    if not w or w <= 0 or h <= 0 then return end
    local box = { fit(x, y, w, h) }
    if not last or opts.reduced then
      track.x, track.y, track.width, track.height = box[1], box[2], box[3], box[4]
    elseif box[1] ~= last[1] or box[2] ~= last[2] or box[3] ~= last[3] or box[4] ~= last[4] then
      if running then running:stop() end
      local far = math.abs(box[1] - last[1]) + math.abs(box[2] - last[2])
      running = travel(last, box, math.floor(math.min(460, (opts.duration or 240) + far * 0.5)))
    end
    last = box
  end, { owner = track })
  return track
end

return M
