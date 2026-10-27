-- Characterization of lib.services.upower against a fake client
-- (PLAN item 7): the behaviour a registry migration must keep.
--
--     morf test library/tests/upower_spec.lua

local test = morf.test

local HOST = [[
  local db = require("lib.services.dbus")
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

-- What the module did before it was a descriptor and must keep doing, over
-- the shared stand-in for `morf.dbus`.
local BUS_HOST = [[
  local fake = require("lib.testing.dbus")
  local U, ROOT = "org.freedesktop.UPower", "/org/freedesktop/UPower"
  local DEV, PROPS = "org.freedesktop.UPower.Device", "org.freedesktop.DBus.Properties"
  local DISPLAY, BAT = ROOT .. "/devices/DisplayDevice", ROOT .. "/devices/battery_BAT0"
  local PP, PP_PATH = "net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"
  local root = fake.object("system", U, ROOT, U, { OnBattery = true, LidIsClosed = true, LidIsPresent = true },
    { EnumerateDevices = function() return { { BAT } } end })
  local display = fake.object("system", U, DISPLAY, DEV, { Percentage = 40, State = 2, IsPresent = true, Type = 2 })
  local battery = fake.object("system", U, BAT, DEV,
    { Percentage = 40, State = 2, IsPresent = true, Type = 2, PowerSupply = true })
  local profiles = fake.object("system", PP, PP_PATH, PP, { ActiveProfile = "balanced",
    Profiles = { { Profile = "balanced" }, { Profile = "performance" } } })
  local function owner(dest, present)
    fake.own("system", dest, present)
    fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
      "NameOwnerChanged", { dest, present and "" or ":1.4", present and ":1.5" or "" })
  end
  local function changed(dest, path, interface, props)
    fake.emit("system", dest, path, PROPS, "PropertiesChanged", { interface, props, {} })
  end

  local power = require("lib.services.upower").connect({ dbus = fake.dbus, debounce_ms = 5 })
  local s = power.state
  morf.ipc.read = function()
    local row = power.devices()[1]
    return { available = s.available, on_battery = s.on_battery, lid_closed = s.lid_is_closed,
             lid_present = s.lid_is_present, percentage = s.display.percentage,
             battery = row and row.percentage or -1, profile = s.profiles.active,
             profiles = s.profiles.available, calls = #fake.calls }
  end
  morf.ipc.profile_elsewhere = function()
    profiles.ActiveProfile = "performance"
    changed(PP, PP_PATH, PP, { ActiveProfile = "performance" })
  end
  morf.ipc.profiles_present = function(present) owner(PP, present) end
  morf.ipc.vanish = function() owner(U, false) end
  morf.ipc.refresh = function()
    root.OnBattery = false
    power.refresh()
    return s.on_battery
  end
  morf.ipc.discharge = function(percent)
    battery.Percentage = percent
    changed(U, BAT, DEV, { Percentage = percent })
  end
  morf.ipc.heard = function(percent) changed(U, DISPLAY, DEV, { Percentage = percent }) end
  morf.ipc.restart = function(percent)
    owner(U, false)
    display.Percentage = percent
    owner(U, true)
  end
]]

test.describe("upower (over the bus stand-in)", function()
  test.before_each(function()
    test.load("upower-bus-fixture.lua", { source = BUS_HOST, size = { 20, 20 } })
    test.settle()
  end)

  test.it("follows the power profile when something else changes it", function()
    test.eq(test.ipc("read").profile, "balanced")
    test.ipc("profile_elsewhere")
    test.settle()
    test.eq(test.ipc("read").profile, "performance")
    test.ipc("profiles_present", false)
    test.settle()
    test.eq(test.ipc("read").profiles, false, "the daemon left")
    test.ipc("profiles_present", true)
    test.settle()
    local back = test.ipc("read")
    test.eq(back.profiles, true, "and came back")
    test.eq(back.profile, "performance")
  end)

  test.it("lets go of the battery and the lid when UPower goes", function()
    local read = test.ipc("read")
    test.eq(read.on_battery, true)
    test.eq(read.lid_closed, true)
    test.ipc("vanish")
    test.settle()
    local gone = test.ipc("read")
    test.eq(gone.available, false)
    test.eq(gone.on_battery, false)
    test.eq(gone.lid_closed, false)
    test.eq(gone.lid_present, false)
    test.eq(gone.percentage, 0)
    test.eq(gone.battery, -1, "no devices")
  end)

  test.it("refreshes at once when asked", function()
    test.eq(test.ipc("refresh"), false, "read before refresh returns")
  end)

  test.it("merges a device's change without reading the bus again", function()
    local before = test.ipc("read")
    test.eq(before.battery, 40)
    test.ipc("discharge", 35)
    test.settle()
    local after = test.ipc("read")
    test.eq(after.battery, 35)
    test.eq(after.calls, before.calls, "no re-read, no re-enumeration")
  end)

  test.it("reads a restarted UPower afresh, not what the old one said", function()
    test.ipc("heard", 50)
    test.settle()
    test.eq(test.ipc("read").percentage, 50)
    test.ipc("restart", 80)
    test.settle()
    test.eq(test.ipc("read").percentage, 80)
  end)
end)
