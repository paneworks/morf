#!/usr/bin/env python3
"""The documentation checked against the engine's own API truth.

    python3 tools/docs_check.py            report and fail on any rot
    python3 tools/docs_check.py --strict   same, as the make gate runs it

Everything is fed by the definitions the built binary's `morf types` emits
into a scratch directory; `library/types/` must match them byte for byte.

1. Inventory diff. Every public `morf.*` / `morf.ui.*` name in those
   definitions must be mentioned in `docs/UI.md` or `docs/IO.md`, or be
   allow-listed in `tools/docs_api_allowlist.txt` with a reason. Every
   `morf.*` name the docs mention (and every `ui.*` constructor named in a
   Lua block) must exist in the inventory or be allow-listed as a dynamic
   name the generated types cannot see (surface properties and compositor
   capabilities are served through `__index`, so they emit no fields).
2. Examples. Every fenced ```lua block in `README.md` and `docs/UI.md` runs
   inside `tools/docs_harness.lua`, which stubs processes, writes, buses and
   network and supplies the surrounding configuration's names. A block that
   sets `morf.surface.namespace` is a complete configuration and goes
   through `morf check`; any other is a fragment, run by a generated spec
   under `morf test`, one case each. A block may opt out only with
   `<!-- morf-check: skip: <reason> -->` on the line before its fence;
   skips are counted and printed.
3. Coverage report (printed, not a failure). Inventory names no spec under
   `library/tests/` or `examples/**/tests/`, and no Rust test, mentions.

The allow-list only ever shrinks: an entry nothing needs any more is an
error, in the spirit of tools/layers.py's PENDING.
"""
import filecmp
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "tools" / "docs_api_allowlist.txt"
HARNESS = ROOT / "tools" / "docs_harness.lua"
WORK = ROOT / "target" / "docs-check"
DOC_FILES = ["README.md", "docs/UI.md"]
INVENTORY_DOCS = ["docs/UI.md", "docs/IO.md"]
SKIP_MARKER = re.compile(r"<!--\s*morf-check:\s*skip:\s*(.*?)\s*-->")
FENCE = re.compile(r"^```lua\s*$")
MORF_NAME = re.compile(r"(?<![\w./-])morf(?:\.[A-Za-z_][A-Za-z0-9_]*)+")
UI_NAME = re.compile(r"(?<![\w./-])ui\.([A-Z][A-Za-z0-9_]*)")
REASON_KINDS = ("undocumented", "internal", "mirror", "data", "dynamic")
WILDCARD_KINDS = ("mirror", "data")
TAP_RESULT = re.compile(r"^(ok|not ok) \d+ - (\S+)")
DOTTED = re.compile(r"(?<![\w.])[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*")


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


def dotted_names(text):
    """Every whole dotted name in `text` with its leading prefixes, so
    `morf.fs.read(` mentions `morf.fs` but `morf.timer` never `morf.time`."""
    names = set()
    for match in DOTTED.finditer(text):
        parts = match.group(0).split(".")
        names.update(".".join(parts[:end]) for end in range(1, len(parts) + 1))
    return names


def mentioned(name, names):
    """`name`, or for morf.ui.X the ui.X spelling a shell author writes."""
    return name in names or (name.startswith("morf.ui.") and name[5:] in names)


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
    """name -> reason, and '+name' -> reason for documented dynamic names.
    A reason starts with its kind; only mirror and data may be subtrees."""
    internal, dynamic = {}, {}
    for number, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, reason = line.partition("\t")
        reason = reason.strip()
        kind = reason.partition(":")[0]
        if not reason:
            raise ValueError(f"allow-list line {number} has no reason: {line!r}")
        if kind not in REASON_KINDS or not reason.partition(":")[2].strip():
            raise ValueError(f"allow-list line {number}: a reason is `<kind>: <why>` with "
                             f"kind one of {', '.join(REASON_KINDS)}: {line!r}")
        if name.endswith(".*") and kind not in WILDCARD_KINDS:
            raise ValueError(f"allow-list line {number}: only a mirror or data subtree may "
                             f"be a wildcard; list the names one by one: {line!r}")
        if name.startswith("+") != (kind == "dynamic"):
            raise ValueError(f"allow-list line {number}: `+name` entries, and only they, "
                             f"are dynamic: {line!r}")
        if name.startswith("+"):
            dynamic[name[1:]] = reason
        else:
            internal[name] = reason
    return internal, dynamic


