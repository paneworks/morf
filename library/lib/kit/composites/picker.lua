-- A picker: choose one of a set from a popup grid or list (Press + Popup +
-- Selection) -- an icon, a size, a mode.
--
--     local node, pick = composites.picker {
--       id = "icon", width = 200, columns = 5,
--       items = { { icon = "home", label = "Home" }, { icon = "star", label = "Star" }, ... },
--       current = 1, on_changed = function(index, item) end,
--       layout = "grid",                  -- or "list"
--     }
--
-- The closed field shows the current item (its icon, or its label) and
-- opens on a press, Space, Return and Alt+Down. In the popup the arrows
-- move (across and down a grid), Return or a press picks, Escape closes,
-- and focus goes back to the field. Items with an icon are drawn as that
-- icon; others by the selection's skin. Other fields: `x`, `y`, `height`
-- (40), `cell` (44, a grid cell's side), `item_height` (36, a list's
-- rows), `popup_width`, `placement`, `placeholder`, `item_id(index,
-- item)` (`<id>-item-<index>` otherwise), `accessible_name`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local selection = require("lib.kit.selection")
local combo = require("lib.kit.composites.combo_box")

local function get(v) if type(v) == "function" then return v() end return v end
local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local function make(spec)
  spec = spec or {}
  local kit = K()
  local id = spec.id
  local W, H = spec.width or 200, spec.height or 40
  local grid = (spec.layout or "grid") == "grid"
  local columns = grid and (spec.columns or 5) or 1
  local CELL = spec.cell or 44
  local ITEM_H = spec.item_height or 36
  local function items() return get(spec.items) or {} end
  local chosen = morf.signal(combo.next_key(spec, "picker.chosen"), type(spec.current) == "number" and spec.current or 0)
  local function current()
    if type(spec.current) == "function" then return tonumber(spec.current()) or 0 end
    return chosen:get()
  end
  local open = morf.signal(combo.next_key(spec, "picker.open"), false)
  local highlight = morf.signal(combo.next_key(spec, "picker.highlight"), 0)
  local item_id = spec.item_id or (id and function(i) return id .. "-item-" .. i end) or nil
  local menu, field
  local function close(reason) if menu then menu.close(reason or "closed") end end
  local function pick(i)
    local item = items()[i]
    if item == nil then return end
    chosen:set(i)
    if spec.on_changed then spec.on_changed(i, item) end
    close("activated")
  end
  local function has_icon(item) return type(item) == "table" and item.icon ~= nil end
  local PW = spec.popup_width or (grid and (columns * CELL + 8) or W)
  local function build()
    local s = {
      id = id and (id .. "-grid") or nil, items = items, orientation = grid and "grid" or "vertical",
      columns = columns, gap = 0, item_width = grid and CELL or (PW - 8), item_height = grid and CELL or ITEM_H,
      press_activates = true, item_id = item_id,
      current = function() return highlight:get() end,
      on_current_changed = function(i) highlight:set(i) end,
      on_activated = pick,
      accessible_name = spec.accessible_name or spec.placeholder,
    }
    -- An icon is drawn as itself; the skin's plate still marks the current one.
    local list = items()
    if list[1] and has_icon(list[1]) and kit.icon then
      s.delegate = function(_, value)
        return ui.Item { anchors = { fill = true },
          kit.icon(value.icon, grid and math.floor(CELL * 0.5) or 20, kit.ink and kit.ink("hi"),
            { anchors = grid and { center_in = true } or { vertical_center = true }, x = grid and nil or 12 }),
          (not grid and kit.text) and kit.text { x = 44, anchors = { vertical_center = true }, text = value.label or "" }
            or nil }
      end
    end
    local node = selection.make(grid and "icon_chooser" or "list_selection", s)
    local holder = ui.Item { z = 1, width = PW - 8,
      height = function() return math.ceil(#items() / columns) * (grid and CELL or ITEM_H) end, node }
    return holder
  end
  local function ensure()
    if menu then return menu end
    menu = popup.make("popover", {
      id = id and (id .. "-popup") or nil, content = build(), padding = 4, width = PW,
      placement = spec.placement or "bottom-start", close_policy = "escape+outside",
      on_closed = function(reason) open:set(false) if spec.on_closed then spec.on_closed(reason) end end,
    })
    return menu
  end
  local function open_popup()
    if menu and menu.is_open() then return end
    highlight:set(current() > 0 and current() or 1)
    open:set(true)
    ensure().open(field)
  end
  local function label() local item = items()[current()] return combo.label_of(item) or "" end
  -- The field shows the current item: its icon beside its label.
  local shown
  if kit.icon and items()[1] and has_icon(items()[1]) then
    shown = ui.Row { gap = 8, align = "center",
      kit.icon(function() local item = items()[current()] return item and item.icon or "" end, 20,
        kit.ink and kit.ink("hi")),
      kit.text and kit.text { width = math.max(1, W - 90), elide = "right",
        text = function() local t = label() return t ~= "" and t or (spec.placeholder or "") end } or nil }
  end
  field = combo.field {
    id = id, x = spec.x, y = spec.y, width = W, height = H, variant = spec.variant, content = shown,
    text = function() local t = label() return t ~= "" and t or (spec.placeholder or "") end,
    placeholder = function() return label() == "" end,
    open = function() return open:get() end,
    accessible_name = function() return (get(spec.accessible_name) or "Choose") .. ": " .. label() end,
    on_clicked = function() if menu and menu.is_open() then close("closed") else open_popup() end end,
    on_key_pressed = function(_, _, modifiers, _, name)
      if combo.opens(name, modifiers) then open_popup() return true end
      return false
    end,
  }
  local handle = { node = field, open = open_popup, close = close, current = current,
    set = function(i) chosen:set(i) end, is_open = function() return menu ~= nil and menu.is_open() end }
  return field, handle
end

return { make = make }
