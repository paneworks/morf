//! The system-service libraries in `examples/lib`, against fake services.
//!
//! NetworkManager, BlueZ, UPower, MPRIS and logind are pure Lua on top of the
//! engine's generic D-Bus client, and every one of them can change the
//! machine it runs on: turn the radio off, forget a network, suspend. So none
//! of these tests goes near a real service. Most hand the library a stand-in
//! for `morf.dbus` written in Lua (`lib_dbus_fake.lua`), which answers in the
//! shapes the engine's decoder produces and records every argument as the
//! library typed it — the thing a real service would reject if it were wrong.
//! One goes over a real session bus, to a fake player this test owns under a
//! name no real player uses, to pin the decoder's shapes to the fake's.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const FAKE: &str = include_str!("lib_dbus_fake.lua");

/// Runs `body` with `fake`, `morf`, `ui` and `done(text)` in scope, beside the
/// examples so `require("lib.x")` finds the libraries, until `done` is called.
pub(super) fn run_with_fake(name: &str, body: &str) -> String {
    let script = format!(
        "local fake = (function()\n{FAKE}\nend)()\n\
         local morf = require(\"morf\")\n\
         local ui = require(\"morf.ui\")\n\
         local verdict = morf.signal(\"verdict\", \"running\")\n\
         local function done(text) verdict:set(text) end\n\
         ui.Text {{ text = function() return verdict:get() end }}\n\
         local function bytes(s) return {{ s:byte(1, -1) }} end\n\
         {body}"
    );
    let path = format!("{}/../../examples/{name}.lua", env!("CARGO_MANIFEST_DIR"));
    let mut runtime = Runtime::default();
    runtime
        .execute(&path, script.as_bytes())
        .unwrap_or_else(|error| panic!("{name} failed to load: {error}"));
    let root = runtime.scene().roots()[0];
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let text = runtime
            .scene()
            .string_value(root, "text")
            .unwrap()
            .to_owned();
        if text != "running" || Instant::now() > deadline {
            return text;
        }
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn networkmanager_reads_the_tree_and_types_every_action() {
    let verdict = run_with_fake(
        "test-networkmanager",
        r#"
        local NM = "org.freedesktop.NetworkManager"
        local ROOT = "/org/freedesktop/NetworkManager"
        local DEV1, DEV2 = ROOT .. "/Devices/1", ROOT .. "/Devices/2"
        local AP = ROOT .. "/AccessPoint/"
        local AC1 = ROOT .. "/ActiveConnection/1"
        local S1, S2 = ROOT .. "/Settings/1", ROOT .. "/Settings/2"
        local IP1 = ROOT .. "/IP4Config/1"
        local PROPS = "org.freedesktop.DBus.Properties"

        fake.object("system", NM, ROOT, NM, {
            Version = "1.50.0", State = 70, Connectivity = 4, NetworkingEnabled = true,
            WirelessEnabled = true, WirelessHardwareEnabled = true,
            PrimaryConnection = AC1, PrimaryConnectionType = "802-11-wireless",
            ActiveConnections = { AC1 }, AllDevices = { DEV1, DEV2 },
        }, {
            -- A lone `o` reply arrives as a one-element list; the library
            -- has to take the path out of it.
            ActivateConnection = function() return { ROOT .. "/ActiveConnection/2" } end,
            AddAndActivateConnection = function() return { ROOT .. "/Settings/9", ROOT .. "/ActiveConnection/3" } end,
            DeactivateConnection = function() return nil end,
        })
        fake.object("system", NM, DEV1, NM .. ".Device", {
            Interface = "wlan0", DeviceType = 2, State = 100, Driver = "iwlwifi", Managed = true,
            ActiveConnection = AC1, Ip4Config = IP1,
        }, { Disconnect = function() return nil end })
        fake.object("system", NM, DEV1, NM .. ".Device.Wireless", {
            HwAddress = "AA:AA", AccessPoints = { AP .. "1", AP .. "2", AP .. "3", AP .. "4", AP .. "5" },
            ActiveAccessPoint = AP .. "1", LastScan = 100,
        }, { RequestScan = function() return nil end })
        fake.object("system", NM, DEV2, NM .. ".Device", {
            Interface = "eth0", DeviceType = 1, State = 30, Driver = "r8169", Managed = true,
        })
        fake.object("system", NM, DEV2, NM .. ".Device.Wired", { Carrier = false, Speed = 0, HwAddress = "BB:BB" })
        local function ap(n, ssid, strength, freq, flags, wpa, rsn)
            fake.object("system", NM, AP .. n, NM .. ".AccessPoint", {
                Ssid = bytes(ssid), Strength = strength, Frequency = freq, Flags = flags,
                WpaFlags = wpa, RsnFlags = rsn, HwAddress = "00:00:00:00:00:0" .. n, MaxBitrate = 0,
            })
        end
        ap(1, "home", 70, 5180, 1, 0, 0x188)
        ap(2, "home", 90, 2412, 1, 0, 0x188)   -- the same network, louder, not in use
        ap(3, "cafe", 40, 2437, 0, 0, 0)
        ap(4, "fortress", 60, 5500, 1, 0, 0x400)
        ap(5, "", 99, 2412, 0, 0, 0)            -- hidden: no row
        fake.object("system", NM, IP1, NM .. ".IP4Config", {
            AddressData = { { address = "192.168.1.5", prefix = 24 } },
        })
        fake.object("system", NM, AC1, NM .. ".Connection.Active", {
            Id = "home", Uuid = "u-home", Type = "802-11-wireless", State = 2, Default = true,
            Vpn = false, Devices = { DEV1 }, Connection = S1,
        })
        fake.object("system", NM, ROOT .. "/Settings", NM .. ".Settings", {}, {
            ListConnections = function() return { { S1, S2 } } end,
        })
        fake.object("system", NM, S1, NM .. ".Settings.Connection", {}, {
            GetSettings = function() return { {
                connection = { id = "home", uuid = "u-home", type = "802-11-wireless", timestamp = 5 },
                ["802-11-wireless"] = { ssid = bytes("home") },
            } } end,
            Delete = function() return nil end,
        })
        fake.object("system", NM, S2, NM .. ".Settings.Connection", {}, {
            GetSettings = function() return { {
                connection = { id = "work", uuid = "u-work", type = "vpn", timestamp = 1 },
            } } end,
        })

        local net = require("lib.networkmanager").connect({ dbus = fake.dbus, debounce_ms = 10 })
        local s = net.state
        local eq = fake.eq
        -- Actions answer on a later turn; what they answered lands here.
        local answered = {}
        local function into(key) return function(value, err) answered[key] = value or err end end

        fake.steps({
            function()
                eq(s.available, true, "available")
                eq(s.version, "1.50.0", "version")
                eq(s.state, "connected_global", "state")
                eq(s.connectivity, "full", "connectivity")
                eq(s.primary.id, "home", "primary")
                eq(s.wifi.device, "wlan0", "wifi device")
                eq(s.wifi.ssid, "home", "wifi ssid")
                eq(s.wifi.strength, 70, "wifi strength")
                eq(s.wifi.security, "wpa2", "wifi security")
                eq(s.wifi.connected, true, "wifi connected")
                eq(s.wired.device, "eth0", "wired device")
                eq(s.wired.connected, false, "wired connected")
                eq(s.wired.hw_address, "BB:BB", "wired hw")
                eq(s.devices:len(), 2, "devices")
                eq(s.devices:get(1).ip4, "192.168.1.5/24", "ip4")
                -- One row per network; the one in use leads, then by strength.
                eq(s.access_points:len(), 3, "access points")
                local first, second, third = s.access_points:get(1), s.access_points:get(2), s.access_points:get(3)
                eq(first.ssid, "home", "first ap")
                eq(first.in_use, true, "in use")
                eq(first.strength, 70, "in-use radio represents the network")
                eq(first.known, true, "known")
                eq(first.band, "5", "band")
                eq(second.ssid, "fortress", "second ap")
                eq(second.security, "wpa3", "sae is wpa3")
                eq(third.ssid, "cafe", "third ap")
                eq(third.security, "open", "open")
                eq(third.secure, false, "open is not secure")
                eq(s.known_connections:len(), 2, "known")
                eq(s.vpn_connections:len(), 1, "vpn")
                eq(s.vpn_connections:get(1).id, "work", "vpn id")
                eq(s.active_connections:len(), 1, "active")
                eq(fake.subscribed("system", NM, DEV1, PROPS, "PropertiesChanged"), 1, "device watched once")
                eq(fake.subscribed("system", NM, AP .. "3", PROPS, "PropertiesChanged"), 0, "access points not watched")
            end,
            function()
                assert(net.request_scan())
                local scan = fake.calls_to("RequestScan")[1]
                eq(scan.path, DEV1, "scan path")
                eq(scan.args[1].signature, "a{sv}", "scan options typed")

                -- An open network nobody saved: a new profile, no security.
                eq(net.connect("cafe", nil, nil, into("open")), true, "connect sent")
                eq(answered.open, nil, "and not answered on the same turn")
                local add = fake.calls_to("AddAndActivateConnection")[1]
                eq(add.args[1].signature, "a{sa{sv}}", "settings typed")
                eq(add.args[1].value["802-11-wireless"].ssid.signature, "ay", "ssid is bytes")
                eq(fake.plain(add.args[1].value["802-11-wireless"].ssid.value)[1], string.byte("c"), "ssid bytes")
                eq(add.args[1].value["802-11-wireless-security"], nil, "no security for open")
                eq(add.args[2].signature, "o", "device is a path")
                eq(add.args[2].value, DEV1, "device")
                eq(add.args[3].value, AP .. "3", "access point")

                -- WPA3 wants SAE, and a password.
                local ok, err = net.connect("fortress")
                eq(ok, nil, "no password")
                eq(err, "a password is needed", "why")
                net.connect(s.access_points:get(2), "sesame")
                local sae = fake.calls_to("AddAndActivateConnection")[2]
                eq(sae.args[1].value["802-11-wireless-security"]["key-mgmt"], "sae", "sae")
                eq(sae.args[1].value["802-11-wireless-security"].psk, "sesame", "psk")

                -- A saved network is activated as saved.
                assert(net.connect("home", nil, nil, into("known")))
                local activate = fake.calls_to("ActivateConnection")[1]
                eq(activate.args[1].signature, "o", "profile typed")
                eq(activate.args[1].value, S1, "profile")
                eq(activate.args[2].value, DEV1, "device")
                eq(activate.args[3].value, AP .. "1", "specific ap")

                assert(net.disconnect())
                eq(fake.calls_to("DeactivateConnection")[1].args[1].value, AC1, "deactivated")

                assert(net.activate_vpn("work"))
                local vpn = fake.calls_to("ActivateConnection")[2]
                eq(vpn.args[1].value, S2, "vpn profile")
                eq(vpn.args[2].value, "/", "no device")

                eq(net.forget("home"), 1, "forgot")
                eq(fake.calls_to("Delete")[1].path, S1, "deleted")

                assert(net.set_wifi(false))
                local set = fake.calls_to("Set")[1]
                eq(set.property, "WirelessEnabled", "radio property")
                eq(set.value, false, "radio off")
            end,
            function()
                eq(answered.open, ROOT .. "/ActiveConnection/3", "connect open")
                eq(answered.known, ROOT .. "/ActiveConnection/2", "known connect")
                for _, call in ipairs(fake.calls) do
                    if call.method == "AddAndActivateConnection" or call.method == "RequestScan"
                        or call.method == "Delete" or call.method == "Set" then
                        eq(call.async, true, call.method .. " did not wait")
                    end
                end
                eq(s.wifi_enabled, false, "radio state re-read")
                -- A scan finished: strengths move without per-AP subscriptions.
                fake.props("system", NM, AP .. "3", NM .. ".AccessPoint").Strength = 95
                fake.props("system", NM, DEV1, NM .. ".Device.Wireless").LastScan = 200
                fake.emit("system", NM, DEV1, PROPS, "PropertiesChanged",
                    { NM .. ".Device.Wireless", { LastScan = 200 }, {} })
            end,
            function()
                eq(s.access_points:get(2).ssid, "cafe", "cafe rose")
                eq(s.access_points:get(2).strength, 95, "new strength")
                eq(s.wifi.last_scan, 200, "last scan")
                -- The service goes away.
                fake.own("system", NM, false)
                fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { NM, ":1.5", "" })
            end,
            function()
                eq(s.available, false, "gone")
                eq(s.access_points:len(), 0, "no access points")
                eq(s.devices:len(), 0, "no devices")
                eq(fake.subscribed("system", NM, DEV1, PROPS, "PropertiesChanged"), 0, "device watch closed")
                eq(s.wifi.ssid, "", "no ssid")
                local ok = net.connect("home")
                eq(ok, nil, "nothing to connect with")
                fake.own("system", NM, true)
                fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { NM, "", ":1.6" })
            end,
            function()
                eq(s.available, true, "back")
                eq(s.access_points:len(), 3, "rows back")
                eq(fake.subscribed("system", NM, DEV1, PROPS, "PropertiesChanged"), 1, "and watched again")
            end,
        }, done)
        "#,
    );
    assert_eq!(verdict, "ok");
}

