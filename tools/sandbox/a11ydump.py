#!/usr/bin/env python3
"""a11ydump.py OUT -- what a screen reader sees on the private session bus:
every application on the AT-SPI desktop and its tree, one line a node --
role, name, value and states -- indented by depth. Run inside the sandbox
only (nested.sh's `a11y` step), never on the person's session."""
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

STATES = ["focused", "focusable", "checked", "expanded", "selected", "enabled", "showing", "modal"]


def describe(node):
    role = node.get_role_name()
    name = node.get_name() or ""
    bits = [role, repr(name)]
    try:
        value = node.get_value()
    except Exception:
        value = None
    if value is not None:
        try:
            bits.append("value=%g" % value.get_current_value())
        except Exception:
            pass
    try:
        text = node.get_text()
        if text is not None and node.get_role_name() in ("entry", "password text", "text"):
            bits.append("text=%r" % text.get_text(0, -1))
    except Exception:
        pass
    states = node.get_state_set()
    on = [s for s in STATES if states.contains(getattr(Atspi.StateType, s.upper()))]
    if on:
        bits.append("[" + " ".join(on) + "]")
    return " ".join(bits)


def walk(node, depth, out, budget):
    if budget[0] <= 0 or depth > 40:
        return
    budget[0] -= 1
    out.append("  " * depth + describe(node))
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child is not None:
            walk(child, depth + 1, out, budget)


def main():
    Atspi.init()
    desktop = Atspi.get_desktop(0)
    out, budget = [], [5000]
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app is not None:
            walk(app, 0, out, budget)
    with open(sys.argv[1], "w") as f:
        f.write("\n".join(out) + "\n")


main()
