//! logind: the session read and inhibitors held by process.

use super::*;

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

        local login = require("lib.services.logind").connect({{
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