#[test]
fn networkmanager_is_empty_and_unstarted_when_absent() {
    // Reading an absent, activatable name would start it. A status bar
    // looking at the network must never be what brings the network up.
    let verdict = run_with_fake(
        "test-networkmanager-absent",
        r#"
        local net = require("lib.networkmanager").connect({ dbus = fake.dbus })
        fake.steps({
            function()
                fake.eq(net.available(), false, "unavailable")
                fake.eq(net.state.access_points:len(), 0, "empty")
                fake.eq(net.state.wifi.device, "", "no wifi")
                for _, call in ipairs(fake.calls) do
                    if call.dest ~= "org.freedesktop.DBus" then
                        error("called the absent service: " .. call.method)
                    end
                end
                local ok, err = net.request_scan()
                fake.eq(ok, nil, "no scan")
                fake.eq(err, "no wifi device", "why")
            end,
        }, done)
        "#,
    );
    assert_eq!(verdict, "ok");
}

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

        local bt = require("lib.bluez").connect({ dbus = fake.dbus, debounce_ms = 10, discovery_poll_ms = 40 })
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

        local upower = require("lib.upower")
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

#[test]
fn mpris_picks_the_active_player_and_interpolates_position() {
    let verdict = run_with_fake(
        "test-mpris",
        r#"
        local PATH = "/org/mpris/MediaPlayer2"
        local PLAYER = "org.mpris.MediaPlayer2.Player"
        local ROOT_IFACE = "org.mpris.MediaPlayer2"
        local PROPS = "org.freedesktop.DBus.Properties"
        local function player(short, props, identity)
            local name = "org.mpris.MediaPlayer2." .. short
            fake.object("session", name, PATH, ROOT_IFACE, { Identity = identity, DesktopEntry = short, CanRaise = true },
                { Raise = function() return nil end })
            local methods = {}
            for _, m in ipairs({ "PlayPause", "Play", "Pause", "Next", "Previous", "Seek", "SetPosition" }) do
                methods[m] = function() return nil end
            end
            return fake.object("session", name, PATH, PLAYER, props, methods), name
        end
        local alpha = player("alpha", {
            PlaybackStatus = "Paused", Rate = 1.0, Volume = 1.0, Position = 0,
            Metadata = { ["xesam:title"] = "A" }, CanPlay = true, CanPause = true,
        }, "Alpha")
        local beta, BETA = player("beta", {
            PlaybackStatus = "Playing", Rate = 1.0, Volume = 0.8, Position = 10000000,
            LoopStatus = "Playlist", Shuffle = true, CanSeek = true, CanGoNext = true,
            Metadata = { ["xesam:title"] = "B", ["xesam:artist"] = { "X", "Y" }, ["xesam:album"] = "Album",
                         ["mpris:length"] = 200000000, ["mpris:trackid"] = "/track/1",
                         ["mpris:artUrl"] = "file:///art.png" },
        }, "Beta")
        player("playerctld", { PlaybackStatus = "Playing", Metadata = {} }, "proxy")

        local now = 1000
        local media = require("lib.mpris").connect({
            dbus = fake.dbus, clock = function() return now end, tick_ms = 20, debounce_ms = 10,
        })
        local s = media.state
        local eq = fake.eq
        local function changed(name, props)
            -- Every player shares the path; the engine routes by sender, so
            -- only this player's subscription hears it.
            fake.emit("session", name, PATH, PROPS, "PropertiesChanged", { PLAYER, props, {} })
        end
        fake.steps({
            function()
                eq(s.available, true, "available")
                eq(s.count, 2, "playerctld is skipped")
                eq(s.active.name, BETA, "the playing one")
                eq(s.active.identity, "Beta", "identity")
                eq(s.active.title, "B", "title")
                eq(s.active.artist, "X, Y", "artists")
                eq(s.active.album, "Album", "album")
                eq(s.active.art_url, "file:///art.png", "art")
                eq(s.active.length, 200.0, "length in seconds")
                eq(s.active.loop, "playlist", "loop")
                eq(s.active.shuffle, true, "shuffle")
                eq(s.active.position, 10.0, "position")
                now = now + 5000
                eq(media.position(), 15.0, "interpolated")
            end,
            function()
                eq(s.active.position, 15.0, "the tick advanced it")
                assert(media.play_pause())
                eq(fake.calls_to("PlayPause")[1].dest, BETA, "to the active player")
                assert(media.seek(-5))
                local seek = fake.calls_to("Seek")[1]
                eq(seek.args[1].signature, "x", "microseconds, signed")
                eq(seek.args[1].value, -5000000, "offset")
                assert(media.set_position(30))
                local jump = fake.calls_to("SetPosition")[1]
                eq(jump.args[1].signature, "o", "track id is a path")
                eq(jump.args[1].value, "/track/1", "track")
                eq(jump.args[2].value, 30000000, "position")
                assert(media.set_volume(0.5))
                local volume = fake.calls_to("Set")[1]
                eq(volume.property, "Volume", "volume")
                eq(volume.value.signature, "d", "a double even when whole")
                assert(media.raise())
                eq(fake.calls_to("Raise")[1].iface, ROOT_IFACE, "raise is on the root interface")
            end,
            function()
                beta.PlaybackStatus = "Paused"
                changed(BETA, { PlaybackStatus = "Paused" })
            end,
            function()
                eq(s.active.name, BETA, "paused, but changed most recently")
                eq(s.active.playing, false, "paused")
                local frozen = s.active.position
                now = now + 5000
                eq(media.position(), frozen, "a paused player stays put")
                alpha.PlaybackStatus = "Playing"
                changed("org.mpris.MediaPlayer2.alpha", { PlaybackStatus = "Playing" })
            end,
            function()
                eq(s.active.name, "org.mpris.MediaPlayer2.alpha", "playing wins")
                media.set_active(BETA)
                eq(s.active.name, BETA, "pinned")
                fake.own("session", BETA, false)
                fake.emit("session", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { BETA, ":1.4", "" })
            end,
            function()
                eq(s.count, 1, "beta left")
                eq(fake.subscribed("session", BETA, PATH, PROPS, "PropertiesChanged"), 0,
                    "and its subscription went with it")
                eq(fake.subscribed("session", "org.mpris.MediaPlayer2.alpha", PATH, PROPS,
                    "PropertiesChanged"), 1, "alpha's stays")
                eq(s.active.name, "org.mpris.MediaPlayer2.alpha", "the pin went with it")
                player("gamma", { PlaybackStatus = "Stopped", Metadata = {} }, "Gamma")
                fake.emit("session", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "NameOwnerChanged", { "org.mpris.MediaPlayer2.gamma", "", ":1.9" })
            end,
            function()
                eq(s.count, 2, "gamma arrived")
                eq(s.players:get(2).identity, "Gamma", "listed")
            end,
        }, done)
        "#,
    );
    assert_eq!(verdict, "ok");
}

