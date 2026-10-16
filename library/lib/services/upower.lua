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
--   local upower = require("lib.services.upower")
--   local power = upower.connect()
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
-- Neither service is ever started by this: a name nobody owns is read as
-- absent, not activated.

local morf = require("morf")
local dbus_client = require("lib.services.dbus_client")
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
local KINDS = {
  [0] = "unknown", "line_power", "battery", "ups", "monitor", "mouse", "keyboard", "pda",
  "phone", "media_player", "tablet", "computer", "gaming_input", "pen", "touchpad", "modem",
  "network", "headset", "speakers", "headphones", "video", "other_audio", "remote_control",
  "printer", "scanner", "camera", "wearable", "toy", "bluetooth_generic",
}
local WARNING_LEVELS = { [0] = "unknown", "none", "discharging", "low", "critical", "action" }

--- A device's properties as a row. Times are seconds; 0 means unknown.
function upower.device_row(path, p)
  local state = STATES[p.State] or "unknown"
  return {
    path = path,
    native_path = p.NativePath or "",
    kind = KINDS[p.Type] or "unknown",
    model = p.Model or "",
    vendor = p.Vendor or "",
    serial = p.Serial or "",
    present = p.IsPresent == true,
    online = p.Online == true,
    power_supply = p.PowerSupply == true,
    rechargeable = p.IsRechargeable == true,
    percentage = p.Percentage or 0,
    state = state,
    charging = state == "charging" or state == "pending_charge",
    time_to_empty = p.TimeToEmpty or 0,
    time_to_full = p.TimeToFull or 0,
    energy = p.Energy or 0,
    energy_full = p.EnergyFull or 0,
    energy_rate = p.EnergyRate or 0,
    capacity = p.Capacity or 0,
    icon_name = p.IconName or "",
    warning_level = WARNING_LEVELS[p.WarningLevel] or "unknown",
  }
end

--- "3 h 12 min", "45 min", or "" for unknown. A convenience for labels.
function upower.format_time(seconds)
  if type(seconds) ~= "number" or seconds <= 0 then return "" end
  local minutes = math.floor(seconds / 60 + 0.5)
  if minutes < 60 then return ("%d min"):format(minutes) end
  return ("%d h %d min"):format(minutes // 60, minutes % 60)
end

local function empty_display()
  return {
    present = false, percentage = 0, state = "unknown", charging = false,
    time_to_empty = 0, time_to_full = 0, icon_name = "", energy_rate = 0,
    kind = "unknown", warning_level = "unknown",
  }
end

--- Starts watching. Call it while the configuration loads.
---
--- Options: `bus` ("system"), `name` (UPower's), `profile_services` (the
--- list above), `dbus` (test seam), `debounce_ms` (60).
function upower.connect(options)
  options = options or {}
  local name = options.name or NAME
  local profile_services = options.profile_services or PROFILE_SERVICES
  local profile_service -- the entry of `profile_services` that answered

  local function read_profiles(client, state)
    for _, service in ipairs(profile_services) do
      -- Never by activation: power-profiles-daemon applies its default
      -- profile when it starts, so reading it into existence would change
      -- how the machine runs.
      local p = client.has_owner(service.name)
        and client.get_all(service.name, service.path, service.interface)
      if p then
        profile_service = service
        local list = {}
        for _, entry in ipairs(p.Profiles or {}) do
          if type(entry) == "table" and entry.Profile then
            list[#list + 1] = { name = entry.Profile, driver = entry.Driver or "" }
          end
        end
        state.profiles.available = true
        state.profiles.active = p.ActiveProfile or ""
        state.profiles.degraded = p.PerformanceDegraded or ""
        state.profiles.service = service.name
        state.profiles.list:replace(list, "name")
        return
      end
    end
    profile_service = nil
    state.profiles.available = false
    state.profiles.active = ""
    state.profiles.degraded = ""
    state.profiles.service = ""
    state.profiles.list:replace({}, "name")
  end

  local function bool_field(prop)
    return { prop = prop, default = false, map = function(v) return v == true end }
  end

  local service = registry.define {
    name = "upower",
    bus = options.bus or "system",
    service = name,
    path = ROOT,
    interface = NAME,
    debounce_ms = options.debounce_ms,
    properties = {
      on_battery = bool_field("OnBattery"),
      lid_is_closed = bool_field("LidIsClosed"),
      lid_is_present = bool_field("LidIsPresent"),
    },
    groups = {
      display = {
        path = DISPLAY,
        interface = DEVICE,
        properties = {
          present = bool_field("IsPresent"),
          percentage = { prop = "Percentage", default = 0 },
          state = { prop = "State", default = "unknown",
                    map = function(v) return STATES[v] or "unknown" end },
          charging = { prop = "State", default = false, map = function(v)
            local s = STATES[v]
            return s == "charging" or s == "pending_charge"
          end },
          time_to_empty = { prop = "TimeToEmpty", default = 0 },
          time_to_full = { prop = "TimeToFull", default = 0 },
          icon_name = { prop = "IconName", default = "" },
          energy_rate = { prop = "EnergyRate", default = 0 },
          kind = { prop = "Type", default = "unknown",
                   map = function(v) return KINDS[v] or "unknown" end },
          warning_level = { prop = "WarningLevel", default = "unknown",
                            map = function(v) return WARNING_LEVELS[v] or "unknown" end },
        },
      },
    },
    objects = {
      field = "devices",
      key = "path",
      interface = DEVICE,
      list = function(client)
        if not client.has_owner(name) then return {} end
        local paths = client.call1(name, ROOT, NAME, "EnumerateDevices")
        return type(paths) == "table" and paths or {}
      end,
      map = function(props, path) return upower.device_row(path, props) end,
    },
    state = {
      -- Batteries that are not the machine's own: a mouse, a headset.
      peripherals = {},
      profiles = { available = false, active = "", degraded = "", service = "", list = {} },
    },
    signals = {
      { member = "DeviceAdded", fn = function(bound) bound.refresh() end },
      { member = "DeviceRemoved", fn = function(bound, body)
        local path = dbus_client.first(body)
        if type(path) == "string" then bound.client.forget(path) end
        bound.refresh()
      end },
    },
    on_refresh = function(bound, client, state)
      if not state.available then
        state.devices:replace({}, "path")
        state.peripherals:replace({}, "path")
        for key, value in pairs(empty_display()) do state.display[key] = value end
      else
        local peripherals = {}
        for _, row in ipairs(bound.rows or {}) do
          if not row.power_supply and row.kind ~= "line_power" and row.present then
            peripherals[#peripherals + 1] = row
          end
        end
        state.peripherals:replace(peripherals, "path")
      end
      read_profiles(client, state)
    end,
    methods = function(client, bound, state)
      return {
        --- Whether UPower is on the bus.
        available = function() return state.available end,
        --- The device rows as plain tables.
        devices = function() return bound.rows or {} end,
        refresh = function() bound.refresh() end,
        --- Switches the power profile: "power-saver", "balanced" or
        --- "performance" (whichever `state.profiles.list` offers).
        set_profile = function(profile)
          if not profile_service then return nil, "no power profile service" end
          local ok, err = client.set(profile_service.name, profile_service.path,
            profile_service.interface, "ActiveProfile", tostring(profile))
          read_profiles(client, state)
          return ok, err
        end,
      }
    end,
  }
  return service.connect({ dbus = options.dbus, bus = options.bus })
end

return upower
