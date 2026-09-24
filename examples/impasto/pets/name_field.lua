-- A pet's name, edited in place.
--
-- The original uses a TextInput: the name is typed straight into the line
-- it is shown on, Escape clears it, and empty shows the species' name as a
-- greyed placeholder. morf has no text input node, so this is one: a click
-- starts editing, keys go to a handler that exists only while editing (so
-- nothing else loses its keys to a name nobody is typing), Return or a
-- second click ends it. Every key renames at once, as `onTextEdited` does.

local ui = require("morf.ui")
local theme = require("theme")
local pets = require("services.pets")
local kit = require("components.kit")

local C = theme.color

local K = { BACKSPACE = 0xff08, RETURN = 0xff0d, KP_ENTER = 0xff8d, ESCAPE = 0xff1b }

-- Which field is being typed into: one at a time across the shell.
local editing = morf.signal("impasto.pets.editing", "")
local fields = 0

local name_field = {}

--- `index` a function giving the record's index; `size`, `weight` the
--- face; `height` the line.
function name_field.new(options)
  fields = fields + 1
  local id = "field" .. fields
  local index = options.index
  local mine = function() return editing:get() == id end
  local blink = morf.signal("impasto.pets.caret." .. fields, true)
  local record = function() return pets.record_at(index()) end
  local name = function() local r = record() return r and r.name or "" end
  local size = options.size or theme.size.small

  local label = kit.text {
    text = name,
    size = size,
    weight = options.weight or 400,
    visible = function() return name() ~= "" end,
  }
  local caret = ui.Rect {
    width = 1.5,
    height = size + 2,
    color = C.accent,
    opacity = function() return (mine() and blink:get()) and 1 or 0 end,
  }

  return ui.Item {
    anchors = options.anchors,
    width = options.width,
    height = options.height or 24,
    layout = options.layout,
    -- Unnamed, the species as a greyed placeholder.
    kit.text {
      anchors = { left = true, top = true, top_margin = ((options.height or 24) - size * 1.2) / 2 },
      text = function() return pets.species_of(record()).label end,
      size = size,
      weight = options.weight or 400,
      color = C.textMuted,
      visible = function() return name() == "" end,
    },
    ui.ClipRect {
      anchors = { fill = true },
      color = "#00000000",
      ui.Row { gap = 1, align = "center", height = options.height or 24, label, caret },
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "text",
      on_clicked = function()
        if mine() then editing:set("") else editing:set(id) end
      end,
    },
    ui.Timer {
      interval = 530, ["repeat"] = true,
      running = mine,
      on_triggered = function() blink:set(not blink:get()) end,
    },
    -- The keys, only while this field is being typed into.
    ui.MouseArea {
      width = 1, height = 1,
      visible = mine,
      focus = true,
      on_key_pressed = function(keysym, text)
        local i = index()
        if keysym == K.RETURN or keysym == K.KP_ENTER then
          pets.rename_at(i, name())
          editing:set("")
        elseif keysym == K.ESCAPE then
          pets.rename_at(i, "")
          editing:set("")
        elseif keysym == K.BACKSPACE then
          local current = name()
          if current ~= "" then
            -- A whole UTF-8 sequence, not one byte of it.
            local cut = #current
            while cut > 1 and current:byte(cut) >= 0x80 and current:byte(cut) < 0xc0 do cut = cut - 1 end
            pets.rename_at(i, current:sub(1, cut - 1), true)
          end
        elseif type(text) == "string" and text ~= "" and keysym < 0xff00 and text:byte(1) >= 0x20 then
          -- Kept as typed: the service trims only what it stores, and a
          -- space typed between two words must survive the keystroke.
          pets.rename_at(i, name() .. text, true)
        end
        blink:set(true)
      end,
    },
  }
end

--- Ends any editing (the panel closing, say).
function name_field.stop() editing:set("") end

return name_field
