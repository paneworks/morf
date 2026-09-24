-- The default Bluetooth adapter, flattened for the control centre.
--
-- Port of BluetoothService.qml over `lib.bluez`, which mirrors BlueZ's
-- object tree from its own signals. The adapter switch, discovery and a
-- device's connection are D-Bus calls, sent only from clicks and through
-- `services.act`.

local bluez = require("lib.bluez")
local act = require("services.act")

local M = {}

local bt = bluez.connect()
M.lib = bt
M.state = bt.state
local s = bt.state

local demo_name -- a made-up device, for testing (below)

function M.available()
  if demo_name and demo_name:get() ~= "" then return true end
  return s.available and s.adapter ~= ""
end
function M.enabled() return M.available() and s.powered end
function M.discovering() return M.available() and s.discovering end

--- The rows as plain tables, sorted by the library (connected, then paired,
--- then named, then signal). A binding that calls this follows the adapter
--- and the connected count; the rows themselves are read as they are.
function M.all_devices()
  local _ = s.connected_count, s.powered, s.discovering, s.adapter
  return (bt.snapshot() or {}).devices or {}
end

function M.connected_devices()
  local out = {}
  for _, device in ipairs(M.all_devices()) do
    if device.connected then out[#out + 1] = device end
  end
  return out
end

function M.connected_count() return s.connected_count end

--- A single device is named; several are counted.
-- TESTING ONLY: `morf ipc call bluetooth_demo <name>` names a made-up
-- connected device for the bar's figure (nothing is sent to BlueZ);
-- `bluetooth_demo` with nothing puts the adapter's own summary back.
demo_name = morf.signal("impasto.bluetooth.demo", "")
morf.ipc.bluetooth_demo = function(name)
  demo_name:set(name or "")
  return demo_name:get()
end

function M.summary()
  if demo_name:get() ~= "" then return demo_name:get() end
  if not M.available() then return "Unavailable" end
  if not s.powered then return "Off" end
  local count = s.connected_count
  if count == 0 then return "No devices" end
  if count == 1 then
    local one = M.connected_devices()[1]
    return one and (one.alias ~= "" and one.alias or one.name) or "1 device"
  end
  return count .. " devices"
end

function M.icon()
  if not M.enabled() then return "󰂲" end
  return s.connected_count > 0 and "󰂱" or "󰂯"
end

--- Whether the device advertised a name of its own, rather than BlueZ's
--- alias falling back to the address.
function M.is_named(device) return device.named == true end

--- Named, paired or connected: the list shown first.
function M.listed(device) return M.is_named(device) or device.paired or device.connected end

function M.device_icon(device)
  local icon = device.icon or ""
  if icon == "audio-headset" or icon == "audio-headphones" then return "󰋋" end
  if icon == "audio-card" then return "󰓃" end
  if icon == "input-mouse" then return "󰍽" end
  if icon == "input-keyboard" then return "󰌌" end
  if icon == "phone" then return "󰄜" end
  if icon == "computer" then return "󰟀" end
  return "󰂯"
end

--- The device list model, for a Repeater.
M.devices = s.devices

-- ---------------------------------------------------------------- actions --

function M.connect_device(device)
  if not device then return end
  if device.connected then
    return act.run("disconnecting " .. tostring(device.alias), bt.disconnect, device.path)
  end
  return act.run("connecting " .. tostring(device.alias), bt.connect, device.path)
end

function M.set_discovering(on)
  if not M.available() then return end
  if on then return act.run("starting Bluetooth discovery", bt.start_discovery) end
  if s.discovering then return act.run("stopping Bluetooth discovery", bt.stop_discovery) end
end

function M.toggle()
  if not M.available() then return end
  return act.run(s.powered and "turning Bluetooth off" or "turning Bluetooth on", bt.set_powered, not s.powered)
end

function M.set_powered(on)
  if not M.available() or s.powered == on then return end
  return act.run(on and "turning Bluetooth on" or "turning Bluetooth off", bt.set_powered, on)
end

return M
