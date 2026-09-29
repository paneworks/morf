# Bundles

A bundle is morf and a configuration in one executable: the configuration,
the Lua it requires, its assets and its fonts ride inside the file, so the
result runs on a machine, or as a user, that has none of them on disk. Bundles are optional for distributing standalone widgets. Shell, lock and
greeter use the normal morf executable; see [system installation](SYSTEM.md).

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
./mything               # the configuration with no arguments
./mything -- preview     # morf.operands = { "preview" }
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
`/usr/bin/morf`. That one is linked against the machine's own libraries,
not the Nix store, so the bundle runs outside the development shell. Run it
outside that shell too, or with its variables cleared, because they hide
the machine's Vulkan driver:

```
env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS /usr/bin/morf bundle path/to/init.lua -o mything \
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

## Shell, lock and greeter

These are installed Lua configurations loaded by the same executable:

```sh
morf shell
morf lock
morf greet
```

Select a named configuration with `morf lock -c caelestia`, or preview it
without locking or authenticating with `morf lock -- window preview`.
See [system installation](SYSTEM.md) for greetd setup and migration.

## Things to know

- **Fonts are found by running the configuration once.** Only nodes built
  at load are seen. A face used only by something built later (a per-screen
  lock builder, a panel made on demand) is not picked up. Put that font in
  a `fonts/` folder beside the configuration, or `--with` it.
- **A bundle made from a bundle** drops the old payload first, so rebundling
  is safe.
- **What is read at run time stays on the machine.** A bundle carries code
  and fonts, not state. The lock still reads your lule colours and
  wallpaper (it runs as you); the greeter reads the system's accounts and
  sessions and an optional `/etc/morf/caelestia-accent`. PAM stacks,
  greetd's config and anything in `/etc` are the machine's.
- **Rebuild after changing the parts or the library.** The bundle is a
  snapshot. Normal shell, lock and greeter installations load Lua from disk.
