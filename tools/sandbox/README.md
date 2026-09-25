# sandbox

Runs a shell under test -- morf with `examples/impasto`, or upstream impasto
on Quickshell for comparison -- inside a nested Hyprland that is itself a
client of a headless cage, sealed off from the session of whoever runs it:
private runtime dir and Wayland/Hyprland sockets, a session bus that
activates nothing, no system bus, a scratch HOME, and stub commands for
anything that reaches the machine (power, root, `pkill`, the network and
audio tools). Stubbed calls are logged, never run.

    tools/sandbox/nested.sh morf NAME steps-file
    tools/sandbox/nested.sh upstream NAME steps-file   # needs UPSTREAM=<clone>

A steps file is shell, sourced inside: `open PANEL`, `close`, `shot LABEL`,
`film LABEL N` (back-to-back frames with timestamps), `hc ARGS` (hyprctl on
the nested instance only), `k ARGS` (wtype), `wait S`. Output lands in
`$WORK/out/<shell>-<NAME>/`. See the header of `nested.sh` for the
environment it reads.

`wlproxy.py` sits between cage and the nested Hyprland: cage 0.3 offers
`xdg_wm_base` v5 and Hyprland binds v6, which only adds a state a compositor
may never send, so the proxy advertises v6 and binds v5.
