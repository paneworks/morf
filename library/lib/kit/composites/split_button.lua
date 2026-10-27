-- AdwSplitButton: a primary action and, joined to it, an arrow that opens
-- a popover menu of the other ones (Press + Popup); its variants are a
-- button's (suggested, destructive, flat).
--
--     local node, split = composites.split_button {
--       id = "save", label = "Save", on_clicked = save, width = 140,
--       items = { { label = "Save As…", on_clicked = save_as } },
--     }
--
-- The menu is a menu button's (lib.kit.composites.menu_button: `items`,
-- `menu_width`, `placement`, `on_opened`, `on_closed`); the arrow opens it
-- on a press and Alt+Down. Ids: `<id>`, `<id>-arrow`, `<id>-menu`.
local ui = require("morf.ui")
local press = require("lib.kit.archetypes.press")
local menu_button = require("lib.kit.composites.menu_button")

return function(spec)
  spec = spec or {}
  local W, H = spec.width or 150, spec.height or 34
  local AW = spec.arrow_width or H
  local node
  local menu = menu_button.menu(spec, function() return node end)
  local main = press.make("button", { id = spec.id, variant = spec.variant, label = spec.label, icon = spec.icon,
    width = W - AW - 1, height = H, on_clicked = spec.on_clicked, on_key_pressed = menu.keys,
    accessible_name = spec.accessible_name or spec.label })
  local arrow = press.make("button", { id = spec.id and (spec.id .. "-arrow") or nil, variant = spec.variant,
    icon = "expand_more", width = AW, height = H, on_clicked = menu.toggle, on_key_pressed = menu.keys,
    accessible_name = "More options" })
  node = ui.Row { x = spec.x, y = spec.y, gap = 1, main, arrow }
  menu.node = node
  return node, menu
end
