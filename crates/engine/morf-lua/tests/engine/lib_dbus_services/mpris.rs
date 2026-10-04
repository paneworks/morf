//! MPRIS: the active player picked and its position interpolated.

use super::*;

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
        local media = require("lib.services.mpris").connect({
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
