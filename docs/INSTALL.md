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
command = "cage -m last -s -- /usr/bin/morf greet -c caelestia"
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

## Unlock pattern

NixOS installations can set `programs.morf.usePackagedTheme = true` to run the
shell, lockscreen and greeter shipped with Morf. This keeps UI fixes from being
shadowed by older Lua entry points in dotfiles, while retaining saved appearance
preferences. Explicit configuration paths and `MORF_CONFIG` still take precedence.

The NixOS module installs `morf-pattern` and enables pattern authentication for
greetd and `morf-lock`. Enroll locally with `sudo morf-pattern setup USER` (Enter
skips), or use `set` to replace a pattern and `clear` to remove it. An existing
pattern is preserved by `setup`. The OS installer can invoke `setup` immediately
after `passwd`; no secret belongs in a Nix option or installation manifest.

The prompt hides input and asks for confirmation. Dots are numbered `123 / 456 /
789`: connect 4–9 distinct neighbouring dots, including diagonals. `123654` is
valid; `123456` is rejected because 3 and 4 are not neighbours. The lock and login
screens enforce the same rule. Password login remains available.

Only a randomly salted yescrypt hash is stored, under `/var/lib/morf/pattern`
(root-only directory and files). `/etc/morf/pattern/USER` is an empty, public
enrollment marker so both screens know when to offer patterns. The compiled
setuid verifier accepts an unprivileged caller only for their own account;
enrollment/removal require root. Five failed pattern attempts trigger a
30-second cooldown, shared by login and lock. The account password remains in
the system password store; it is never copied or recovered from the pattern.

On non-NixOS systems, `sudo tools/pattern/install.sh USER` builds the helper,
preserves the existing PAM password/account checks, and starts enrollment.
It requires a C compiler, libxcrypt headers and Python 3. NixOS uses the module
instead of this imperative installer. `programs.morf.pattern.enable = false`
disables the integration without changing the account password.
