-- The on-screen display: a slim drawer on the right edge with two upright
-- sliders, the output's volume and the screen's brightness. It opens when
-- either changes (from anywhere: keys, another program) and shuts two
-- seconds after the last change unless the pointer is on it; dragging or
-- scrolling a slider sets it. Over IPC: `drawers toggle osd`, `osd`.
--
-- The volume is `morf.audio`'s default output; the brightness is the
-- backlight lib/sysinfo.lua reads (and writes when it may). Without either
-- the slider rests at zero, as the reference's do in the sandbox.
--
-- Measured off the reference at 1920x1080: 52 x 346, centred on the right
-- edge; sliders 30 wide and 152 tall, 13 px in, 12 apart; a 30 px handle.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local drawer = require("drawer")

local C = theme.color
local M = {}

local WIDTH, PAD = 52, 11
local SLIDER_W, SLIDER_H, GAP = 30, 152, 12
local HEIGHT = 2 * 17 + 2 * SLIDER_H + GAP
local HOLD = 2000

local audio = morf.audio
local sysinfo = require("lib.sysinfo")

local function volume()
  local ok, sink = pcall(function() return audio and audio.available() and audio.default_sink() end)
  if not ok or not sink then return 0, true, false end
  return sink.volume or 0, sink.muted, true
end

local function brightness()
  local ok, b = pcall(sysinfo.backlight)
  if not ok or not b or not b.percent then return 0, false end
  return b.percent / 100, true
end

local function set_volume(v)
  local ok, sink = pcall(function() return audio.default_sink() end)
  if ok and sink then pcall(audio.set_volume, sink.id, math.max(0, math.min(1, v))) end
end

local function set_brightness(v)
  pcall(sysinfo.set_brightness, math.max(1, math.min(100, v * 100)))
end

local function slider(id, value, set, icon)
  local held = false
  local function at(y) return 1 - math.max(0, math.min(1, (y - SLIDER_W / 2) / (SLIDER_H - SLIDER_W))) end
  local motion = { duration = theme.duration.small, easing = theme.ease.standard_decel }
  local function top() return (SLIDER_H - SLIDER_W) * (1 - value()) end
  return ui.MouseArea {
    id = id, width = SLIDER_W, height = SLIDER_H, cursor = "pointer",
    on_pressed = function(_, _, _, y) held = true set(at(y)) end,
    on_released = function() held = false end,
    on_dragged = function(_, _, _, _, _, y) if held then set(at(y)) end end,
    on_wheel = function(_, _, _, _, _, step_y)
      if step_y ~= 0 then set(value() + (step_y > 0 and -0.05 or 0.05)) end
    end,
    ui.Rect {
      anchors = { fill = true }, radius = SLIDER_W / 2,
      color = function() return C.surfaceContainer end,
    },
    -- The level, up from the bottom to the handle.
    ui.Rect {
      x = 0, width = SLIDER_W, radius = SLIDER_W / 2,
      y = function() return top() end,
      height = function() return SLIDER_H - top() end,
      color = function() return C.primary end,
      opacity = function() return value() > 0.01 and 1 or 0 end,
      behavior = { y = motion, height = motion },
    },
    ui.Rect {
      id = id .. "-handle",
      x = 0, width = SLIDER_W, height = SLIDER_W, radius = SLIDER_W / 2,
      y = top,
      color = function() return C.inverseSurface end,
      behavior = { y = motion },
      kit.icon(icon, 18, function() return C.inverseOnSurface end, { anchors = { center_in = true } }),
    },
  }
end

local volume_slider = slider("osd-volume", function() return (volume()) end, set_volume, function()
  local v, muted = volume()
  if muted or v <= 0 then return "volume_mute" end
  if v < 0.5 then return "volume_down" end
  return "volume_up"
end)
local brightness_slider = slider("osd-brightness", brightness, set_brightness, function()
  local b = brightness()
  if b < 0.34 then return "brightness_low" end
  if b < 0.67 then return "brightness_medium" end
  return "brightness_high"
end)

local content = ui.Item {
  anchors = { fill = true },
  ui.Column { x = PAD, y = 17, gap = GAP, volume_slider, brightness_slider },
}

M.drawer = drawer.new {
  name = "osd",
  edge = "right",
  width = WIDTH,
  height = HEIGHT,
  content = content,
}

-- Opens on a change, shuts after a while unless the pointer is on it.
local hide
local function over() return volume_slider.hovered or brightness_slider.hovered end
local function linger()
  if hide then hide:cancel() end
  hide = morf.timer(HOLD, function()
    hide = nil
    if over() then linger() else M.drawer.set(false) end
  end, false)
end

--- Shows the OSD for a moment.
function M.flash()
  M.drawer.set(true)
  linger()
end

-- A change is a reading that moved from one known value to another: the
-- first reading of either, when its service answers, is not one.
local seen_volume, seen_brightness
morf.effect("caelestia.osd.follow", function()
  local v, muted, known_v = volume()
  local b, known_b = brightness()
  local changed = false
  if known_v then
    local now = ("%.3f/%s"):format(v, tostring(muted))
    if seen_volume and now ~= seen_volume then changed = true end
    seen_volume = now
  end
  if known_b then
    local now = ("%.3f"):format(b)
    if seen_brightness and now ~= seen_brightness then changed = true end
    seen_brightness = now
  end
  if changed then M.flash() end
end)

morf.effect("caelestia.osd.hover", function()
  if over() and hide then linger() end
end)

return M