def field_types(types_dir):
    """Inventory name -> its LuaLS type, for the fields the types declare."""
    types, current = {}, None
    for line in (types_dir / "morf.lua").read_text().splitlines():
        if match := re.match(r"---@class (\S+)", line):
            current = match.group(1)
        elif (match := re.match(r"---@field (\S+) (.+)", line)) and current \
                and current.startswith("Morf"):
            types[f"{class_path(current)}.{match.group(1)}"] = match.group(2)
    return types


def subtree_problems(names, types, internal):
    """A mirror subtree must only repeat morf.* names; a data subtree must
    hold no functions. Otherwise a wildcard would hide new API."""
    problems = []
    inventory_set = set(names)
    for entry, reason in sorted(internal.items()):
        if not entry.endswith(".*"):
            continue
        root, kind = entry[:-2], reason.partition(":")[0]
        for name in names:
            if not name.startswith(root + "."):
                continue
            if kind == "mirror" and "morf." + name[len(root) + 1:] not in inventory_set:
                problems.append(f"{name}: under the mirror {entry} but morf."
                                f"{name[len(root) + 1:]} does not exist; document it")
            if kind == "data" and types.get(name, "").startswith("fun"):
                problems.append(f"{name}: a function under the data subtree {entry}; "
                                "document it or list it on its own")
    return problems


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
    """MORF_BIN, else the debug build the make recipe makes, else release."""
    candidate = os.environ.get("MORF_BIN")
    candidates = [candidate] if candidate else ["target/debug/morf", "target/release/morf"]
    for candidate in candidates:
        path = ROOT / candidate
        if path.is_file():
            return str(path)
    return None


def isolated_env(scratch):
    """The environment every morf run here gets: a home, XDG folders, a
    runtime directory and an empty PATH of its own, and no session."""
    env = {key: value for key, value in os.environ.items()
           if key not in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY", "PIPEWIRE_REMOTE",
                          "PULSE_SERVER", "MORF_CONFIG", "MORF_RUNTIME_PATH")}
    for name, folder in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"),
                         ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state"),
                         ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime"),
                         ("TMPDIR", "tmp"), ("PATH", "bin")):
        path = scratch / folder
        path.mkdir(parents=True, exist_ok=True)
        env[name] = str(path)
    (scratch / "runtime").chmod(0o700)
    env["DBUS_SESSION_BUS_ADDRESS"] = env["DBUS_SYSTEM_BUS_ADDRESS"] = "unix:path=/nonexistent"
    return env


def generated_types(binary, scratch):
    """Writes this build's definitions under scratch; returns their folder."""
    out = scratch / "types"
    shutil.rmtree(out, ignore_errors=True)
    run = subprocess.run([binary, "types", str(out)], capture_output=True, text=True,
                         timeout=120, cwd=ROOT, env=isolated_env(scratch / "env"))
    if run.returncode != 0:
        raise RuntimeError(f"`morf types` failed: {run.stderr.strip()}")
    return out


def types_drift(checked_in, generated):
    """Files under library/types that differ from what `morf types` emits."""
    def files(folder):
        return {path.relative_to(folder) for path in folder.rglob("*") if path.is_file()}
    have, want = files(checked_in), files(generated)
    problems = [f"library/types/{name}: emitted by `morf types` but not checked in"
                for name in sorted(want - have)]
    problems += [f"library/types/{name}: checked in but no longer emitted"
                 for name in sorted(have - want)]
    problems += [f"library/types/{name}: differs from what `morf types` emits"
                 for name in sorted(have & want)
                 if not filecmp.cmp(checked_in / name, generated / name, shallow=False)]
    if problems:
        problems.append("  regenerate with `target/debug/morf types library/types`")
    return problems


def long_string(text):
    level = 1
    while f"]{'=' * level}]" in text:
        level += 1
    return f"[{'=' * level}[\n{text}]{'=' * level}]"


def fragment_spec(fragments):
    """A `morf test` spec with one case per (file, line, code) fragment."""
    cases = ",\n".join(f'  {{ "{path}", {line}, {long_string(code)} }}'
                       for path, line, code in fragments)
    harness = HARNESS.read_text()
    return f"""-- Generated by tools/docs_check.py: one case per documentation fragment.
local test = morf.test
local HARNESS = {long_string(harness)}
local CASES = {{
{cases}
}}
for _, case in ipairs(CASES) do
  test.it(case[1] .. ":" .. case[2], function()
    test.source("local harness = (function()\\n" .. HARNESS .. "\\nend)()\\nharness.run("
      .. ("%q, %q, %d"):format(case[3], case[1], case[2]) .. ")\\n", {{ size = {{ 800, 600 }} }})
    test.advance(48)
    local result = test.ipc("docs.result")
    for _, call in ipairs(result.blocked or {{}}) do test.note("blocked " .. call) end
    if not result.ok then test.fail(result.message) end
    local said = {{}}
    for _, entry in ipairs(test.logs("warn")) do
      if not entry.message:find("^lint:") then
        said[#said + 1] = entry.level .. ": " .. entry.message
      end
    end
    if #said > 0 then test.fail(table.concat(said, "\\n")) end
  end)
end
"""


