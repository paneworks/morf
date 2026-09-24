-- Games: recent games with their bests and a button that opens the arcade.
-- Games need keyboard focus, which the resting island does not hold, so
-- none runs here. On the bar "games" is the arcade's door button; this is
-- the module the desktop and the settings know.
--
-- Port of GamesModule.qml.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local games = require("services.games")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local M = {}

local function plural(n, one, many) return n .. " " .. (n == 1 and one or many) end

--- "12 rounds · 11 games", or "Nothing played yet".
function M.summary()
  local plays = games.total_plays()
  if plays == 0 then return "Nothing played yet" end
  return plural(plays, "round", "rounds") .. " · " .. plural(#games.catalogue, "game", "games")
end

--- One of the last three played: its icon in its tint, its name, its best.
local function row(index, width)
  local item = function() return games.ranked()[index] end
  local best = kit.text {
    anchors = { right = true, vertical_center = true },
    text = function()
      local g = item()
      return g and (games.best_of(g.id) .. " " .. g.unit) or ""
    end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  return ui.Item {
    width = width, height = 18,
    visible = function() return item() ~= nil end,
    kit.glyph {
      anchors = { vertical_center = true }, width = 18,
      glyph = function() local g = item() return g and g.icon or "" end, size = 13,
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

function M.detail()
  local w = modules.entry("games").width
  local inner = w - 8 - 28
  local play = controls.pill {
    text = "Play", icon = "󰐊",
    on_click = function() modules.request_panel("games") end,
  }
  return ui.Item {
    anchors = { fill = true },
    ui.Column {
      anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
      gap = 8,
      ui.Item {
        width = inner, height = 36,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
          kit.glyph { glyph = "󰊗", size = 20, color = C.accent },
          ui.Column {
            gap = 1,
            kit.text { text = "Games", size = theme.size.medium, weight = 600 },
            kit.text { text = M.summary, size = theme.size.small, color = C.textMuted },
          },
        },
        ui.Item { anchors = { right = true, vertical_center = true }, height = 28,
          width = function() return play.layout_width or 60 end, play },
      },
      -- The last three, most recent first.
      row(1, inner), row(2, inner), row(3, inner),
    },
  }
end

modules.define("games", {
  glyph = function() return "󰊗" end,
  value = function() return tostring(games.total_plays()) end,
  has = function() return true end,
  detail = M.detail,
})

return M
