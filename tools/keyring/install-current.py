#!/usr/bin/python3
"""Install the validated keyring-only additions into the existing shell."""
from pathlib import Path
import hashlib, json, os, shutil
repo=Path(__file__).resolve().parents[2]
home=Path.home(); shell=home/'.config/morf/caelestia/shell'
stage=repo/'target/keyring-install'
manifest=json.loads((stage/'manifest.json').read_text())
init=shell/'init.lua'
assert hashlib.sha256(init.read_bytes()).hexdigest()==manifest['init_sha256'], 'Installed shell changed since validation; no changes made'
assert (repo/'target/dist/morf-keyring').is_file()
assert (stage/'caelestia/shell/init.lua').read_text().count('local keyring = require("keyring")')==1
backup=home/'.local/state/morf/previous'
if backup.is_symlink() or backup.is_file():backup.unlink()
elif backup.exists():shutil.rmtree(backup)
backup.mkdir(parents=True)
files=[(repo/'target/dist/morf-keyring',home/'.local/bin/morf-keyring',0o755),
       (repo/'library/lib/keyring_agent.lua',home/'.local/share/morf/library/lib/keyring_agent.lua',0o644),
       (repo/'examples/shells/caelestia/shell/keyring_view.lua',shell/'keyring_view.lua',0o644),
       (repo/'examples/shells/caelestia/shell/keyring.lua',shell/'keyring.lua',0o644),
       (stage/'caelestia/shell/init.lua',init,0o644)]
for index,(source,destination,mode) in enumerate(files):
    if destination.exists():shutil.copy2(destination,backup/(str(index)+'-'+destination.name))
    destination.parent.mkdir(parents=True,exist_ok=True)
    temp=destination.with_name('.'+destination.name+'.keyring-install')
    temp.write_bytes(source.read_bytes());temp.chmod(mode);os.replace(temp,destination)
print('Installed keyring bridge and dialog. Existing-file backups:',backup)
print('Updated the installed shell entry last so its file watcher can reload once.')
