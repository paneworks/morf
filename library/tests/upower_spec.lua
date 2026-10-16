-- Characterization of lib.services.upower against a fake client
-- (PLAN item 7): the behaviour a registry migration must keep.
--
--     morf test library/tests/upower_spec.lua

local test = morf.test

local HOST = [[
  local db = require("lib.services.dbus_client")
  local NAME = "org.freedesktop.UPower"
  local ROOT = "/org/freedesktop/UPower"
  local DISPLAY = ROOT .. "/devices/DisplayDevice"
  local BATTERY = ROOT .. "/devices/battery_BAT0"
  local MOUSE = ROOT .. "/devices/mouse_1"
  local owned = true
  local root_props = { OnBattery = true, LidIsClosed = false, LidIsPresent = true }
  local display_props = { IsPresent = true, Percentage = 64, State = 1, Type = 2,
                          PowerSupply = true, IsRechargeable = true, IconName = "battery-060" }
  local battery_props = { IsPresent = true, Percentage = 64, State = 1, Type = 2,
                          PowerSupply = true, IsRechargeable = true }
  local mouse_props = { IsPresent = true, Percentage = 88, State = 2, Type = 5,
                        PowerSupply = false, IsRechargeable = true }
  local profiles_props = { ActiveProfile = "balanced", PerformanceDegraded = "",
                           Profiles = { { Profile = "balanced" }, { Profile = "power-saver" } } }
  local changes, signals = {}, {}
  local client = {}
  client.has_owner = function(name) return owned or name:find("PowerProfiles") ~= nil end
  client.get_all = function(_, path)
    if path == ROOT then return root_props end
    if path == DISPLAY then return display_props end
    if path == BATTERY then return battery_props end
    if path == MOUSE then return mouse_props end
    return profiles_props
  end
  client.call1 = function(_, _, _, method)
    if method == "EnumerateDevices" then return { BATTERY, MOUSE } end
  end
  client.watch_name = function() return { close = function() end } end
  client.on_properties = function(_, path, fn)
    changes[path] = fn
    return { close = function() end }
  end
  client.on_signal = function(_, _, _, member, fn) signals[member] = fn end
  client.set = function() return true end
  client.forget = function() end
  db.new = function() return client end

  local upower = require("lib.services.upower")
  local power = upower.connect({ debounce_ms = 1 })
  morf.ipc.read = function()
    local names = {}
    for _, row in ipairs(power.devices()) do names[#names + 1] = row.kind .. ":" .. row.percentage end
    return {
      available = power.state.available, on_battery = power.state.on_battery,
      percentage = power.state.display.percentage,
      charging = power.state.display.charging,
      devices = table.concat(names, ","),
      peripheral_count = #power.state.peripherals,
      profile = power.state.profiles.active,
      profile_count = #power.state.profiles.list,
    }
  end
  morf.ipc.set_profile = function() return power.set_profile("power-saver") end
  morf.ipc.vanish = function() owned = false end
]]

test.describe("upower (characterization)", function()
  test.it("reads root, display, devices, peripherals and profiles", function()
    test.load("upower-fixture.lua", { source = HOST, size = { 20, 20 } })
    test.settle()
    local read = test.ipc("read")
    test.eq(read.available, true)
    test.eq(read.on_battery, true)
    test.eq(read.percentage, 64)
    test.eq(read.charging, true)
    test.eq(read.devices, "battery:64,mouse:88")
    test.eq(read.peripheral_count, 1)
    test.eq(read.profile, "balanced")
    test.eq(read.profile_count, 2)
    test.truthy(test.ipc("set_profile"))
  end)
end)
