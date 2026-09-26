-- The lock's clock: the date above, then the time, stacked or on one line.
--
-- Port of LockClock.qml. Hours over minutes with the minutes softer, or the
-- two on one line. Shared with Settings' preview, which passes a fixed time.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")

local C = theme.color

local DATE_SIZE = 26
local INLINE_SIZE = 212
local STACKED_SIZE = 300
-- A line is 1.2 of the size here; stacked lines overlap by their empty
-- leading, so the figures sit a gap apart rather than a line apart.
local STACKED_GAP = 18
local STACKED_STEP = math.floor(STACKED_SIZE * 1.2 + 0.5) - math.floor(STACKED_SIZE * 0.48 + 0.5) + STACKED_GAP
local STACKED_LIFT = math.floor(STACKED_SIZE * 0.24 + 0.5)

--- `style()` "stacked" | "inline" (Settings by default); `at` a fixed time
--- (seconds since the epoch) for a preview.
return function(values)
  values = values or {}
  local style = values.style or function() return settings.lockClock end
  local stacked = function() return style() ~= "inline" end

  local function format(pattern)
    if values.at then return morf.time.format(pattern, values.at) end
    if pattern:find("%%[STr]") then morf.clock:get() else morf.minute_clock:get() end
    return morf.time.format(pattern)
  end
  -- The clock's own format, split on its separator for the stacked form.
  local time = function() return format(settings.clockFormat) end
  local part = function(first)
    local text = time()
    local head, rest = text:match("^([^:]*):(.*)$")
    if not head then return first and text or "" end
    return first and head or rest
  end

  local date = kit.text {
    text = function() return format("%A, %-d %B") end,
    font_family = function() return theme.font_display() end,
    size = DATE_SIZE, weight = 600, opacity = 0.92,
  }

  local function figure(text, opacity)
    return kit.text {
      text = text,
      font_family = function() return theme.font_display() end,
      size = STACKED_SIZE, weight = 700, opacity = opacity,
      letter_spacing = -math.floor(STACKED_SIZE * 0.04 + 0.5),
    }
  end
  local hours = figure(function() return part(true) end, 1)
  local minutes = figure(function() return part(false) end, 0.55)
  local inline = kit.text {
    text = time,
    font_family = function() return theme.font_display() end,
    size = INLINE_SIZE, weight = 600,
    letter_spacing = -math.floor(INLINE_SIZE * 0.033 + 0.5),
  }

  local function w(node) return node.layout_width or 0 end
  local function h(node) return node.layout_height or 0 end
  -- Everything is centred on the widest line.
  local width = function()
    if stacked() then return math.max(w(date), w(hours), w(minutes)) end
    return math.max(w(date), w(inline))
  end
  local function centred(node) return function() return (width() - w(node)) / 2 end end

  local hours_y = function() return h(date) + STACKED_GAP - STACKED_LIFT end
  local minutes_y = function() return hours_y() + STACKED_STEP - STACKED_SIZE * 1.2 + h(hours) end
  local inline_y = function() return h(date) - math.floor(INLINE_SIZE * 0.07 + 0.5) end

  local out = {
    width = width,
    height = function()
      if stacked() then return minutes_y() + h(minutes) - STACKED_LIFT end
      return inline_y() + h(inline)
    end,
    ui.Item { x = centred(date), date },
    ui.Item { x = centred(hours), y = hours_y, visible = stacked, hours },
    ui.Item { x = centred(minutes), y = minutes_y, visible = stacked, minutes },
    ui.Item { x = centred(inline), y = inline_y, visible = function() return not stacked() end, inline },
  }
  for key, value in pairs(values) do
    if key ~= "style" and key ~= "at" then out[key] = value end
  end
  return ui.Item(out)
end
