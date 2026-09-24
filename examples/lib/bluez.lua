-- Bluetooth, from BlueZ, as reactive state a shell can bind to.
--
-- `org.bluez` publishes everything it knows as one ObjectManager tree:
-- adapters at `/org/bluez/hciN`, devices beneath them, batteries as a second
-- interface on a device's own path, and a thicket of GATT services nobody
-- draws. This reads that tree into two lists — adapters and devices — and
-- keeps them current. The engine's D-Bus client does the talking; what a
-- "device" is to a panel is decided here.
--
--   local bluez = require("lib.bluez")
--   local bt = bluez.connect()
--   ui.Switch { checked = function() return bt.state.powered end,
--               on_toggled = function(on) bt.set_powered(on) end }
--   ui.Repeater { model = bt.state.devices, ... }
--
-- Staying current. Objects arriving and leaving come from `InterfacesAdded`
-- and `InterfacesRemoved`; property changes come from `PropertiesChanged` on
-- each adapter and each *paired* device. Strangers found by a scan are not
-- subscribed one by one — every one is a match rule on the bus that can
-- never be removed, and a scan in a train carriage finds hundreds — so while
-- discovery runs, the tree is re-read on a timer instead, which is how their
-- names and signal strengths move.
--
-- Actions and waiting. `Connect` and `Pair` do not answer until the radio
-- work is done, seconds later, and the engine's calls block the thread that
-- draws. So actions wait a short while (`action_timeout_ms`, 2500) and then
-- stop waiting: BlueZ carries on regardless, the proxy's connection stays
-- open so nothing is cancelled, and the outcome arrives as the device's
-- `Connected` or `Paired` changing. Such a call returns `true, "pending"`.
--
-- Pairing a device that asks for a PIN needs an `org.bluez.Agent1`, which
-- this does not register; devices that pair without one ("just works") pair.

local morf = require("morf")
local dbus_client = require("lib.dbus_client")

local bluez = {}

local NAME = "org.bluez"
local ADAPTER = "org.bluez.Adapter1"
local DEVICE = "org.bluez.Device1"
local BATTERY = "org.bluez.Battery1"
local OBJECT_MANAGER = "org.freedesktop.DBus.ObjectManager"

local o = dbus_client.o

local function is_timeout(err)
  err = tostring(err or "")
  return err:find("NoReply", 1, true) ~= nil or err:find("Timeout", 1, true) ~= nil
    or err:find("timed out", 1, true) ~= nil
end

