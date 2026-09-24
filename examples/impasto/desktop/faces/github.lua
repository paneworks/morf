-- The contribution wall and nothing else.
--
-- Port of faces/GithubFace.qml and ContributionGrid. The cell size is fixed
-- by the height, which the families share, so a wider face shows more weeks
-- rather than smaller cells; the newest week is on the right. Green and grey
-- in every palette. Drawn as one SVG document, so a wall of three hundred
-- cells is one image rather than three hundred nodes.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local github = require("services.github")
local draw = require("pets.draw")

local M = {}

--- The wall as an SVG document `w` by `h`: `weeks` newest last, seven
--- levels each; `spacing`, `radius`, `max_cell`.
function M.document(weeks, w, h, spacing, radius, max_cell)
  local cell = math.min(max_cell or 24, (h - 6 * spacing) / 7)
  local step = cell + spacing
  local columns = math.max(1, math.floor((w + spacing) / step))
  local shown = math.min(columns, #weeks)
  local width = shown * step - spacing
  local left = (w - width) / 2
  local top = (h - (7 * step - spacing)) / 2
  local parts = {}
  for i = 1, shown do
    local week = weeks[#weeks - shown + i] or {}
    for d = 1, 7 do
      local level = week[d]
      if type(level) == "number" and level >= 0 then
        local colour = theme.github_levels[math.min(5, math.floor(level) + 1)]
        parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>',
          draw.n(left + (i - 1) * step), draw.n(top + (d - 1) * step), draw.n(cell), draw.n(cell), draw.n(radius), colour)
      end
    end
  end
  return string.format('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %s %s">%s</svg>',
    draw.n(w), draw.n(h), table.concat(parts))
end

function M.build(ctx)
  local w, h = ctx.width, ctx.height
  return ui.Item {
    width = w, height = h,
    ui.Image {
      x = 12, y = 12, width = w - 24, height = h - 24,
      visible = github.available,
      source = function() return M.document(github.weeks(), w - 24, h - 24, 3, 2.5, 24) end,
    },
    kit.text {
      anchors = { center_in = true }, width = w - 24, wrap = true,
      horizontal_alignment = "center",
      visible = function() return not github.available() end,
      text = github.reason, size = theme.size.small, color = ctx.ink.muted,
    },
  }
end

return M
