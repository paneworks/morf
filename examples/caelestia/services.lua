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

-- ---------------------------------------------------------------- system --

M.hyprland = hyprland

return M
