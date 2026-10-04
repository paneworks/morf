//! UPower: batteries and power profiles, read without starting anything.

use super::*;

#[test]
fn upower_reads_batteries_and_profiles_without_starting_anything() {
    let verdict = run_with_fake(
        "test-upower",
        r#"
        local U = "org.freedesktop.UPower"
        local ROOT = "/org/freedesktop/UPower"
        local DEV = "org.freedesktop.UPower.Device"
        local DISPLAY = ROOT .. "/devices/DisplayDevice"
        local BAT, MOUSE, AC = ROOT .. "/devices/battery_BAT0", ROOT .. "/devices/mouse_1", ROOT .. "/devices/line_power_AC"
        local PROPS = "org.freedesktop.DBus.Properties"
        local enumerated = { BAT, MOUSE, AC }
        fake.object("system", U, ROOT, U, { OnBattery = true, LidIsClosed = false, LidIsPresent = true }, {
            EnumerateDevices = function() return { enumerated } end,
        })
        fake.object("system", U, DISPLAY, DEV, {
            Type = 2, State = 2, Percentage = 42.0, TimeToEmpty = 3600, TimeToFull = 0,
            IconName = "battery-good-symbolic", EnergyRate = 7.5, IsPresent = true, WarningLevel = 1,
        })
        fake.object("system", U, BAT, DEV, { Type = 2, PowerSupply = true, Percentage = 42.0, State = 2, IsPresent = true, Model = "BAT" })
        fake.object("system", U, MOUSE, DEV, { Type = 5, PowerSupply = false, Percentage = 15.0, State = 2, IsPresent = true, Model = "Mouse" })
        fake.object("system", U, AC, DEV, { Type = 1, PowerSupply = true, Online = false, IsPresent = false })
        -- Only the old name answers; the new one is absent and must not be
        -- read into existence.
        local PP = "net.hadess.PowerProfiles"
        fake.object("system", PP, "/net/hadess/PowerProfiles", PP, {
            ActiveProfile = "balanced", PerformanceDegraded = "",
            Profiles = { { Profile = "power-saver", Driver = "x" }, { Profile = "balanced", Driver = "x" },
                         { Profile = "performance", Driver = "x" } },
        })

        local upower = require("lib.services.upower")
        local power = upower.connect({ dbus = fake.dbus, debounce_ms = 10 })
        local s = power.state
        local eq = fake.eq
        fake.steps({
            function()
                eq(s.available, true, "available")
                eq(s.on_battery, true, "on battery")
                eq(s.lid_is_present, true, "lid")
                eq(s.display.percentage, 42.0, "percentage")
                eq(s.display.state, "discharging", "state")
                eq(s.display.charging, false, "not charging")
                eq(s.display.time_to_empty, 3600, "time to empty")
                eq(s.display.icon_name, "battery-good-symbolic", "icon")
                eq(s.display.energy_rate, 7.5, "rate")
                eq(s.devices:len(), 3, "devices")
                eq(s.peripherals:len(), 1, "peripherals")
                eq(s.peripherals:get(1).kind, "mouse", "mouse")
                eq(s.peripherals:get(1).percentage, 15.0, "mouse battery")
                eq(s.profiles.available, true, "profiles")
                eq(s.profiles.service, PP, "fell back to the old name")
                eq(s.profiles.active, "balanced", "active profile")
                eq(s.profiles.list:len(), 3, "three profiles")
                for _, call in ipairs(fake.calls) do
                    if call.dest == "org.freedesktop.UPower.PowerProfiles" then
                        error("read an absent service, which would start it")
                    end
                end
                eq(upower.format_time(3720), "1 h 2 min", "format")
                eq(upower.format_time(0), "", "unknown time")
            end,
            function()
                assert(power.set_profile("performance"))
                local set = fake.calls_to("Set")[1]
                eq(set.dest, PP, "profile service")
                eq(set.property, "ActiveProfile", "property")
                eq(set.value, "performance", "value")
                eq(s.profiles.active, "performance", "re-read")
                local display = fake.props("system", U, DISPLAY, DEV)
                display.State, display.Percentage = 1, 43.0
                fake.emit("system", U, DISPLAY, PROPS, "PropertiesChanged", { DEV, { State = 1, Percentage = 43.0 }, {} })
                fake.emit("system", U, ROOT, PROPS, "PropertiesChanged", { U, { OnBattery = false }, {} })
                enumerated[2] = AC
                enumerated[3] = nil
                fake.emit("system", U, ROOT, U, "DeviceRemoved", MOUSE)
            end,
            function()
                eq(s.display.charging, true, "charging")
                eq(s.display.percentage, 43.0, "moved")
                eq(s.on_battery, false, "plugged in")
                eq(s.devices:len(), 2, "mouse gone")
                eq(s.peripherals:len(), 0, "no peripherals")
                fake.own("system", U, false)
                fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { U, ":1.3", "" })
            end,
            function()
                eq(s.available, false, "gone")
                eq(s.devices:len(), 0, "empty")
                eq(s.display.percentage, 0, "no display")
            end,
        }, done)
        "#,
    );
    assert_eq!(verdict, "ok");
}
