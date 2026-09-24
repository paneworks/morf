-- Volume for the default output, and the microphone's mute.
--
-- Port of AudioService.qml over `morf.audio`: the sound server pushes its
-- changes, so nothing polls and the OSD answers on the frame the key was
-- pressed. Every reader here is tracked, so a binding that calls one
-- follows the default device, whichever that is. Above this file the level
-- is spoken in percent.

local act = require("services.act")

local M = {}

local audio = morf.audio

local function sink() return audio.available() and audio.default_sink() or nil end
local function source() return audio.available() and audio.default_source() or nil end

M.sink, M.source = sink, source

function M.ready() return sink() ~= nil end
function M.source_ready() return source() ~= nil end

--- The output level, 0..100 (a boosted device reads above 100).
function M.volume()
  local device = sink()
  return device and math.floor((device.volume or 0) * 100 + 0.5) or 0
end

function M.muted()
  local device = sink()
  return device ~= nil and device.muted == true
end

function M.source_muted()
  local device = source()
  return device ~= nil and device.muted == true
end

function M.source_icon() return M.source_muted() and "󰍭" or "󰍬" end

function M.icon()
  if not M.ready() or M.muted() then return "󰝟" end
  local volume = M.volume()
  if volume == 0 then return "󰕿" end
  return volume < 50 and "󰖀" or "󰕾"
end

--- The output's name, for the detail.
function M.device_name()
  local device = sink()
  return device and (device.description ~= "" and device.description or device.name) or ""
end

-- ---------------------------------------------------------------- actions --

function M.set_volume(percent)
  local device = sink()
  if not device then return end
  percent = math.max(0, math.min(100, math.floor((tonumber(percent) or 0) + 0.5)))
  return act.run("setting the volume to " .. percent .. "%", audio.set_volume, device.id, percent / 100)
end

function M.toggle_mute()
  local device = sink()
  if not device then return end
  return act.run(device.muted and "unmuting the output" or "muting the output",
    audio.set_mute, device.id, not device.muted)
end

function M.toggle_source_mute()
  local device = source()
  if not device then return end
  return act.run(device.muted and "unmuting the microphone" or "muting the microphone",
    audio.set_mute, device.id, not device.muted)
end

return M
