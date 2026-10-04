-- NetworkManager, as reactive state a shell can bind to.
--
-- Wi-Fi, wired and VPN in a status bar are all one service on the system bus,
-- `org.freedesktop.NetworkManager`, and every panel that shows them ends up
-- writing the same translation: object paths into rows, flag words into
-- "wpa2", byte arrays into SSIDs. This is that translation, once, in Lua,
-- on the engine's generic D-Bus client — the engine knows nothing about
-- networks and should not.
--
--   local networkmanager = require("lib.services.networkmanager")
--   local net = networkmanager.connect()
--   ui.Text { text = function() return net.state.wifi.ssid end }
--   ui.Repeater { model = net.state.access_points, ... }
--   net.connect("home", "hunter2")
--
-- How it stays current: NetworkManager exports an ObjectManager at
-- `/org/freedesktop`, so the whole tree — devices, access points, active
-- connections, IP configs — is one `GetManagedObjects` call. Any change
-- worth hearing (the manager's own properties, a device's, an object coming
-- or going) schedules one re-read of that tree, debounced, rather than
-- patching rows signal by signal. Access points are deliberately *not*
-- subscribed one by one: their paths are never reused and come and go by
-- the dozen, so a week in a busy building would be a churn of match rules
-- for nothing. Their strengths arrive with each scan instead, because a scan
-- changes the device's `LastScan`, and the device *is* subscribed — for as
-- long as it exists, and no longer.
--
-- Saved profiles are not in that tree in readable form — their names and
-- SSIDs live behind `GetSettings` — so they are read separately, when the
-- settings service says a profile was added or removed.
--
-- When NetworkManager is not running, `available()` is false and every list
-- is empty; when it appears, the state fills in. Nothing raises.
--
-- Every action is a real change to the machine, and none of them waits for
-- it: NetworkManager may be waiting on polkit, or on a radio, and the shell
-- keeps drawing meanwhile. An action returns true once it is on its way (or
-- nil and why it could not be sent — no Wi-Fi device, no password), and takes
-- an optional last argument `done(result, err)` that hears how it ended:
-- `result` is what the action produces (a path, a count, `true`), or nil with
-- the service's error. The state follows by itself either way.

local morf = require("morf")
local dbus_client = require("lib.services.dbus_client")

local networkmanager = {}

local NAME = "org.freedesktop.NetworkManager"
local ROOT = "/org/freedesktop/NetworkManager"
local IFACE = NAME
local DEVICE = NAME .. ".Device"
local WIRELESS = NAME .. ".Device.Wireless"
local WIRED = NAME .. ".Device.Wired"
local ACCESS_POINT = NAME .. ".AccessPoint"
local ACTIVE = NAME .. ".Connection.Active"
local IP4 = NAME .. ".IP4Config"
local SETTINGS_PATH = ROOT .. "/Settings"
local SETTINGS = NAME .. ".Settings"
local SETTINGS_CONNECTION = SETTINGS .. ".Connection"
local OBJECT_MANAGER = "org.freedesktop.DBus.ObjectManager"

local o, u, typed = dbus_client.o, dbus_client.u, dbus_client.typed

-- The protocol's numbers, as words a configuration can compare against.
local NM_STATES = {
  [0] = "unknown", [10] = "asleep", [20] = "disconnected", [30] = "disconnecting",
  [40] = "connecting", [50] = "connected_local", [60] = "connected_site",
  [70] = "connected_global",
}
local CONNECTIVITY = { [0] = "unknown", "none", "portal", "limited", "full" }
local DEVICE_TYPES = {
  [0] = "unknown", "ethernet", "wifi", "unused1", "unused2", "bluetooth", "olpc_mesh",
  "wimax", "modem", "infiniband", "bond", "vlan", "adsl", "bridge", "generic", "team",
  "tun", "ip_tunnel", "macvlan", "vxlan", "veth", "macsec", "dummy", "ppp", "ovs_interface",
  "ovs_port", "ovs_bridge", "wpan", "6lowpan", "wireguard", "wifi_p2p", "vrf", "loopback",
  "hsr", "ipvlan",
}
local DEVICE_STATES = {
  [0] = "unknown", [10] = "unmanaged", [20] = "unavailable", [30] = "disconnected",
  [40] = "prepare", [50] = "config", [60] = "need_auth", [70] = "ip_config",
  [80] = "ip_check", [90] = "secondaries", [100] = "activated", [110] = "deactivating",
  [120] = "failed",
}
local ACTIVE_STATES = { [0] = "unknown", "activating", "activated", "deactivating", "deactivated" }

