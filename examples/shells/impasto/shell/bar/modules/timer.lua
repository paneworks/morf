-- The countdown: a ring on the bar that drains, and a detail that sets,
-- holds, restarts or stops it.
--
-- Port of TimerModule.qml and TimerWidget.qml. Idle, the ring is empty and
-- the detail sets a countdown with three steppers; running, the ring
-- drains in the timer's own hues (blue, yellow in the last two minutes,
-- red in the last thirty seconds) and the detail holds, restarts or stops
-- it. Clicking the chip opens the detail; it never cancels.
--
-- On the island at rest the countdown is an activity beside the time
-- (`mark` and `figure`, drawn by bar/layers/rest.lua), unless Settings
-- keeps it off the island (`islandActivities`); the glance lists it among
-- its readings through `value`.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local timer = require("services.timer")
local kit = require("components.kit")
local controls = require("components.controls")
local stepper = require("components.stepper")

local C = theme.color
local M = {}

--- TimerWidget: the ring is the time left, a clock glyph in the middle
--- rather than a number, which would be too small to read.
function M.widget(size)
  return controls.ring {
    size = size, thickness = 2.5,
    progress = function() return timer.running() and timer.progress() or 0 end,
    track_color = C.indicatorDim,
    fill_color = timer.tint,
    kit.glyph {
      anchors = { center_in = true }, glyph = "󰥔",
      size = math.floor(size * 0.36 + 0.5),
      color = function()
        return (timer.paused() or not timer.running()) and C.textMuted() or C.indicator
      end,
      behavior = { color = theme.behave("fast") },
    },
  }
end

-- ---------------------------------------------------------------- detail --

local count = 0

--- A large ring with the time inside it, beside two rows: the duration (or
--- what is counting) and the controls.
function M.detail()
  count = count + 1
  -- The duration Start will use, per detail as in the original.
  local hours = morf.signal("impasto.timer.detail.hours." .. count, 0)
  local minutes = morf.signal("impasto.timer.detail.minutes." .. count, 5)
  local seconds = morf.signal("impasto.timer.detail.seconds." .. count, 0)
  local pending = function()
    return (hours:get() * 3600 + minutes:get() * 60 + seconds:get()) * 1000
  end
  -- The countdown Start would begin, shown on the idle ring.
  local preview = function()
    if hours:get() > 0 then return ("%d:%02d:%02d"):format(hours:get(), minutes:get(), seconds:get()) end
    return ("%d:%02d"):format(minutes:get(), seconds:get())
  end

  local w, h = modules.open_size("timer")
  local inner_h = h - 8
  local column_w = (w - 8) - 16 - 14 - 66 - 16
  local running = timer.running

  local field_w = (column_w - 2 * 9) / 3
  local function field(signal, maximum, unit)
    return stepper {
      width = field_w, maximum = maximum, unit = unit,
      value = function() return signal:get() end,
      on_changed = function(value) signal:set(value) end,
    }
  end

  -- Idle: Start and Clear share the row. Running: Hold takes the spare
  -- width and Restart and Stop stay square, icon only.
  local half = (column_w - 9) / 2
  local idle_row = ui.Row {
    gap = 9, align = "center",
    visible = function() return not running() end,
    controls.pill {
      width = half, height = 30, text = "Start", icon = "󰐊", active = true,
      enabled = function() return pending() > 0 end,
      on_click = function() if pending() > 0 then timer.start(pending(), "") end end,
    },
    controls.pill {
      width = half, height = 30, text = "Clear", icon = "󰜉",
      on_click = function() hours:set(0) minutes:set(0) seconds:set(0) end,
    },
  }
  local running_row = ui.Row {
    gap = 9, align = "center",
    visible = running,
    controls.pill {
      width = column_w - 2 * 46 - 2 * 9, height = 30,
      text = function() return timer.paused() and "Resume" or "Hold" end,
      icon = function() return timer.paused() and "󰐊" or "󰏤" end,
      active = timer.paused,
      on_click = function() timer.toggle() end,
    },
    controls.pill { width = 46, height = 30, icon = "󰜉", on_click = function() timer.restart() end },
    controls.pill { width = 46, height = 30, icon = "󰓛", on_click = function() timer.cancel() end },
  }

  return ui.Item {
    anchors = { fill = true },
    ui.Row {
      x = 16, y = function() return (inner_h - 66) / 2 end,
      gap = 16, align = "center",
      controls.ring {
        size = 66, thickness = 4,
        progress = function() return running() and timer.progress() or 0 end,
        track_color = C.indicatorDim,
        fill_color = function() return running() and timer.tint() or C.indicatorDim end,
        kit.text {
          anchors = { center_in = true },
          text = function() return running() and timer.display() or preview() end,
          mono = true, size = theme.size.medium, weight = 600,
          color = function() return timer.paused() and C.textMuted() or C.indicator end,
          behavior = { color = theme.behave("fast") },
        },
      },
      ui.Column {
        gap = 10, width = column_w,
        -- Idle, this row sets the duration; running, it describes the
        -- countdown. Same height either way.
        ui.Item {
          width = column_w, height = 28,
          ui.Row {
            anchors = { left = true, vertical_center = true }, gap = 9,
            visible = function() return not running() end,
            field(hours, 23, "h"), field(minutes, 59, "m"), field(seconds, 59, "s"),
          },
          ui.Column {
            anchors = { left = true, vertical_center = true }, gap = 1,
            visible = running,
            kit.text {
              text = function() local l = timer.label() return l ~= "" and l or "Countdown" end,
              width = column_w, elide = "right", size = theme.size.small, weight = 600,
            },
            kit.text {
              text = function() return timer.paused() and "Held" or "Running" end,
              size = theme.size.label, color = C.textMuted,
            },
          },
        },
        ui.Item { width = column_w, height = 30, idle_row, running_row },
      },
    },
  }
end

-- -------------------------------------------------------------- activity --

--- The countdown beside the time (IslandRest.qml:152-176).
function M.mark()
  return controls.ring {
    size = 16, thickness = 2,
    progress = timer.progress,
    track_color = C.indicatorDim, fill_color = timer.tint,
  }
end

function M.figure()
  return kit.text {
    text = timer.display, mono = true, size = theme.size.small, weight = 600,
    color = function() return timer.paused() and C.textMuted() or C.text() end,
  }
end

modules.define("timer", {
  glyph = function() return "󰔛" end,
  -- Never empty: an idle countdown reads "0:00".
  value = function() return timer.running() and timer.display() or "0:00" end,
  tint = function() return timer.running() and timer.tint() or C.text() end,
  has = function() return true end,
  runs = timer.running,
  chip = function() return M.widget(theme.capsule_height()) end,
  detail = M.detail,
  mark = M.mark,
  figure = M.figure,
})

-- `morf ipc call timer 25m` starts one; `pause`, `resume`, `toggle`,
-- `restart` and `cancel` drive it; no argument reads it.
morf.ipc.timer = function(arg)
  arg = arg or ""
  if arg == "pause" or arg == "resume" or arg == "toggle" then timer.toggle()
  elseif arg == "restart" then timer.restart()
  elseif arg == "cancel" then timer.cancel()
  elseif arg ~= "" then
    local ms = timer.parse(arg)
    if ms <= 0 then return "not a duration: " .. arg end
    timer.start(ms, "")
  end
  if not timer.running() then return "idle" end
  return timer.display() .. (timer.paused() and " (held)" or "")
end

return M