--- Starts watching. Call it while the configuration loads.
---
--- Options: `bus` ("system"), `name` ("org.bluez"), `dbus` (test seam),
--- `adapter` (a path; the first adapter otherwise), `action_timeout_ms`
--- (2500), `discovery_poll_ms` (2000), `debounce_ms` (60).
function bluez.connect(options)
  options = options or {}
  local name = options.name or NAME
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or "system" })
  local action_timeout = options.action_timeout_ms or 2500

  local state = morf.state({
    available = false,
    -- The default adapter's, flattened: the switch in a panel is about one
    -- radio, and making every binding dig through a list for it is noise.
    adapter = "",
    address = "",
    powered = false,
    discovering = false,
    discoverable = false,
    pairable = false,
    connected_count = 0,
    adapters = {},
    devices = {},
  })

  local bt = { state = state }
  local objects = {} -- path -> interface -> properties: the mirror
  local watched = {}
  local rows = { adapters = {}, devices = {} }
  local present = false
  local poll

  local function default_adapter()
    local chosen
    for path, interfaces in pairs(objects) do
      if interfaces[ADAPTER] and (options.adapter == nil or options.adapter == path) then
        if not chosen or path < chosen then chosen = path end
      end
    end
    return chosen
  end

  local function watch(path)
    if watched[path] then return end
    watched[path] = true
    client.on_properties(name, path, function(interface, changed, invalidated)
      local held = objects[path] and objects[path][interface]
      if held then
        dbus_client.merge(held, changed, invalidated)
        bt.publish()
      end
    end)
  end

  local publish_now

  --- Rebuilds the rows from the mirror. Cheap: no bus traffic.
  function publish_now()
    local adapters, devices = {}, {}
    local connected = 0
    for path, interfaces in pairs(objects) do
      local adapter = interfaces[ADAPTER]
      local device = interfaces[DEVICE]
      if adapter then
        watch(path)
        adapters[#adapters + 1] = {
          path = path,
          name = adapter.Alias or adapter.Name or "",
          address = adapter.Address or "",
          powered = adapter.Powered == true,
          discovering = adapter.Discovering == true,
          discoverable = adapter.Discoverable == true,
          pairable = adapter.Pairable == true,
        }
      elseif device then
        if device.Paired == true or device.Connected == true then watch(path) end
        local battery = interfaces[BATTERY]
        local percentage = battery and battery.Percentage
        local row = {
          path = path,
          adapter = device.Adapter or "",
          address = device.Address or "",
          name = device.Name or "",
          alias = device.Alias or device.Name or device.Address or "",
          named = device.Name ~= nil and device.Name ~= "",
          icon = device.Icon or "",
          paired = device.Paired == true,
          bonded = device.Bonded == true,
          trusted = device.Trusted == true,
          blocked = device.Blocked == true,
          connected = device.Connected == true,
          -- Only present while a scan hears the device; 0 otherwise.
          rssi = device.RSSI or 0,
          in_range = device.RSSI ~= nil,
          battery = type(percentage) == "number" and percentage or -1,
          has_battery = type(percentage) == "number",
        }
        if row.connected then connected = connected + 1 end
        devices[#devices + 1] = row
      end
    end
    table.sort(adapters, function(a, b) return a.path < b.path end)
    -- Connected, then paired, then things with names, then the rest by
    -- signal: the order a person scans the list in.
    table.sort(devices, function(a, b)
      if a.connected ~= b.connected then return a.connected end
      if a.paired ~= b.paired then return a.paired end
      if a.named ~= b.named then return a.named end
      if a.rssi ~= b.rssi then return a.rssi > b.rssi end
      return a.alias < b.alias
    end)
    rows.adapters, rows.devices = adapters, devices

    local chosen = default_adapter()
    local adapter = chosen and objects[chosen][ADAPTER] or {}
    state.available = present
    state.adapter = chosen or ""
    state.address = adapter.Address or ""
    state.powered = adapter.Powered == true
    state.discovering = adapter.Discovering == true
    state.discoverable = adapter.Discoverable == true
    state.pairable = adapter.Pairable == true
    state.connected_count = connected
    state.adapters:replace(adapters, "path")
    state.devices:replace(devices, "path")

    -- A scan in progress is when strangers move; see the header.
    if state.discovering and not poll then
      poll = morf.timer(options.discovery_poll_ms or 2000, function()
        if not state.discovering then
          poll:cancel()
          poll = nil
          return
        end
        bt.refresh()
      end)
    end
  end

  bt.publish = dbus_client.debounce(options.debounce_ms or 60, function() publish_now() end)

  --- Re-reads the whole tree.
  function bt.refresh()
    -- Only a running bluetoothd is read: reading an activatable name starts
    -- it, and a bluetoothd started that way may power the radio on.
    local tree = client.has_owner(name) and client.managed_objects(name, "/") or nil
    present = tree ~= nil
    objects = {}
    if tree then
      for path, interfaces in pairs(tree) do
        if interfaces[ADAPTER] or interfaces[DEVICE] then objects[path] = interfaces end
      end
    end
    publish_now()
  end

  local function find_device(which)
    if type(which) == "table" then which = which.path end
    for _, row in ipairs(rows.devices) do
      if row.path == which or row.address == which then return row end
    end
  end

  local function device_call(which, method)
    local row = find_device(which)
    if not row then return nil, "no device " .. tostring(which) end
    local ok, err = client.call(name, row.path, DEVICE, method, nil, action_timeout)
    if ok then return true end
    if is_timeout(err) then return true, "pending" end
    return nil, err
  end

  local function adapter_path(which)
    if type(which) == "table" then return which.path end
    return which or state.adapter
  end

  --- Whether BlueZ is on the bus.
  function bt.available() return state.available end

  --- The current rows as plain tables: `adapters`, `devices`.
  function bt.snapshot() return rows end

  --- Turns the default adapter (or the named one) on or off.
  function bt.set_powered(on, adapter)
    local path = adapter_path(adapter)
    if path == "" then return nil, "no adapter" end
    return client.set(name, path, ADAPTER, "Powered", on == true, action_timeout)
  end

  function bt.set_discoverable(on, adapter)
    local path = adapter_path(adapter)
    if path == "" then return nil, "no adapter" end
    return client.set(name, path, ADAPTER, "Discoverable", on == true, action_timeout)
  end

  function bt.set_pairable(on, adapter)
    local path = adapter_path(adapter)
    if path == "" then return nil, "no adapter" end
    return client.set(name, path, ADAPTER, "Pairable", on == true, action_timeout)
  end

  --- Starts a scan. BlueZ ties a scan to the connection that asked for it
  --- and stops it when that connection leaves; the proxy is cached, so the
  --- scan lasts until `stop_discovery` (from the same proxy) or the shell
  --- exits.
  function bt.start_discovery(adapter)
    local path = adapter_path(adapter)
    if path == "" then return nil, "no adapter" end
    local ok, err = client.call(name, path, ADAPTER, "StartDiscovery", nil, action_timeout)
    return ok and true or nil, err
  end

  function bt.stop_discovery(adapter)
    local path = adapter_path(adapter)
    if path == "" then return nil, "no adapter" end
    local ok, err = client.call(name, path, ADAPTER, "StopDiscovery", nil, action_timeout)
    return ok and true or nil, err
  end

  --- Connects a device (a row, a path, or an address). See the header for
  --- why this may return `true, "pending"`.
  function bt.connect(device) return device_call(device, "Connect") end
  function bt.disconnect(device) return device_call(device, "Disconnect") end
  function bt.pair(device) return device_call(device, "Pair") end
  function bt.cancel_pairing(device) return device_call(device, "CancelPairing") end

  --- Marks a device trusted (or not), which lets it connect on its own.
  function bt.trust(device, on)
    local row = find_device(device)
    if not row then return nil, "no device " .. tostring(device) end
    return client.set(name, row.path, DEVICE, "Trusted", on ~= false, action_timeout)
  end

  --- Unpairs and forgets a device.
  function bt.remove(device)
    local row = find_device(device)
    if not row then return nil, "no device " .. tostring(device) end
    local ok, err = client.call(name, row.adapter, ADAPTER, "RemoveDevice", { o(row.path) },
      action_timeout)
    return ok and true or nil, err
  end

  client.watch_name(name, function(owned)
    if owned then
      bt.refresh()
    else
      objects = {}
      present = false
      publish_now()
    end
  end)
  client.on_signal(name, "/", OBJECT_MANAGER, "InterfacesAdded", function(body)
    if type(body) ~= "table" then return end
    local path, interfaces = body[1], body[2]
    if type(path) ~= "string" or type(interfaces) ~= "table" then return end
    if not (interfaces[ADAPTER] or interfaces[DEVICE] or objects[path]) then return end
    objects[path] = objects[path] or {}
    for interface, properties in pairs(interfaces) do
      objects[path][interface] = properties
    end
    bt.publish()
  end)
  client.on_signal(name, "/", OBJECT_MANAGER, "InterfacesRemoved", function(body)
    if type(body) ~= "table" then return end
    local path, interfaces = body[1], body[2]
    local held = objects[path]
    if not held then return end
    for _, interface in ipairs(interfaces or {}) do held[interface] = nil end
    if not (held[ADAPTER] or held[DEVICE]) then
      objects[path] = nil
      client.forget(path)
    end
    bt.publish()
  end)

  bt.refresh()
  return bt
end

return bluez
