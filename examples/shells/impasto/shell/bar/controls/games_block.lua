-- The control centre's arcade block: GamesBlock.qml.
--
-- One row: the mark, the last game's best and Play. Two rows: the last
-- three games under it.

local ui = require("morf.ui")
local theme = require("theme")
local games = require("services.games")
local kit = require("components.kit")
local controls = require("components.controls")
local tasks_block = require("bar.controls.tasks_block")

local C = theme.color
local M = {}

local function last_line()
  local last = games.last_played()
  local entry = games.entry(last)
  if not entry then return "Nothing played yet" end
  return entry.name .. " · best " .. games.best_of(last)
end

local function row(index, width)
  local item = function() return games.ranked()[index] end
  local best = kit.text {
    anchors = { right = true, vertical_center = true },
    text = function() local g = item() return g and tostring(games.best_of(g.id)) or "" end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  return ui.Item {
    width = width, height = 18,
    visible = function() return item() ~= nil end,
    kit.glyph {
      anchors = { vertical_center = true }, width = 18, size = 13,
      glyph = function() local g = item() return g and g.icon or "" end,
      color = function() local g = item() return g and games.tint_of(g.id) or C.text() end,
    },
    kit.text {
      x = 28, anchors = { vertical_center = true },
      width = function() return width - 28 - (best.layout_width or 0) - 10 end, elide = "right",
      text = function() local g = item() return g and g.name or "" end,
      size = theme.size.small,
    },
    best,
  }
end

function M.build(o)
  local w = o.width - 28
  local play = controls.pill { text = "Play", height = 26, on_click = function() o.on_panel("games") end }
  local column = {
    gap = 8,
    ui.Item {
      width = w, height = 34,
      ui.Row {
        anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
        tasks_block.badge("󰊗", C.accent),
        ui.Column {
          gap = 1,
          kit.text { text = "Games", size = theme.size.small, weight = 600 },
          kit.text {
            width = function() return w - 34 - 12 - (play.layout_width or 56) - 12 end, elide = "right",
            text = last_line, size = theme.size.label, color = C.textMuted,
          },
        },
      },
      ui.Item { anchors = { right = true, vertical_center = true }, height = 26,
        width = function() return play.layout_width or 56 end, play },
    },
  }
  if o.rows >= 2 then
    for index = 1, 3 do column[#column + 1] = row(index, w) end
  end
  return controls.card {
    width = o.width, height = o.height,
    ui.Item {
      width = w, height = o.height - 28,
      ui.Column(column),
    },
  }
end

return M
