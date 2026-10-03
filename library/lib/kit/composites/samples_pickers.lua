-- Gallery samples for composites (pickers): name -> function(kit, composites)
-- returning a node that fits a 520x340 area
-- (examples/shells/caelestia/tests/composites_gallery_spec.lua).
local ui = require("morf.ui")

local FONTS = { "Rubik", "IBM Plex Mono", "JetBrains Mono", "Noto Sans", "Noto Serif", "Adwaita Sans",
  "Material Symbols Rounded" }

return {
  date_picker = function(_, composites)
    local inline = composites.date_picker { id = "sample-date-inline", inline = true, calendar_width = 252,
      value = "2026-10-03" }
    local single = composites.date_picker { id = "sample-date", value = "2026-10-14", width = 230 }
    local range = composites.date_picker { id = "sample-dates", range = true, from = "2026-10-03", to = "2026-10-09",
      width = 230 }
    return ui.Item { width = 520, height = 340, inline,
      ui.Column { x = 280, y = 4, gap = 12, single, range } }
  end,
  time_picker = function(_, composites)
    return ui.Item { width = 520, height = 340,
      ui.Column { gap = 24,
        (composites.time_picker { id = "sample-time-inline", inline = true, seconds = true, value = "14:30:05" }),
        (composites.time_picker { id = "sample-time-12", inline = true, twelve_hour = true, value = "21:45" }) },
      ui.Item { x = 330, y = 4, (composites.time_picker { id = "sample-time", value = "07:15", width = 150 }) } }
  end,
  colour_picker = function(_, composites)
    return ui.Item { width = 520, height = 340,
      (composites.colour_picker { id = "sample-colour", width = 300, height = 336, alpha = true, value = "#3a7bd5" }),
      ui.Item { x = 330, y = 0,
        (composites.colour_picker { id = "sample-colour-popup", popup = true, value = "#e8a33d", field_width = 170 }) } }
  end,
  font_picker = function(_, composites)
    return (composites.font_picker { id = "sample-font", width = 380, height = 336, fonts = FONTS, value = "Rubik" })
  end,
  emoji_picker = function(_, composites)
    return (composites.emoji_picker { id = "sample-emoji", width = 360, height = 330 })
  end,
  calendar = function(_, composites)
    return (composites.calendar { id = "sample-calendar", width = 336, cell_height = 40, value = "2026-10-03",
      marked = function(date) local d = tonumber(date:sub(-2)) return d % 5 == 0 end })
  end,
}