#[test]
fn logind_reads_the_session_and_holds_inhibitors_by_process() {
    let backlight = std::env::temp_dir().join(format!("morf-backlight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&backlight);
    std::fs::create_dir_all(backlight.join("panel")).unwrap();
    std::fs::write(backlight.join("panel/brightness"), "300\n").unwrap();
    std::fs::write(backlight.join("panel/max_brightness"), "600\n").unwrap();
    let body = format!(
        r#"
        local L = "org.freedesktop.login1"
        local M = "/org/freedesktop/login1"
        local MANAGER = "org.freedesktop.login1.Manager"
        local SESSION = "org.freedesktop.login1.Session"
        local REAL = M .. "/session/_33"
        local function ok() return nil end
        fake.object("system", L, M, MANAGER, {{
            LidClosed = false, HandleLidSwitch = "suspend", Docked = false, IdleHint = false,
            PreparingForSleep = false,
        }}, {{
            CanSuspend = function() return "yes" end,
            CanHibernate = function() return "challenge" end,
            CanHybridSleep = function() return "no" end,
            CanSuspendThenHibernate = function() return "na" end,
            CanReboot = function() return "yes" end,
            CanPowerOff = function() return "yes" end,
            GetSession = function(args) if args[1] == "3" then return {{ REAL }} end error("no session") end,
            Inhibit = function() return {{ fake.fd("inhibitor") }} end,
            ListInhibitors = function() return {{ {{ {{ "sleep", "NM", "networks", "delay", 0, 12 }} }} }} end,
            Suspend = ok, Reboot = ok, PowerOff = ok, Hibernate = ok,
        }})
        fake.object("system", L, M .. "/session/auto", SESSION, {{ Id = "3" }})
        fake.object("system", L, REAL, SESSION, {{
            Id = "3", Name = "me", User = {{ 1000, "/u" }}, Seat = {{ "seat0", "/s" }}, Type = "wayland",
            Class = "user", Active = true, LockedHint = false, IdleHint = false, VTNr = 2, State = "active",
        }}, {{ Lock = ok, SetLockedHint = ok, SetBrightness = ok }})

        local login = require("lib.logind").connect({{
            dbus = fake.dbus, backlight_dir = "{dir}", udev = false,
        }})
        local s = login.state
        local eq = fake.eq
        local locked, sleeping = 0, {{}}
        local inhibitor, early
        login.on_lock(function() locked = locked + 1 end)
        login.on_prepare_for_sleep(function(going) sleeping[#sleeping + 1] = going end)
        fake.steps({{
            function()
                eq(s.available, true, "available")
                eq(s.session.id, "3", "session id")
                eq(s.session.path, REAL, "the real path, not auto")
                eq(s.session.user, "me", "user")
                eq(s.session.uid, 1000, "uid")
                eq(s.session.seat, "seat0", "seat")
                eq(s.session.type, "wayland", "type")
                eq(s.session.active, true, "active")
                eq(s.can.suspend, "yes", "can suspend")
                eq(s.can.hibernate, "challenge", "hibernate needs auth")
                eq(s.can.suspend_then_hibernate, "na", "na")
                eq(s.handle_lid_switch, "suspend", "lid switch")
                eq(s.inhibitors:len(), 1, "inhibitors")
                eq(s.inhibitors:get(1).who, "NM", "who")
                eq(s.brightness.device, "panel", "backlight")
                eq(s.brightness.max, 600, "max")
                eq(s.brightness.percent, 50, "percent")
            end,
            function()
                assert(login.lock())
                eq(fake.calls_to("Lock")[1].path, REAL, "lock the session")
                assert(login.set_brightness(0.25))
                local b = fake.calls_to("SetBrightness")[1]
                eq(b.args[1], "backlight", "subsystem")
                eq(b.args[2], "panel", "device")
                eq(b.args[3].signature, "u", "unsigned")
                eq(b.args[3].value, 150, "a quarter")
                eq(s.brightness.value, 150, "state follows")
                eq(s.brightness.percent, 25, "percent follows")
                assert(login.suspend())
                eq(fake.calls_to("Suspend")[1].args[1], true, "interactive")
                assert(login.power_off(false))
                eq(fake.calls_to("PowerOff")[1].args[1], false, "not interactive")
                assert(login.set_locked_hint(true))
                eq(fake.calls_to("SetLockedHint")[1].args[1], true, "locked hint")
                inhibitor = assert(login.inhibit("sleep", "me", "saving", "delay"))
                local asked = fake.calls_to("Inhibit")[1]
                eq(asked.args[1], "sleep", "what")
                eq(asked.args[2], "me", "who")
                eq(asked.args[3], "saving", "why")
                eq(asked.args[4], "delay", "mode")
                eq(inhibitor.held, false, "not until logind answers")
                -- Released before the answer: the lock goes as it arrives.
                early = assert(login.inhibit("idle", "me", "never mind", "block"))
                eq(early.release(), true, "released early")
                eq(login.can("reboot"), "yes", "asked now")
            end,
            function()
                eq(inhibitor.held, true, "the lock is held by its descriptor")
                eq(fake.fds[1]:is_open(), true, "open while held")
                eq(inhibitor.release(), true, "released")
                eq(inhibitor.release(), false, "once")
                eq(fake.fds[1]:is_open(), false, "released means closed")
                eq(fake.fds[2]:is_open(), false, "an early release closed it on arrival")
                eq(early.held, false, "never held")
            end,
            function()
                fake.emit("system", L, REAL, SESSION, "Lock", nil)
                fake.emit("system", L, M, MANAGER, "PrepareForSleep", true)
                eq(s.preparing_for_sleep, true, "preparing")
                fake.emit("system", L, M, MANAGER, "PrepareForSleep", false)
                eq(locked, 1, "on_lock")
                eq(sleeping[1], true, "going")
                eq(sleeping[2], false, "back")
            end,
        }}, done)
        "#,
        dir = backlight.display()
    );
    let verdict = run_with_fake("test-logind", &body);
    let _ = std::fs::remove_dir_all(&backlight);
    assert_eq!(verdict, "ok");
}

/// Set in the child process that runs on a bus of its own.
pub(super) const PRIVATE_BUS: &str = "MORF_TEST_PRIVATE_SESSION_BUS";

/// A session bus that lets its one user do anything and activates nothing.
const PRIVATE_BUS_CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

#[test]
fn services_over_a_private_session_bus() {
    // The fake above answers in the shapes the engine's decoder is believed
    // to produce. The `private_bus_` tests pin that belief over a real bus: a
    // player served from a second runtime, read and pressed by the MPRIS
    // library, and a notification with an image sent to the notification
    // server.
    //
    // Never on the person's own session bus. Serving a name there -- even one
    // no real player uses -- is this test reaching into a live desktop, and
    // the library it drives lists and presses players. So the test re-runs
    // itself under `dbus-run-session`, which starts a bus for the child alone
    // and tears it down after; where that is not installed, it does not run.
    //
    // The bus is configured with no service directories, so nothing on it
    // can be activated: a runtime starting up asks for the desktop portal,
    // and on a stock session bus that would launch one against the private
    // bus for nothing.
    run_under_private_bus("tests::lib_dbus_services::private_bus_");
}

/// Re-runs the ignored tests matching `filter` in a child process under
/// `dbus-run-session`, on a bus of its own; see the test above for why.
pub(super) fn run_under_private_bus(filter: &str) {
    let Ok(test_binary) = std::env::current_exe() else {
        return;
    };
    let config = std::env::temp_dir().join(format!(
        "morf-private-bus-{}-{}.conf",
        std::process::id(),
        filter.replace(':', "_")
    ));
    std::fs::write(&config, PRIVATE_BUS_CONFIG).unwrap();
    let status = std::process::Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(test_binary)
        .args([filter, "--ignored", "--test-threads=1"])
        .env(PRIVATE_BUS, "1")
        .status();
    let _ = std::fs::remove_file(&config);
    match status {
        Ok(status) => assert!(status.success(), "the private-bus run failed: {status}"),
        // No `dbus-run-session`: nothing here can make a private bus.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("could not start dbus-run-session: {error}"),
    }
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_mpris_round_trip() {
    // Refuses to run on an ambient bus: `--ignored` by hand would otherwise
    // serve a name on whatever session the shell it was typed in belongs to.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let prefix = format!("org.morf.test.mpris{}.", std::process::id());
    let player_name = format!("{prefix}fake");
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();
    let server = {
        let stop = Arc::clone(&stop);
        let player_name = player_name.clone();
        thread::spawn(move || {
            let mut runtime = Runtime::default();
            let source = format!(
                r#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local log = morf.signal("player.log", "")
                ui.Text {{ text = function() return log:get() end }}
                local PATH = "/org/mpris/MediaPlayer2"
                local PLAYER = "org.mpris.MediaPlayer2.Player"
                local service, outcome = morf.dbus.serve("session", "{player_name}", PATH, false)
                assert(outcome == "owned", outcome)
                local status = "Paused"
                local function player()
                    return {{
                        PlaybackStatus = status, Rate = 1.0, Volume = 0.5,
                        Position = {{ signature = "x", value = 42000000 }},
                        CanPlay = true, CanPause = true, CanSeek = true, CanControl = true,
                        Metadata = {{
                            ["xesam:title"] = "Wire",
                            ["xesam:artist"] = {{ "One", "Two" }},
                            ["mpris:length"] = {{ signature = "x", value = 180000000 }},
                            ["mpris:trackid"] = {{ signature = "o", value = "/org/morf/track/7" }},
                        }},
                    }}
                end
                service:on_call(function(call)
                    if call.member == "GetAll" then
                        if call.arguments[1] == PLAYER then
                            service:reply(call.id, player())
                        else
                            service:reply(call.id, {{ Identity = "Fake", DesktopEntry = "fake" }})
                        end
                    elseif call.member == "PlayPause" then
                        log:set(log:get() .. "PlayPause;")
                        status = status == "Playing" and "Paused" or "Playing"
                        service:reply(call.id, nil)
                        service:emit(PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged",
                            {{ PLAYER, {{ PlaybackStatus = status }}, {{ signature = "as", value = {{}} }} }})
                    else
                        service:reply_error(call.id, "org.freedesktop.DBus.Error.UnknownMethod", call.member)
                    end
                end)
                "#
            );
            let started = runtime.execute("fake-player.lua", source.as_bytes());
            let _ = ready_tx.send(started.is_ok());
            if started.is_err() {
                return String::new();
            }
            while !stop.load(Ordering::Relaxed) {
                runtime.poll_services();
                thread::sleep(Duration::from_millis(1));
            }
            let root = runtime.scene().roots()[0];
            runtime
                .scene()
                .string_value(root, "text")
                .unwrap()
                .to_owned()
        })
    };
    if !ready_rx.recv().unwrap() {
        stop.store(true, Ordering::Relaxed);
        let _ = server.join();
        return;
    }

    let body = format!(
        r#"
        local media = require("lib.mpris").connect({{ prefix = "{prefix}", debounce_ms = 10 }})
        local s = media.state
        local eq = fake.eq
        fake.steps({{
            function()
                eq(s.count, 1, "found by ListNames")
                eq(s.active.identity, "Fake", "root interface read")
                eq(s.active.title, "Wire", "metadata through a variant")
                eq(s.active.artist, "One, Two", "an `as` inside a variant")
                eq(s.active.length, 180.0, "an `x` inside a variant")
                eq(s.active.track_id, "/org/morf/track/7", "an `o` inside a variant")
                eq(s.active.position, 42.0, "position")
                eq(s.active.status, "paused", "status")
                assert(media.play_pause())
            end,
            function() end,
            function()
                eq(s.active.status, "playing", "PropertiesChanged over the bus, re-read")
            end,
        }}, done, 300)
        "#
    );
    let verdict = run_with_fake("test-mpris-bus", &body);
    stop.store(true, Ordering::Relaxed);
    let log = server.join().unwrap();
    assert_eq!(verdict, "ok");
    assert_eq!(log, "PlayPause;", "the button reached the player once");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_notification_with_an_image_and_a_resident_action() {
    // The notification server owns `org.freedesktop.Notifications` with
    // `replace` -- which on a live session would take the name from the
    // desktop's own daemon. That is why this runs only on a private bus.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let mut runtime = Runtime::default();
    let path = format!(
        "{}/../../examples/test-notifications-bus.lua",
        env!("CARGO_MANIFEST_DIR")
    );
    runtime
        .execute(
            &path,
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local notifications = require("lib.notifications")
            local shown = morf.signal("shown", "waiting")
            ui.Text { text = function() return shown:get() end }
            local server
            server = assert(notifications.serve {
                on_change = function(list)
                    local n = list[1]
                    if not n or shown:get() ~= "waiting" then return end
                    local image = n.image_data
                    local described = table.concat({
                        n.summary, n.image_path, image and (image.width .. "x" .. image.height) or "none",
                        image and tostring(#image.data) or "0", tostring(image and image.has_alpha),
                        tostring(n.urgency), n.category, n.desktop_entry, tostring(n.resident),
                        n.image_source and n.image_source:match("^memory:image/") or "no source",
                    }, "|")
                    morf.timer(1, function()
                        server.invoke(n.id, "open")
                        shown:set(described .. "|after=" .. #server.list)
                    end, false)
                end,
            })
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];

    let caller = thread::spawn(|| {
        use morf_io::DbusValue as V;
        use std::collections::BTreeMap;
        let proxy = morf_io::DbusProxy::connect_with_timeout(
            morf_io::Bus::Session,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            Duration::from_secs(5),
        )
        .expect("a caller can connect");
        let typed = |signature: &str, value: V| V::Typed {
            signature: signature.to_owned(),
            value: Box::new(value),
        };
        let mut hints = BTreeMap::new();
        hints.insert("urgency".to_owned(), typed("y", V::Integer(2)));
        hints.insert("category".to_owned(), V::String("im.received".to_owned()));
        hints.insert("desktop-entry".to_owned(), V::String("chat".to_owned()));
        hints.insert("resident".to_owned(), V::Bool(true));
        hints.insert(
            "image-path".to_owned(),
            V::String("/tmp/face.png".to_owned()),
        );
        // A 2x1 RGBA picture: eight bytes.
        hints.insert(
            "image-data".to_owned(),
            typed(
                "(iiibiiay)",
                V::List(vec![
                    V::Integer(2),
                    V::Integer(1),
                    V::Integer(8),
                    V::Bool(true),
                    V::Integer(8),
                    V::Integer(4),
                    V::List((0..8).map(V::Integer).collect()),
                ]),
            ),
        );
        proxy.call_value_with(
            "Notify",
            &V::List(vec![
                V::String("chat".to_owned()),
                typed("u", V::Integer(0)),
                V::String("chat".to_owned()),
                V::String("hello".to_owned()),
                V::String("body".to_owned()),
                typed(
                    "as",
                    V::List(vec![
                        V::String("open".to_owned()),
                        V::String("Open".to_owned()),
                    ]),
                ),
                typed("a{sv}", V::Map(hints)),
                typed("i", V::Integer(-1)),
            ]),
        )
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime.scene().string_value(root, "text").unwrap() == "waiting"
        && Instant::now() < deadline
    {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
    // One more turn for the timer that invokes the action.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime
        .scene()
        .string_value(root, "text")
        .unwrap()
        .contains("after=")
        && Instant::now() < deadline
    {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
    let reply = caller.join().expect("the caller finished");
    assert!(reply.is_ok(), "Notify was answered: {reply:?}");
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "hello|/tmp/face.png|2x1|8|true|2|im.received|chat|true|memory:image/|after=1",
        "every hint read, and a resident notification outlives its action"
    );
}
