-- A pet's name, edited in place.
--
-- Port of the TextInput in PetFamily.qml and PetPanel.qml: the name is typed
-- straight into the line it is shown on, every edit renames at once
-- (`onTextEdited`), Escape clears it, and empty shows the species' name as a
-- greyed placeholder. A `ui.TextInput`, so the caret, selection, clipboard
-- and input method are the engine's.
--
--     name_field.new { index = fn, size = 11, weight = 400, height = 28 }
--     name_field.boxed { ... }   -- in its box, the border in the accent while typing

local ui = require("morf.ui")
local theme = require("theme")
local pets = require("services.pets")

local C = theme.color

local name_field = {}

local fields = {}
local count = 0

--- `index` a function giving the record's index; `size`, `weight` the
--- face; `height` the line. `on_focus(bool)` hears the field take and lose
--- the keyboard.
function name_field.new(options)
  count = count + 1
  local index = options.index
  local record = function() return pets.record_at(index()) end
  local name = function() local r = record() return r and r.name or "" end
  local field
  field = ui.TextInput {
    anchors = options.anchors,
    width = options.width,
    height = options.height or 24,
    layout = options.layout,
    vertical_alignment = "center",
    text = name(),
    placeholder = function() return pets.species_of(record()).label end,
    placeholder_color = C.textMuted,
    font_family = function() return theme.font() end,
    font_size = options.size or theme.size.small,
    font_weight = options.weight or 400,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    clip = true,
    -- Kept as typed: the service trims only what it stores, and a space
    -- typed between two words must survive the keystroke.
    on_text_changed = function(text) pets.rename_at(index(), text, true) end,
    on_accepted = function(text)
      pets.rename_at(index(), text)
      field.focus = false
    end,
    on_escape = function()
      field.text = ""
      pets.rename_at(index(), "")
    end,
    on_focus_changed = function(on)
      if options.on_focus then options.on_focus(on) end
      -- Letting go stores the name as the service keeps it.
      if not on then pets.rename_at(index(), field.text) end
    end,
  }
  -- Another pet, or a rename from elsewhere, while nobody is typing here:
  -- the field shows it. Never while typing, so the caret stays put.
  morf.effect("impasto.pets.name." .. count, function()
    local shown = name()
    if not field.focus and field.text ~= shown then field.text = shown end
  end, { owner = field })
  fields[#fields + 1] = setmetatable({ field = field }, { __mode = "v" })
  return field
end

--- The field in its box: the island's colour, a hairline that turns to the
--- accent while the name is being typed (PetFamily.qml's `activeFocus`).
function name_field.boxed(options)
  local focused = morf.signal("impasto.pets.name.focus." .. (count + 1), false)
  local values = {}
  for k, v in pairs(options) do values[k] = v end
  values.anchors = { fill = true, left_margin = 10, right_margin = 10 }
  values.on_focus = function(on) focused:set(on) end
  return ui.Rect {
    layout = options.layout, visible = options.visible,
    width = options.box_width, height = options.height or 28,
    radius = theme.radius_small,
    color = C.island,
    border_width = 1,
    border_color = function() return focused:get() and C.accent() or C.islandBorder end,
    behavior = { border_color = theme.behave("fast") },
    name_field.new(values),
  }
end

--- Ends any typing (the panel closing, say).
function name_field.stop()
  for _, entry in ipairs(fields) do
    local field = entry.field
    if field then pcall(function() if field.focus then field.focus = false end end) end
  end
end

return name_field
