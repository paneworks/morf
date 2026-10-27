-- GtkMenuButton: a button that opens a popover menu under it (Press +
-- Popup); its variants are a button's (flat, circular, pill, suggested).
--
--     local node, menu = composites.menu_button {
--       id = "file", label = "File", icon = "description", width = 140,
--       items = {
--         { label = "New", icon = "add", on_clicked = new },
--         { label = "Wrap lines", checked = function() return wrap:get() end, on_toggled = set_wrap },
--       },
--     }
--     menu.open() menu.close() menu.is_open()
--
-- The button opens on a press, Space, Return and Alt+Down; the menu's own
-- keys walk it, Return runs an item, Escape closes it and focus goes back
-- to the button. Items are a popover menu's (lib.kit.archetypes.popup:
-- `label`, `icon`, `on_clicked`, `checked`/`on_toggled`, `group`, `id`;
-- `<id>-item-<index>` otherwise). Other fields: `x`, `y`, `height` (34),
-- `menu_width` (200), `placement` ("bottom-start"), `on_opened`,
-- `on_closed(reason)`, `accessible_name`. `menu(spec, anchor)` is the
-- menu half alone, for a split button.
local ui = require("morf.ui")
local popup = require("lib.kit.archetypes.popup")
local press = require("lib.kit.archetypes.press")
local widgets = require("lib.kit.widgets")

local M = {}
local serial = 0

--- The popover menu of `spec.items`, opened from `anchor()`: `open`,
--- `close`, `toggle`, `is_open`, `open_now` (a signal), and `keys`, a key
--- handler that opens it on Alt+Down.
function M.menu(spec, anchor)
  serial = serial + 1
  local id = spec.id
  local open = morf.signal("kit.composites.menu_button.open." .. tostring(id) .. "." .. serial, false)
  local menu
  local MW = spec.menu_width or math.max(spec.width or 150, 200)
  local function close(reason) if menu then menu.close(reason or "closed") end end
  local function ensure()
    if menu then return menu end
    menu = popup.make("popover_menu", {
      id = id and (id .. "-menu") or nil, items = spec.items, item_id = spec.item_id, width = MW, padding = 4,
      placement = spec.placement or "bottom-start",
      on_opened = spec.on_opened,
      on_closed = function(reason) open:set(false) if spec.on_closed then spec.on_closed(reason) end end,
    })
    return menu
  end
  local function open_menu()
    if menu and menu.is_open() then return end
    open:set(true)
    ensure().open(anchor())
  end
  local handle = { open = open_menu, close = close, open_now = open,
    is_open = function() return menu ~= nil and menu.is_open() end }
  function handle.toggle() if menu and menu.is_open() then close("closed") else open_menu() end end
  function handle.keys(_, _, modifiers, _, name)
    if (name == "Down" or name == "Up") and tostring(modifiers or ""):find("alt", 1, true) then open_menu() return true end
    return false
  end
  return handle
end

function M.make(spec)
  spec = spec or {}
  local W, H = spec.width or 150, spec.height or 34
  local node
  local menu = M.menu(spec, function() return node end)
  local icon_only = spec.icon and not spec.label
  local chevron = not icon_only and ui.Item { z = 1, width = 20, height = 20,
    anchors = { right = true, right_margin = 8, vertical_center = true },
    rotation = function() return menu.open_now:get() and 180 or 0 end,
    behavior = { rotation = { duration = 200, easing = "out_cubic" } },
    widgets.image { icon = "expand_more", pixel_size = 20 } } or nil
  node = press.make("button", { id = spec.id and (spec.id .. "-button") or nil, x = spec.x, y = spec.y,
    variant = spec.variant, label = spec.label, icon = spec.icon, width = W, height = H, on_clicked = menu.toggle,
    on_key_pressed = menu.keys, accessible_name = spec.accessible_name or spec.label,
    insets = chevron and { 0, 0, 24, 0 } or nil })
  if chevron then ui.reparent(chevron, node) end
  menu.node = node
  return node, menu
end

return M
