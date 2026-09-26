-- A recording on the island at rest: a red mark that breathes, and the
-- time it has run. A click on either stops it (bar/layers/rest.lua calls
-- `stop`), since nothing else on screen can.
--
-- Port of IslandRest.qml's recorder mark and figure. `indicatorBad`, not
-- the palette's red: warnings do not follow the palette.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local kit = require("components.kit")
local recorder = require("services.recorder")

local C = theme.color

modules.define("recorder", {
  runs = function() return recorder.recording() end,
  has = function() return recorder.available() end,
  glyph = function() return "󰑊" end,
  value = function() return recorder.display() end,
  tint = function() return C.indicatorBad end,
  stop = function() recorder.stop() end,
  -- `hovered()`, when the rest layer passes it, says the pointer is on the
  -- side: the dot stops breathing and squares off into a stop button.
  mark = function(hovered)
    local over = function() return hovered ~= nil and hovered() or false end
    return ui.Rect {
      width = function() return over() and 9 or 8 end,
      height = function() return over() and 9 or 8 end,
      radius = function() return over() and 2 or 4 end,
      color = C.indicatorBad,
      behavior = { radius = theme.behave("fast") },
      loop = function()
        if over() then return nil end
        return { opacity = { from = 1, to = 0.4, duration = 900, easing = "in_out_sine", alternate = true } }
      end,
    }
  end,
  figure = function()
    return kit.text { text = recorder.display, mono = true, size = theme.size.small, weight = 600 }
  end,
})
