-- A toolbar: a row of tool buttons, toggles and separators; what does not
-- fit its width goes into an overflow menu at its end (Press items +
-- overflow Popup).
--
--     local node, bar = composites.toolbar {
--       id = "tools", width = 420,                  -- or a binding: it refits as it changes
--       items = {
--         { id = "new", icon = "add", label = "New", on_clicked = new },
--         { id = "bold", icon = "format_bold", tooltip = "Bold", checked = false, on_toggled = set_bold },
--         { separator = true },
--         { id = "share", icon = "share", tooltip = "Share", on_clicked = share },
--       },
--     }
--     bar.shown() bar.overflowed() bar.open_overflow()
--
-- An item with `label` shows it beside its icon (`show_label = false`
-- keeps it in the menu only); one without shows its icon and a tooltip.
-- `checked` (true, false or a binding) makes a toggle: it stays down
-- while on, and `on_toggled(on)` hears it. When the items are wider than
-- the toolbar the last ones leave it, a "more" button takes their place
-- and its menu lists them -- a press there runs the item (or toggles it)
-- as the button would. The buttons are one Tab stop (the Roving archetype,
-- headless): Tab enters at the one last used, Left/Right, Home and End
-- walk the shown ones, passing over disabled ones; Space and Return press
-- it. What fits is the Overflow archetype's (headless): higher `priority`
-- stays longer. Other fields: `x`, `y`, `height` (40), `menu_width` (220),
-- `accessible_name`. Ids: `<id>-item-<n>` (or the item's own `id`),
-- `<id>-more`, `<id>-menu`, `<id>-menu-<n>`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local popup = require("lib.kit.popup")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local skey = "kit.composites.toolbar." .. tostring(spec.id) .. "." .. serial
  local id = spec.id
  local H = spec.height or 40
  local GAP = 4
  local items = spec.items or {}
  local MW = spec.menu_width or 220
  local function width() return get(spec.width) or 400 end

  -- Each item's room, and each toggle's state.
  local sizes, states = {}, {}
  for i, item in ipairs(items) do
    if item.separator or item.kind == "separator" then
      sizes[i] = 9
    elseif item.label and item.show_label ~= false then
      sizes[i] = math.ceil((item.icon and 40 or 16) + utf8.len(tostring(item.label)) * 7.6 + 14)
    else
      sizes[i] = H
    end
    if item.checked ~= nil and type(item.checked) ~= "function" then
      states[i] = morf.signal(skey .. ".on." .. i, item.checked == true)
    end
  end
  local function is_on(i)
    local item = items[i]
    if type(item.checked) == "function" then return item.checked() == true end
    return states[i] and states[i]:get() or false
  end
  local function toggle(i, on)
    if states[i] then states[i]:set(on) end
    if items[i].on_toggled then items[i].on_toggled(on) end
  end

  local function is_separator(i) return items[i].separator or items[i].kind == "separator" end
  -- (The archetypes' bindings end with this node, made first.)
  local owner = ui.Item {}
  -- What fits: the Overflow archetype's, from each item's room, the
  -- width, and a square "more" button.
  local priorities = {}
  for i, item in ipairs(items) do priorities[i] = tonumber(item.priority) or 0 end
  local over = control.headless("Overflow", { widths = sizes, priorities = priorities, gap = GAP, more_width = H,
    available = width, owner = owner })
  local function overflowed() return over.t.overflowing == true end
  -- Shown: what fits -- a separator never ends the row nor starts the menu.
  local function shown_item(i)
    if not control.has(over.t.shown, i) then return false end
    if is_separator(i) then
      local after = false
      for j = i + 1, #items do
        if control.has(over.t.shown, j) and not is_separator(j) then after = true break end
      end
      if not after then return false end
    end
    return true
  end
  local function fit()
    local n = 0
    for i = 1, #items do if shown_item(i) then n = n + 1 end end
    return n
  end

  -- The buttons as one Tab stop: the Roving archetype over the
  -- non-separator items (members), hidden and disabled ones passed over.
  local members, member_of = {}, {}
  for i = 1, #items do
    if not is_separator(i) then members[#members + 1] = i member_of[i] = #members end
  end
  local nodes = {}
  local rove = control.headless("Roving", { count = #members, owner = owner,
    disabled = function()
      local out = {}
      for m, i in ipairs(members) do
        local enabled = items[i].enabled
        if type(enabled) == "function" then enabled = enabled() end
        if enabled == false or not shown_item(i) then out[#out + 1] = m end
      end
      return out
    end,
    on_current_changed = function(m) local node = nodes[members[m]] if node then morf.focus.set(node, true) end end })
  local function roving_key(name, modifiers)
    return rove.key(name or "", modifiers or "")
  end

  local function item_id(i) return items[i].id or (id and (id .. "-item-" .. i)) or nil end

  local function button(i)
    local item = items[i]
    if item.separator or item.kind == "separator" then
      return ui.Item { width = sizes[i], height = H, visible = function() return shown_item(i) end,
        kit.separator { vertical = true, length = H - 16, anchors = { center_in = true } } }
    end
    local checkable = item.checked ~= nil
    local t
    local look = ui.Item { anchors = { fill = true } }
    local node
    node, t = control.make("Press", "area", { widget = "area", id = item_id(i), width = sizes[i], height = H,
      checkable = checkable, checked = checkable and function() return is_on(i) end or nil,
      enabled = item.enabled, cursor = "pointer",
      visible = function() return shown_item(i) end,
      accessible_name = item.label or item.tooltip or item.icon,
      on_key_pressed = function(_, _, modifiers, _, name) return roving_key(name, modifiers) end,
      on_clicked = item.on_clicked,
      on_toggled = checkable and function(on) toggle(i, on) end or nil },
      { children = { look } })
    nodes[i] = node
    local m = member_of[i]
    -- Tab stops at the current member only; a member taking focus is current.
    node.focus_policy = function()
      local current = rove.t.current
      -- (The current one gone into the menu: the first shown takes the stop.)
      if not shown_item(members[current] or 0) then
        for k, j in ipairs(members) do if shown_item(j) then current = k break end end
      end
      return current == m and "strong" or "click"
    end
    morf.effect(skey .. ".focus." .. i, function() if node.focused then rove.send("focus_in", m) end end, { owner = node })
    local function ink()
      if checkable and is_on(i) then return kit.signal("accent")() end
      return kit.ink("hi")()
    end
    ui.reparent(kit.surface { anchors = { fill = true, margins = 2 }, radius = kit.round and kit.round(10) or 10,
      color = function()
        local accent = kit.signal("accent")()
        local base = (checkable and is_on(i)) and accent:alpha(0.18) or accent:alpha(0)
        if t.down then return (checkable and is_on(i)) and accent:alpha(0.28) or kit.ink("hi")():alpha(0.12) end
        if t.hovered then return (checkable and is_on(i)) and accent:alpha(0.24) or kit.ink("hi")():alpha(0.07) end
        return base
      end,
      behavior = { color = { duration = 120 } } }, look)
    local row = ui.Row { anchors = { center_in = true }, gap = 8, align = "center" }
    if item.icon then ui.reparent(kit.icon(item.icon, 20, ink, { fill = checkable and function() return is_on(i) end or nil }), row) end
    if item.label and item.show_label ~= false then
      ui.reparent(kit.text { text = item.label, color = ink }, row)
    end
    ui.reparent(row, look)
    if not (item.label and item.show_label ~= false) and (item.tooltip or item.label) and popup.tooltip then
      popup.tooltip(node, item.tooltip or item.label)
    end
    return node
  end

  -- -------------------------------------------------------- the overflow --
  local menu, more
  local function menu_rows()
    local column = { gap = 0, z = 1 }
    for i, item in ipairs(items) do
      if not (item.separator or item.kind == "separator") then
        local entry = { id = id and (id .. "-menu-" .. i) or nil, label = item.label or item.tooltip or item.icon,
          icon = item.icon, width = MW - 8, height = 36, visible = function() return not shown_item(i) end }
        if item.checked ~= nil then
          entry.checked = function() return is_on(i) end
          entry.on_toggled = function(on) toggle(i, on) end
          column[#column + 1] = require("lib.kit.widgets").check_menu_item(entry)
        else
          entry.on_clicked = function()
            if menu then menu.close("activated") end
            if item.on_clicked then item.on_clicked() end
          end
          column[#column + 1] = require("lib.kit.widgets").menu_item(entry)
        end
      end
    end
    return ui.Column(column)
  end
  local function open_overflow()
    if not menu then
      menu = popup.make("menu", { id = id and (id .. "-menu") or nil, content = menu_rows(), width = MW, padding = 4,
        placement = "bottom-end", close_policy = "escape+outside" })
    end
    if not menu.is_open() then menu.open(more) end
  end
  more = control.make("Press", "icon", { widget = "icon", id = id and (id .. "-more") or nil,
    icon_off = "more_vert", icon_on = "more_vert", width = H, height = H, size = 20, accessible_name = "More",
    visible = overflowed,
    on_clicked = function() if menu and menu.is_open() then menu.close("closed") else open_overflow() end end })
  if popup.tooltip then popup.tooltip(more, "More") end

  local row = { gap = GAP, align = "center" }
  for i = 1, #items do row[#row + 1] = button(i) end
  local node = ui.Item { id = id, x = spec.x, owner,
    on_destroyed = function() over.drop() rove.drop() end, y = spec.y, anchors = spec.anchors, width = width, height = H,
    accessible_role = "group", accessible_name = spec.accessible_name or "Toolbar",
    ui.Row(row),
    ui.Item { anchors = { right = true, vertical_center = true }, width = H, height = H, more } }
  local handle = { node = node }
  handle.shown = fit
  handle.overflowed = overflowed
  handle.open_overflow = open_overflow
  handle.close_overflow = function() if menu then menu.close("closed") end end
  handle.menu_open = function() return menu ~= nil and menu.is_open() end
  handle.is_on = is_on
  return node, handle
end

return { make = make }
