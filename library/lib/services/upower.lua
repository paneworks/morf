-- Batteries and power profiles, as reactive state a shell can bind to.
--
-- Two services, one question a panel asks: how much power is left, and how
-- hard is the machine allowed to work. UPower (`org.freedesktop.UPower`)
-- answers the first for the laptop's own battery and for everything else that
-- reports one — a mouse, a headset, a UPS. power-profiles-daemon answers the
-- second, under one of two names: `org.freedesktop.UPower.PowerProfiles`
-- since 0.20, and `net.hadess.PowerProfiles` before that (still installed as
-- an alias on most systems). Either is used; neither is required.
--
--   local power = morf.service("power")          -- or require(...).connect()
--   ui.Text { text = function()
--     return ("%d%%"):format(power.state.display.percentage) end }
--   power.set_profile("power-saver")
--
-- The *display device* is UPower's own summary — every system battery
-- folded into one percentage and one state — and is what a status bar
-- should show; `devices` lists the parts. A device's path is stable for as
-- long as the device exists (it is derived from the kernel's name for it), so
-- each is subscribed for `PropertiesChanged` once, and the subscription is
-- closed when the device goes — a headset that comes and goes all day does
-- not leave a match rule behind each time.
--
-- This module is a registry descriptor (lib/services/registry.lua): the
-- binder reads, merges each change in place, and follows both daemons
-- coming and going. Neither service is ever started by this: a name nobody
-- owns is read as absent, not activated.

local registry = require("lib.services.registry")

local upower = {}

local NAME = "org.freedesktop.UPower"
local ROOT = "/org/freedesktop/UPower"
local DEVICE = "org.freedesktop.UPower.Device"
local DISPLAY = ROOT .. "/devices/DisplayDevice"

-- power-profiles-daemon, newest name first.
local PROFILE_SERVICES = {
  { name = "org.freedesktop.UPower.PowerProfiles", path = "/org/freedesktop/UPower/PowerProfiles",
    interface = "org.freedesktop.UPower.PowerProfiles" },
  { name = "net.hadess.PowerProfiles", path = "/net/hadess/PowerProfiles",
    interface = "net.hadess.PowerProfiles" },
}

local STATES = {
  [0] = "unknown", "charging", "discharging", "empty", "fully_charged",
  "pending_charge", "pending_discharge",
}
local CHARGING = { 1, 5 } -- charging, pending_charge
local KINDS = {
  [0] = "unknown", "line_power", "battery", "ups", "monitor", "mouse", "keyboard", "pda",
  "phone", "media_player", "tablet", "computer", "gaming_input", "pen", "touchpad", "modem",
  "network", "headset", "speakers", "headphones", "video", "other_audio", "remote_control",
  "printer", "scanner", "camera", "wearable", "toy", "bluetooth_generic",
}
local WARNING_LEVELS = { [0] = "unknown", "none", "discharging", "low", "critical", "action" }

-- The fields the display device and every device row share. Times are
-- seconds; 0 means unknown.
local function device_fields(extra)
  local fields = {
    present = { prop = "IsPresent", as = "bool" },
    percentage = { prop = "Percentage", as = "percent" },
    state = { prop = "State", as = "enum", values = STATES, default = "unknown" },
    charging = { prop = "State", as = "one_of", values = CHARGING },
    time_to_empty = { prop = "TimeToEmpty", as = "number" },
    time_to_full = { prop = "TimeToFull", as = "number" },
    icon_name = { prop = "IconName", as = "string" },
    energy_rate = { prop = "EnergyRate", as = "number" },
    kind = { prop = "Type", as = "enum", values = KINDS, default = "unknown" },
    warning_level = { prop = "WarningLevel", as = "enum", values = WARNING_LEVELS, default = "unknown" },
  }
  for name, spec in pairs(extra or {}) do fields[name] = spec end
  return fields
end

local ROW = device_fields {
  path = { meta = "key" },
  native_path = { prop = "NativePath", as = "string" },
  model = { prop = "Model", as = "string" },
  vendor = { prop = "Vendor", as = "string" },
  serial = { prop = "Serial", as = "string" },
  online = { prop = "Online", as = "bool" },
  power_supply = { prop = "PowerSupply", as = "bool" },
  rechargeable = { prop = "IsRechargeable", as = "bool" },
  energy = { prop = "Energy", as = "number" },
  energy_full = { prop = "EnergyFull", as = "number" },
  capacity = { prop = "Capacity", as = "number" },
}

upower.descriptor = registry.define {
  name = "power",
  bus = "system",
  service = NAME,
  debounce_ms = 60,
  interfaces = {
    root = { path = ROOT, interface = NAME },
    display = { path = DISPLAY, interface = DEVICE },
    device = { set = "devices", interface = DEVICE },
    profiles = { any_of = PROFILE_SERVICES },
  },
  objects = {
    devices = { list = { from = "root", method = "EnumerateDevices" }, from = "device", fields = ROW },
  },
  signals = {
    { from = "root", member = "DeviceAdded", adds = "devices" },
    { from = "root", member = "DeviceRemoved", removes = "devices" },
  },
  fields = {
    available = { from = "root", meta = "available" },
    on_battery = { from = "root", prop = "OnBattery", as = "bool" },
    lid_is_closed = { from = "root", prop = "LidIsClosed", as = "bool" },
    lid_is_present = { from = "root", prop = "LidIsPresent", as = "bool" },
    display = { from = "display", fields = device_fields() },
    devices = { from = "devices" },
    -- Batteries that are not the machine's own: a mouse, a headset.
    peripherals = { from = "devices",
                    where = { power_supply = false, present = true, kind = { except = "line_power" } } },
    profiles = { from = "profiles", fields = {
      available = { meta = "available" },
      service = { meta = "service" },
      active = { prop = "ActiveProfile", as = "string" },
      degraded = { prop = "PerformanceDegraded", as = "string" },
      list = { prop = "Profiles", as = "records", key = "name", fields = {
        name = { prop = "Profile", as = "string" },
        driver = { prop = "Driver", as = "string" },
      } },
    } },
  },
  actions = {
    --- Switches the power profile: "power-saver", "balanced" or
    --- "performance" (whichever `state.profiles.list` offers).
    set_profile = { write = "ActiveProfile", on = "profiles", as = "string",
                    missing = "no power profile service" },
    --- The device rows as plain tables.
    devices = { rows = "devices" },
  },
}

--- A device's properties as a row: the same coercions the binder applies.
function upower.device_row(path, props)
  local row = {}
  for name, spec in pairs(ROW) do
    if spec.meta then row[name] = path
    else row[name] = registry.coerce[spec.as](props[spec.prop], spec) end
  end
  return row
end

--- "3 h 12 min", "45 min", or "" for unknown. A convenience for labels.
function upower.format_time(seconds)
  if type(seconds) ~= "number" or seconds <= 0 then return "" end
  local minutes = math.floor(seconds / 60 + 0.5)
  if minutes < 60 then return ("%d min"):format(minutes) end
  return ("%d h %d min"):format(minutes // 60, minutes % 60)
end

--- Starts watching. Call it while the configuration loads.
---
--- Options: `bus` ("system"), `name` (UPower's), `profile_services` (the
--- list above), `dbus` (test seam), `debounce_ms` (60).
function upower.connect(options)
  options = options or {}
  return registry.connect("power", {
    dbus = options.dbus, bus = options.bus, debounce_ms = options.debounce_ms,
    service = options.name,
    interfaces = options.profile_services and { profiles = { any_of = options.profile_services } },
  })
end

return upower
