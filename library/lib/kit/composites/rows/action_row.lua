-- AdwActionRow: a boxed-list row with a `title`, a `subtitle` under it,
-- an `icon` before them and a `suffix` node (`suffix_width` px) after.
-- `on_activated` makes the row a press, with a chevron at its end unless
-- `chevron = false`. The `property` variant reads the subtitle as the
-- value and the title as its name.
--
--     local node = composites.action_row { title = "About", on_activated = about }
local widgets = require("lib.kit.widgets")
local rows = require("lib.kit.composites.rows")

return function(spec)
  spec = spec or {}
  local suffix, sw = spec.suffix, spec.suffix_width or 0
  if not suffix and spec.on_activated and spec.chevron ~= false then
    suffix, sw = widgets.image { icon = "chevron_right", pixel_size = 20, variant = "dim" }, 20
  elseif suffix and sw == 0 then
    sw = (type(suffix.width) == "number" and suffix.width) or 120
  end
  local node = rows.frame(spec, { suffix = suffix, suffix_w = sw,
    on_clicked = spec.on_activated and function() spec.on_activated() end or nil })
  return node, { node = node }
end
