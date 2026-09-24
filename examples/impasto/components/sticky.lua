-- A sticky note: pastel paper, a title and a handwritten body.
--
-- Port of Sticky.qml, used wherever a note appears (deck cards, the peek
-- beside an edge tab, the dragged tab). The ink is a fixed dark
-- (`theme.color.paperInk`), not the palette's text colour.
--
--     sticky.build {
--       note = function() return notes.entry(key) end,   -- a row, or nil
--       width = 160, height = 120,
--       padding = 12, title_size = 11, body_size = 16, show_age = true,
--       placeholder = "",        -- what an empty deck says
--     }

local ui = require("morf.ui")
local theme = require("theme")
local notes = require("services.notes")
local kit = require("components.kit")

local C = theme.color
local sticky = {}

--- Text in the handwriting face, which the shell carries with it.
function sticky.hand(values)
  values.font_family = function() return theme.font_hand() end
  values.font_source = function() return theme.font_hand_source() end
  values.font_size = values.size or 16
  values.size = nil
  values.color = values.color or C.paperInk
  return ui.Text(values)
end

function sticky.build(values)
  local note = values.note
  local pad = values.padding or 12
  local title_size = values.title_size or theme.size.small
  local body_size = values.body_size or 16
  local show_age = values.show_age ~= false
  local width, height = values.width, values.height
  local head = title_size + 8
  -- Wrapped text keeps `max_lines` lines and elides the last, which is how
  -- a card cuts its body off at the bottom.
  local line = body_size * 1.1
  local lines = math.max(1, math.floor((height - pad - (pad - 2) - head - 4) / (line * 1.25)))

  local age = kit.text {
    anchors = { right = true, right_margin = pad, top = true, top_margin = pad - 2 },
    height = head, vertical_alignment = "center",
    visible = function() return show_age and note() ~= nil end,
    text = function() local n = note() return n and notes.age_of(n.edited) or "" end,
    mono = true, size = theme.size.label, color = C.paperInkMuted,
  }
  return ui.Rect {
    x = values.x, y = values.y, anchors = values.anchors,
    width = width, height = height,
    shadow_color = values.shadow_color, shadow_blur = values.shadow_blur,
    shadow_offset_y = values.shadow_offset_y,
    radius = theme.paper_radius,
    color = function() local n = note() return notes.paper_of(n and n.tint or "yellow") end,
    behavior = { color = theme.behave("fast") },
    kit.text {
      x = pad, y = pad - 2, height = head, vertical_alignment = "center",
      width = function()
        return width - 2 * pad - ((show_age and note()) and ((age.layout_width or 0) + 8) or 0)
      end,
      elide = "right",
      text = function() local n = note() return n and notes.title_of(n) or "" end,
      size = title_size, weight = 600,
      color = function()
        local n = note()
        return (n and (n.title or ""):match("%S")) and C.paperInk or C.paperInkMuted
      end,
    },
    age,
    sticky.hand {
      x = pad, y = pad - 2 + head + 4, width = width - 2 * pad,
      wrap = true, max_lines = lines,
      text = function()
        local n = note()
        if n then return notes.display(n.text) end
        return values.placeholder or ""
      end,
      size = body_size, line_height = 1.25,
      color = function() return note() and C.paperInk or C.paperInkMuted end,
    },
  }
end

return sticky
