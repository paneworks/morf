#!/usr/bin/env python3
"""Exercise protected storage inside a private user namespace, never live PAM."""
import contextlib
import importlib.machinery
import importlib.util
import io
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class Pattern(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="morf-pattern-test-")
        cls.base = Path(cls.tmp.name)
        cls.private = cls.base / "private"
        cls.public = cls.base / "public"
        cls.helper = str(cls.base / "check")
        env = dict(os.environ, PATH="/usr/bin:/bin", LD_LIBRARY_PATH="")
        for key in ("NIX_LDFLAGS", "NIX_CFLAGS_COMPILE", "LIBRARY_PATH", "COMPILER_PATH"):
            env.pop(key, None)
        subprocess.run(["/usr/bin/cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
            f'-DPATTERN_PARENT="{cls.private}"', f'-DPATTERN_PUBLIC_PARENT="{cls.public}"',
            str(HERE / "check.c"), "-lcrypt", "-o", cls.helper], check=True, env=env)
        cls.pam_test = str(cls.base / "pam-test")
        subprocess.run(["/usr/bin/cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
            str(HERE / "pam-test.c"), "-lpam", "-o", cls.pam_test], check=True, env=env)
        cls.account = pwd.getpwnam(os.environ["PATTERN_TEST_USER"])
        loader = importlib.machinery.SourceFileLoader("pattern_cli", str(HERE / "morf-pattern"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        cls.cli = importlib.util.module_from_spec(spec)
        loader.exec_module(cls.cli)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def run_helper(self, action, secret=None, user=None):
        args = [self.helper, action]
        if action != "--validate": args.append(user or self.account.pw_name)
        return subprocess.run(args, input=secret, capture_output=True)

    def setUp(self):
        self.assertEqual(self.run_helper("--set", b"123654").returncode, 0)
        self.hash = self.private / "pattern" / f"{self.account.pw_uid}.hash"
        self.rate = self.private / "pattern" / f"{self.account.pw_uid}.attempts"

    def test_adjacent_including_diagonals_but_no_jumps_or_repeats(self):
        for value in (b"123654", b"1598", b"14785", b"123698745"):
            self.assertEqual(self.run_helper("--validate", value).returncode, 0, value)
        for value in (b"123456", b"1357", b"1232", b"123", b"0123", b"1236547891", b"123654\0x", b"123654\nignored"):
            self.assertNotEqual(self.run_helper("--validate", value).returncode, 0, value)

    def test_only_root_can_read_hash_public_marker_is_empty(self):
        self.assertEqual(self.hash.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.hash.parent.stat().st_mode & 0o777, 0o700)
        marker = self.public / "pattern" / self.account.pw_name
        self.assertEqual(marker.read_bytes(), b"")
        self.assertEqual(marker.stat().st_mode & 0o777, 0o644)
        self.assertEqual(marker.parent.stat().st_mode & 0o777, 0o755)
        self.assertTrue(self.hash.read_text().startswith("$y$"))
        before = self.hash.read_bytes()
        self.assertEqual(self.run_helper("--set", b"123654").returncode, 0)
        self.assertNotEqual(before, self.hash.read_bytes(), "each enrollment needs a fresh random salt")

    def test_verify_rejects_wrong_pattern_and_preserves_current_on_invalid_set(self):
        self.assertEqual(self.run_helper("--check", b"123654\0").returncode, 0)
        self.assertNotEqual(self.run_helper("--check", b"14785").returncode, 0)
        before = self.hash.read_bytes()
        self.assertNotEqual(self.run_helper("--set", b"123456").returncode, 0)
        self.assertEqual(before, self.hash.read_bytes())
        self.assertEqual(self.run_helper("--check", b"123654").returncode, 0)

    def test_cooldown_survives_new_processes_and_recovers(self):
        for _ in range(5): self.assertNotEqual(self.run_helper("--check", b"14785").returncode, 0)
        self.assertNotEqual(self.run_helper("--check", b"123654").returncode, 0)
        self.assertEqual(self.rate.read_text().split()[0], "5")
        self.rate.write_text("5 1\n")  # expire in the isolated test store
        self.assertEqual(self.run_helper("--check", b"123654").returncode, 0)
        self.assertEqual(self.rate.read_text(), "0 0\n")

    def test_insecure_files_and_symlinks_fail_closed(self):
        self.hash.chmod(0o644)
        self.assertNotEqual(self.run_helper("--check", b"123654").returncode, 0)
        self.hash.chmod(0o600)
        self.rate.unlink()
        self.rate.symlink_to(self.hash)
        self.assertNotEqual(self.run_helper("--check", b"123654").returncode, 0)
        self.rate.unlink()

    def test_clear_disables_verification(self):
        self.assertEqual(self.run_helper("--clear").returncode, 0)
        self.assertFalse(self.hash.exists())
        self.assertFalse((self.public / "pattern" / self.account.pw_name).exists())
        self.assertNotEqual(self.run_helper("--check", b"123654").returncode, 0)

    def test_enrollment_retries_invalid_and_mismatch_without_echoing_secrets(self):
        answers = iter(("123456", "14785", "1598", "1598", "1598"))
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertTrue(self.cli.enroll(self.account.pw_name, self.helper, prompt=lambda _: next(answers)))
        self.assertIn("neighbouring", output.getvalue())
        self.assertIn("do not match", output.getvalue())
        for secret in ("123456", "14785", "1598"): self.assertNotIn(secret, output.getvalue())
        self.assertEqual(self.run_helper("--check", b"1598").returncode, 0)

    def test_cancel_and_skip_do_not_change_existing_hash(self):
        before = self.hash.read_bytes()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertFalse(self.cli.enroll(self.account.pw_name, self.helper, optional=True, prompt=lambda _: ""))
            answers = iter(("1598",))
            def cancel(_):
                try: return next(answers)
                except StopIteration: raise KeyboardInterrupt
            with self.assertRaises(KeyboardInterrupt): self.cli.enroll(self.account.pw_name, self.helper, prompt=cancel)
        self.assertEqual(before, self.hash.read_bytes())

    def test_pam_install_preserves_required_checks_and_is_idempotent(self):
        spec = importlib.util.spec_from_file_location("pattern_install", HERE / "install.py")
        install = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(install)
        original = "#%PAM-1.0\nauth required pam_securetty.so\nauth requisite pam_nologin.so\nauth include system-login\naccount include system-local-login\nsession include system-local-login\n"
        updated = install.pam_with_pattern(original)
        self.assertLess(updated.index("pam_nologin"), updated.index(install.BEGIN))
        self.assertLess(updated.index(install.END), updated.index("auth include system-login"))
        self.assertEqual(install.pam_with_pattern(updated), updated)
        self.assertTrue(updated.endswith(original[original.index("auth include"):]))
        with self.assertRaises(ValueError): install.pam_with_pattern("auth required custom-module.so\n")

    def test_real_pam_conversation_and_password_fallback(self):
        conf = self.base / "pam.d"
        conf.mkdir(exist_ok=True)
        path = conf / "test-pattern"
        rule = f"auth sufficient /usr/lib/security/pam_exec.so quiet expose_authtok {self.helper} --check\n"
        def authenticate(value):
            env = dict(os.environ, LD_LIBRARY_PATH="")
            return subprocess.run([self.pam_test, str(conf), self.account.pw_name], input=value,
                                  capture_output=True, env=env).returncode
        path.write_text(rule + "auth required /usr/lib/security/pam_deny.so\naccount required /usr/lib/security/pam_permit.so\n")
        self.assertEqual(authenticate(b"123654\n"), 0)
        self.assertNotEqual(authenticate(b"14785\n"), 0)
        # Model an accepted password downstream: a failed pattern must fall through.
        path.write_text(rule + "auth required /usr/lib/security/pam_permit.so\naccount required /usr/lib/security/pam_permit.so\n")
        self.assertEqual(authenticate(b"account-password\n"), 0)
        # Even a valid pattern must not bypass a failing account policy.
        path.write_text(rule + "auth required /usr/lib/security/pam_permit.so\naccount required /usr/lib/security/pam_deny.so\n")
        self.assertNotEqual(authenticate(b"123654\n"), 0)


if __name__ == "__main__":
    if "--inside" not in sys.argv:
        env = dict(os.environ, PATTERN_TEST_USER=pwd.getpwuid(os.getuid()).pw_name)
        sys.exit(subprocess.run(["unshare", "--user", "--map-root-user", sys.executable, __file__, "--inside"], env=env).returncode)
    sys.argv.remove("--inside")
    unittest.main()
