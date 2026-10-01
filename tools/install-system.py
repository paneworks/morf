#!/usr/bin/env python3
"""Installation helpers for `oslo make install` and `oslo make apply`.

Prepare files as the user, commit system files through sudo, then update the
user's configuration. Never alter PAM or restart greetd/the desktop session.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[1]
CONFIG = Path('/etc/greetd/config.toml')
FORMAT = 'morf-install-v1'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def clean_env():
    env = dict(os.environ)
    for name in ('LD_LIBRARY_PATH', 'XDG_DATA_DIRS', 'GREETD_SOCK', 'MORF_RUNTIME_PATH', 'MORF_CONFIG', 'MORF_FONT_PATH'):
        env.pop(name, None)
    return env


def migrate_config(text, example):
    parsed = tomllib.loads(text)
    assert isinstance(parsed.get('default_session', {}).get('user'), str), 'greetd has no greeter user'
    command = f'cage -m last -s -- /usr/bin/morf greet -c {example}'
    lines = text.splitlines(keepends=True)
    section, replaced = None, 0
    for i, line in enumerate(lines):
        match = re.match(r'^\s*\[([^\]]+)\]', line)
        if match: section = match[1]
        if section == 'default_session' and re.match(r'^\s*command\s*=', line):
            lines[i] = 'command = ' + json.dumps(command) + '\n'
            replaced += 1
    assert replaced == 1, 'expected one single-line default_session command'
    updated = ''.join(lines).replace(
        '# the login screen, which is the way out if something is wrong. logre is\n# morf and the greeter in one file, with its font inside it.\n',
        '# the login screen, which is the way out if something is wrong.\n# The same morf executable runs shell, lock and greeter.\n')
    expected = dict(parsed, default_session=dict(parsed['default_session'], command=command))
    assert tomllib.loads(updated) == expected, 'migration changed unrelated greetd settings'
    return updated, parsed['default_session']['user']


def new_stage(directory):
    if directory.exists():
        marker = directory / 'manifest.json'
        assert marker.is_file() and json.loads(marker.read_text()).get('format') == FORMAT, f'unrecognized stage: {directory}'
        shutil.rmtree(directory)
    tree = directory / 'files'
    tree.mkdir(parents=True)
    return tree


def save_stage(directory, manifest):
    manifest['format'] = FORMAT
    manifest['files'] = {str(path.relative_to(directory / 'files')): digest(path)
                         for path in (directory / 'files').rglob('*') if path.is_file()}
    (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Staged {manifest["kind"]}: {len(manifest["files"])} files in {directory}')


def stage_runtime(directory, binary):
    assert binary.is_file(), 'build the dist executable first: oslo make dist'
    subprocess.run([str(binary), '--version'], env=clean_env(), check=True)
    tree = new_stage(directory)
    (tree / 'usr/bin').mkdir(parents=True)
    shutil.copy2(binary, tree / 'usr/bin/morf')
    shutil.copytree(REPO / 'library', tree / 'usr/share/morf/library', ignore=shutil.ignore_patterns('tests'))
    subprocess.run([str(binary), 'types', str(tree / 'usr/share/morf/library/types')], env=clean_env(), check=True)
    save_stage(directory, {'kind': 'runtime', 'trees': ['usr/share/morf/library']})


def selected_example():
    root = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'morf/default'
    return root.resolve().name if root.is_symlink() else 'caelestia'


def appearance_defaults(example):
    """Carry the selected skin into accounts without personal preferences."""
    defaults = {'theme': 'material', 'font': ''}
    config = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config'))
    path = config / 'morf' / example / 'appearance.json'
    if not path.is_file(): return defaults
    selected = json.loads(path.read_text())
    theme = selected.get('theme', 'material')
    if isinstance(theme, str) and re.fullmatch(r'[a-z][a-z0-9_]*', theme):
        if (REPO / 'examples/shells' / example / 'themes' / theme / 'manifest.lua').is_file():
            defaults['theme'] = theme
    if isinstance(selected.get('font'), str): defaults['font'] = selected['font']
    return defaults


def stage_config(directory, example):
    assert re.fullmatch(r'[A-Za-z0-9_-]+', example), 'invalid shell name'
    source = REPO / 'examples/shells' / example
    assert (source / 'shell/init.lua').is_file(), f'no shell named {example}'
    assert Path('/usr/bin/morf').is_file(), 'run make install first: /usr/bin/morf is required'
    tree = new_stage(directory)
    base = tree / 'etc/xdg/morf' / example
    parts = []
    for part in ('shell', 'lock', 'greet'):
        if not (source / part / 'init.lua').is_file(): continue
        shutil.copytree(source / part, base / part, ignore=shutil.ignore_patterns('*.pending', '__pycache__'))
        parts.append(part)
    # Include fonts used by both themes so the greeter needs no user home.
    if example == 'caelestia':
        appearance = appearance_defaults(example)
        fonts = {}
        families = ['Roboto', 'Material Symbols Rounded', 'IBM Plex Mono', 'M+1 Nerd Font']
        if appearance['font']: families.append(appearance['font'])
        for family in families:
            found = subprocess.check_output(['fc-match', '-f', '%{file}', family], env=clean_env(), text=True).strip()
            assert found and Path(found).is_file(), f'font unavailable: {family}'
            fonts[Path(found).name] = Path(found)
        for part in parts:
            (base / part / 'appearance-default.json').write_text(json.dumps(appearance, indent=2) + '\n')
            target = base / part / 'fonts'
            target.mkdir(exist_ok=True)
            for name, font in fonts.items(): shutil.copy2(font, target / name)
    manifest = {'kind': 'config', 'example': example, 'parts': parts,
                'trees': [f'etc/xdg/morf/{example}/{part}' for part in parts]}
    if 'greet' in parts and CONFIG.is_file():
        text = CONFIG.read_text()
        updated, user = migrate_config(text, example)
        target = tree / 'etc/greetd/config.toml'
        target.parent.mkdir(parents=True)
        target.write_text(updated)
        manifest.update(previous_greetd_sha256=digest(CONFIG), greeter_user=user)
    save_stage(directory, manifest)


def validate(directory):
    manifest = json.loads((directory / 'manifest.json').read_text())
    assert manifest['format'] == FORMAT and manifest['kind'] in ('runtime', 'config')
    allowed = ('usr/bin/morf', 'usr/share/morf/library/') if manifest['kind'] == 'runtime' else ('etc/xdg/morf/', 'etc/greetd/config.toml')
    for name, checksum in manifest['files'].items():
        path = Path(name)
        assert not path.is_absolute() and '..' not in path.parts and name.startswith(allowed), 'unsafe staged path'
        assert digest(directory / 'files' / path) == checksum, f'staged file changed: {name}'
    return manifest


def backup_dir():
    return Path('/var/backups/morf/previous')


def reset_backup(path):
    """One previous copy, replaced on every operation; never follow a link."""
    if path.is_symlink() or path.is_file(): path.unlink()
    elif path.exists(): shutil.rmtree(path)
    path.mkdir(parents=True)
    return path


def user_backup():
    return Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'morf/previous'


def commit(directory):
    assert os.geteuid() == 0, 'system installation needs sudo'
    manifest = validate(directory)
    if 'previous_greetd_sha256' in manifest:
        assert digest(CONFIG) == manifest['previous_greetd_sha256'], 'greetd changed since staging; run make apply again'
    backup = reset_backup(backup_dir())
    # Retire snapshots made by older installers on the next privileged run.
    for old in backup.parent.iterdir():
        if re.fullmatch(r'\d{8}-\d{6}(?:-\d+)?', old.name):
            if old.is_symlink() or old.is_file(): old.unlink()
            elif old.is_dir(): shutil.rmtree(old)
    changed, retired = [], []

    def remember(dst):
        saved = backup / dst.relative_to('/')
        if dst.exists() or dst.is_symlink():
            saved.parent.mkdir(parents=True, exist_ok=True)
            if dst.is_symlink(): saved.symlink_to(os.readlink(dst))
            elif dst.is_dir(): shutil.copytree(dst, saved, symlinks=True)
            else: shutil.copy2(dst, saved)
        changed.append((dst, saved))

    def remove(path):
        if path.is_dir() and not path.is_symlink(): shutil.rmtree(path)
        elif path.exists() or path.is_symlink(): path.unlink()

    def place(name):
        dst = Path('/') / name
        remember(dst)
        dst.parent.mkdir(parents=True, exist_ok=True)
        temporary = Path(tempfile.mkdtemp(prefix='.morf-install-', dir=dst.parent))
        candidate = temporary / dst.name
        try:
            src = directory / 'files' / name
            if src.is_dir(): shutil.copytree(src, candidate)
            else: shutil.copy2(src, candidate)
            if candidate.is_dir():
                candidate.chmod(0o755)
                for p in candidate.rglob('*'): p.chmod(0o755 if p.is_dir() else 0o644)
                remove(dst)
            else: candidate.chmod(0o755 if name == 'usr/bin/morf' else 0o644)
            os.replace(candidate, dst)
        finally: shutil.rmtree(temporary)

    try:
        if manifest['kind'] == 'runtime': place('usr/bin/morf')
        for name in manifest['trees']: place(name)
        if manifest['kind'] == 'runtime':
            subprocess.run(['/usr/bin/morf', '--version'], env=clean_env(), check=True)
        else:
            default = Path('/etc/xdg/morf/default')
            assert not default.exists() or default.is_symlink(), 'system default is a real directory'
            remember(default); remove(default); default.symlink_to(manifest['example'])
            if 'greeter_user' in manifest:
                user = pwd.getpwnam(manifest['greeter_user'])
                with tempfile.TemporaryDirectory(prefix='morf-greeter-check-') as scratch:
                    os.chown(scratch, user.pw_uid, user.pw_gid)
                    subprocess.run(['runuser', '-u', user.pw_name, '--', 'env',
                        '-u', 'LD_LIBRARY_PATH', '-u', 'GREETD_SOCK', '-u', 'MORF_RUNTIME_PATH', '-u', 'MORF_FONT_PATH',
                        f'HOME={scratch}', f'XDG_CONFIG_HOME={scratch}/.config', f'XDG_DATA_HOME={scratch}/.local/share',
                        f'XDG_CACHE_HOME={scratch}/.cache', 'XDG_DATA_DIRS=/usr/local/share:/usr/share',
                        'CAELESTIA_DRY_RUN=1', '/usr/bin/morf', 'check',
                        f'/etc/xdg/morf/{manifest["example"]}/greet/init.lua', '--no-dbus', '--strict', '--after', '1200'],
                        check=True, timeout=90)
                place('etc/greetd/config.toml')
                for parent in (Path('/usr/bin'), Path('/usr/local/bin')):
                    for old in [parent / 'logre', *parent.glob('logre.*')]:
                        if old.is_file() or old.is_symlink():
                            saved = backup / old.relative_to('/')
                            saved.parent.mkdir(parents=True, exist_ok=True)
                            shutil.move(str(old), str(saved)); retired.append((old, saved))
    except BaseException:
        for old, saved in reversed(retired): shutil.move(str(saved), str(old))
        for dst, saved in reversed(changed):
            remove(dst)
            if saved.is_symlink(): dst.symlink_to(os.readlink(saved))
            elif saved.is_dir(): shutil.copytree(saved, dst, symlinks=True)
            elif saved.exists(): shutil.copy2(saved, dst)
        raise
    print(f'Installed system {manifest["kind"]}; backup: {backup}')


def finish_runtime():
    assert os.geteuid() != 0, 'run make as your normal user'
    subprocess.run(['/usr/bin/morf', '--version'], env=clean_env(), check=True)
    home = Path.home()
    backup = reset_backup(user_backup())
    # Existing compositor commands must work before the old executable goes.
    for name in ('binds.lua', 'startup.lua'):
        path = Path(os.environ.get('XDG_CONFIG_HOME', home / '.config')) / 'hypr/lua' / name
        if not path.is_file(): continue
        before = path.read_text()
        after = before.replace('ctx.home .. "/.local/bin/morf', '"/usr/bin/morf').replace('logre -- lock', '/usr/bin/morf lock')
        if after != before:
            shutil.copy2(path, backup / name)
            temporary = path.with_name(path.name + '.morf-new'); temporary.write_text(after); os.replace(temporary, path)
    for path, label in ((home / '.local/bin/morf', 'morf'),
                        (Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) / 'morf/library', 'library')):
        if path.exists() or path.is_symlink(): shutil.move(str(path), str(backup / label))
    # An already-running older engine still searches this location. Point it
    # at the system library so live reloads and editor paths keep working.
    library = Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) / 'morf/library'
    library.parent.mkdir(parents=True, exist_ok=True)
    library.symlink_to('/usr/share/morf/library')
    print(f'User commands now use /usr/bin/morf; retired local files: {backup}')


def apply_user(directory):
    assert os.geteuid() != 0, 'run make as your normal user'
    manifest = validate(directory)
    assert manifest['kind'] == 'config'
    root = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'morf'
    default = root / 'default'
    assert not default.exists() or default.is_symlink(), 'user default is a real directory'
    folder = root / manifest['example']; folder.mkdir(parents=True, exist_ok=True)
    backup = reset_backup(user_backup())
    for part in manifest['parts']:
        src = directory / 'files/etc/xdg/morf' / manifest['example'] / part
        target = folder / part
        with tempfile.TemporaryDirectory(prefix='.morf-apply-', dir=root.parent) as temporary:
            ready = Path(temporary) / part
            shutil.copytree(src, ready)
            if target.exists(): shutil.move(str(target), str(backup / part))
            shutil.move(str(ready), str(target))
    if default.is_symlink(): default.unlink()
    default.symlink_to(manifest['example'])
    print(f'Applied {manifest["example"]} for this user and the greeter; backup: {backup}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('stage-runtime', 'stage-config', 'validate', 'commit', 'finish-runtime', 'apply-user'))
    parser.add_argument('directory', type=Path, nargs='?')
    parser.add_argument('--binary', type=Path, default=REPO / 'target/dist/release/morf')
    parser.add_argument('--example', default=None)
    args = parser.parse_args()
    if args.action == 'finish-runtime': finish_runtime(); return
    assert args.directory, 'stage directory required'
    directory = args.directory.resolve()
    if args.action == 'stage-runtime': stage_runtime(directory, args.binary.resolve())
    elif args.action == 'stage-config': stage_config(directory, args.example or selected_example())
    elif args.action == 'validate': validate(directory); print('Stage checksums verified')
    elif args.action == 'commit': commit(directory)
    else: apply_user(directory)


if __name__ == '__main__':
    main()
