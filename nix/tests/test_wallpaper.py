import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("handoff", Path(__file__).resolve().parents[1] / "nixos/scripts/wallpaper.py")
handoff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(handoff)


class HandoffTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.state = self.root / "shared"
        self.state.mkdir()
        self.image = self.root / "private wallpaper.png"
        self.image.write_bytes(b"original image")
        self.palette = {"wallpaper": str(self.image), "theme": "dark", "colors": ["#111111", "#abcdef"]}

    def test_private_image_is_copied_and_adopted_with_palette(self):
        image = handoff.publish(self.state, self.image, self.palette)
        self.image.unlink()
        self.assertEqual(image.read_bytes(), b"original image")
        with patch.dict(os.environ, {"XDG_STATE_HOME": str(self.root / "user-state")}):
            adopted = handoff.adopt(self.state, self.root / "user-cache", self.root)
        self.assertEqual(adopted, image)
        self.assertEqual((self.root / "user-state/lule/wallpaper").resolve(), image)
        self.assertEqual(json.loads((self.root / "user-cache/colors.json").read_text())["colors"], self.palette["colors"])
        self.assertEqual((self.root / "user-cache/colors.json").stat().st_mode & 0o777, 0o600)

    def test_generation_once_per_boot_and_logout_keeps_latest_selection(self):
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            if command[1] == "wallpaper":
                Path(next(x[9:] for x in command if x.startswith("--output="))).write_bytes(str(len(calls)).encode())
            else:
                cache = Path(next(x[8:] for x in command if x.startswith("--cache=")))
                cache.mkdir()
                (cache / "colors.json").write_text(json.dumps(self.palette))
        cfg = {"lule": "lule-test", "lule_config": "isolated config", "fallback_logo": "fallback.svg"}
        with patch.object(handoff.subprocess, "run", run), patch.object(handoff, "dimensions", return_value=(1116, 2484)):
            first = handoff.greet(cfg, self.state, "boot-one")
            self.assertEqual(len(calls), 2)
            self.assertIn("--width=1116", calls[0])
            latest = handoff.publish(self.state, self.image, self.palette)
            self.assertNotEqual(latest, first)
            self.assertEqual(handoff.greet(cfg, self.state, "boot-one"), latest)
            self.assertEqual(len(calls), 2, "return to greetd generated over the user's selection")
            self.assertNotEqual(handoff.greet(cfg, self.state, "boot-two"), latest)
            self.assertEqual(len(calls), 4)

    def test_failed_generation_does_not_replace_last_valid_wallpaper(self):
        image = handoff.publish(self.state, self.image, self.palette)
        with patch.object(handoff.subprocess, "run", side_effect=OSError("generator unavailable")):
            with self.assertRaises(OSError):
                handoff.greet({"lule": "lule-test", "lule_config": "test", "fallback_logo": "logo.svg"}, self.state, "new-boot")
        self.assertEqual(handoff.read_palette(self.state / "colors.json")["wallpaper"], str(image))
        self.assertFalse((self.state / "boot-id").exists())


if __name__ == "__main__":
    unittest.main()