def parse_tap(output):
    """name -> (passed, diagnostic lines), and every `blocked` note."""
    results, blocked, current = {}, [], None
    for row in output.splitlines():
        if match := TAP_RESULT.match(row):
            current = match.group(2)
            results[current] = (match.group(1) == "ok", [])
        elif row.startswith("#   ") and current:
            text = row[4:].strip()
            if text.startswith("blocked "):
                blocked.append((current, text[len("blocked "):]))
            else:
                results[current][1].append(text)
    return results, blocked


def sandboxed_config(code, path, line):
    """A complete configuration inside the harness, failing its load when
    the configuration raises, so `morf check` reports it."""
    return (f"local harness = (function()\n{HARNESS.read_text()}\nend)()\n"
            f"local result = harness.run({long_string(code)}, \"{path}\", {line})\n"
            "if not result.ok then error(result.message, 0) end\n")


def run_fragments(binary, fragments, work=WORK):
    """Runs fragments through the harness; returns (problems, blocked)."""
    work.mkdir(parents=True, exist_ok=True)
    spec = work / "fragments_spec.lua"
    spec.write_text(fragment_spec(fragments))
    run = subprocess.run([binary, "test", "--no-dbus", str(spec)], capture_output=True,
                         text=True, timeout=600, cwd=ROOT, env=isolated_env(work / "env"))
    results, blocked = parse_tap(run.stdout)
    problems = []
    for path, line, _ in fragments:
        name = f"{path}:{line}"
        passed, diagnostics = results.get(name, (False, ["no result: the run ended early"]))
        if not passed:
            problems.append(f"{name}: fragment fails")
            problems.extend("  " + re.sub(r"@([\w./-]+\.md:\d+)", r"\1",
                                          row.replace(f"{ROOT}/", ""))
                            for row in diagnostics[:6])
    if run.returncode != 0 and not problems:
        problems.append(f"fragment run failed: {(run.stdout + run.stderr).splitlines()[-3:]}")
    return problems, blocked


def check_examples(blocks_by_file, binary, work=WORK):
    """Runs every unskipped block; returns problem lines."""
    problems, fragments = [], []
    work.mkdir(parents=True, exist_ok=True)
    configs = 0
    for path, blocks in blocks_by_file.items():
        for line, code, skip in blocks:
            if skip:
                continue
            if "morf.surface.namespace" not in code:
                fragments.append((path, line, code))
                continue
            config = work / f"config-{path.replace('/', '-')}-{line}.lua"
            config.write_text(sandboxed_config(code, path, line))
            run = subprocess.run(
                [binary, "check", str(config), "--isolate", "--no-dbus"],
                capture_output=True, text=True, timeout=120, cwd=ROOT,
                env=isolated_env(work / "env"))
            configs += 1
            if run.returncode != 0:
                problems.append(f"{path}:{line}: complete configuration fails morf check")
                problems.extend("  " + row for row in run.stdout.splitlines()[-5:])
    fragment_problems, blocked = run_fragments(binary, fragments, work)
    problems += fragment_problems
    print(f"examples: {configs} complete configurations checked, {len(fragments)} "
          f"fragments run; {len(blocked)} outside calls stubbed")
    for name, call in blocked:
        print(f"  stubbed: {name}: {call}")
    return problems


def cfg_test_bodies(path, text):
    """The source of every `#[cfg(test)]` item in a Rust file: a braced
    body in place, or the file a `mod name;` declaration points at."""
    bodies = []
    for match in re.finditer(r"#\[cfg\(test\)\]", text):
        rest = text[match.end():]
        stop = re.search(r"[{;]", rest)
        if not stop:
            continue
        if stop.group(0) == ";":
            if module := re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;", rest):
                base = path.parent if path.name in ("mod.rs", "lib.rs", "main.rs") \
                    else path.with_suffix("")
                for candidate in (base / f"{module.group(1)}.rs", base / module.group(1) / "mod.rs"):
                    if candidate.is_file():
                        bodies.append(candidate.read_text(errors="replace"))
            continue
        depth, start = 0, stop.start()
        for offset, char in enumerate(rest[start:]):
            depth += {"{": 1, "}": -1}.get(char, 0)
            if depth == 0:
                bodies.append(rest[start:start + offset + 1])
                break
    return bodies


