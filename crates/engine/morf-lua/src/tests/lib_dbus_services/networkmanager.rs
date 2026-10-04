//! NetworkManager: the tree read and every action typed, and nothing
//! started when the service is absent.

use super::*;

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

        local net = require("lib.services.networkmanager").connect({ dbus = fake.dbus, debounce_ms = 10 })
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
                eq(fake.plain(add.args[1].value["802-11-wireless"].ssid.value), "cafe", "ssid bytes, as a string")
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
        local net = require("lib.services.networkmanager").connect({ dbus = fake.dbus })
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
