#!/usr/bin/env python3
"""The crate layering of PLAN.md, checked.

    python3 tools/layers.py           report what breaks the rules
    python3 tools/layers.py --strict  and fail (exit 1) when anything does

Every crate lives at crates/<group>/<crate>. A crate may depend on crates of
its own group or of the groups above it (core <- graphics <- platform <-
engine <- frontend), and on exactly the morf crates its row below names.
Nothing below morf-lua names `luna`; nothing outside morf-app and
morf-desktop names a Wayland crate.

Crates not yet in the table (old names still being merged) are reported,
not failed, unless --strict.
"""
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GROUPS = ["core", "graphics", "platform", "engine", "frontend"]

# PLAN.md section 4: the morf crates each crate may depend on.
ALLOWED = {
    "morf-value": set(),
    "morf-scene": {"morf-value"},
    "morf-layout": {"morf-scene", "morf-value"},
    "morf-vector": {"morf-image", "morf-value"},
    "morf-text": {"morf-scene", "morf-layout", "morf-vector", "morf-value"},
    "morf-image": {"morf-value"},
    "morf-render": {"morf-scene", "morf-layout", "morf-text", "morf-vector", "morf-image", "morf-value"},
    "morf-app": {"morf-value"},
    "morf-desktop": {"morf-app", "morf-value"},
    "morf-io": {"morf-value"},
    "morf-audio": {"morf-io", "morf-value"},
    "morf-terminal": {"morf-io", "morf-scene", "morf-value"},
    "morf-system": {"morf-io", "morf-image", "morf-value"},
    "morf-runtime": {"morf-scene", "morf-layout", "morf-text", "morf-app", "morf-value"},
    "morf-kit": {"morf-value"},
    "morf-lua": {"morf-runtime", "morf-kit", "morf-scene", "morf-layout", "morf-text", "morf-vector",
                 "morf-image", "morf-render", "morf-app", "morf-desktop", "morf-io", "morf-audio",
                 "morf-terminal", "morf-system", "morf-value"},
    "morf-host": None,  # everything above
    "morf-cli": {"morf-host", "morf-value"},
}
NO_LUA_BELOW = {"morf-lua", "morf-host", "morf-cli"}
WAYLAND_ALLOWED = {"morf-app", "morf-desktop", "morf-host", "morf-cli"}
WAYLAND = re.compile(r"^(wayland-|smithay-client-toolkit)")


def crates():
    for manifest in sorted(ROOT.glob("crates/*/*/Cargo.toml")) + sorted(ROOT.glob("crates/*/Cargo.toml")):
        data = tomllib.loads(manifest.read_text())
        name = data.get("package", {}).get("name")
        if not name:
            continue
        rel = manifest.parent.relative_to(ROOT).parts
        group = rel[1] if len(rel) == 3 else None
        deps = set()
        for table in ("dependencies", "build-dependencies"):
            deps |= set(data.get(table, {}).keys())
        yield name, group, deps


def main():
    strict = "--strict" in sys.argv
    found = list(crates())
    group_of = {name: group for name, group, _ in found}
    problems, notes = [], []
    for name, group, deps in found:
        if group is None:
            problems.append(f"{name}: not in a group directory (crates/<group>/<crate>)")
            continue
        if group not in GROUPS:
            problems.append(f"{name}: unknown group {group}")
            continue
        morf_deps = {d for d in deps if d.startswith("morf-")}
        for dep in sorted(morf_deps):
            dep_group = group_of.get(dep)
            if dep_group in GROUPS and GROUPS.index(dep_group) > GROUPS.index(group):
                problems.append(f"{name} ({group}) depends on {dep} ({dep_group}), a group below it")
        allowed = ALLOWED.get(name, "missing")
        if allowed == "missing":
            notes.append(f"{name}: not in the plan's table (being merged or renamed)")
        elif allowed is not None:
            for dep in sorted(morf_deps - allowed):
                (problems if dep in ALLOWED else notes).append(f"{name} depends on {dep}, not in its row")
        if "luna" in deps and name not in NO_LUA_BELOW:
            problems.append(f"{name} names luna; nothing below morf-lua may")
        for dep in sorted(deps):
            if WAYLAND.match(dep) and name not in WAYLAND_ALLOWED:
                (problems if name in ALLOWED else notes).append(f"{name} names {dep}; only morf-app and morf-desktop may")
    for line in problems:
        print("error:", line)
    for line in notes:
        print("note: ", line)
    print(f"{len(found)} crates, {len(problems)} errors, {len(notes)} notes")
    if strict and (problems or notes):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
