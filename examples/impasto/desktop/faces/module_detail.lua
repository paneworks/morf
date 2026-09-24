-- A module with no wide face of its own shows its island detail instead.
--
-- Port of the fallback in faces/Wides.qml: the detail at its catalogue size,
-- centred in the widget. The grid's cell was chosen to fit it, so a module
-- works on the desk before it has a face of its own. The detail is the
-- module's own (`modules.providers[id].detail`); the arcade, which has no
-- chip on the bar here, draws GamesModule.qml's card itself.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local modules = require("services.modules")

local C = theme.color
local M = {}

-- GamesModule.qml: the rounds played, a button to the arcade, and the three
-- most played with their bests. Nothing runs here: games need the keyboard.
local function games_detail(w, h)
  local games = require("services.games")
  local inner = w - 28
  local function plural(n, one, many) return n .. " " .. (n == 1 and one or many) end
  local rows = {}
  for i = 1, 3 do
    local function game() return games.ranked()[i] end
    rows[i] = ui.Item {
      width = inner, height = 18,
      visible = function() return game() ~= nil end,
      kit.glyph { x = 0, y = 0, width = 18, height = 18, vertical_alignment = "center", size = 13,
        glyph = function() local g = game() return g and g.icon or "" end,
        color = function() local g = game() return g and games.tint_of(g.id) or C.text() end },
      kit.text { x = 28, y = 0, height = 18, vertical_alignment = "center", elide = "right",
        width = inner - 28 - 70, size = theme.size.small, color = C.text,
        text = function() local g = game() return g and g.name or "" end },
      kit.text { x = inner - 70, y = 0, width = 70, height = 18, vertical_alignment = "center",
        horizontal_alignment = "right", mono = true, size = theme.size.label, color = C.textMuted,
        text = function()
          local g = game()
          return g and (tostring(games.best_of(g.id)) .. " " .. (g.unit or "")) or ""
        end },
    }
  end
  return ui.Item {
    width = w, height = h,
    ui.Column {
      x = 14, y = 12, width = inner, gap = 8,
      ui.Row {
        gap = 12, align = "center", width = inner,
        kit.glyph { glyph = "󰊗", size = 20, width = 24, color = C.accent },
        ui.Column {
          gap = 1,
          kit.text { text = "Games", size = theme.size.medium, weight = 600, color = C.text },
          kit.text {
            width = inner - 24 - 12 - 70 - 12, elide = "right", size = theme.size.small, color = C.textMuted,
            text = function()
              local n = games.total_plays()
              if n == 0 then return "Nothing played yet" end
              return plural(n, "round", "rounds") .. " · " .. plural(#games.catalogue, "game", "games")
            end,
          },
        },
        require("components.pill_button") {
          text = "Play", icon = "󰐊", height = 26, width = 70,
          on_click = function() modules.request_panel("games") end,
        },
      },
      table.unpack(rows),
    },
  }
end

local own = { games = games_detail }

--- Whether `id` has a detail to fall back on.
function M.has(id)
  local provider = modules.providers[id]
  return own[id] ~= nil or (provider ~= nil and type(provider.detail) == "function")
end

function M.build(ctx)
  local entry = modules.entry(ctx.id)
  local w, h = entry.width, entry.height
  local provider = modules.providers[ctx.id]
  local node
  if provider and type(provider.detail) == "function" then
    node = provider.detail()
  elseif own[ctx.id] then
    node = own[ctx.id](w, h)
  end
  return ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Item {
      x = math.floor((ctx.width - w) / 2), y = math.floor((ctx.height - h) / 2),
      width = w, height = h,
      node,
    },
  }
end

return M
