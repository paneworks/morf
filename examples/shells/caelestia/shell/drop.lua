-- Placeholder for the future https://github.com/termworks/drop integration.
local ui = require("morf.ui")
local kit = require("kit")
local theme = require("theme")
local C = theme.color
local M = {}

function M.build(w, h)
  return kit.card { id = "drop-page", width = w, height = h,
    ui.Column { id = "drop-placeholder", anchors = { center_in = true }, gap = 16, align = "center",
      kit.icon("forum", 48, function() return C.primary end),
      kit.text { text = "Drop", font_size = 32, font_weight = 700 },
      kit.text { text = "Messaging & file sharing", font_size = theme.size.large,
        color = function() return C.onSurfaceVariant end },
      kit.text { text = "Not connected yet", font_size = theme.size.small,
        color = function() return C.outline end },
    },
  }
end
return M
