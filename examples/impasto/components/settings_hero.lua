-- A settings page's header: its mark and its name, a step larger than the
-- group headings (SettingsHero).

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color

--- `icon()` and `title()`, values or bindings.
return function(values)
  return ui.Row {
    gap = 10, align = "center", height = 28,
    ui.Item { width = 4, height = 1 },
    kit.glyph { glyph = values.icon, size = 17, color = C.accent },
    kit.text { text = values.title, size = theme.size.large, weight = 600 },
  }
end
