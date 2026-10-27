#!/usr/bin/env python3
"""Docs gate tests: extraction, inventory, allow-list rules, coverage, types
drift, and fragments run in the sandboxed harness (with a built morf)."""
import contextlib
import importlib.util
import io
import shutil
import socket
import tempfile
import time
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'docs_check', Path(__file__).with_name('docs_check.py'))
docs_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs_check)

BINARY = docs_check.morf_binary()
WORK = docs_check.ROOT / 'target' / 'docs-check-test'


class Extraction(unittest.TestCase):
    def test_blocks_and_skip_marker(self):
        text = ("prose\n<!-- morf-check: skip: elided -->\n```lua\n"
                "ui.Rect {}\n```\n```lua\nlocal x = 1\n```\n")
        blocks = docs_check.extract_blocks(text)
        self.assertEqual(len(blocks), 2)
        self.assertEqual(blocks[0][1], "ui.Rect {}")
        self.assertEqual(blocks[0][2], "elided")
        self.assertIsNone(blocks[1][2])

    def test_documented_names_ignore_urls_and_app_ids(self):
        text = ('see https://morf.dev and app_id = "dev.morf.Settings", '
                'but `morf.signal` and `morf.font_families()` are API')
        names = docs_check.documented_names([text], [])
        self.assertIn("morf.signal", names)
        self.assertIn("morf.font_families", names)
        self.assertNotIn("morf.Settings", names)
        self.assertNotIn("morf.dev", names)

    def test_ui_names_only_from_blocks(self):
        names = docs_check.documented_names(["construct ui.X here"],
                                            ["ui.Rect {} ui.ListView {}"])
        self.assertIn("ui.Rect", names)
        self.assertIn("ui.ListView", names)
        self.assertNotIn("ui.X", names)

    def test_a_name_is_mentioned_whole_not_as_a_prefix(self):
        tokens = docs_check.dotted_names("morf.timer(1) and morf.fs.read(path)")
        self.assertFalse(docs_check.mentioned("morf.time", tokens))
        self.assertTrue(docs_check.mentioned("morf.fs", tokens))
        self.assertTrue(docs_check.mentioned("morf.fs.read", tokens))
        self.assertTrue(docs_check.mentioned("morf.ui.Rect", docs_check.dotted_names("ui.Rect {}")))


class Allowlist(unittest.TestCase):
    def test_parse_requires_a_kinded_reason(self):
        for bad in ("morf.x\n", "morf.x\tbecause\n", "morf.x\tundocumented:\n"):
            with self.assertRaises(ValueError):
                docs_check.parse_allowlist(bad)
        internal, dynamic = docs_check.parse_allowlist(
            "morf.core.*\tmirror: same table\n+morf.surface.blend\tdynamic: __index\n"
            "morf.args\tundocumented: no section\n")
        self.assertTrue(docs_check.allowlisted("morf.core.args", internal))
        self.assertFalse(docs_check.allowlisted("morf.corex", internal))
        self.assertIn("morf.surface.blend", dynamic)

    def test_only_mirror_and_data_subtrees_may_be_wildcards(self):
        for line in ("morf.fs.*\tundocumented: files\n", "morf.fs.*\tinternal: files\n"):
            with self.assertRaises(ValueError):
                docs_check.parse_allowlist(line)
        docs_check.parse_allowlist("morf.keys.*\tdata: constants\n")

    def test_dynamic_kind_and_plus_go_together(self):
        with self.assertRaises(ValueError):
            docs_check.parse_allowlist("+morf.surface.blend\tundocumented: x\n")
        with self.assertRaises(ValueError):
            docs_check.parse_allowlist("morf.surface.blend\tdynamic: x\n")

    def test_stale_detection(self):
        used = docs_check.matching_entries(["morf.core.args"],
                                           {"morf.core.*": "r", "morf.x": "r"})
        self.assertEqual(used, {"morf.core.*"})

    def test_subtrees_cannot_hide_new_api(self):
        names = ["morf.core.args", "morf.core.secret", "morf.args",
                 "morf.keys.Return", "morf.keys.press"]
        types = {"morf.keys.Return": "integer", "morf.keys.press": "fun(...): any"}
        internal = {"morf.core.*": "mirror: x", "morf.keys.*": "data: x"}
        problems = docs_check.subtree_problems(names, types, internal)
        self.assertEqual(len(problems), 2, problems)
        self.assertIn("morf.core.secret", problems[0])
        self.assertIn("morf.keys.press", problems[1])


