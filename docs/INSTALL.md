# One Morf installation

Shell, lock and greeter run the same executable:

```sh
morf shell                 # bare morf also starts the shell
morf lock
morf greet
morf lock -c caelestia     # choose a named configuration
morf lock -- window preview
morf greet -- preview
```

Run the build recipes as your normal user. Each invokes sudo for its system
installation step (sudo may reuse an existing authentication timestamp):

```sh
oslo make install
oslo make apply --example caelestia
# Subsequent updates can use: oslo make apply
```

`make install` builds the portable executable, installs `/usr/bin/morf` and
`/usr/share/morf/library`, then migrates existing Hyprland commands away from
`~/.local/bin/morf`. It backs up and removes that local executable. The old
user library becomes a symlink to the system library, allowing an already
running engine and editor configurations to keep finding it.

`make apply` stages the selected shell's Lua, themes, assets and fonts. It
publishes the parts under `/etc/xdg/morf/NAME/` and `~/.config/morf/NAME/`, with
`default` links in both roots. User appearance/settings JSON beside the parts
is preserved. With no `--example`, it uses the user's current default shell
(or Caelestia if none is selected). A system Morf installation is required first.

When greetd is installed and the selected shell provides a greeter, apply
validates the greeter under greetd's own account with an empty temporary home
before updating `/etc/greetd/config.toml`:

```toml
[default_session]
command = "cage -s -- /usr/bin/morf greet -c caelestia"
user = "greeter"
```

Existing greetd account, terminal and other settings are retained. Apply
never changes PAM or restarts greetd. The command takes effect at the next
login. After the check succeeds and greetd points to Morf, the obsolete
`logre` executables are moved into the system backup directory.

User configurations take priority; system configurations are found through
`XDG_CONFIG_DIRS` (default `/etc/xdg`). Shared libraries are also found through
`XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`). The greeter uses its
own account, so it finds the system configuration without reading your home.

System replacements keep one backup in `/var/backups/morf/previous/`.
User replacements keep one backup in `~/.local/state/morf/previous/`.
Each operation replaces the previous backup; no dated history is retained. A failed system
validation restores the replaced files before returning an error. User
configuration is applied only after the system step succeeds.