-- Access point security bits (NM80211ApFlags / NM80211ApSecurityFlags).
local AP_PRIVACY = 0x1
local KEY_PSK, KEY_8021X, KEY_SAE = 0x100, 0x200, 0x400
local KEY_OWE, KEY_OWE_TM, KEY_EAP_SUITE_B = 0x800, 0x1000, 0x2000

local function has(word, bit) return type(word) == "number" and (word & bit) ~= 0 end

--- What kind of lock an access point has, as the word a panel shows and
--- `connect` needs: "open", "owe", "wep", "wpa", "wpa2", "wpa3" or
--- "enterprise". Strongest wins: an access point offering both WPA2 and WPA3
--- is "wpa3", because that is what a connection to it will negotiate.
function networkmanager.security(flags, wpa_flags, rsn_flags)
  local both = (wpa_flags or 0) | (rsn_flags or 0)
  if has(both, KEY_8021X) or has(both, KEY_EAP_SUITE_B) then return "enterprise" end
  if has(rsn_flags, KEY_SAE) then return "wpa3" end
  if has(rsn_flags, KEY_PSK) then return "wpa2" end
  if has(wpa_flags, KEY_PSK) then return "wpa" end
  if has(rsn_flags, KEY_OWE) or has(rsn_flags, KEY_OWE_TM) then return "owe" end
  if has(flags, AP_PRIVACY) then return "wep" end
  return "open"
end

--- "2.4", "5" or "6" GHz, from a frequency in MHz.
function networkmanager.band(frequency)
  if type(frequency) ~= "number" or frequency <= 0 then return "" end
  if frequency < 3000 then return "2.4" end
  if frequency < 5925 then return "5" end
  return "6"
end

local function first_address(ip4)
  local data = ip4 and ip4.AddressData
  if type(data) == "table" and type(data[1]) == "table" and data[1].address then
    return tostring(data[1].address) .. "/" .. tostring(data[1].prefix or "")
  end
  return ""
end

local function empty_wifi()
  return {
    device = "", state = "unavailable", ssid = "", strength = 0, frequency = 0,
    security = "", connected = false, last_scan = -1,
  }
end

local function empty_wired()
  return {
    device = "", state = "unavailable", connected = false, carrier = false, speed = 0,
    hw_address = "", ip4 = "",
  }
end