class Inventory(unittest.TestCase):
    NAMES = ["morf.signal", "morf.timer", "morf.ui.Rect", "ui.Rect"]

    def test_a_misspelled_documented_name_fails(self):
        doc = "`morf.signal`, `morf.timer`, `ui.Rect` and `morf.signall(name)`"
        problems = docs_check.inventory_problems(self.NAMES, [doc], [], {}, {})
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("morf.signall: documented but not in the API inventory", problems[0])

    def test_an_undocumented_name_needs_an_entry_and_a_stale_entry_fails(self):
        doc = "`morf.signal` and `ui.Rect`"
        problems = docs_check.inventory_problems(self.NAMES, [doc], [], {}, {})
        self.assertEqual(problems, ["morf.timer: in the API inventory but not in "
                                    "docs/UI.md, docs/IO.md or the allow-list"])
        allowed = {"morf.timer": "undocumented: x", "morf.gone": "undocumented: x"}
        problems = docs_check.inventory_problems(self.NAMES, [doc], [], allowed, {})
        self.assertEqual(problems, ["allow-list: morf.gone is no longer needed; remove it"])


class Coverage(unittest.TestCase):
    def test_only_test_code_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "lib.rs"
            (Path(tmp) / "checks.rs").write_text('fn t() { "morf.from_checks" }')
            text = ('pub fn define() { "morf.defined" }\n#[cfg(test)]\nmod checks;\n'
                    '#[cfg(test)]\nmod tests {\n  #[test] fn t() { let _ = "morf.tested"; }\n}\n'
                    'fn after() { "morf.after" }\n')
            corpus = "\n".join(docs_check.cfg_test_bodies(src, text))
        with contextlib.redirect_stdout(io.StringIO()):
            uncovered = docs_check.coverage_report(
                ["morf.defined", "morf.tested", "morf.from_checks", "morf.after"], corpus)
        self.assertEqual(uncovered, ["morf.defined", "morf.after"])

    def test_test_files_by_name_and_folder(self):
        root = docs_check.ROOT
        self.assertTrue(docs_check.is_rust_test_file(root / "crates/a/b/tests/x.rs"))
        self.assertTrue(docs_check.is_rust_test_file(root / "crates/a/b/src/tests.rs"))
        self.assertTrue(docs_check.is_rust_test_file(root / "crates/a/b/src/frame_tests.rs"))
        self.assertTrue(docs_check.is_rust_test_file(root / "crates/a/b/src/tests/docs.rs"))
        self.assertFalse(docs_check.is_rust_test_file(root / "crates/a/b/src/api.rs"))


class TypesDrift(unittest.TestCase):
    def test_differing_missing_and_extra_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            have, want = Path(tmp) / "have", Path(tmp) / "want"
            for folder in (have, want):
                (folder / "morf").mkdir(parents=True)
                (folder / "same.lua").write_text("x")
            (have / "morf.lua").write_text("old")
            (want / "morf.lua").write_text("new")
            (have / "gone.lua").write_text("x")
            (want / "morf" / "ui.lua").write_text("x")
            problems = docs_check.types_drift(have, want)
        self.assertEqual(len(problems), 4, problems)
        self.assertIn("morf/ui.lua: emitted by `morf types` but not checked in", problems[0])
        self.assertIn("gone.lua: checked in but no longer emitted", problems[1])
        self.assertIn("morf.lua: differs", problems[2])
        self.assertEqual(docs_check.types_drift(want, want), [])


