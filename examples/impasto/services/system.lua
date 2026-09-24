-- The radios and the power profile, for the toggles.
--
-- Port of SystemService.qml. The original polled a Python helper every four
-- seconds for the Wi-Fi and Bluetooth radios and the power profile, because
-- it had no push interface to them. Here each comes from its own service
-- over D-Bus -- NetworkManager, BlueZ and power-profiles-daemon (through
-- `lib.upower`) -- so nothing polls, and `subscribe`/`release` only keep
-- the original's call sites.

local network = require("services.network")
local bluetooth = require("services.bluetooth")
local battery = require("services.battery")
local act = require("services.act")

local M = {}

local profiles = battery.state.profiles

function M.wifi() return network.radio_on() end
function M.bluetooth() return bluetooth.enabled() end
function M.power_profile() return profiles.available and (profiles.active or "") or "" end
function M.ready() return profiles.available == true end
function M.performance_mode() return M.power_profile() == "performance" end

function M.subscribe() end
function M.release() end
function M.refresh() battery.lib.refresh() end

-- The cycle the tile steps through; "performance" only where the machine
-- offers it.
local function offered(name)
  local list = profiles.list
  if not list or not list.len then return name ~= "performance" end
  for index = 1, list:len() do
    local row = list:get(index)
    if row and row.name == name then return true end
  end
  return false
end

--- `target`: "wifi", "bluetooth" or "power-profile".
function M.toggle(target)
  if target == "wifi" then return network.toggle_wifi() end
  if target == "bluetooth" then
    require("services.osd").suppress_audio()
    return bluetooth.toggle()
  end
  if target == "power-profile" then
    if not M.ready() then return end
    local next = M.performance_mode() and "balanced"
      or (offered("performance") and "performance" or "power-saver")
    return act.run("switching the power profile to " .. next, battery.lib.set_profile, next)
  end
end

return M
