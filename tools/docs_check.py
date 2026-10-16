#!/usr/bin/env python3
"""The documentation checked against the engine's own API truth.

    python3 tools/docs_check.py            report and fail on any rot
    python3 tools/docs_check.py --strict   same, as the make gate runs it

Three checks, all fed by the definitions `morf types` emits (the checked-in
copies under `library/types/`, generated from the running engine):

1. Inventory diff. Every public `morf.*` / `morf.ui.*` name in those
   definitions must be mentioned in `docs/UI.md` or `docs/IO.md`, or be
   allow-listed in `tools/docs_api_allowlist.txt` with a reason. Every
   `morf.*` name the docs mention (and every `ui.*` constructor named in a
   Lua block) must exist in the inventory or be allow-listed as a dynamic
   name the generated types cannot see (surface properties and compositor
   capabilities are served through `__index`, so they emit no fields).
2. Examples. Every fenced ```lua block in `README.md` and `docs/UI.md` is
   syntax-checked by loading it in one batched headless harness, and a
   block that configures a surface (it sets `morf.surface.namespace`) is
   a complete configuration and runs through `morf check` on its own. A
   block may opt out only with `<!-- morf-check: skip: <reason> -->` on
   the line before its fence; skips are counted and printed.
3. Coverage report (printed, not a failure). Inventory names no spec or
   Rust test mentions, so the undocumented-and-untested tail stays visible.

The allow-list only ever shrinks: an entry nothing needs any more is an
error, in the spirit of tools/layers.py's PENDING.
"""
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "tools" / "docs_api_allowlist.txt"
DOC_FILES = ["README.md", "docs/UI.md"]
INVENTORY_DOCS = ["docs/UI.md", "docs/IO.md"]
SKIP_MARKER = re.compile(r"<!--\s*morf-check:\s*skip:\s*(.+?)\s*-->")
FENCE = re.compile(r"^```lua\s*$")
MORF_NAME = re.compile(r"(?<![\w./-])morf(?:\.[A-Za-z_][A-Za-z0-9_]*)+")
UI_NAME = re.compile(r"(?<![\w./-])ui\.([A-Z][A-Za-z0-9_]*)")


def extract_blocks(text):
    """(line, code, skip_reason) per fenced lua block; line is 1-based."""
    lines = text.splitlines()
    blocks = []
    index = 0
    while index < len(lines):
        if FENCE.match(lines[index]):
            skip = None
            if index > 0:
                marker = SKIP_MARKER.search(lines[index - 1])
                if marker:
                    skip = marker.group(1)
            body = []
            end = index + 1
            while end < len(lines) and not lines[end].startswith("```"):
                body.append(lines[end])
                end += 1
            blocks.append((index + 1, "\n".join(body), skip))
            index = end
        index += 1
    return blocks


def class_path(class_name):
    return ".".join(part[:1].lower() + part[1:] for part in class_name.split("."))


def inventory(types_dir):
    """Public names from the generated LuaLS definitions."""
    names = set()
    classes = {}
    current = None
    for line in (types_dir / "morf.lua").read_text().splitlines():
        if match := re.match(r"---@class (\S+)", line):
            current = match.group(1)
            classes.setdefault(current, [])
        elif (match := re.match(r"---@field (\S+)", line)) and current:
            classes[current].append(match.group(1))
    for class_name, fields in classes.items():
        if class_name.startswith("Morf"):
            for field in fields:
                names.add(f"{class_path(class_name)}.{field}")
    ui_text = (types_dir / "morf" / "ui.lua").read_text()
    for constructor in re.findall(r"function (ui\.[A-Za-z]+)\(", ui_text):
        names.add(constructor)
        names.add(f"morf.{constructor}")
    return sorted(names)


def documented_names(texts, block_texts):
    """morf.* anywhere (word-boundaried, so URLs and app ids stay out);
    ui.* constructors from Lua blocks only (prose says `ui.X` generically)."""
    names = set()
    for text in texts:
        names.update(match.group(0).rstrip(".") for match in MORF_NAME.finditer(text))
    for text in block_texts:
        for match in UI_NAME.finditer(text):
            names.add(f"ui.{match.group(1)}")
    return names


def parse_allowlist(text):
    """name -> reason, and '+name' -> reason for documented dynamic names."""
    internal, dynamic = {}, {}
    for number, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, reason = line.partition("\t")
        if not reason.strip():
            raise ValueError(f"allow-list line {number} has no reason: {line!r}")
        if name.startswith("+"):
            dynamic[name[1:]] = reason.strip()
        else:
            internal[name] = reason.strip()
    return internal, dynamic


def entry_matches(entry, name):
    if entry.endswith(".*"):
        return name == entry[:-2] or name.startswith(entry[:-1])
    return name == entry


def allowlisted(name, entries):
    return any(entry_matches(entry, name) for entry in entries)


def matching_entries(names, entries):
    used = set()
    for name in names:
        for entry in entries:
            if entry_matches(entry, name):
                used.add(entry)
    return used


def morf_binary():
    candidate = os.environ.get("MORF_BIN")
    candidates = [candidate] if candidate else []
    candidates += ["target/release/morf", "target/debug/morf"]
    for candidate in candidates:
        if candidate and (ROOT / candidate).exists():
            return str(ROOT / candidate)
    return None


