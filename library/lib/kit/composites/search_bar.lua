-- A search bar: a search field that slides into view on Ctrl+F (or its
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
-- follows. Other fields: `x`, `y`, `height` (the field's, 40), `text`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local disclosure = require("lib.kit.disclosure")
local group = require("lib.kit.composites.input_group")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local id = spec.id
  local W, FH = spec.width or 400, spec.height or 40
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
  field, input = group.make {
    id = id, width = W - 16, height = FH, x = 8, y = 6, widget = "search", icon = "search",
    text = spec.text, placeholder = spec.placeholder or "Search",
    on_edited = function(text) if spec.on_search then spec.on_search(text) end end,
    on_accepted = function(text) if spec.on_accepted then spec.on_accepted(text) end end,
    on_escape = hide,
  }
  -- The reveal: a disclosure with no header of its own, its content the
  -- field, growing and shrinking with it.
  local content = ui.Item { width = W, height = FH + 12, field }
  local revealer = disclosure.make("area", {
    -- (A sliver of a header: a height of nought is no height at all, and
    -- the clip would take the field's.)
    id = id and (id .. "-revealer") or nil, width = W, header_height = 1e-3, focus_policy = "none",
    expanded = function() return revealed:get() end, content = content,
  })
  -- Hidden once folded away, so Tab passes it by.
  content.visible = function() return revealed:get() or (revealer.layout_height or 0) > 0.5 end
  local shortcuts = { ["ctrl+f"] = function() reveal() end }
  if spec.scope ~= "local" then shortcuts.scope = "surface" end
  local children = {}
  local y = 0
  if spec.toggle or spec.title then
    local TH = 44
    local title = spec.title and kit.text and kit.text { x = 8, anchors = { vertical_center = true },
      text = spec.title, width = W - 64, elide = "right" } or nil
    local button = spec.toggle ~= false and control.make("Press", "icon", { widget = "icon",
      id = id and (id .. "-toggle") or nil, width = 36, height = 36, size = 20, icon_off = "search",
      icon_on = "search", anchors = { right = true, right_margin = 4, vertical_center = true },
      accessible_name = "Search",
      on_clicked = function() if revealed:get() then hide() else reveal() end end }) or nil
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

return { make = make }
