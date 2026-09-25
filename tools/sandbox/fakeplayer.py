#!/usr/bin/env python3
"""fakeplayer.py [ART] -- a media player on the session bus that plays nothing.

Serves org.mpris.MediaPlayer2.fakeplayer with a fixed track (title, artist,
album, cover ART, a four-minute length) that "plays" from 1:12 on, so a
shell's media panel has something to show in the sandbox, where there is no
audio and no real player. PlayPause/Play/Pause/Next/Previous change what it
reports; nothing is ever heard. Runs until killed (the sandbox tears it down
with the bus).
"""
import sys, time
from gi.repository import Gio, GLib

ART = sys.argv[1] if len(sys.argv) > 1 else ""
LENGTH_US = 240 * 1_000_000

XML = """
<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/><method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="DesktopEntry" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/><method name="Previous"/><method name="Pause"/>
    <method name="PlayPause"/><method name="Stop"/><method name="Play"/>
    <method name="Seek"><arg direction="in" name="Offset" type="x"/></method>
    <method name="SetPosition"><arg direction="in" name="TrackId" type="o"/><arg direction="in" name="Position" type="x"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="LoopStatus" type="s" access="readwrite"/>
    <property name="Rate" type="d" access="readwrite"/>
    <property name="Shuffle" type="b" access="readwrite"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="readwrite"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>
"""

TRACKS = [
    ("Northern Lights", "Aurora Field", "Polar Nights"),
    ("Slow Tide", "Harbour Lamps", "Night Ferries"),
]
state = {"playing": True, "track": 0, "base": 72 * 1_000_000, "since": time.monotonic()}


def position():
    p = state["base"]
    if state["playing"]:
        p += int((time.monotonic() - state["since"]) * 1_000_000)
    return min(p, LENGTH_US)


def metadata():
    title, artist, album = TRACKS[state["track"]]
    m = {
        "mpris:trackid": GLib.Variant("o", "/org/mpris/MediaPlayer2/track/%d" % state["track"]),
        "mpris:length": GLib.Variant("x", LENGTH_US),
        "xesam:title": GLib.Variant("s", title),
        "xesam:artist": GLib.Variant("as", [artist]),
        "xesam:album": GLib.Variant("s", album),
    }
    if ART:
        m["mpris:artUrl"] = GLib.Variant("s", "file://" + ART)
    return m


def prop(iface, name):
    if iface == "org.mpris.MediaPlayer2":
        return {
            "CanQuit": GLib.Variant("b", False), "CanRaise": GLib.Variant("b", False),
            "HasTrackList": GLib.Variant("b", False), "Identity": GLib.Variant("s", "Fake Player"),
            "DesktopEntry": GLib.Variant("s", "fakeplayer"),
            "SupportedUriSchemes": GLib.Variant("as", []), "SupportedMimeTypes": GLib.Variant("as", []),
        }.get(name)
    return {
        "PlaybackStatus": GLib.Variant("s", "Playing" if state["playing"] else "Paused"),
        "LoopStatus": GLib.Variant("s", "None"), "Rate": GLib.Variant("d", 1.0),
        "Shuffle": GLib.Variant("b", False), "Metadata": GLib.Variant("a{sv}", metadata()),
        "Volume": GLib.Variant("d", 1.0), "Position": GLib.Variant("x", position()),
        "MinimumRate": GLib.Variant("d", 1.0), "MaximumRate": GLib.Variant("d", 1.0),
        "CanGoNext": GLib.Variant("b", True), "CanGoPrevious": GLib.Variant("b", True),
        "CanPlay": GLib.Variant("b", True), "CanPause": GLib.Variant("b", True),
        "CanSeek": GLib.Variant("b", True), "CanControl": GLib.Variant("b", True),
    }.get(name)


conn = None


def changed(names):
    values = {n: prop("org.mpris.MediaPlayer2.Player", n) for n in names}
    conn.emit_signal(None, "/org/mpris/MediaPlayer2", "org.freedesktop.DBus.Properties", "PropertiesChanged",
                     GLib.Variant("(sa{sv}as)", ("org.mpris.MediaPlayer2.Player", values, [])))


def set_playing(on):
    state["base"] = position()
    state["since"] = time.monotonic()
    state["playing"] = on
    changed(["PlaybackStatus"])


def call(_c, _s, _p, iface, method, params, inv):
    if method in ("PlayPause",):
        set_playing(not state["playing"])
    elif method == "Play":
        set_playing(True)
    elif method in ("Pause", "Stop"):
        set_playing(False)
    elif method in ("Next", "Previous"):
        state["track"] = (state["track"] + 1) % len(TRACKS)
        state["base"], state["since"] = 0, time.monotonic()
        changed(["Metadata", "PlaybackStatus"])
    inv.return_value(None)


def get(_c, _s, _p, iface, name):
    return prop(iface, name)


def acquired(c, _name):
    global conn
    conn = c
    info = Gio.DBusNodeInfo.new_for_xml(XML)
    for i in info.interfaces:
        c.register_object("/org/mpris/MediaPlayer2", i, call, get, None)


Gio.bus_own_name(Gio.BusType.SESSION, "org.mpris.MediaPlayer2.fakeplayer", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.MainLoop().run()
