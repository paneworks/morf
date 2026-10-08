"""Wallpaper handoff between the unprivileged greeter and primary user."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid


def atomic(path, data, mode=0o644):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".handoff-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data.encode() if isinstance(data, str) else data)
            os.fchmod(stream.fileno(), mode)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def read_palette(path):
    data = json.loads(Path(path).read_text())
    if not isinstance(data, dict) or not isinstance(data.get("colors"), list) or len(data["colors"]) < 2:
        raise ValueError("Wallpaper palette has no colors")
    return data


def publish(state, image, palette):
    image = Path(image)
    if not image.is_file() or image.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("Wallpaper must be a regular image smaller than 64 MiB")
    data = image.read_bytes()
    suffix = image.suffix.lower()
    if suffix not in (".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".avif", ".svg"):
        raise ValueError("Unsupported wallpaper format")
    target = state / ("image-" + hashlib.sha256(data).hexdigest() + suffix)
    if not target.exists():
        atomic(target, data)
    palette = dict(palette, wallpaper=str(target))
    # Commit the complete image first. Both users see either the old pair or
    # the new pair, never a JSON pointer to an unfinished image.
    atomic(state / "colors.json", json.dumps(palette))
    # Retain the previous image too: a desktop may still be displaying it.
    images = sorted(state.glob("image-*"), key=lambda p: p.stat().st_mtime, reverse=True)
    keep = {target, *images[:3]}
    for old in images:
        if old not in keep:
            old.unlink(missing_ok=True)
    return target


def dimensions():
    # This runs before greetd: use the connected panel's native mode without
    # needing a compositor, a display server, or root privileges.
    for connector in sorted(Path("/sys/class/drm").glob("card*-*")):
        try:
            if (connector / "status").read_text().strip() != "connected":
                continue
            width, height = map(int, (connector / "modes").read_text().splitlines()[0].split("x"))
            factor = min(1, 4096 / max(width, height))
            return max(64, int(width * factor)), max(64, int(height * factor))
        except (OSError, ValueError, IndexError):
            continue
    return 1920, 1080


def greet(cfg, state, boot):
    try:
        current = read_palette(state / "colors.json")
        if (state / "boot-id").read_text().strip() == boot and Path(current["wallpaper"]).is_file():
            return Path(current["wallpaper"])
    except (OSError, ValueError, KeyError):
        pass
    logo = state / "logo.svg"
    if not logo.is_file():
        logo = Path(cfg["fallback_logo"])
    width, height = dimensions()
    with tempfile.TemporaryDirectory(prefix=".generate-", dir=state) as temporary:
        root = Path(temporary)
        image = root / "wallpaper.png"
        commands = [
            [cfg["lule"], "wallpaper", "--logo=" + str(logo), "--size=40",
             "--width=" + str(width), "--height=" + str(height), "--output=" + str(image)],
            [cfg["lule"], "--configs=" + cfg["lule_config"], "--cache=" + str(root / "cache"),
             "create", "--image=" + str(image), "--theme=dark", "--", "set"],
        ]
        for command in commands:
            subprocess.run(command, stdin=subprocess.DEVNULL, stdout=sys.stderr, check=True, timeout=120)
        result = publish(state, image, read_palette(root / "cache/colors.json"))
    atomic(state / "boot-id", boot + "\n")
    return result


def adopt(state, cache, home):
    palette = read_palette(state / "colors.json")
    image = Path(palette["wallpaper"])
    if not image.is_file():
        raise ValueError("Shared wallpaper is missing")
    atomic(cache / "colors.json", json.dumps(palette), 0o600)
    atomic(cache / "wallpaper", str(image), 0o600)
    atomic(cache / "theme", palette.get("theme", "dark"), 0o600)
    # Hyprpaper can display the adopted image immediately at its next start.
    directory = Path(os.environ.get("XDG_STATE_HOME", str(home / ".local/state"))) / "lule"
    directory.mkdir(parents=True, exist_ok=True)
    link = directory / (".wallpaper-" + uuid.uuid4().hex)
    try:
        link.symlink_to(image)
        os.replace(link, directory / "wallpaper")
    finally:
        link.unlink(missing_ok=True)
    return image


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("logo", "greet", "adopt", "publish"))
    parser.add_argument("--config", default="/etc/morf/wallpaper.json")
    parser.add_argument("--image")
    parser.add_argument("--palette")
    args = parser.parse_args()
    cfg = json.loads(Path(args.config).read_text())
    state, home = Path(cfg["directory"]), Path.home()
    cache = Path(os.environ.get("LULE_A") or os.environ.get("XDG_CACHE_HOME", str(home / ".cache")) + "/lule")
    # The NixOS tmpfiles rule creates the shared directory with the two users'
    # group. Files are ordinary data, and no operation runs with root's UID.
    with (state / ".lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if args.action == "logo":
            source = Path(cfg["logo"])
            if source.is_file():
                atomic(state / "logo.svg", source.read_bytes())
            return
        if args.action == "greet":
            image = greet(cfg, state, Path("/proc/sys/kernel/random/boot_id").read_text().strip())
        elif args.action == "adopt":
            image = adopt(state, cache, home)
        else:
            palette = read_palette(args.palette or cache / "colors.json")
            image = publish(state, args.image or palette["wallpaper"], palette)
        print(image)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("Morf wallpaper: " + str(error), file=sys.stderr)
        sys.exit(1)