def is_rust_test_file(path):
    relative = path.relative_to(ROOT)
    return ("tests" in relative.parts[:-1] or path.name == "tests.rs"
            or path.name.endswith("_tests.rs"))


def coverage_corpus():
    """Specs and Rust tests, never the code that defines the API."""
    texts = []
    for pattern in ("library/tests/**/*.lua", "examples/**/tests/**/*.lua"):
        texts.extend(path.read_text(errors="replace") for path in ROOT.glob(pattern))
    for path in ROOT.glob("crates/**/*.rs"):
        text = path.read_text(errors="replace")
        if is_rust_test_file(path):
            texts.append(text)
        else:
            texts.extend(cfg_test_bodies(path, text))
    return "\n".join(texts)


def coverage_report(names, corpus=None):
    tokens = dotted_names(coverage_corpus() if corpus is None else corpus)
    uncovered = [name for name in names if not mentioned(name, tokens)]
    print(f"coverage: {len(names) - len(uncovered)}/{len(names)} inventory names "
          f"appear in a spec or Rust test; {len(uncovered)} uncovered (report only)")
    for name in uncovered[:30]:
        print(f"  uncovered: {name}")
    if len(uncovered) > 30:
        print(f"  ... and {len(uncovered) - 30} more")
    return uncovered


def inventory_problems(names, doc_texts, block_texts, internal, dynamic):
    """Both directions of the inventory diff, and stale allow-list entries."""
    problems = []
    tokens = dotted_names("\n".join(doc_texts))
    undocumented = [name for name in names if not mentioned(name, tokens)]
    for name in undocumented:
        if not allowlisted(name, internal):
            problems.append(f"{name}: in the API inventory but not in docs/UI.md, "
                            "docs/IO.md or the allow-list")
    for entry in sorted(set(internal) - matching_entries(undocumented, internal)):
        problems.append(f"allow-list: {entry} is no longer needed; remove it")
    inventory_set = set(names)
    named = documented_names(doc_texts, block_texts)
    for name in sorted(named):
        candidates = [name, f"morf.{name}"] if name.startswith("ui.") else [name]
        if not any(c in inventory_set for c in candidates) \
                and not any(allowlisted(c, dynamic) for c in candidates):
            problems.append(f"{name}: documented but not in the API inventory "
                            "or the dynamic allow-list")
    for entry in sorted(set(dynamic) - matching_entries(named, dynamic)):
        problems.append(f"allow-list: +{entry} is not documented; remove it")
    return problems


def main():
    problems = []
    binary = morf_binary()
    if binary is None:
        print("error: no morf binary found (build morf-cli first, or set MORF_BIN)")
        return 1
    WORK.mkdir(parents=True, exist_ok=True)
    try:
        types_dir = generated_types(binary, WORK)
    except (RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"error: {error}")
        return 1
    problems += types_drift(ROOT / "library" / "types", types_dir)
    names = inventory(types_dir)
    doc_texts = [(ROOT / path).read_text() for path in INVENTORY_DOCS]

    blocks_by_file = {}
    block_texts = []
    skips = []
    for path in DOC_FILES:
        blocks = extract_blocks((ROOT / path).read_text())
        blocks_by_file[path] = blocks
        block_texts.extend(code for _, code, skip in blocks if skip is None)
        for line, _, skip in blocks:
            if skip is not None:
                skips.append((path, line, skip))
                if not skip.strip():
                    problems.append(f"{path}:{line}: skip marker without a reason")
    print(f"examples: {sum(len(b) for b in blocks_by_file.values())} lua blocks, "
          f"{len(skips)} skipped with a reason")
    for path, line, reason in skips:
        print(f"  skipped: {path}:{line}: {reason}")

    try:
        internal, dynamic = parse_allowlist(ALLOWLIST.read_text())
    except ValueError as error:
        print(f"error: {error}")
        return 1
    problems += inventory_problems(names, doc_texts, block_texts, internal, dynamic)
    problems += subtree_problems(names, field_types(types_dir), internal)
    problems += check_examples(blocks_by_file, binary)

    coverage_report(names)
    for problem in problems:
        print(problem if problem.startswith("  ") else f"error: {problem}")
    failures = sum(1 for problem in problems if not problem.startswith("  "))
    print(f"{len(names)} inventory names, {len(skips)} skipped blocks, {failures} problems")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
