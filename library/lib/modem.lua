-- The mobile network: a phone's modem, through ModemManager, and mobile
-- data on or off through NetworkManager -- what postmarketOS, Mobian and
-- every other Linux phone run.
--
--   local modem = require("lib.modem")
--   local mobile = modem.connect()
--   mobile.state.available      -- a modem is there
--   mobile.state.signal         -- 0..100
--   mobile.state.technology     -- "5G", "LTE", "3G", "2G", ""
--   mobile.state.operator       -- "Vodafone NL"
--   mobile.state.registered, .connected, .enabled, .locked
--   mobile.state.data           -- mobile data on (NetworkManager's WwanEnabled)
--   mobile.set_data(true)
--
-- A machine without ModemManager, or without a modem, reads as not
-- available and costs one look at the bus: a laptop is not asked twice.

local morf = require("morf")
local dbus_client = require("lib.dbus_client")

local modem = {}

local MM = "org.freedesktop.ModemManager1"
local ROOT = "/org/freedesktop/ModemManager1"
local MODEM = "org.freedesktop.ModemManager1.Modem"
local GPP = "org.freedesktop.ModemManager1.Modem.Modem3gpp"
local OBJECT_MANAGER = "org.freedesktop.DBus.ObjectManager"
local NM = "org.freedesktop.NetworkManager"
local NM_PATH = "/org/freedesktop/NetworkManager"

-- MMModemAccessTechnology, best first: the generation a person reads.
local TECHNOLOGIES = {
  { 1 << 15, "5G" }, { 1 << 14, "LTE" }, { 1 << 16, "LTE" }, { 1 << 17, "LTE" },
  { 1 << 9, "H+" }, { 1 << 8, "H" }, { 1 << 7, "H" }, { 1 << 6, "H" }, { 1 << 5, "3G" },
  { 1 << 13, "3G" }, { 1 << 12, "3G" }, { 1 << 11, "3G" }, { 1 << 10, "2G" },
  { 1 << 4, "E" }, { 1 << 3, "G" }, { 1 << 1, "2G" }, { 1 << 2, "2G" },
}

--- The generation a technology mask stands for ("5G", "LTE", ... or "").
function modem.technology(mask)
  mask = tonumber(mask) or 0
  for _, t in ipairs(TECHNOLOGIES) do
    if mask & t[1] ~= 0 then return t[2] end
  end
  return ""
end

-- MMModemState.
local LOCKED, DISABLED, ENABLED, REGISTERED, CONNECTED = 2, 3, 6, 8, 11

local function first(value)
  if type(value) == "table" then return value[1] end
  return value
end

function modem.connect(options)
  options = options or {}
  local client = dbus_client.new({ dbus = options.dbus, bus = "system" })
  local state = morf.state {
    available = false, signal = 0, technology = "", operator = "",
    registered = false, connected = false, enabled = false, locked = false,
    data = false, path = "",
  }
  local mobile = { state = state }
  local objects = {}
  local watching = {}

  local function publish()
    local chosen
    for path, interfaces in pairs(objects) do
      if interfaces[MODEM] and (not chosen or path < chosen) then chosen = path end
    end
    if not chosen then
      state.available = false
      return
    end
    local m = objects[chosen][MODEM] or {}
    local gpp = objects[chosen][GPP] or {}
    local s = tonumber(m.State) or 0
    state.available = true
    state.path = chosen
    state.signal = tonumber(first(m.SignalQuality)) or 0
    state.technology = modem.technology(m.AccessTechnologies)
    state.operator = tostring(gpp.OperatorName or "")
    state.locked = s == LOCKED
    state.enabled = s >= ENABLED
    state.registered = s >= REGISTERED
    state.connected = s == CONNECTED
  end

  local function watch(path)
    if watching[path] then return end
    watching[path] = client.on_properties(MM, path, function(interface, changed, invalidated)
      local held = objects[path] and objects[path][interface]
      if held then
        dbus_client.merge(held, changed, invalidated)
        publish()
      end
    end) or true
  end

  local function read_all()
    objects = {}
    if not client.has_owner(MM) then
      publish()
      return
    end
    local tree = client.call1(MM, ROOT, OBJECT_MANAGER, "GetManagedObjects")
    for path, interfaces in pairs(type(tree) == "table" and tree or {}) do
      if type(interfaces) == "table" and interfaces[MODEM] then
        objects[path] = interfaces
        watch(path)
      end
    end
    publish()
  end

  local function read_data()
    if not client.has_owner(NM) then state.data = false return end
    local nm = client.get_all(NM, NM_PATH, NM) or {}
    state.data = nm.WwanEnabled == true
  end

  --- Mobile data on or off: NetworkManager's WWAN switch.
  function mobile.set_data(on)
    local ok, err = client.set(NM, NM_PATH, NM, "WwanEnabled", on == true)
    read_data()
    return ok, err
  end

  client.watch_name(MM, read_all)
  client.on_signal(MM, ROOT, OBJECT_MANAGER, "InterfacesAdded", function() read_all() end)
  client.on_signal(MM, ROOT, OBJECT_MANAGER, "InterfacesRemoved", function() read_all() end)
  client.on_properties(NM, NM_PATH, function(_, changed)
    if changed.WwanEnabled ~= nil then state.data = changed.WwanEnabled == true end
  end)
  read_all()
  read_data()
  return mobile
end

return modem
