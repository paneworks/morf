-- GitHub: the year's contributions, the streak, and the recent weeks of the
-- wall. Not a bar piece (the catalogue keeps it off the bar); its detail
-- and `has` serve the desktop and the settings.
--
-- Port of GithubModule.qml. The wall is drawn by the desktop face's
-- document, as one SVG image, with as many recent weeks as fit the island;
-- the full year is the widget's.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local github = require("services.github")
local kit = require("components.kit")

local C = theme.color
local M = {}

M.MAX_WEEKS = 30

--- How old the reading is (GithubService.qml:53-63).
function M.age()
  if not github.available() then return "" end
  local at = tonumber(github.now().updated) or 0
  if at <= 0 then return "" end
  morf.clock:get()
  local minutes = math.floor((morf.time.now() - at) / 60)
  if minutes < 2 then return "just now" end
  if minutes < 60 then return minutes .. " min ago" end
  return math.floor(minutes / 60 + 0.5) .. " h ago"
end

local function recent()
  local weeks = github.weeks()
  if #weeks <= M.MAX_WEEKS then return weeks end
  local out = {}
  for i = #weeks - M.MAX_WEEKS + 1, #weeks do out[#out + 1] = weeks[i] end
  return out
end

function M.detail()
  local w, h = modules.open_size("github")
  local inner_w = w - 8 - 28
  local text_w = inner_w - 28 - 13
  local grid_h = (h - 8) - 12 - 12 - 40 - 8
  local total = kit.text { text = function() return github.grouped(github.total()) end,
    mono = true, size = theme.size.medium, weight = 600 }
  return ui.Item {
    anchors = { fill = true },
    ui.Column {
      anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
      gap = 8,
      ui.Row {
        gap = 13, align = "center", height = 40,
        kit.glyph { glyph = "󰊤", size = 28, width = 28, color = C.indicator },
        ui.Column {
          gap = 2, width = text_w,
          ui.Item {
            width = text_w, height = 20,
            kit.text {
              anchors = { left = true, vertical_center = true },
              width = function() return text_w - (total.layout_width or 0) - 10 end, elide = "right",
              text = function()
                local user = (require("services.settings").githubUser or "")
                return user ~= "" and user or "GitHub"
              end,
              size = theme.size.medium, weight = 600,
            },
            ui.Item { anchors = { right = true, vertical_center = true }, height = 20,
              width = function() return total.layout_width or 0 end,
              ui.Item { anchors = { vertical_center = true }, height = 20, total } },
          },
          kit.text {
            width = text_w, elide = "right", size = theme.size.small, color = C.textMuted,
            text = function()
              if not github.available() then return github.reason() end
              local parts = { "contributions this year" }
              if github.streak() > 0 then parts[#parts + 1] = github.streak() .. "-day streak" end
              if M.age() ~= "" then parts[#parts + 1] = M.age() end
              return table.concat(parts, " · ")
            end,
          },
        },
      },
      ui.Image {
        width = inner_w, height = grid_h,
        visible = github.available,
        source = function()
          return require("desktop.faces.github").document(recent(), inner_w, grid_h, 3, 2, 24)
        end,
      },
    },
  }
end

modules.define("github", {
  glyph = function() return "󰊤" end,
  value = function() return github.grouped(github.total()) end,
  -- False until a name is set and a wall comes back, so nothing shows on
  -- an unconfigured machine.
  has = github.available,
  detail = M.detail,
})

return M
