-- The shape most module details share: a gauge and two lines of text on
-- top, a row of figures (and a pill) under them, inside 14 px of margin.
--
-- Not a QML file of its own in the original, where each detail repeats the
-- same ColumnLayout; one function here keeps the seven of them alike.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local modules = require("services.modules")

local C = theme.color
local detail = {}

--- `id` sizes it from the catalogue. `mark` is the 44 px node on the left;
--- `title`, `subtitle` its lines; `figures` a list of `{ label, value,
--- note }`; `pill` an optional node at the row's end.
function detail.card(id, values)
  local w = modules.entry(id).width
  local inner = w - 8 - 28
  local text_w = inner - 44 - 13
  local figures = values.figures or {}
  local pill_w = values.pill_width or 0
  local figure_w = (inner - pill_w - 14 * (#figures - (pill_w > 0 and 0 or 1))) / math.max(1, #figures)
  local row = { gap = 14, align = "center" }
  for _, figure in ipairs(figures) do
    row[#row + 1] = controls.figure {
      width = figure_w, label = figure.label, value = figure.value, note = figure.note,
    }
  end
  if values.pill then row[#row + 1] = values.pill end
  return ui.Item {
    anchors = { fill = true },
    ui.Column {
      anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
      gap = 12,
      ui.Row {
        gap = 13, align = "center",
        values.mark,
        ui.Column {
          gap = 2,
          kit.text { text = values.title, size = theme.size.medium, weight = 600,
            width = text_w, elide = "right" },
          kit.text { text = values.subtitle, size = theme.size.small, color = C.textMuted,
            width = text_w, elide = "right" },
        },
      },
      ui.Row(row),
    },
  }
end

return detail
