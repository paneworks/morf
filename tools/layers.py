#!/usr/bin/env python3
"""The crate layering of PLAN.md, checked.

    python3 tools/layers.py           report what breaks the rules
    python3 tools/layers.py --strict  and fail (exit 1) when anything does

Every crate lives at crates/<group>/<crate>. A crate may depend on crates of
its own group or of the groups above it (core <- graphics <- platform <-
engine <- frontend), and on exactly the morf crates its row below names.
Nothing below morf-lua names `luna`; nothing outside morf-app and
morf-desktop names a Wayland crate. And section 3's house rules: no Rust
file over 500 lines, each crate's root opens with a `//!` header, and each
crate has tests (a `tests/` directory or `#[cfg(test)]` code).

Violations a later phase of the plan removes are listed in PENDING, each
with its phase: --strict fails on anything else, and on a PENDING entry
that no longer happens (so the list only ever shrinks).
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
    "morf-shader": {"morf-value"},
    "morf-kit": {"morf-value"},
    "morf-lua": {"morf-runtime", "morf-kit", "morf-scene", "morf-layout", "morf-text", "morf-vector",
                 "morf-image", "morf-render", "morf-shader", "morf-app", "morf-desktop", "morf-io", "morf-audio",
                 "morf-terminal", "morf-system", "morf-value"},
    "morf-host": None,  # everything above
    "morf-cli": {"morf-host", "morf-value"},
}
# morf-shader reads Lua-syntax shaders with luna's parser: the Lua layer.
# What still breaks the rules, and the phase of PLAN.md that ends it: nothing.
PENDING = {}
NO_LUA_BELOW = {"morf-shader", "morf-lua", "morf-host", "morf-cli"}
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
        yield name, group, deps, manifest.parent


MAX_LINES = 500


def house_rules(name, path):
    """PLAN.md section 3, rules 7 and 8, for the crate at `path`."""
    found = []
    sources = sorted(path.rglob("*.rs"))
    for source in sources:
        if "target" in source.relative_to(path).parts:
            continue
        lines = source.read_text(errors="replace").count("\n")
        if lines > MAX_LINES:
            found.append(f"{source.relative_to(ROOT)}: {lines} lines, more than {MAX_LINES}")
    root = next((path / "src" / f for f in ("lib.rs", "main.rs") if (path / "src" / f).exists()), None)
    if root is None or not root.read_text().lstrip().startswith("//!"):
        found.append(f"{name}: its root has no //! header saying what it owns")
    tested = (path / "tests").is_dir() or any(
        "#[cfg(test)]" in source.read_text(errors="replace") for source in sources
    )
    if not tested:
        found.append(f"{name}: has no tests")
    return found


def main():
    strict = "--strict" in sys.argv
    found = list(crates())
    group_of = {name: group for name, group, _, _ in found}
    problems, notes = [], []
    for name, _, _, path in found:
        problems.extend(house_rules(name, path))
    for name, group, deps, _ in found:
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
    found_lines = problems + notes
    new = [line for line in found_lines if line not in PENDING]
    gone = [line for line in PENDING if line not in found_lines]
    for line in found_lines:
        tag = "pending" if line in PENDING else ("error" if line in problems else "note")
        suffix = f"  ({PENDING[line]})" if line in PENDING else ""
        print(f"{tag}: {line}{suffix}")
    for line in gone:
        print(f"fixed:   {line}  -- remove it from PENDING")
    print(f"{len(found)} crates, {len(new)} new, {len(found_lines) - len(new)} pending, {len(gone)} fixed")
    if strict and (new or gone):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
