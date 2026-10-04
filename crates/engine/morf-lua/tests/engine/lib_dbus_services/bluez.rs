//! BlueZ: the object tree mirrored, and the brief wait for it.

use super::*;

#[test]
fn bluez_mirrors_the_object_tree_and_waits_briefly() {
    let verdict = run_with_fake(
        "test-bluez",
        r#"
        local B = "org.bluez"
        local HCI = "/org/bluez/hci0"
        local A, S = HCI .. "/dev_AA", HCI .. "/dev_BB"
        local PROPS = "org.freedesktop.DBus.Properties"
        local OM = "org.freedesktop.DBus.ObjectManager"
        fake.object("system", B, HCI, "org.bluez.Adapter1", {
            Alias = "laptop", Address = "00:11", Powered = true, Discovering = false,
            Discoverable = false, Pairable = true,
        }, {
            StartDiscovery = function() return nil end,
            StopDiscovery = function() return nil end,
            RemoveDevice = function() return nil end,
        })
        local device_methods = {
            -- Connecting outlasts the short wait; BlueZ carries on.
            Connect = function() error("org.freedesktop.DBus.Error.NoReply: Did not receive a reply", 0) end,
            Disconnect = function() return nil end,
            Pair = function() return nil end,
        }
        fake.object("system", B, A, "org.bluez.Device1", {
            Adapter = HCI, Address = "AA:AA", Name = "Buds", Alias = "Buds", Icon = "audio-headset",
            Paired = true, Trusted = true, Connected = true,
        }, device_methods)
        fake.object("system", B, A, "org.bluez.Battery1", { Percentage = 80 })
        fake.object("system", B, S, "org.bluez.Device1", {
            Adapter = HCI, Address = "BB:BB", Name = "Stranger", Alias = "Stranger",
            Paired = false, Trusted = false, Connected = false, RSSI = -60,
        }, device_methods)
        fake.object("system", B, A .. "/service0001", "org.bluez.GattService1", {})

        local bt = require("lib.services.bluez").connect({ dbus = fake.dbus, debounce_ms = 10, discovery_poll_ms = 40 })
        local s = bt.state
        local eq = fake.eq
        fake.steps({
            function()
                eq(s.available, true, "available")
                eq(s.adapter, HCI, "adapter")
                eq(s.powered, true, "powered")
                eq(s.pairable, true, "pairable")
                eq(s.adapters:len(), 1, "adapters")
                eq(s.devices:len(), 2, "devices, and no GATT services")
                eq(s.connected_count, 1, "connected")
                local buds, stranger = s.devices:get(1), s.devices:get(2)
                eq(buds.alias, "Buds", "connected first")
                eq(buds.battery, 80, "battery")
                eq(buds.has_battery, true, "has battery")
                eq(buds.icon, "audio-headset", "icon")
                eq(stranger.rssi, -60, "rssi")
                eq(stranger.in_range, true, "in range")
                eq(stranger.battery, -1, "no battery")
                eq(fake.subscribed("system", B, A, PROPS, "PropertiesChanged"), 1, "paired watched")
                eq(fake.subscribed("system", B, S, PROPS, "PropertiesChanged"), 0, "strangers not")
            end,
            function()
                assert(bt.set_powered(false))
                local set = fake.calls_to("Set")[1]
                eq(set.path, HCI, "adapter path")
                eq(set.property, "Powered", "powered")
                eq(set.value, false, "off")
                local ok, note = bt.connect("AA:AA")
                eq(ok, true, "connect accepted")
                eq(note, "pending", "and still going")
                assert(bt.pair(S))
                eq(fake.calls_to("Pair")[1].path, S, "paired the stranger")
                assert(bt.trust({ path = S }))
                eq(fake.calls_to("Set")[2].property, "Trusted", "trust")
                assert(bt.remove(S))
                local remove = fake.calls_to("RemoveDevice")[1]
                eq(remove.path, HCI, "removed via adapter")
                eq(remove.args[1].signature, "o", "device path typed")
                eq(remove.args[1].value, S, "which device")
                assert(bt.start_discovery())
                eq(#fake.calls_to("StartDiscovery"), 1, "scan")
                eq(fake.last_timeout, 2500, "short action wait")
            end,
            function()
                -- The service's tree changes, and says so.
                fake.props("system", B, A, "org.bluez.Battery1").Percentage = 50
                local adapter = fake.props("system", B, HCI, "org.bluez.Adapter1")
                adapter.Powered, adapter.Discovering = false, true
                fake.object("system", B, HCI .. "/dev_CC", "org.bluez.Device1",
                    { Adapter = HCI, Address = "CC:CC", Alias = "CC:CC", Paired = false, Connected = false },
                    device_methods)
                fake.remove("system", B, S)
                fake.emit("system", B, A, PROPS, "PropertiesChanged", { "org.bluez.Battery1", { Percentage = 50 }, {} })
                fake.emit("system", B, HCI, PROPS, "PropertiesChanged", { "org.bluez.Adapter1", { Powered = false, Discovering = true }, {} })
                fake.emit("system", B, "/", OM, "InterfacesAdded", { HCI .. "/dev_CC", {
                    ["org.bluez.Device1"] = { Adapter = HCI, Address = "CC:CC", Alias = "CC:CC", Paired = false, Connected = false },
                } })
                fake.emit("system", B, "/", OM, "InterfacesRemoved", { S, { "org.bluez.Device1" } })
            end,
            function()
                eq(s.devices:get(1).battery, 50, "battery moved")
                eq(s.powered, false, "powered off")
                eq(s.discovering, true, "discovering")
                eq(s.devices:len(), 2, "one came, one went")
                eq(s.devices:get(2).address, "CC:CC", "the newcomer")
                eq(s.devices:get(2).named, false, "unnamed")
                eq(s.devices:get(2).in_range, false, "not heard yet")
                -- While discovering, strangers move by polling: the tree
                -- changes with no signal at all.
                fake.props("system", B, HCI .. "/dev_CC", "org.bluez.Device1").RSSI = -40
            end,
            function()
                eq(s.devices:get(2).rssi, -40, "polled")
                eq(s.devices:get(2).in_range, true, "heard")
                fake.own("system", B, false)
                fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { B, ":1.2", "" })
            end,
            function()
                eq(s.available, false, "gone")
                eq(s.devices:len(), 0, "no devices")
                eq(s.powered, false, "no radio")
            end,
        }, done)
        "#,
    );
    assert_eq!(verdict, "ok");
}
