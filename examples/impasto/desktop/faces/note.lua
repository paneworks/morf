-- A note on the desk, in any of the four families: the paper the deck
-- draws, a title and a handwritten body.
--
-- Port of faces/NoteFace.qml and the Sticky it draws. The notes themselves
-- are another port's (`services.notes`); this asks it for the note a row
-- names and for its helpers (`paper_of`, `title_of`, `display`, `age_of`)
-- when it has them, and draws plain cream paper saying "No notes yet"
-- while it is not there. Read-only: a click opens the note in the island.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local S = require("desktop.sources")

local C = theme.color
local M = {}

local function helper(name, ...)
  local ok, notes = pcall(require, "services.notes")
  if not ok or type(notes) ~= "table" or type(notes[name]) ~= "function" then return nil end
  local okc, value = pcall(notes[name], ...)
  return okc and value or nil
end

-- A checklist reads as boxes, as the original's `display` writes it.
local function display(text)
  local shown = helper("display", text)
  if shown then return shown end
  text = tostring(text or "")
  text = text:gsub("%- %[[xX]%] ?", "☑ "):gsub("%- %[ %] ?", "☐ "):gsub("%[[xX]%] ?", "☑ "):gsub("%[ %] ?", "☐ ")
  return text
end

function M.build(ctx)
  local w, h = ctx.width, ctx.height
  local family = ctx.family
  local pad = w > 300 and 18 or 14
  local title_size = family == "2x2" and theme.size.small or theme.size.regular
  local body_size = family == "2x2" and 16 or family == "4x2" and 18 or 20
  local function note() return S.notes.note_for(ctx.row and ctx.row() or nil) end
  local function title()
    local n = note()
    if not n then return "" end
    return helper("title_of", n) or n.title or ""
  end
  local function body()
    local n = note()
    if not n then return "No notes yet" end
    return display(n.text or n.body or "")
  end
  return ui.Item {
    width = w, height = h,
    ui.Rect {
      anchors = { fill = true }, radius = theme.paper_radius,
      color = function()
        local n = note()
        local paper = n and helper("paper_of", n.tint or "yellow")
        return paper or morf.color("#fbf6ea")
      end,
    },
    kit.text {
      x = pad, y = pad - 2, width = w - 2 * pad - 50, height = title_size + 8,
      vertical_alignment = "center", elide = "right",
      text = title, size = title_size, weight = 600,
      color = function() return note() and C.paperInk or C.paperInkMuted end,
    },
    kit.text {
      mono = true, x = w - pad - 60, y = pad - 2, width = 60, height = title_size + 8,
      vertical_alignment = "center", horizontal_alignment = "right",
      size = theme.size.label, color = C.paperInkMuted,
      text = function()
        local n = note()
        return n and (helper("age_of", n.edited or n.updated) or "") or ""
      end,
    },
    kit.text {
      x = pad, y = pad - 2 + title_size + 12, width = w - 2 * pad,
      height = h - (pad - 2 + title_size + 12) - pad,
      wrap = true, max_lines = math.max(1, math.floor((h - (pad + title_size + 10) - pad) / (body_size * 1.1))),
      line_height = 1.1,
      font_family = function() return theme.font_hand() end,
      size = body_size,
      text = body,
      color = function() return note() and C.paperInk or C.paperInkMuted end,
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_clicked = function()
        local n = note()
        S.notes.open(n and n.key or "")
      end,
    },
  }
end

return M
