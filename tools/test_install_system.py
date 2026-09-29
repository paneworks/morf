#!/usr/bin/env python3
"""Migration tests with temporary data, no sudo or live configuration writes."""
import importlib.util
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import subprocess

spec = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('install-system.py'))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class Migration(unittest.TestCase):
    def test_applied_appearance_uses_selected_theme_for_greeter_defaults(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'morf/caelestia'
            config.mkdir(parents=True)
            with patch.dict(installer.os.environ, {'XDG_CONFIG_HOME': tmp}):
                self.assertEqual(installer.appearance_defaults('caelestia'), {'theme': 'material', 'font': ''})
                (config / 'appearance.json').write_text('{"theme":"tsugumori","font":"Goku"}')
                self.assertEqual(installer.appearance_defaults('caelestia'), {'theme': 'tsugumori', 'font': 'Goku'})
                (config / 'appearance.json').write_text('{"theme":"../invalid"}')
                self.assertEqual(installer.appearance_defaults('caelestia')['theme'], 'material')

    def test_greetd_preserves_session_and_terminal_options(self):
        text = '''[terminal]
vt = 2
[initial_session]
command = "keep-this-command"
user = "someone"
[default_session]
command = 'cage -s -- /usr/bin/logre'
user = "greeter"
[general]
source_profile = false
'''
        updated, user = installer.migrate_config(text, 'caelestia')
        before, after = tomllib.loads(text), tomllib.loads(updated)
        self.assertEqual(user, 'greeter')
        self.assertEqual(after['default_session']['command'], 'cage -s -- /usr/bin/morf greet -c caelestia')
        after['default_session']['command'] = before['default_session']['command']
        self.assertEqual(before, after)

    def test_multiline_command_is_rejected_without_modifying_settings(self):
        with self.assertRaises((AssertionError, tomllib.TOMLDecodeError)):
            installer.migrate_config('[default_session]\nuser="greeter"\ncommand="""\ncage -- old\n"""\n', 'caelestia')

    def test_checksum_rejects_changed_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tree = installer.new_stage(root / 'stage')
            target = tree / 'usr/bin/morf'
            target.parent.mkdir(parents=True)
            target.write_bytes(b'fixture')
            installer.save_stage(root / 'stage', {'kind': 'runtime', 'trees': ['usr/share/morf/library']})
            installer.validate(root / 'stage')
            target.write_bytes(b'changed')
            with self.assertRaises(AssertionError): installer.validate(root / 'stage')

    def test_stage_refuses_to_delete_an_unrecognized_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'important').write_text('keep')
            with self.assertRaises(AssertionError): installer.new_stage(root)
            self.assertEqual((root / 'important').read_text(), 'keep')

    def test_runtime_commit_rolls_back_a_failed_binary_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = root / 'stage'
            tree = installer.new_stage(stage)
            (tree / 'usr/bin').mkdir(parents=True)
            (tree / 'usr/bin/morf').write_bytes(b'new binary')
            (tree / 'usr/share/morf/library/lib').mkdir(parents=True)
            (tree / 'usr/share/morf/library/lib/new.lua').write_text('new')
            installer.save_stage(stage, {'kind': 'runtime', 'trees': ['usr/share/morf/library']})
            system = root / 'system'
            (system / 'usr/bin').mkdir(parents=True)
            (system / 'usr/bin/morf').write_bytes(b'old binary')
            (system / 'usr/share/morf/library/lib').mkdir(parents=True)
            (system / 'usr/share/morf/library/lib/old.lua').write_text('old')
            def paths(value): return system if value == '/' else Path(value)
            with patch.object(installer, 'Path', side_effect=paths), \
                 patch.object(installer.os, 'geteuid', return_value=0), \
                 patch.object(installer, 'backup_dir', return_value=root / 'backup'), \
                 patch.object(installer.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, 'morf')):
                with self.assertRaises(subprocess.CalledProcessError): installer.commit(stage)
            self.assertEqual((system / 'usr/bin/morf').read_bytes(), b'old binary')
            self.assertEqual((system / 'usr/share/morf/library/lib/old.lua').read_text(), 'old')
            self.assertFalse((system / 'usr/share/morf/library/lib/new.lua').exists())

    def test_backup_replaces_history_and_does_not_follow_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            previous = root / 'previous'
            installer.reset_backup(previous)
            (previous / 'old-file').write_text('old')
            installer.reset_backup(previous)
            self.assertEqual(list(previous.iterdir()), [])
            (previous / 'new-file').write_text('new')
            self.assertEqual(list(root.iterdir()), [previous])
            outside = root / 'keep'
            outside.mkdir()
            (outside / 'important').write_text('keep')
            link = root / 'linked-backup'
            link.symlink_to(outside)
            installer.reset_backup(link)
            self.assertFalse(link.is_symlink())
            self.assertEqual((outside / 'important').read_text(), 'keep')


if __name__ == '__main__': unittest.main()
