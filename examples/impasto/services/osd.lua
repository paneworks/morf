-- Every transient system change reaches the island as one event.
--
-- Port of OsdService.qml. Volume, brightness and the charger all render in
-- the same OSD layer, through `island_state.flash(icon, label, progress)`;
-- a new kind of event is a call to `flash`, not a new layer. A level bar
-- is drawn when `progress` is 0 or more; -1 is a message with no level.

local state = require("bar.island_state")
local audio = require("services.audio")
local brightness = require("services.brightness")
local battery = require("services.battery")

local M = {}

-- Bindings run once as they start; flashing then would pop an OSD every
-- time the shell starts, so nothing is sent until this is armed.
local armed = false
morf.timer(600, function() armed = true end, false)

-- A device connecting reports its own volume, which is not the user asking
-- to see it. Ignored for as long as the switch takes.
local suppressed_until = 0
local elapsed = morf.elapsed_timer()

function M.suppress_audio()
  suppressed_until = elapsed:elapsed_ms() + 2000
end

local function open_detail()
  local ok, modules = pcall(require, "services.modules")
  return ok and modules.open_id:get() or ""
end

--- Sends one event to the island.
function M.request(icon, label, progress)
  state.flash(icon, label, progress or -1)
end

-- One frame of debounce: a held volume key fires dozens of changes and the
-- island would stutter through every one.
local DEBOUNCE = 16
local volume_generation = 0

local function volume_changed()
  if not armed or elapsed:elapsed_ms() < suppressed_until then return end
  volume_generation = volume_generation + 1
  local mine = volume_generation
  morf.timer(DEBOUNCE, function()
    if mine ~= volume_generation then return end
    -- The volume detail's own slider already shows it.
    if open_detail() == "volume" then return end
    -- Muted keeps the real level, so raising it while muted shows the value.
    M.request(audio.icon(), audio.volume() .. "%", audio.volume() / 100)
  end, false)
end

local last_volume, last_muted, last_sink
morf.effect("impasto.osd.audio", function()
  local sink = audio.sink()
  local id = sink and sink.id or nil
  local volume, muted = audio.volume(), audio.muted()
  -- A new default device is not the user asking to see its level.
  if last_sink ~= nil and id == last_sink and (volume ~= last_volume or muted ~= last_muted) then
    volume_changed()
  end
  last_sink, last_volume, last_muted = id, volume, muted
end)

-- Not on the reading, which also changes when the focus moves to another
-- screen; on the adjustment, which the service counts.
local brightness_generation = 0
morf.effect("impasto.osd.brightness", function()
  local count = brightness.adjusted:get()
  if count == 0 or not armed then return end
  brightness_generation = brightness_generation + 1
  local mine = brightness_generation
  morf.timer(DEBOUNCE, function()
    if mine ~= brightness_generation then return end
    if open_detail() == "brightness" then return end
    local display = brightness.display(brightness.adjusted_display:get()) or brightness.current()
    if not display then return end
    M.request(display.icon(), display.percent() .. "%", display.percent() / 100)
  end, false)
end)

-- Plugging in cannot repeat fast enough to need a debounce.
local last_charging
morf.effect("impasto.osd.battery", function()
  local charging = battery.charging()
  if last_charging ~= nil and charging ~= last_charging and armed and battery.available() then
    M.request(battery.icon(), charging and "Charging" or "On battery", battery.percent() / 100)
  end
  last_charging = charging
end)

return M
