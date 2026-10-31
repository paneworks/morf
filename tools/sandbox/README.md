# sandbox

Runs a shell under test -- morf with `examples/shells/impasto` or another
configuration, upstream impasto on Quickshell as a reference, or a kind a
shell brings of its own -- inside a nested Hyprland that is itself a client of a headless cage,
sealed off from the session of whoever runs it: private runtime dir and
Wayland/Hyprland sockets, a session bus that activates nothing, no system bus
(for the compositor and the shell alike; `inner.sh` refuses to run if it
sees one), a scratch HOME, and stub commands for anything that reaches the
machine (power, root, `pkill`, the network and audio tools). Stubbed calls
are logged in `stubbed.log`, never run.

    tools/sandbox/nested.sh morf NAME steps-file
    tools/sandbox/nested.sh upstream NAME steps-file    # needs UPSTREAM=<clone>
    tools/sandbox/nested.sh path/to/kind.sh NAME steps-file

A steps file is shell, sourced inside: `open PANEL`, `close`, `shot LABEL`,
`film LABEL N` (back-to-back frames with timestamps; `FILM_GEOMETRY="x,y WxH"`
crops them), `point X Y` / `click X Y [B]` / `scroll N` (the nested pointer),
`hc ARGS` (hyprctl on the nested instance only), `k ARGS` (wtype), `wait S`.
Output lands in `$WORK/out/<shell>-<NAME>/`. See the header of `nested.sh` for
the environment it reads.

`A11Y=1` puts an accessibility bus on the private session bus, switched
on as a screen reader would switch it on (the bus launcher keeps its
switches in memory: no dconf in here; its registry is started by hand: no
systemd), before the shell starts. The step `a11y LABEL` then writes what a
screen reader sees -- every application's tree over AT-SPI, through
libatspi (`a11ydump.py`) -- to `a11y-LABEL.txt`:

    A11Y=1 MORF_CONFIG=path/to/shell/init.lua \
      tools/sandbox/nested.sh morf a11y path/to/a11y.steps

`wlproxy.py` sits between cage and the nested Hyprland: cage 0.3 offers
`xdg_wm_base` v5 and Hyprland binds v6, which only adds a state a compositor
may never send, so the proxy advertises v6 and binds v5.

`vpointer.py` is the pointer: one `zwlr_virtual_pointer_v1` on the nested
compositor for the whole run, fed through a FIFO. A pointer made per command
(`wlrctl`) flips the seat's pointer capability each time, and a client
binding its `wl_pointer` on the flip misses the click that follows; a
cursor warp alone moves the cursor but enters no surface.

## A shell's own kind

A shell that is not a morf configuration -- a reference to compare against,
say -- brings a kind file, passed as the SHELL argument (a path, with a `/`
in it). `nested.sh` sources it, then calls `kind_setup` before the session
(with `H`, the scratch HOME, `OUT` and `KIND_DIR`, the file's own folder, set:
more stubs in `$H/shim`, seeded config) and puts what `kind_start` prints
into the inner script, where it starts the shell as `S` and defines `open`,
`close`, `ipc` and `notify` for the steps. Such a shell paints its own
wallpaper. Output lands in `$WORK/out/<file name>-<NAME>/`.

Not there in the sandbox: audio (no PipeWire), Bluetooth, UPower and power
profiles (no system bus), networks (nmcli is a stub), media players other
than `fakeplayer.py`, and unlocking (PAM from nix finds no modules; nothing
is typed).

## Watching it

`VISIBLE=1` opens the nested Hyprland as a window on your own compositor
instead of under a headless cage, so you can watch and use the session
under test. Only that window touches your session; the runtime dir, bus,
HOME and stubs stay private. `EXTRA_FONTS=DIR` adds a font folder to the
scratch HOME. A long `TIMEOUT` leaves the session to you once the steps are
done:

    VISIBLE=1 TIMEOUT=3600 tools/sandbox/nested.sh morf island tools/sandbox/impasto-island.steps

## headless

`headless.sh NAME STEPS` runs morf straight under a headless sway on the
GPU, with the same private runtime dir, bus and HOME but no nested
Hyprland and no stubs -- for configurations that do not reach the
machine, such as the frame-stall rehearsal. Steps have `ipc`, `sw`
(swaymsg on the private instance) and `wait`. On an NVIDIA-only machine,
where the nested Hyprland cannot allocate its buffers, `HOST_GL=1` gives
the Nix-built sway the host's NVIDIA EGL and GBM libraries, and `VULKAN`
names the wrapper morf runs under:

    HOST_GL=1 SWAY=$(nix build --print-out-paths nixpkgs#sway)/bin/sway \
      VULKAN=nixVulkanNvidia tools/sandbox/headless.sh stall tools/sandbox/frame-stall.steps

A headless sway output paces frame callbacks near 30 Hz on NVIDIA whatever
its mode says; compare runs with each other, not with a desk's refresh.
