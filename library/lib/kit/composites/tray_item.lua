-- A desktop shell's tray item (the shell group's `tray_item`): a
-- StatusNotifier item in a bar -- its icon on a flat button; a press is
-- `on_activated`, a right press opens the item's own menu (read over the
-- bus by lib.services.tray_menu) in a popover menu (Press + Popup).
--
--     local node = composites.tray_item { address = ":1.42/StatusNotifierItem",
--       icon = function() return "volume_up" end, title = "Volume", on_activated = activate }
--
-- `size` (28) is the button's. Ids: `<id>`, `<id>-menu`.
local press = require("lib.kit.archetypes.press")
local popup = require("lib.kit.archetypes.popup")

return function(spec)
  spec = spec or {}
  local S = spec.size or 28
  local node
  local function open_menu()
    if not spec.address then return end
    local menu = require("lib.services.tray_menu").for_item(spec.address)
    if not menu then return end
    local items = {}
    for _, entry in ipairs(menu.entries()) do
      if not entry.separator and entry.visible ~= false then
        items[#items + 1] = { label = entry.text, icon = entry.icon, enabled = entry.enabled,
          on_clicked = function() menu.click(tonumber(entry.id)) end }
      end
    end
    if #items == 0 then return end
    popup.make("popover_menu", { id = spec.id and (spec.id .. "-menu") or nil, items = items, width = 220,
      padding = 4, root = spec.root }).open(node)
  end
  node = press.make("button", { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = S, height = S,
    variant = "flat circular", icon = spec.icon or "apps", accessible_name = spec.title or "Tray item",
    on_clicked = function() if spec.on_activated then spec.on_activated() end end,
    on_pressed = function(_, _, _, _, button) if button == "right" then open_menu() end end })
  return node, { node = node, open_menu = open_menu }
end
