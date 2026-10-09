#!/usr/bin/env python3
"""Exercise screen power without a compositor, systemd, or a real display."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "nixos/scripts/screen.sh"
STUB = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
p = Path(os.environ["TEST_SCREEN_STATE"])
s = json.loads(p.read_text())
name = Path(sys.argv[0]).name
args = sys.argv[1:]
s["calls"].append([name] + args)
status = 0
if name == "hyprctl":
    if args == ["-j", "monitors"]:
        print(json.dumps([] if s.get("no_outputs") else [{"dpmsStatus": s["powered"]}]))
    elif args == ["-j", "locked"]:
        print(json.dumps({"locked": s["locked"]}))
    else:
        status = 2
elif name == "systemctl":
    if "start" in args:
        s["locked"] = not s.get("lock_failed", False)
    elif "is-active" in args:
        status = int(s.get("lock_failed", False))
    else:
        status = 2
elif name == "phone-dpms":
    status_path = Path(os.environ["XDG_RUNTIME_DIR"]) / "phone-screen-test.status"
    if args[0] == "off":
        s["shield_before_blank"] = status_path.read_text().strip() == "off"
    if s.get("dpms_failed"):
        status = 1
    else:
        s["powered"] = args[0] == "on"
p.write_text(json.dumps(s))
sys.exit(status)
'''

class ScreenTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        binary = root / "bin"
        binary.mkdir()
        for name in ("hyprctl", "systemctl", "phone-dpms"):
            p = binary / name
            p.write_text(STUB)
            p.chmod(0o755)
        self.state = root / "state.json"
        self.state.write_text(json.dumps({"powered": True, "locked": False, "calls": []}))
        self.env = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ["PATH"],
                        XDG_RUNTIME_DIR=str(root), HYPRLAND_INSTANCE_SIGNATURE="test",
                        TEST_SCREEN_STATE=str(self.state), MORF_PHONE_GREETER="0")

    def update(self, **values):
        s = self.read()
        s.update(values)
        self.state.write_text(json.dumps(s))

    def read(self):
        return json.loads(self.state.read_text())

    def run_action(self, action, success=True):
        result = subprocess.run(["bash", str(SCRIPT), action], env=self.env,
                                text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return self.read()

    def test_locks_before_blanking(self):
        s = self.run_action("off")
        self.assertTrue(s["locked"])
        self.assertFalse(s["powered"])
        self.assertTrue(s["shield_before_blank"])
        lock = s["calls"].index(["systemctl", "--user", "start", "morf-idle-lock.service"])
        blank = s["calls"].index(["phone-dpms", "off"])
        self.assertLess(lock, blank)

    def test_failed_lock_leaves_screen_on(self):
        self.update(lock_failed=True)
        s = self.run_action("off", success=False)
        self.assertTrue(s["powered"])
        self.assertNotIn(["phone-dpms", "off"], s["calls"])

    def test_greeter_blanks_without_starting_a_locker(self):
        self.env["MORF_PHONE_GREETER"] = "1"
        s = self.run_action("off")
        self.assertFalse(s["powered"])
        self.assertFalse(any(c[0] == "systemctl" for c in s["calls"]))

    def test_power_wakes_without_unlocking(self):
        self.update(powered=False, locked=True)
        s = self.run_action("toggle")
        self.assertTrue(s["powered"])
        self.assertTrue(s["locked"])
        self.assertEqual((Path(self.tmp.name) / "phone-screen-test.status").read_text(), "on\n")

    def test_failed_blanking_removes_the_input_shield(self):
        self.update(dpms_failed=True)
        self.run_action("off", success=False)
        self.assertEqual((Path(self.tmp.name) / "phone-screen-test.status").read_text(), "on\n")

    def test_repeated_wake_repairs_a_stale_sleeping_marker(self):
        marker = Path(self.tmp.name) / "phone-screen-test.status"
        marker.write_text("off\n")
        s = self.run_action("wake")
        self.assertTrue(s["powered"])
        self.assertEqual(marker.read_text(), "on\n")

    def test_idle_resume_then_power_signal_does_not_reblank(self):
        self.update(powered=False, locked=True)
        self.run_action("wake")
        s = self.run_action("toggle")
        self.assertTrue(s["powered"])
        self.assertNotIn(["phone-dpms", "off"], s["calls"])

    def test_power_signal_then_idle_resume_only_wakes_once(self):
        self.update(powered=False, locked=True)
        self.run_action("toggle")
        s = self.run_action("wake")
        self.assertEqual(s["calls"].count(["phone-dpms", "on"]), 1)

    def test_missing_outputs_do_not_attempt_to_lock(self):
        self.update(no_outputs=True)
        s = self.run_action("off", success=False)
        self.assertFalse(any(c[0] == "systemctl" for c in s["calls"]))

if __name__ == "__main__":
    unittest.main()
