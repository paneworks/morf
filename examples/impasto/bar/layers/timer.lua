-- The countdown on the island: while a timer runs, the island at rest grows
-- a little to carry it beside whatever it shows.
--
-- impasto shows a running timer beside the island (ModuleService's "timer"
-- piece and TimerWidget.qml) in the timer's own hues: blue, then yellow in
-- the last two minutes, red in the last thirty seconds. Here it is an
-- activity on the rest layer itself: the layer registered before this file
-- keeps its width and contents, and the island widens by the chip. A click
-- on the chip holds or resumes the countdown; a right-click cancels it.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local timer = require("services.timer")

local CHIP = 88

local rest = island.layers.modules
if not rest then return end

local function base_size()
  local w, h, pad = rest.size()
  return w, h, pad or 0
end

island.register_layer("modules", {
  size = function()
    local w, h, pad = base_size()
    if timer.running() then w = w + CHIP end
    return w, h, pad
  end,
  build = function(owner)
    local base_width = function() local w = base_size() return w end
    return ui.Item { anchors = { fill = true },
      ui.Item {
        anchors = { left = true, top = true, bottom = true },
        width = base_width,
        rest.build(owner),
      },
      ui.Item {
        anchors = { right = true, top = true, bottom = true, right_margin = 12 },
        width = CHIP - 12,
        visible = timer.running,
        opacity = function() return timer.running() and 1 or 0 end,
        behavior = { opacity = theme.behave("fast") },
        enter = { opacity = 0 },
        ui.Row {
          anchors = { right = true, vertical_center = true },
          gap = 6, align = "center",
          kit.glyph {
            glyph = function() return timer.paused() and "󰏤" or "󰔟" end,
            size = 13, color = timer.tint,
          },
          kit.text {
            text = timer.display, mono = true,
            size = theme.size.small, weight = 600, color = timer.tint,
          },
        },
        ui.MouseArea {
          anchors = { fill = true },
          cursor = "pointer",
          on_clicked = function(button)
            if button == "right" then timer.cancel() else timer.toggle() end
          end,
        },
      },
    }
  end,
})

morf.ipc.timer = function(arg)
  arg = arg or ""
  if arg == "pause" or arg == "resume" or arg == "toggle" then timer.toggle()
  elseif arg == "cancel" then timer.cancel()
  elseif arg ~= "" then
    local ms = timer.parse(arg)
    if ms <= 0 then return "not a duration: " .. arg end
    timer.start(ms, "")
  end
  if not timer.running() then return "idle" end
  return timer.display() .. (timer.paused() and " (held)" or "")
end
