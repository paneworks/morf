# sandbox

Runs a shell under test -- morf with `examples/impasto`, upstream impasto on
Quickshell, or caelestia-dots/shell on Quickshell, the last two as references
-- inside a nested Hyprland that is itself a client of a headless cage,
sealed off from the session of whoever runs it: private runtime dir and
Wayland/Hyprland sockets, a session bus that activates nothing, no system bus
(for the compositor and the shell alike; `inner.sh` refuses to run if it
sees one), a scratch HOME, and stub commands for anything that reaches the
machine (power, root, `pkill`, the network and audio tools). Stubbed calls
are logged in `stubbed.log`, never run.

    tools/sandbox/nested.sh morf NAME steps-file
    tools/sandbox/nested.sh upstream NAME steps-file    # needs UPSTREAM=<clone>
    tools/sandbox/nested.sh caelestia NAME steps-file   # needs CAELESTIA, CAELESTIA_PKG

A steps file is shell, sourced inside: `open PANEL`, `close`, `shot LABEL`,
`film LABEL N` (back-to-back frames with timestamps; `FILM_GEOMETRY="x,y WxH"`
crops them), `point X Y` / `click X Y [B]` / `scroll N` (the nested pointer),
`hc ARGS` (hyprctl on the nested instance only), `k ARGS` (wtype), `wait S`.
Output lands in `$WORK/out/<shell>-<NAME>/`. See the header of `nested.sh` for
the environment it reads.

`wlproxy.py` sits between cage and the nested Hyprland: cage 0.3 offers
`xdg_wm_base` v5 and Hyprland binds v6, which only adds a state a compositor
may never send, so the proxy advertises v6 and binds v5.

`vpointer.py` is the pointer: one `zwlr_virtual_pointer_v1` on the nested
compositor for the whole run, fed through a FIFO. A pointer made per command
(`wlrctl`) flips the seat's pointer capability each time, and a client
binding its `wl_pointer` on the flip misses the click that follows; a
cursor warp alone moves the cursor but enters no surface.

## caelestia

caelestia-dots/shell wants git Quickshell and a native QML plugin built
against the same Qt, plus libqalculate, aubio, cava, fftw, Qt ShaderTools,
qt6-imageformats (webp), the m3shapes QML module and its fonts. Arch's
Quickshell 0.3.1 and Qt 6.11 lack ShaderTools, imageformats' webp and the C
libraries, so everything comes from the clone's own flake instead, into the
nix store -- nothing system-wide:

    git clone https://github.com/caelestia-dots/shell caelestia
    nix build ./caelestia#caelestia-shell --out-link caelestia-pkg

That builds Quickshell from its git flake input (with `qtimageformats` and
m3shapes), the plugin and the helper library against nixpkgs' Qt, and a
launcher. The launcher is never run: it puts the real `ddcutil`,
`brightnessctl`, `nmcli`, ... first on PATH. `caelestia-env.sh` reads its
environment back out of the two nix wrappers (QML import path for the plugin
and m3shapes via `NIXPKGS_QT6_QML_IMPORT_PATH`, Qt plugin path, fonts.conf
with Material Symbols, Rubik and CaskaydiaCove, `CAELESTIA_LIB_DIR`, xkb
rules), drops PATH, and the sandbox starts Quickshell's own `qs` under it,
under `nixGLIntel` (`NIXGL`) for nix's GL driver, with `-p` the clone.

The scratch HOME gets `~/.config/caelestia/shell.json` (`CAEL_CONFIG`, else
one naming a fixed weather place so it never geolocates the machine),
optionally `~/.local/state/caelestia/scheme.json` (`CAEL_SCHEME`; without
it the built-in palette), `~/Pictures/Wallpapers`, and
`~/.local/state/caelestia/wallpaper/path.txt` (`WALLPAPER`, else its own
`assets/wallpaper.webp`). caelestia paints its own wallpaper; no `awww`.

- `open NAME` dispatches `hl.dsp.global("caelestia:NAME")` -- dashboard,
  sidebar, utilities, session, showall, lock, nexus, ... -- except launcher,
  which toggles on the key's release (a dispatched global only presses) and
  so goes through its IPC. `close [D]` toggles drawer D, or every open one,
  over IPC (the dashboard does not close on Escape).
- `ipc ARGS` is `qs ipc call ARGS`: `ipc drawers toggle osd`,
  `ipc toaster info T M ICON`, `ipc nexus open`.
- `notify ARGS` is a real `notify-send`: the shell is the notification
  server on the private bus.
- Stubbed besides the common set: its `caelestia` CLI (which still records
  `wallpaper -f P`), swappy, gpu-screen-recorder, nvidia-smi, lspci,
  glxinfo, warp-cli, tailscale, netbird, wg-quick, ping, asdbctl,
  fprintd-list (answers "nothing enrolled"), logout, hibernate, suspend,
  app2unit.

`caelestia-gallery.steps` shoots rest, launcher (apps, search, actions,
wallpapers), each dashboard tab, sidebar, utilities, session, osd, the bar's
popouts, notifications, a toast, showall, nexus and the lock screen;
`caelestia-films.steps` films drawers opening and closing, cropped to each.

Not there in the sandbox: audio (no PipeWire, so the osd and media show
nothing), Bluetooth, UPower and power profiles (no system bus), networks
(nmcli is a stub), media players, and unlocking (PAM from nix finds no
modules; nothing is typed). The weather comes from the network.
