#!/usr/bin/env python3
"""Install pattern authentication on non-NixOS systems, preserving PAM checks."""
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile

BEGIN = "# morf-pattern begin"
END = "# morf-pattern end"
HELPER = "/usr/local/libexec/morf-pattern-check"


def pam_with_pattern(text, helper=HELPER):
    # Remove only our own previous insertion, preserving required checks.
    text = re.sub(r"(?m)^# morf-pattern begin\n.*?^# morf-pattern end\n", "", text, flags=re.S)
    lines = [line for line in text.splitlines(keepends=True)
             if not (re.match(r"^\s*auth\s+", line) and "morf-pattern-check" in line)]
    insertion = next((i for i, line in enumerate(lines)
        if re.match(r"^\s*auth\s+(?:include|substack)\s+", line)
        or re.match(r"^\s*auth\s+.*\bpam_unix\.so\b", line)), None)
    if insertion is None:
        raise ValueError("No supported password-authentication entry found; PAM was not changed")
    lines.insert(insertion, f"{BEGIN}\nauth sufficient pam_exec.so quiet quiet_log expose_authtok {helper} --check\n{END}\n")
    return "".join(lines)


def replace(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".morf-pattern-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)


def main():
    if os.getuid() != 0 or Path("/etc/NIXOS").exists():
        raise SystemExit("This installer requires root on a non-NixOS system")
    here = Path(__file__).resolve().parent
    pam = Path("/etc/pam.d")
    lock = pam / "morf-lock"
    original = {}
    if lock.exists():
        lock_text = lock.read_text()
    elif (pam / "system-auth").is_file():
        lock_text = "#%PAM-1.0\nauth include system-auth\naccount include system-auth\n"
    elif (pam / "common-auth").is_file() and (pam / "common-account").is_file():
        lock_text = "#%PAM-1.0\nauth include common-auth\naccount include common-account\n"
    else:
        raise SystemExit("No supported password fallback; no changes made")
    updates = {lock: pam_with_pattern(lock_text).encode()}
    greet = pam / "greetd"
    if greet.exists(): updates[greet] = pam_with_pattern(greet.read_text()).encode()
    # Validate all PAM edits before installing anything. The binary is compiled
    # before invocation; an executable script must never be made setuid.
    binary = Path(sys.argv[1]).read_bytes()
    if not binary.startswith(b"\x7fELF"):
        raise SystemExit("Expected a compiled verifier")
    replace(Path(HELPER), binary, 0o4755)
    replace(Path("/usr/local/bin/morf-pattern"), (here / "morf-pattern").read_bytes(), 0o755)
    for path in updates:
        original[path] = path.read_bytes() if path.exists() else None
        if path.exists(): shutil.copy2(path, path.with_name(path.name + ".bak-pattern"))
    try:
        for path, data in updates.items(): replace(path, data, 0o644)
    except BaseException:
        for path, data in original.items():
            if data is None: path.unlink(missing_ok=True)
            else: replace(path, data, 0o644)
        raise
    print("Pattern authentication installed for Morf lock and greetd.")


if __name__ == "__main__":
    main()
