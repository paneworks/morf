#!/usr/bin/env python3
"""Docs gate tests: extraction, inventory and allow-list matching."""
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'docs_check', Path(__file__).with_name('docs_check.py'))
docs_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs_check)


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


class Allowlist(unittest.TestCase):
    def test_parse_requires_reason(self):
        with self.assertRaises(ValueError):
            docs_check.parse_allowlist("morf.core.*\n")
        internal, dynamic = docs_check.parse_allowlist(
            "morf.core.*\tmirror\n+morf.surface.blend\tdynamic\n")
        self.assertTrue(docs_check.allowlisted("morf.core.args", internal))
        self.assertFalse(docs_check.allowlisted("morf.corex", internal))
        self.assertIn("morf.surface.blend", dynamic)

    def test_stale_detection(self):
        used = docs_check.matching_entries(["morf.core.args"],
                                           {"morf.core.*": "r", "morf.x": "r"})
        self.assertEqual(used, {"morf.core.*"})


if __name__ == '__main__':
    unittest.main()