--- Starts watching. Call it while the configuration loads: the state it
--- returns is made of signals, and signals are declared up front.
---
--- Options: `bus` ("system"), `name` (the service's bus name), `dbus` (a
--- stand-in for `morf.dbus`, for tests), `debounce_ms` (80), and
--- `action_timeout_ms` (5000) — how long an action may wait on the service,
--- which may be waiting on polkit.
function networkmanager.connect(options)
  options = options or {}
  local name = options.name or NAME
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or "system" })
  local action_timeout = options.action_timeout_ms or 5000

  local state = morf.state({
    available = false,
    version = "",
    state = "unknown",
    connectivity = "unknown",
    networking_enabled = false,
    wifi_enabled = false,
    wifi_hardware_enabled = false,
    primary = { id = "", type = "", path = "" },
    wifi = empty_wifi(),
    wired = empty_wired(),
    devices = {},
    active_connections = {},
    access_points = {},
    known_connections = {},
    vpn_connections = {},
  })

  local net = { state = state }
  local function nothing() end
  -- The last reading, in plain Lua, for the actions to consult. The state is
  -- for bindings; this is for logic, and reading it costs no bus round trip.
  local snapshot = { devices = {}, active = {}, access_points = {}, known = {} }
  local watched = {} -- device path -> subscription handle
  local refresh, refresh_known
  -- Whether the name has an owner. Reads are made only then: a call to an
  -- absent but activatable name would start NetworkManager, and starting the
  -- network stack is not something a status bar should do by looking.
  local present = client.has_owner(name)

  local function assign(target, values)
    for key, value in pairs(values) do target[key] = value end
  end

  local function clear()
    state.available = false
    state.version = ""
    state.state = "unknown"
    state.connectivity = "unknown"
    state.networking_enabled = false
    state.wifi_enabled = false
    state.wifi_hardware_enabled = false
    assign(state.primary, { id = "", type = "", path = "" })
    assign(state.wifi, empty_wifi())
    assign(state.wired, empty_wired())
    state.devices:replace({}, "path")
    state.active_connections:replace({}, "path")
    state.access_points:replace({}, "key")
    state.known_connections:replace({}, "path")
    state.vpn_connections:replace({}, "path")
    snapshot = { devices = {}, active = {}, access_points = {}, known = {} }
    for path, handle in pairs(watched) do
      handle.close()
      watched[path] = nil
    end
  end

  local schedule = dbus_client.debounce(options.debounce_ms or 80, function() refresh() end)
  local schedule_known = dbus_client.debounce(options.debounce_ms or 80, function()
    refresh_known()
    refresh()
  end)

  local function watch_device(path)
    if watched[path] then return end
    watched[path] = client.on_properties(name, path, function() schedule() end) or nil
  end

  --- Ends the subscriptions of devices that have gone (a USB adapter
  --- unplugged, a VPN's tun device torn down).
  local function unwatch_missing(present_paths)
    for path, handle in pairs(watched) do
      if not present_paths[path] then
        handle.close()
        watched[path] = nil
      end
    end
  end

  local function known_for_ssid(ssid)
    local found = {}
    for _, row in ipairs(snapshot.known) do
      if row.ssid == ssid and ssid ~= "" then found[#found + 1] = row end
    end
    return found
  end

  --- Re-reads the saved profiles. Each is one `GetSettings`; secrets are
  --- never in the reply, which is the service's rule and the right one.
  function refresh_known()
    local paths = present and client.call1(name, SETTINGS_PATH, SETTINGS, "ListConnections")
    local rows = {}
    if type(paths) == "table" then
      for _, path in ipairs(paths) do
        local settings = client.call1(name, path, SETTINGS_CONNECTION, "GetSettings")
        if type(settings) == "table" then
          local connection = settings.connection or {}
          local wireless = settings["802-11-wireless"] or {}
          local kind = connection.type or ""
          rows[#rows + 1] = {
            path = path,
            id = connection.id or "",
            uuid = connection.uuid or "",
            type = kind,
            ssid = dbus_client.bytes_to_string(wireless.ssid),
            autoconnect = connection.autoconnect ~= false,
            timestamp = connection.timestamp or 0,
            vpn = kind == "vpn" or kind == "wireguard",
          }
        end
      end
    end
    table.sort(rows, function(a, b)
      if a.timestamp ~= b.timestamp then return a.timestamp > b.timestamp end
      return a.id < b.id
    end)
    snapshot.known = rows
    state.known_connections:replace(rows, "path")
  end

  --- Re-reads the whole tree and republishes every row.
  function refresh()
    local objects = present and client.managed_objects(name, "/org/freedesktop")
    local main = objects and objects[ROOT] and objects[ROOT][IFACE]
    if not main then
      clear()
      return
    end
    state.available = true
    state.version = main.Version or ""
    state.state = NM_STATES[main.State] or "unknown"
    state.connectivity = CONNECTIVITY[main.Connectivity] or "unknown"
    state.networking_enabled = main.NetworkingEnabled == true
    state.wifi_enabled = main.WirelessEnabled == true
    state.wifi_hardware_enabled = main.WirelessHardwareEnabled == true

    local function iface(path, interface)
      return objects[path] and objects[path][interface]
    end

    -- Active connections first: devices and the primary refer to them.
    local active_rows, active_by_path = {}, {}
    for _, path in ipairs(main.ActiveConnections or {}) do
      local active = iface(path, ACTIVE)
      if active then
        local devices = {}
        for _, device_path in ipairs(active.Devices or {}) do
          local device = iface(device_path, DEVICE)
          if device then devices[#devices + 1] = device.Interface end
        end
        local row = {
          path = path,
          id = active.Id or "",
          uuid = active.Uuid or "",
          type = active.Type or "",
          state = ACTIVE_STATES[active.State] or "unknown",
          default = active.Default == true,
          vpn = active.Vpn == true or active.Type == "wireguard",
          devices = devices,
          connection = active.Connection or "",
        }
        active_rows[#active_rows + 1] = row
        active_by_path[path] = row
      end
    end

    local primary = active_by_path[main.PrimaryConnection or ""]
    assign(state.primary, {
      id = primary and primary.id or "",
      type = primary and primary.type or (main.PrimaryConnectionType or ""),
      path = primary and primary.path or "",
    })

    -- Known SSIDs, so an access point row can say "you have been here".
    local known_ssids = {}
    for _, row in ipairs(snapshot.known) do
      if row.ssid ~= "" then known_ssids[row.ssid] = true end
    end

    local device_rows, ap_rows, by_ssid = {}, {}, {}
    local wifi_summary, wired_summary
    local device_paths = main.AllDevices or main.Devices or {}
    local seen = {}
    for _, path in ipairs(device_paths) do
      local device = iface(path, DEVICE)
      if device then
        seen[path] = true
        watch_device(path)
        local wireless = iface(path, WIRELESS)
        local wired = iface(path, WIRED)
        local active = active_by_path[device.ActiveConnection or ""]
        local row = {
          path = path,
          interface = device.Interface or "",
          type = DEVICE_TYPES[device.DeviceType] or "unknown",
          state = DEVICE_STATES[device.State] or "unknown",
          driver = device.Driver or "",
          managed = device.Managed == true,
          hw_address = (wireless and wireless.HwAddress) or (wired and wired.HwAddress)
            or device.HwAddress or "",
          ip4 = first_address(iface(device.Ip4Config or "", IP4)),
          connection = active and active.id or "",
          -- A wired port's: whether a cable is in, and at what speed (Mb/s).
          carrier = wired ~= nil and wired.Carrier == true,
          speed = wired and wired.Speed or 0,
        }
        device_rows[#device_rows + 1] = row

        if wireless then
          local active_ap = wireless.ActiveAccessPoint or "/"
          for _, ap_path in ipairs(wireless.AccessPoints or {}) do
            local ap = iface(ap_path, ACCESS_POINT)
            if ap then
              local ssid = dbus_client.bytes_to_string(ap.Ssid)
              local entry = {
                key = ssid ~= "" and ssid or (ap.HwAddress or ap_path),
                path = ap_path,
                device = row.interface,
                ssid = ssid,
                bssid = ap.HwAddress or "",
                strength = ap.Strength or 0,
                frequency = ap.Frequency or 0,
                band = networkmanager.band(ap.Frequency),
                security = networkmanager.security(ap.Flags, ap.WpaFlags, ap.RsnFlags),
                in_use = ap_path == active_ap,
                known = known_ssids[ssid] == true,
                max_bitrate = ap.MaxBitrate or 0,
              }
              entry.secure = entry.security ~= "open" and entry.security ~= "owe"
              -- One row per network, not per radio: a mesh at home is
              -- six BSSIDs and one thing a person chooses. The one in use
              -- represents it, else the loudest.
              local held = by_ssid[entry.key]
              if ssid ~= "" and (not held
                  or (entry.in_use and not held.in_use)
                  or (entry.in_use == held.in_use and entry.strength > held.strength)) then
                by_ssid[entry.key] = entry
              end
            end
          end
          if not wifi_summary then
            local ap = iface(active_ap, ACCESS_POINT)
            wifi_summary = {
              device = row.interface,
              state = row.state,
              ssid = ap and dbus_client.bytes_to_string(ap.Ssid) or "",
              strength = ap and ap.Strength or 0,
              frequency = ap and ap.Frequency or 0,
              security = ap and networkmanager.security(ap.Flags, ap.WpaFlags, ap.RsnFlags) or "",
              connected = row.state == "activated",
              last_scan = wireless.LastScan or -1,
            }
          end
        elseif row.type == "ethernet" and not wired_summary then
          wired_summary = {
            device = row.interface,
            state = row.state,
            connected = row.state == "activated",
            carrier = wired ~= nil and wired.Carrier == true,
            speed = wired and wired.Speed or 0,
            hw_address = row.hw_address,
            ip4 = row.ip4,
          }
        end
      end
    end
    unwatch_missing(seen)
    for _, entry in pairs(by_ssid) do ap_rows[#ap_rows + 1] = entry end
    table.sort(ap_rows, function(a, b)
      if a.in_use ~= b.in_use then return a.in_use end
      if a.strength ~= b.strength then return a.strength > b.strength end
      return a.ssid < b.ssid
    end)

    local vpn_rows = {}
    for _, known in ipairs(snapshot.known) do
      if known.vpn then
        local live
        for _, active in ipairs(active_rows) do
          if active.uuid == known.uuid then live = active end
        end
        vpn_rows[#vpn_rows + 1] = {
          path = known.path, id = known.id, uuid = known.uuid, type = known.type,
          active = live ~= nil and live.state == "activated",
          state = live and live.state or "deactivated",
        }
      end
    end

    snapshot.devices = device_rows
    snapshot.active = active_rows
    snapshot.access_points = ap_rows
    assign(state.wifi, wifi_summary or empty_wifi())
    assign(state.wired, wired_summary or empty_wired())
    state.devices:replace(device_rows, "path")
    state.active_connections:replace(active_rows, "path")
    state.access_points:replace(ap_rows, "key")
    state.vpn_connections:replace(vpn_rows, "path")
  end

  local function wifi_device(interface)
    for _, row in ipairs(snapshot.devices) do
      if row.type == "wifi" and (interface == nil or row.interface == interface) then
        return row
      end
    end
  end

  local function find_known(which)
    for _, row in ipairs(snapshot.known) do
      if row.id == which or row.uuid == which or row.path == which then return row end
    end
  end

  local function find_active(which)
    for _, row in ipairs(snapshot.active) do
      if row.id == which or row.uuid == which or row.path == which then return row end
    end
  end

  --- An action on the manager, answered later: `done(reply, err)`.
  local function call_manager(method, arguments, done)
    return client.call_async(name, ROOT, IFACE, method, arguments, action_timeout, done)
  end

  --- Whether NetworkManager is on the bus.
  function net.available() return state.available end

  --- The last reading as plain tables: `devices`, `active`,
  --- `access_points`, `known`. Cheap; for logic, not for bindings.
  function net.snapshot() return snapshot end

  --- Re-reads everything now rather than on the next signal.
  function net.refresh()
    present = client.has_owner(name)
    refresh_known()
    refresh()
  end

  --- Asks every Wi-Fi device (or the named one) to scan. The results arrive
  --- as the device's `LastScan` changes, not as this call's reply; `done`
  --- hears each device's answer.
  function net.request_scan(interface, done)
    done = done or nothing
    local asked, err = false, "no wifi device"
    for _, row in ipairs(snapshot.devices) do
      if row.type == "wifi" and (interface == nil or row.interface == interface) then
        -- `a{sv}` of options, none of them: a normal scan.
        local ok, failure = client.call_async(name, row.path, WIRELESS, "RequestScan",
          { typed("a{sv}", {}) }, action_timeout, done)
        if ok then asked = true else err = failure end
      end
    end
    if asked then return true end
    return nil, err
  end

  --- Joins a network. `target` is an access point row from
  --- `state.access_points` or an SSID; `password` is needed for a network
  --- this machine does not know yet. A known network is activated as saved;
  --- a new one gets a profile made from what the access point advertises.
  --- A password given for a known network makes a new profile beside the old
  --- one (`forget` removes both), because rewriting a saved profile means
  --- sending all of it back, and the engine cannot type every field of it.
  ---
  --- `done(path, err)` hears the active connection's path, or why not.
  function net.connect(target, password, interface, done)
    done = done or nothing
    local ssid, ap_path, security, device_name
    if type(target) == "table" then
      ssid, ap_path, security, device_name = target.ssid, target.path, target.security, target.device
    else
      ssid = tostring(target or "")
      for _, row in ipairs(snapshot.access_points) do
        if row.ssid == ssid then
          ap_path, security, device_name = row.path, row.security, row.device
          break
        end
      end
    end
    if not ssid or ssid == "" then return nil, "no network named" end
    local device = wifi_device(interface or device_name) or wifi_device()
    if not device then return nil, "no wifi device" end

    local known = known_for_ssid(ssid)[1]
    if known and password == nil then
      return call_manager("ActivateConnection",
        { o(known.path), o(device.path), o(ap_path or "/") }, function(reply, err)
          schedule()
          reply = dbus_client.first(reply)
          if type(reply) == "string" then return done(reply) end
          done(nil, err or "no active connection")
        end)
    end

    if security == "enterprise" then
      return nil, "802.1X networks need a profile with their credentials; make one with nmcli or nm-connection-editor"
    end
    local wireless = { ssid = typed("ay", ssid) }
    if not ap_path then wireless.hidden = true end
    local settings = { ["802-11-wireless"] = wireless }
    if security == "wpa3" then
      if not password then return nil, "a password is needed" end
      settings["802-11-wireless-security"] = { ["key-mgmt"] = "sae", psk = password }
    elseif security == "wpa" or security == "wpa2" or (security == nil and password) then
      if not password then return nil, "a password is needed" end
      settings["802-11-wireless-security"] = { ["key-mgmt"] = "wpa-psk", psk = password }
    elseif security == "wep" then
      if not password then return nil, "a password is needed" end
      settings["802-11-wireless-security"] = {
        ["key-mgmt"] = "none", ["wep-key0"] = password, ["wep-key-type"] = u(1),
      }
    elseif security == "owe" then
      settings["802-11-wireless-security"] = { ["key-mgmt"] = "owe" }
    end
    return call_manager("AddAndActivateConnection",
      { typed("a{sa{sv}}", settings), o(device.path), o(ap_path or "/") }, function(reply, err)
        schedule_known()
        -- The reply is `(oo)`: the new profile and the active connection.
        if type(reply) == "table" then return done(reply[2]) end
        done(nil, err or "no active connection")
      end)
  end

  --- Takes a connection down. `which` is an active connection's id, uuid or
  --- path, or a device's interface name; nothing means the Wi-Fi connection.
  function net.disconnect(which, done)
    done = done or nothing
    local function finished(ok, err)
      schedule()
      done(ok and true or nil, err)
    end
    local active
    if which == nil then
      local device = wifi_device()
      if not device then return nil, "no wifi device" end
      for _, row in ipairs(snapshot.active) do
        for _, interface in ipairs(row.devices) do
          if interface == device.interface then active = row end
        end
      end
    else
      active = find_active(which)
      if not active then
        for _, row in ipairs(snapshot.devices) do
          if row.interface == which then
            return client.call_async(name, row.path, DEVICE, "Disconnect", nil, action_timeout,
              finished)
          end
        end
      end
    end
    if not active then return nil, "nothing to disconnect" end
    return call_manager("DeactivateConnection", { o(active.path) }, finished)
  end

  --- Deletes every saved profile for an SSID. Returns how many it asked to
  --- delete; `done(removed, err)` hears how many went.
  function net.forget(ssid, done)
    done = done or nothing
    local rows = known_for_ssid(ssid)
    if #rows == 0 then return 0 end
    local waiting, removed, err, asked = #rows, 0, nil, 0
    local function one(ok, failure)
      if ok then removed = removed + 1 else err = failure end
      waiting = waiting - 1
      if waiting > 0 then return end
      schedule_known()
      if removed == 0 and err then return done(nil, err) end
      done(removed)
    end
    for _, row in ipairs(rows) do
      local path = row.path
      local sent, failure = client.call_async(name, path, SETTINGS_CONNECTION, "Delete", nil,
        action_timeout, function(ok, why)
          if ok then client.forget(path) end
          one(ok, why)
        end)
      if sent then asked = asked + 1 else one(nil, failure) end
    end
    if asked == 0 then return nil, err end
    return asked
  end

  --- Turns the Wi-Fi radio on or off (the software switch; the hardware one
  --- is `state.wifi_hardware_enabled`, and nothing here can move it).
  function net.set_wifi(enabled, done)
    done = done or nothing
    return client.set_async(name, ROOT, IFACE, "WirelessEnabled", enabled == true, action_timeout,
      function(ok, err)
        schedule()
        done(ok and true or nil, err)
      end)
  end

  --- Brings an interface up with whatever profile suits it -- a wired
  --- port that was disconnected, say. `done(ok, err)`.
  function net.connect_device(interface, done)
    done = done or nothing
    local device
    for _, row in ipairs(snapshot.devices) do
      if row.interface == interface then device = row end
    end
    if not device then return nil, "no device " .. tostring(interface) end
    return call_manager("ActivateConnection", { o("/"), o(device.path), o("/") }, function(ok, err)
      schedule()
      done(ok and true or nil, err)
    end)
  end

  --- Mobile broadband (WWAN) on or off, as a phone's mobile data.
  function net.set_wwan(enabled, done)
    done = done or nothing
    return client.set_async(name, ROOT, IFACE, "WwanEnabled", enabled == true, action_timeout,
      function(ok, err)
        schedule()
        done(ok and true or nil, err)
      end)
  end

  --- Brings up any saved profile by id, uuid or path — a VPN, a wired
  --- profile, a Wi-Fi network — on whatever device it names.
  --- `done(path, err)` hears the active connection's path.
  function net.activate(which, done)
    done = done or nothing
    local known = find_known(which)
    if not known then return nil, "no saved connection " .. tostring(which) end
    return call_manager("ActivateConnection", { o(known.path), o("/"), o("/") },
      function(reply, err)
        schedule()
        reply = dbus_client.first(reply)
        if type(reply) == "string" then return done(reply) end
        done(nil, err or "no active connection")
      end)
  end

  --- Takes down an active connection by id, uuid or path.
  function net.deactivate(which, done)
    done = done or nothing
    local active = find_active(which)
    if not active then return nil, "not active: " .. tostring(which) end
    return call_manager("DeactivateConnection", { o(active.path) }, function(ok, err)
      schedule()
      done(ok and true or nil, err)
    end)
  end

  function net.activate_vpn(which, done) return net.activate(which, done) end
  function net.deactivate_vpn(which, done) return net.deactivate(which, done) end

  -- Subscriptions, made once. Made even while the service is absent: a match
  -- rule names the service by its well-known name, and the bus applies it to
  -- whoever owns that name when it arrives.
  client.watch_name(name, function(owned)
    present = owned
    if owned then
      refresh_known()
      refresh()
    else
      clear()
    end
  end)
  client.on_properties(name, ROOT, function() schedule() end)
  client.on_signal(name, "/org/freedesktop", OBJECT_MANAGER, "InterfacesAdded", function() schedule() end)
  client.on_signal(name, "/org/freedesktop", OBJECT_MANAGER, "InterfacesRemoved", function() schedule() end)
  client.on_signal(name, SETTINGS_PATH, SETTINGS, "NewConnection", function() schedule_known() end)
  client.on_signal(name, SETTINGS_PATH, SETTINGS, "ConnectionRemoved", function(body)
    local path = dbus_client.first(body)
    if type(path) == "string" then client.forget(path) end
    schedule_known()
  end)

  refresh_known()
  refresh()
  return net
end

return networkmanager
