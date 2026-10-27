-- AdwEntryRow: the title over a text field, both in the row; `text`,
-- `placeholder`, `validator`, `on_changed(text)`, `on_accepted(text)`.
-- `widget` names the field ("entry"; a password entry row says
-- "password_entry").
local widgets = require("lib.kit.widgets")
local text_field = require("lib.kit.archetypes.text_field")

return function(spec)
  spec = spec or {}
  local W, H = spec.width or 360, spec.height or 56
  local field, input = text_field.make("entry_row", {
    id = spec.id and (spec.id .. "-entry") or nil, width = W, height = H, label = spec.title,
    inset = { 12, 24, spec.widget == "password_entry" and 44 or 12, 6 }, echo = spec.widget == "password_entry" and "password" or nil,
    reveal = spec.widget == "password_entry" or nil,
    text = spec.text, placeholder = spec.placeholder, validator = spec.validator, accessible_name = spec.title,
    on_edited = function(text) if spec.on_changed then spec.on_changed(text) end end,
    on_accepted = function(text) if spec.on_accepted then spec.on_accepted(text) end end,
  })
  local node = widgets.box { id = spec.id, width = W, height = H, field }
  return node, { node = node, input = input }
end
