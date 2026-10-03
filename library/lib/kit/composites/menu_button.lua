-- A menu button: a button that opens a menu under it (Press + Popup); with
-- `split = true`, a split button -- a primary action, and an arrow beside
-- it that opens the menu.
--
--     local node, menu = composites.menu_button {
--       id = "file", label = "File", icon = "description", width = 140,
--       items = {
--         { label = "New", icon = "add", on_clicked = new },
--         { label = "Wrap lines", checked = function() return wrap:get() end, on_toggled = set_wrap },
--       },
--       split = false, on_clicked = save,     -- the primary action, when split
--     }
--     menu.open() menu.close() menu.is_open()
--
-- The button (or the arrow) opens on a press, Space, Return and Alt+Down;
-- the menu's own keys walk it, Return runs an item, Escape closes it and
-- focus goes back to the button. Items are a popup menu's (lib.kit.popup:
-- `label`, `icon`, `on_clicked`, `checked`/`on_toggled`, `group`, `id`;
-- `<id>-item-<index>` otherwise). Other fields: `x`, `y`, `height` (36),
-- `widget` (the button's Press widget, "pill"), `menu_width` (200),
-- `placement` ("bottom-start"), `on_opened`, `on_closed(reason)`,
-- `accessible_name`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local popup = require("lib.kit.popup")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local id = spec.id
  local W, H = spec.width or 150, spec.height or 36
  local widget = spec.widget or "pill"
  local open = morf.signal("kit.composites.menu_button.open." .. tostring(id) .. "." .. serial, false)
  local menu, anchor
  local MW = spec.menu_width or math.max(W, 200)
  local function close(reason) if menu then menu.close(reason or "closed") end end
  -- The menu's rows: menu items, check and radio items (Press widgets).
  local function rows()
    local widgets = require("lib.kit.widgets")
    local column = { gap = 0, z = 1 }
    for i, item in ipairs(spec.items or {}) do
      local entry = {}
      for k, v in pairs(item) do entry[k] = v end
      entry.id = item.id or (id and (id .. "-item-" .. i)) or nil
      entry.width, entry.height = MW - 8, item.height or spec.item_height or 36
      local kind = "menu_item"
      if item.checked ~= nil then kind = item.group and "radio_menu_item" or "check_menu_item" end
      local clicked = item.on_clicked
      entry.on_clicked = function(...)
        if clicked then clicked(...) end
        -- A plain item closes its menu; a checkable one stays to be toggled again.
        if kind == "menu_item" then close("activated") end
      end
      column[#column + 1] = widgets[kind](entry)
    end
    return ui.Column(column)
  end
  local function ensure()
    if menu then return menu end
    menu = popup.make("menu", {
      id = id and (id .. "-menu") or nil, content = rows(), width = MW, padding = 4,
      placement = spec.placement or "bottom-start", close_policy = "escape+outside",
      on_opened = spec.on_opened,
      on_closed = function(reason) open:set(false) if spec.on_closed then spec.on_closed(reason) end end,
    })
    return menu
  end
  local function open_menu()
    if menu and menu.is_open() then return end
    open:set(true)
    ensure().open(anchor)
  end
  local function toggle() if menu and menu.is_open() then close("closed") else open_menu() end end
  local function keys(_, _, modifiers, _, name)
    if (name == "Down" or name == "Up") and tostring(modifiers or ""):find("alt", 1, true) then open_menu() return true end
    return false
  end
  local function chevron(ink)
    if not kit.icon then return nil end
    return ui.Item { z = 1, width = 20, height = 20, anchors = { right = true, right_margin = 8, vertical_center = true },
      rotation = function() return open:get() and 180 or 0 end,
      behavior = { rotation = { duration = 200, easing = "out_cubic" } },
      kit.icon("expand_more", 20, ink or (kit.ink and kit.ink("hi"))) }
  end
  local node
  if spec.split then
    local AW = spec.arrow_width or H
    local main = control.make("Press", widget, { widget = widget, id = id, label = spec.label, icon = spec.icon,
      width = W - AW - 2, height = H, on_clicked = spec.on_clicked, on_key_pressed = keys,
      accessible_name = spec.accessible_name or spec.label })
    local arrow = control.make("Press", widget, { widget = widget, id = id and (id .. "-arrow") or nil,
      icon = "expand_more", width = AW, height = H, on_clicked = toggle, on_key_pressed = keys,
      accessible_name = "More options" })
    node = ui.Row { x = spec.x, y = spec.y, gap = 2, main, arrow }
    anchor = node
  else
    -- The label and icon the button's skin draws, kept clear of the arrow.
    node = control.make("Press", widget, { widget = widget, id = id and (id .. "-button") or nil, x = spec.x,
      y = spec.y, label = spec.label, icon = spec.icon, width = W, height = H, on_clicked = toggle,
      on_key_pressed = keys, accessible_name = spec.accessible_name or spec.label, insets = { 0, 0, 24, 0 } },
      { children = { chevron(spec.ink) } })
    anchor = node
  end
  local handle = { node = node, open = open_menu, close = close, toggle = toggle,
    is_open = function() return menu ~= nil and menu.is_open() end }
  return node, handle
end

return { make = make }