def check_examples(blocks_by_file, binary):
    """Syntax-load every block in one harness; run complete configs alone.
    Returns problem lines. Needs a built morf; the make recipe builds one."""
    problems = []
    work = ROOT / "target" / "docs-check"
    work.mkdir(parents=True, exist_ok=True)
    harness = [
        'local ui = require("morf.ui")',
        'morf.surface.namespace = "docs-check"',
        "morf.surface.width = 800",
        "morf.surface.height = 600",
        "ui.Rect { width = 800, height = 600 }",
        "local failures = {}",
    ]
    checked = 0
    for path, blocks in blocks_by_file.items():
        for number, (line, code, skip) in enumerate(blocks):
            if skip:
                continue
            if "morf.surface.namespace" in code:
                config = work / f"config-{path.replace('/', '-')}-{line}.lua"
                config.write_text(code + "\n")
                run = subprocess.run(
                    [binary, "check", str(config), "--isolate", "--no-dbus"],
                    capture_output=True, text=True, timeout=120, cwd=ROOT,
                )
                checked += 1
                if run.returncode != 0:
                    problems.append(f"{path}:{line}: complete configuration fails morf check")
                    problems.extend("  " + row for row in run.stdout.splitlines()[-5:])
                continue
            harness.append(
                f'local chunk, load_err = load([===[\n{code}\n]===], "{path}:{line}")'
            )
            harness.append(
                'if not chunk then failures[#failures + 1] = '
                f'"{path}:{line}: " .. tostring(load_err) end'
            )
            checked += 1
    harness.append("for _, failure in ipairs(failures) do morf.log.error(failure) end")
    harness_path = work / "harness.lua"
    harness_path.write_text("\n".join(harness) + "\n")
    run = subprocess.run(
        [binary, "check", str(harness_path), "--isolate", "--no-dbus"],
        capture_output=True, text=True, timeout=300, cwd=ROOT,
    )
    for row in run.stdout.splitlines():
        if "error:" in row and "0 errors" not in row:
            problems.append(row.strip())
    if run.returncode != 0 and not problems:
        problems.append(f"example harness failed: {run.stdout.splitlines()[-1:]}")
    print(f"examples: {checked} blocks syntax-checked or run")
    return problems


def coverage_report(names):
    corpus = []
    for pattern in ("library/tests/**/*.lua", "examples/**/tests/**/*.lua"):
        corpus.extend(ROOT.glob(pattern))
    texts = []
    for path in corpus:
        texts.append(path.read_text(errors="replace"))
    for path in ROOT.glob("crates/**/*.rs"):
        texts.append(path.read_text(errors="replace"))
    blob = "\n".join(texts)
    uncovered = [name for name in names if name not in blob]
    print(f"coverage: {len(names) - len(uncovered)}/{len(names)} inventory names "
          f"appear in a spec or Rust test; {len(uncovered)} uncovered (report only)")
    for name in uncovered[:30]:
        print(f"  uncovered: {name}")
    if len(uncovered) > 30:
        print(f"  ... and {len(uncovered) - 30} more")


def main():
    problems = []
    names = inventory(ROOT / "library" / "types")
    doc_texts = [(ROOT / path).read_text() for path in INVENTORY_DOCS]
    joined_docs = "\n".join(doc_texts)

    blocks_by_file = {}
    block_texts = []
    skips = 0
    for path in DOC_FILES:
        blocks = extract_blocks((ROOT / path).read_text())
        blocks_by_file[path] = blocks
        block_texts.extend(code for _, code, skip in blocks if not skip)
        for line, _, skip in blocks:
            if skip:
                skips += 1
            if skip is not None and not skip.strip():
                problems.append(f"{path}:{line}: skip marker without a reason")
    print(f"examples: {sum(len(b) for b in blocks_by_file.values())} lua blocks, "
          f"{skips} skipped with a reason")

    internal, dynamic = parse_allowlist(ALLOWLIST.read_text())
    inventory_set = set(names)
    # Inventory -> docs: documented means the name, or (for morf.ui.X) the
    # ui.X spelling a shell author actually writes.
    missing = [
        name for name in names
        if name not in joined_docs
        and not (name.startswith("morf.ui.") and name[5:] in joined_docs)
        and not allowlisted(name, internal)
    ]
    for name in missing:
        problems.append(f"{name}: in the API inventory but not in docs/UI.md, "
                        "docs/IO.md or the allow-list")
    used_internal = matching_entries(
        [n for n in names if n not in joined_docs], internal)
    for entry in sorted(set(internal) - used_internal):
        problems.append(f"allow-list: {entry} is no longer needed; remove it")

    documented = documented_names(doc_texts, block_texts)
    for name in sorted(documented):
        candidates = [name]
        if name.startswith("ui."):
            candidates.append(f"morf.{name}")
        if not any(c in inventory_set for c in candidates) \
                and not allowlisted(name, dynamic) \
                and not allowlisted(candidates[-1], dynamic):
            problems.append(f"{name}: documented but not in the API inventory "
                            "or the dynamic allow-list")
    used_dynamic = matching_entries(documented, dynamic)
    for entry in sorted(set(dynamic) - used_dynamic):
        problems.append(f"allow-list: +{entry} is not documented; remove it")

    binary = morf_binary()
    if binary is None:
        problems.append("no morf binary found (build morf-cli first); "
                        "examples were not checked")
    else:
        problems.extend(check_examples(blocks_by_file, binary))

    coverage_report(names)
    for problem in problems:
        print(f"error: {problem}")
    print(f"{len(names)} inventory names, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
