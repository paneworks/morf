# Bundles

A bundle is morf and a configuration in one executable: the configuration,
the Lua it requires, its assets and its fonts ride inside the file, so the
result runs on a machine, or as a user, that has none of them on disk. A
greeter started by greetd as the `greeter` user is the usual reason: that
user cannot read your `~/.config`, your `~/.local/share/morf/library` or
your `~/.fonts`, and should not depend on them.

`logre` is the example in this repository: caelestia's greeter and lock
screen as one file in `/usr/bin`.

## What a bundle is

`morf bundle CONFIG -o OUT` copies the morf executable that is running and
appends a compressed payload to it, then an 8-byte length and the magic
`MORFBNDL`. An ELF loader never reads past the headers, so the file still
starts as morf. On start, morf looks at its own tail, finds the payload,
unpacks it and runs the configuration inside. The payload is unpacked to
`$XDG_RUNTIME_DIR/morf/bundle-<hash>/` (or `/tmp/morf-<uid>/…` when there is
no runtime directory, as for a greeter). The folder is keyed on the
payload's hash, so starting the same bundle twice unpacks it once, and a
rebuilt bundle never runs stale files.

Arguments go to the configuration, after `--`, exactly as with
`morf CONFIG -- ARGS`:

```
logre                   # the configuration with no arguments
logre -- lock           # morf.operands = { "lock" }
```

## What goes in

In this order:

1. **The configuration file.** It is always the first entry; that is how
   the unpacker knows what to run.
2. **The folders beside it:** `lib/`, `assets/`, `plugin/` and `fonts/` next
   to the configuration, when they exist.
3. **Anything given with `--with PATH`,** a file or a folder:
   - a path inside the configuration's folder keeps its place (`--with
     greet` lands at `greet/…`);
   - a path from anywhere else goes in by its own name (`--with
     library/lib` lands at `lib/…`), which is exactly where
     `require("lib.auth")` looks.
4. **Every font the configuration names.** morf runs the configuration once,
   headless, reads the `font_family` of every node it built, and carries
   every file of each family into `fonts/`. Generic names (`sans-serif`,
   `monospace`, …) are left to the machine. A family that is not installed
   where you bundle is an error, not a silent fallback.

At run time the bundle puts its `fonts/` first on `MORF_FONT_PATH`, so its
faces win over whatever the machine has.

## Building one

The executable copied into the bundle is whichever morf runs `bundle`. So
run it with the **dist** binary, the one `make install` puts in
`~/.local/bin/morf`. That one is linked against the machine's own libraries,
not the Nix store, so the bundle runs outside the development shell. Run it
outside that shell too, or with its variables cleared, because they hide
the machine's Vulkan driver:

```
env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS ~/.local/bin/morf bundle path/to/init.lua -o mything \
  --with library/lib
```

`make bundle --example path/to/init.lua --name mything` does the same from
`target/dist/release/morf`, but without `--with`, so it only suits a
configuration that needs nothing beyond its own folder.

### Anything that uses the library

A configuration that does `require("lib.something")` needs `--with
library/lib`. Without it the bundle starts and then fails at the first
`require`, because a bundle does not look in `~/.local/share/morf/library`.
Carrying the whole library costs a few hundred kilobytes; it is simpler than
working out which modules are used.

### Several parts in one bundle

One bundle runs one configuration, but that configuration can choose
between parts by its first argument. `examples/shells/caelestia/logre.lua`
is all of it:

```lua
local morf = require("morf")

if morf.operands[1] == "lock" then
  table.remove(morf.operands, 1)   -- the part reads its own arguments from the first
  require("lock.init")
else
  require("greet.init")
end
```

Each part stays a self-contained folder (`greet/init.lua`, `lock/init.lua`),
runnable on its own with `morf -c caelestia/greet`. The entry file only
picks one, and `--with` carries both.

## logre

```
env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS ~/.local/bin/morf bundle examples/shells/caelestia/logre.lua \
  -o target/dist/logre \
  --with examples/shells/caelestia/greet \
  --with examples/shells/caelestia/lock \
  --with library/lib
```

That makes about 56 files: the entry, both parts, the library, and Roboto
and Material Symbols Rounded. The result is about 48 MB, most of it morf
itself.

`tools/logre/update.sh` does the build and the install in one go. Run it as
yourself; it asks sudo only for what needs root:

1. it builds `target/dist/logre` as above;
2. it installs it as `/usr/bin/logre`, keeping the first one it replaces as
   `/usr/bin/logre.bak-old`;
3. it points greetd at it: `command = "cage -s -- /usr/bin/logre"` in
   `/etc/greetd/config.toml`, keeping the old config beside it.

Then:

| what | runs |
|---|---|
| the login screen | greetd → `cage -s -- /usr/bin/logre` |
| the lock key | `logre -- lock` (Super+L in `~/.config/hypr/lua/binds.lua`) |
| the lock, to look at | `logre -- lock window preview` |

If the greeter does not come up after an update, switch to a text console
(Ctrl+Alt+F2) and put the old one back:

```
sudo cp /usr/bin/logre.bak-old /usr/bin/logre
```

## Checking a bundle before installing it

A bundle can be started in a headless cage with a throwaway home, which is
close to what the `greeter` user sees: nothing of yours on disk.

```
env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS -u WAYLAND_DISPLAY -u XDG_DATA_HOME \
  HOME=$(mktemp -d) WLR_BACKENDS=headless timeout 6 cage -- target/dist/logre
```

It should run until the timeout (exit 124) with no `morf:` errors. Do the
same with `-- lock window preview` for the lock. `preview` never touches
PAM, and that matters: a lock that is killed in the middle of a PAM
conversation counts as a failed login, and three of those lock the account
for ten minutes.

## Things to know

- **Fonts are found by running the configuration once.** Only nodes built
  at load are seen. A face used only by something built later (a per-screen
  lock builder, a panel made on demand) is not picked up. Put that font in
  a `fonts/` folder beside the configuration, or `--with` it.
- **A bundle made from a bundle** drops the old payload first, so rebundling
  is safe.
- **What is read at run time stays on the machine.** A bundle carries code
  and fonts, not state. `logre`'s lock still reads your lule colours and
  wallpaper (it runs as you); the greeter reads the system's accounts and
  sessions and an optional `/etc/morf/caelestia-accent`. PAM stacks,
  greetd's config and anything in `/etc` are the machine's.
- **Rebuild after changing the parts or the library.** The bundle is a
  snapshot; `make install` and `make apply` do not touch `/usr/bin/logre`.
