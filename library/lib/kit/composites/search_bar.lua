-- GtkSearchBar: a search entry that slides into view on Ctrl+F (or its
-- toggle) and away on Escape (TextField + Disclosure).
--
--     local node, bar = composites.search_bar {
--       id = "find", width = 480, placeholder = "Search",
--       on_search = function(text) end,           -- as it is typed
--       on_accepted = function(text) end,         -- Return
--       revealed = false, toggle = true,          -- a search button beside a title
--       title = "Files",                          -- the strip the toggle sits in
--     }
--     bar.reveal() bar.hide() bar.revealed()
--
-- Ctrl+F reveals it and puts the keys in it (from anywhere on the surface
-- unless `scope = "local"`, when only from inside it); Escape clears it,
-- hides it and gives focus back to what had it. `on_revealed(open)`
-- follows. Other fields: `x`, `y`, `height` (the entry's, 34), `text`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local serial = 0

return function(spec)
  spec = spec or {}
  serial = serial + 1
  local id = spec.id
  local W, FH = spec.width or 400, spec.height or 34
  local revealed = morf.signal("kit.composites.search_bar." .. tostring(id) .. "." .. serial, spec.revealed == true)
  local before
  local input
  local function set(open)
    if open == revealed:get() then return end
    revealed:set(open)
    if spec.on_revealed then spec.on_revealed(open) end
  end
  local function reveal()
    if not revealed:get() then before = morf.focus.get() end
    set(true)
    if input then morf.focus.set(input, true) end
  end
  local function hide()
    if input then input.text = "" end
    if spec.on_search then spec.on_search("") end
    set(false)
    if before then morf.focus.set(before, true) before = nil
    elseif input then morf.focus.clear(input) end
  end
  local field
  field, input = widgets.search_entry {
    id = id, width = W - 16, height = FH, x = 8, y = 6, icon = "search",
    text = spec.text, placeholder = spec.placeholder or "Search",
    on_edited = function(text) if spec.on_search then spec.on_search(text) end end,
    on_accepted = function(text) if spec.on_accepted then spec.on_accepted(text) end end,
    on_escape = hide,
  }
  local content = ui.Item { width = W, height = FH + 12, field }
  local revealer = widgets.revealer { id = id and (id .. "-revealer") or nil, width = W,
    expanded = function() return revealed:get() end, content = content }
  -- Hidden once folded away, so Tab passes it by.
  content.visible = function() return revealed:get() or (revealer.layout_height or 0) > 0.5 end
  local shortcuts = { ["ctrl+f"] = function() reveal() end }
  if spec.scope ~= "local" then shortcuts.scope = "surface" end
  local children = {}
  local y = 0
  if spec.toggle or spec.title then
    local TH = 44
    local title = spec.title and widgets.label { x = 8, anchors = { vertical_center = true },
      text = spec.title, width = W - 64, elide = "right", variant = "heading" } or nil
    local button = spec.toggle ~= false and widgets.toggle_button {
      id = id and (id .. "-toggle") or nil, width = 34, height = 34, icon = "search", variant = "flat",
      anchors = { right = true, right_margin = 4, vertical_center = true }, accessible_name = "Search",
      checked = function() return revealed:get() end,
      on_clicked = function() if revealed:get() then hide() else reveal() end end } or nil
    children[#children + 1] = ui.Item { width = W, height = TH, title, button }
    y = TH
  end
  revealer.y = y
  children[#children + 1] = revealer
  local node = ui.Item { id = id and (id .. "-bar") or nil, x = spec.x, y = spec.y, width = W,
    height = function() return y + (revealer.layout_height or 0) end, shortcuts = shortcuts,
    table.unpack(children) }
  local handle = { node = node, input = input, reveal = reveal, hide = hide,
    revealed = function() return revealed:get() end }
  return node, handle
end
