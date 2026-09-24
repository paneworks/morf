-- System load on a chip: the processor's load as a ring stepping through
-- the indicator hues like the battery's; the detail shows the processor
-- and memory with their recent history. Cores, disks and temperatures are
-- in the stats panel.
--
-- Port of StatsModule.qml.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local controls = require("components.controls")
local stat_card = require("components.stat_card")
local stats = require("services.stats")

local C = theme.color
local M = {}

function M.tint()
  local cpu = stats.cpu()
  if cpu >= 90 then return C.indicatorBad end
  if cpu >= 70 then return C.indicatorWarn end
  return C.indicator
end

function M.widget(size)
  return controls.ring_glyph {
    size = size, thickness = 2.5,
    progress = function() return stats.cpu() / 100 end,
    track_color = C.indicatorDim, fill_color = M.tint,
    glyph = "󰻠", glyph_size = math.floor(size * 0.38 + 0.5), glyph_color = C.indicator,
  }
end

modules.define("stats", {
  glyph = function() return "󰍛" end,
  value = function() return ("%.0f%%"):format(stats.cpu()) end,
  has = function() return true end,
  tint = function()
    local cpu = stats.cpu()
    if cpu >= 90 then return C.indicatorBad end
    if cpu >= 70 then return C.indicatorWarn end
    return C.text()
  end,
  chip = function() return M.widget(theme.capsule_height()) end,
  detail = function()
    local item = modules.entry("stats")
    local w = (item.width - 24 - 10) / 2
    local h = item.height - 24
    return ui.Item {
      anchors = { fill = true },
      stat_card {
        x = 12, y = 12, width = w, height = h, bare = true,
        icon = "󰻠", title = "Processor",
        reading = function() return ("%.0f%%"):format(stats.cpu()) end,
        detail = function() return ("load %.2f"):format(stats.load()[1] or 0) end,
        series = function() return stats.history("cpu") end,
        accent = C.accent,
      },
      stat_card {
        x = 12 + w + 10, y = 12, width = w, height = h, bare = true,
        icon = "󰍛", title = "Memory",
        reading = function() return ("%.0f%%"):format(stats.memory_fraction() * 100) end,
        detail = function() return stats.bytes(stats.memory_used()) .. " of " .. stats.bytes(stats.memory_total()) end,
        series = function() return stats.history("memory") end,
        accent = C.blue,
      },
    }
  end,
})

return M
