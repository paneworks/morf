-- What the bar and the dashboard read, each as a function a binding can
-- call: the compositor's workspaces and window (lib/hyprland.lua, Hyprland
-- only; elsewhere one workspace and "Desktop"), the network, Bluetooth and
-- power (NetworkManager, BlueZ, UPower over D-Bus; absent services read as
-- off, never started).

local morf = require("morf")
local hyprland = require("lib.hyprland")

local M = {}

local function quiet_connect(name)
  local ok, lib = pcall(require, "lib." .. name)
  if not ok then return nil end
  local ok2, service = pcall(lib.connect)
  if ok2 then return service end
  morf.log("warn", "caelestia: " .. name .. " unavailable: " .. tostring(service))
end

-- ------------------------------------------------------------ workspaces --

M.workspace = {}

function M.workspace.active()
  local id = hyprland.state.active_workspace.id
  if type(id) ~= "number" or id < 1 then return 1 end
  return id
end

function M.workspace.occupied(id)
  local model = hyprland.state.workspaces
  for i = 1, model:len() do
    local row = model:get(i)
    if row.id == id then return (row.windows or 0) > 0 end
  end
  return false
end

function M.workspace.go(id)
  if hyprland.available() then hyprland.dispatch("workspace", tostring(id)) end
end

function M.workspace.step(delta)
  if hyprland.available() then hyprland.dispatch("workspace", (delta > 0 and "r+1" or "r-1")) end
end

-- ---------------------------------------------------------------- window --

M.window = {}

function M.window.title()
  local w = hyprland.state.active_window
  local title = w.title or ""
  if title == "" then title = w.class or "" end
  if title == "" then return "Desktop" end
  return title
end

-- --------------------------------------------------------------- network --

local net = quiet_connect("networkmanager")
local bt = quiet_connect("bluez")
local power = quiet_connect("upower")

-- The services themselves, for the popouts (nil when absent).
M.net, M.bt, M.upower = net, bt, power

M.network = {}

--- A Material Symbols name for the connection.
function M.network.icon()
  if not net or not net.state.available then return "wifi_off" end
  local primary = net.state.primary or {}
  if primary.type == "802-3-ethernet" or (net.state.wired and net.state.wired.connected) then
    return "lan"
  end
  local wifi = net.state.wifi or {}
  if not net.state.wifi_enabled then return "wifi_off" end
  if not wifi.connected then return "signal_wifi_statusbar_not_connected" end
  local s = wifi.strength or 0
  if s >= 80 then return "signal_wifi_4_bar" end
  if s >= 55 then return "network_wifi_3_bar" end
  if s >= 30 then return "network_wifi_2_bar" end
  if s >= 10 then return "network_wifi_1_bar" end
  return "signal_wifi_0_bar"
end

function M.network.name()
  if not net or not net.state.available then return nil end
  local wifi = net.state.wifi or {}
  return wifi.connected and wifi.ssid or nil
end

M.bluetooth = {}

function M.bluetooth.icon()
  if not bt or not bt.state.available or not bt.state.powered then return "bluetooth_disabled" end
  if (bt.state.connected_count or 0) > 0 then return "bluetooth_connected" end
  return "bluetooth"
end

M.power = {}

local PROFILE_ICON = {
  ["power-saver"] = "energy_savings_leaf",
  balanced = "balance",
  performance = "rocket_launch",
}

--- The battery when the machine has one, else the power profile.
function M.power.icon()
  if power and power.state.available then
    local d = power.state.display or {}
    if d.present then
      local p = d.percentage or 0
      if d.charging then return "battery_charging_full" end
      if p >= 95 then return "battery_full" end
      return ("battery_%d_bar"):format(math.max(0, math.min(6, math.floor(p / 100 * 7))))
    end
    local profiles = power.state.profiles or {}
    if profiles.available then return PROFILE_ICON[profiles.active] or "balance" end
  end
  return "balance"
end

-- ----------------------------------------------------------------- media --

-- MPRIS players on the session bus (lib/mpris.lua), or nil without one.
do
  local ok, mpris = pcall(require, "lib.mpris")
  if ok then
    local ok2, media = pcall(mpris.connect)
    if ok2 then M.media = media end
  end
end

--- The active player's state (`{}` when there is none).
function M.player()
  local media = M.media
  if not media or not media.state.available then return {} end
  return media.state.active or {}
end

--- Whether something is loaded in a player.
function M.playing_something()
  local a = M.player()
  return (a.title or "") ~= "" or (a.name or "") ~= ""
end

--- `m:ss` for seconds.
function M.duration(seconds)
  seconds = math.max(0, math.floor(tonumber(seconds) or 0))
  local h, m, s = seconds // 3600, (seconds % 3600) // 60, seconds % 60
  if h > 0 then return ("%d:%02d:%02d"):format(h, m, s) end
  return ("%d:%02d"):format(m, s)
end

-- --------------------------------------------------------------- weather --

local weather

--- The weather where the settings say (or where the address says), read
--- through lib/weather.lua: `{ available, ... }`.
function M.weather()
  if not weather then
    local config = require("config")
    local location = config.get("services.weather_location")
    weather = require("lib.weather").new {
      location = location ~= "" and location or nil,
      units = config.get("services.imperial") and "imperial" or "metric",
    }
  end
  return weather:get()
end

--- A Material Symbols name for a WMO weather code.
function M.weather_symbol(code, is_day)
  code = tonumber(code) or -1
  if code == 0 then return is_day == false and "clear_night" or "clear_day" end
  if code == 1 or code == 2 then return is_day == false and "partly_cloudy_night" or "partly_cloudy_day" end
  if code == 3 then return "cloud" end
  if code == 45 or code == 48 then return "foggy" end
  if (code >= 51 and code <= 57) then return "rainy" end
  if (code >= 61 and code <= 67) or (code >= 80 and code <= 82) then return "rainy" end
  if (code >= 71 and code <= 77) or code == 85 or code == 86 then return "weather_snowy" end
  if code >= 95 then return "thunderstorm" end
  return "cloud"
end

-- ---------------------------------------------------------------- system --

M.hyprland = hyprland

return M