@unittest.skipUnless(BINARY, "needs a built morf (cargo build -p morf-cli)")
class Fragments(unittest.TestCase):
    """Fragments really run: errors fail with their doc line, and nothing
    a fragment does reaches outside the run."""

    @classmethod
    def setUpClass(cls):
        shutil.rmtree(WORK, ignore_errors=True)
        cls.markers = WORK / "markers"
        cls.markers.mkdir(parents=True)
        cls.tcp = socket.socket()
        cls.tcp.bind(("127.0.0.1", 0))
        cls.tcp.listen(4)
        cls.tcp.settimeout(0.5)
        cls.unix_dir = tempfile.mkdtemp(prefix="morf-docs-")
        cls.unix_path = str(Path(cls.unix_dir) / "s")
        cls.unix = socket.socket(socket.AF_UNIX)
        cls.unix.bind(cls.unix_path)
        cls.unix.listen(4)
        cls.unix.settimeout(0.5)
        port = cls.tcp.getsockname()[1]
        mark = str(cls.markers) + "/"
        touch, sh = shutil.which("touch"), shutil.which("sh")
        outside = f"""
local mark, touch = "{mark}", "{touch}"
morf.process.run({{ "{sh}", "-c", touch .. " " .. mark .. "run" }}, function() end)
morf.process.spawn {{ command = {{ touch, mark .. "spawn" }} }}
morf.process.detach({{ touch, mark .. "detached" }})
require("morf.process").run({{ touch, mark .. "ns-run" }}, function() end)
require("morf").process.spawn {{ command = {{ touch, mark .. "required" }} }}
ui.Terminal {{ command = {{ touch, mark .. "terminal" }}, width = 80, height = 40 }}
morf.fs.write(mark .. "fs-write", "x")
morf.fs.write(mark .. "fs-append", "x", {{ append = true }})
morf.fs.mkdir(mark .. "fs-mkdir")
morf.http.get("http://127.0.0.1:{port}/", function() end)
morf.socket.connect {{ host = "127.0.0.1", port = {port}, on_line = function() end }}
morf.socket.connect {{ path = "{cls.unix_path}", on_line = function() end }}
morf.socket.request("{cls.unix_path}", "ping", function() end)
morf.dbus.call_async("session", "org.freedesktop.DBus", "/", "org.freedesktop.DBus", "ListNames")
ui.Rect {{ width = 10, height = 10 }}
"""
        cls.fragments = [
            ("docs/A.md", 10, "ui.Rect { width = 20, height = 10, ui.Text { text = 'hi' } }"),
            ("docs/A.md", 20, "local panel = ui.Item {}\nlocal n = nil\nn.call()"),
            ("docs/A.md", 30, "morf.signall('docs.x', 1)"),
            ("docs/A.md", 40, "ui.Rect { color = colour_of_the_day }"),
            ("docs/A.md", 50, "ui.Text { text = function() return nil .. 'x' end }"),
            ("docs/A.md", 60, "ui.Rect { width = 10, colr = '#fff' }"),
            ("docs/A.md", 70, outside),
        ]
        cls.problems, cls.blocked = docs_check.run_fragments(BINARY, cls.fragments, WORK)
        time.sleep(0.5)

    @classmethod
    def tearDownClass(cls):
        cls.tcp.close()
        cls.unix.close()
        shutil.rmtree(cls.unix_dir, ignore_errors=True)

    def failure(self, line):
        name = f"docs/A.md:{line}: fragment fails"
        if name not in self.problems:
            return None
        start = self.problems.index(name) + 1
        rows = []
        for row in self.problems[start:]:
            if not row.startswith("  "):
                break
            rows.append(row)
        return "\n".join(rows)

    def test_a_correct_fragment_passes(self):
        self.assertIsNone(self.failure(10), self.problems)

    def test_a_runtime_error_fails_at_its_doc_line(self):
        self.assertIn("docs/A.md:23", self.failure(20) or "")

    def test_a_misspelled_api_fails(self):
        self.assertIsNotNone(self.failure(30))

    def test_a_name_from_nowhere_fails(self):
        self.assertIn("colour_of_the_day", self.failure(40) or "")

    def test_an_error_in_a_binding_fails(self):
        self.assertIsNotNone(self.failure(50))

    def test_a_misspelled_property_fails(self):
        self.assertIn("colr", self.failure(60) or "")

    def test_outside_calls_are_stubbed_and_reach_nothing(self):
        self.assertIsNone(self.failure(70), self.problems)
        self.assertEqual(sorted(path.name for path in self.markers.iterdir()), [])
        for server in (self.tcp, self.unix):
            with self.assertRaises(socket.timeout):
                server.accept()
        stubbed = [call for name, call in self.blocked if name == "docs/A.md:70"]
        for call in ("process: morf.process.run", "process: morf.process.spawn",
                     "process: morf.process.detach", "process: morf.ui.Terminal",
                     "write: morf.fs.write", "write: morf.fs.mkdir",
                     "network: morf.http.get", "network: morf.socket.connect",
                     "network: morf.socket.request", "network: morf.dbus.call_async"):
            self.assertIn(call, stubbed)
        self.assertEqual(len(stubbed), 14, stubbed)


@unittest.skipUnless(BINARY, "needs a built morf (cargo build -p morf-cli)")
class CompleteConfigurations(unittest.TestCase):
    def test_a_complete_configuration_runs_sandboxed_through_morf_check(self):
        work = WORK / "configs"
        good = 'morf.surface.namespace = "x"\nmorf.process.spawn { command = { "true" } }\nui.Rect { width = 4 }'
        bad = 'morf.surface.namespace = "x"\nui.Rect { width = 4 }\nlocal n = nil\nn.call()'
        blocks = {"docs/B.md": [(5, good, None), (15, bad, None), (25, "syntax error here", "elided")]}
        with contextlib.redirect_stdout(io.StringIO()):
            problems = docs_check.check_examples(blocks, BINARY, work)
        self.assertEqual(problems[0], "docs/B.md:15: complete configuration fails morf check")
        self.assertNotIn("docs/B.md:5: complete configuration fails morf check", problems)


if __name__ == '__main__':
    unittest.main()
