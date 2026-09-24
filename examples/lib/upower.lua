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
--   local upower = require("lib.upower")
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
local dbus_client = require("lib.dbus_client")

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
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or "system" })
  local profile_services = options.profile_services or PROFILE_SERVICES

  local state = morf.state({
    available = false,
    on_battery = false,
    lid_is_closed = false,
    lid_is_present = false,
    display = empty_display(),
    devices = {},
    -- Batteries that are not the machine's own: a mouse, a headset.
    peripherals = {},
    profiles = {
      available = false,
      active = "",
      degraded = "",
      service = "",
      list = {},
    },
  })

  local power = { state = state }
  local watched = {}
  local rows = {}
  local profile_service -- the entry of `profile_services` that answered

  local function assign(target, values)
    for key, value in pairs(values) do target[key] = value end
  end

  local function watch(path, handler)
    if watched[path] then return end
    watched[path] = client.on_properties(name, path, handler) or nil
  end

  --- Closes the subscriptions of devices no longer enumerated. The display
  --- device is UPower's own and never goes.
  local function unwatch_missing(present)
    for path, handle in pairs(watched) do
      if path ~= DISPLAY and not present[path] then
        handle.close()
        watched[path] = nil
      end
    end
  end

  local refresh_devices

  local function read_root()
    -- Only a running service is read. A read of an absent but activatable
    -- name starts it, and starting a system daemon is not a watcher's call.
    local root = client.has_owner(name) and client.get_all(name, ROOT, NAME)
    if not root then
      state.available = false
      state.on_battery = false
      state.lid_is_closed = false
      state.lid_is_present = false
      return false
    end
    state.available = true
    state.on_battery = root.OnBattery == true
    state.lid_is_closed = root.LidIsClosed == true
    state.lid_is_present = root.LidIsPresent == true
    return true
  end

  local function read_display()
    local p = client.get_all(name, DISPLAY, DEVICE)
    if not p then
      assign(state.display, empty_display())
      return
    end
    local row = upower.device_row(DISPLAY, p)
    assign(state.display, {
      present = row.present, percentage = row.percentage, state = row.state,
      charging = row.charging, time_to_empty = row.time_to_empty,
      time_to_full = row.time_to_full, icon_name = row.icon_name,
      energy_rate = row.energy_rate, kind = row.kind, warning_level = row.warning_level,
    })
  end

  local schedule = dbus_client.debounce(options.debounce_ms or 60, function() refresh_devices() end)

  function refresh_devices()
    local paths = client.call1(name, ROOT, NAME, "EnumerateDevices")
    local list, peripherals = {}, {}
    local present = {}
    for _, path in ipairs(type(paths) == "table" and paths or {}) do
      local p = client.get_all(name, path, DEVICE)
      if p then
        present[path] = true
        watch(path, function() schedule() end)
        local row = upower.device_row(path, p)
        list[#list + 1] = row
        if not row.power_supply and row.kind ~= "line_power" and row.present then
          peripherals[#peripherals + 1] = row
        end
      end
    end
    unwatch_missing(present)
    table.sort(list, function(a, b) return a.path < b.path end)
    table.sort(peripherals, function(a, b) return a.path < b.path end)
    rows = list
    state.devices:replace(list, "path")
    state.peripherals:replace(peripherals, "path")
  end

  local function clear_devices()
    rows = {}
    state.devices:replace({}, "path")
    state.peripherals:replace({}, "path")
    assign(state.display, empty_display())
  end

  local function refresh()
    if read_root() then
      read_display()
      refresh_devices()
    else
      clear_devices()
    end
  end

  local function read_profiles()
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

  --- Whether UPower is on the bus.
  function power.available() return state.available end

  --- The device rows as plain tables.
  function power.devices() return rows end

  function power.refresh()
    refresh()
    read_profiles()
  end

  --- Switches the power profile: "power-saver", "balanced" or
  --- "performance" (whichever `state.profiles.list` offers).
  function power.set_profile(profile)
    if not profile_service then return nil, "no power profile service" end
    local ok, err = client.set(profile_service.name, profile_service.path,
      profile_service.interface, "ActiveProfile", tostring(profile))
    read_profiles()
    return ok, err
  end

  client.watch_name(name, function() refresh() end)
  client.on_properties(name, ROOT, function(_, changed)
    if changed.OnBattery ~= nil then state.on_battery = changed.OnBattery == true end
    if changed.LidIsClosed ~= nil then state.lid_is_closed = changed.LidIsClosed == true end
    if changed.LidIsPresent ~= nil then state.lid_is_present = changed.LidIsPresent == true end
  end)
  watch(DISPLAY, function() read_display() end)
  client.on_signal(name, ROOT, NAME, "DeviceAdded", function() schedule() end)
  client.on_signal(name, ROOT, NAME, "DeviceRemoved", function(body)
    local path = dbus_client.first(body)
    if type(path) == "string" then client.forget(path) end
    schedule()
  end)
  for _, service in ipairs(profile_services) do
    client.watch_name(service.name, function() read_profiles() end)
    client.on_properties(service.name, service.path, function() read_profiles() end)
  end

  refresh()
  read_profiles()
  return power
end

return upower
