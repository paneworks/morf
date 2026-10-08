# Morf keyring prompter

The Caelestia keyring dialog uses GCR's `GcrSystemPrompter` and `GcrPrompt`
interfaces. GCR implements the encrypted secret exchange with GNOME Keyring.
This is independent of polkit and does not modify PAM or stored keyrings.

The Nix `morf` package includes this bridge in its runtime closure. The existing
`morf-x86_64-linux` and `morf-aarch64-linux` Cachix pins therefore distribute it
alongside the engine; no separate pin or local compilation is needed when the
release is cached. `programs.morf.enable` keeps GNOME Keyring as secret storage,
unlocks it through greetd/lock PAM, and adds sudo progress markers. Morf supplies
the keyring and Polkit dialogs; do not start a second Polkit agent in that session.

Build with `sh tools/keyring/build.sh`. Native development dependencies are
GCR 4, GLib/GIO (including gio-unix), json-glib and a C compiler. The result
is `target/dist/morf-keyring`; install it on PATH, for example in
`~/.local/bin/morf-keyring`. `MORF_KEYRING_HELPER` can name an explicit path.
The runtime needs the corresponding shared libraries, not GTK or Python.

Caelestia starts one bridge on its primary output. Request metadata is routed
to the monitor focused when the request arrives, like polkit. The dialog stays
there until it finishes, so changing focus cannot duplicate it or move typed
input. Answers travel directly to the bridge through a private Unix socket;
secrets do not cross morf's multioutput IPC. `morf ipc call keyring`
reports registration and whether a prompt is open, never credentials.
Both Material and Tsugumori use the same controls with their theme components.
Unlock, new-password confirmation, warnings, choice checkboxes, confirmation
prompts and cancellation are supported.

Try the dialog on the running shell with `morf ipc call keyring demo`. Enter a
dummy password such as `test`; this preview performs no authentication and does
not touch stored keyrings. `keyring demo new` previews matching password fields,
and `keyring demo confirm` previews a confirmation. A demo cannot replace an
active authentication request; incoming real keyring requests take priority.

The bridge queues for the SystemPrompter and PrivatePrompter D-Bus names.
It does not replace an existing prompt or kill GNOME's prompter. Once the
existing prompter exits, morf takes over. On shell shutdown/reload, the
private input pipe closes and the bridge exits, so the system's original
D-Bus activation service is available as a fallback. No service files change.

Passwords only travel through the UI's local input buffers and the bridge's
private stdin pipe or answer socket. The socket has mode 0600 inside a random
0700 directory in the user's runtime directory and checks the peer UID.
Connections have a five-second deadline and bounded input; stale request IDs
cannot answer a newer prompt. Shutdown removes the socket and its directory.
Passwords are excluded from signals, public shell IPC, argv, environment,
files and diagnostic output. The bridge disables core dumps. It clears its
retained password after GCR finishes with it; GCR owns D-Bus encryption.

Validation (private session bus; no real keyring or PAM attempts):

```
sh tools/keyring/build.sh
dbus-run-session -- /usr/bin/python3 tools/keyring/test_prompter.py
TEST_GCR_VERSION=3 dbus-run-session -- /usr/bin/python3 tools/keyring/test_prompter.py
TEST_KEYRING_SOCKET=1 dbus-run-session -- /usr/bin/python3 tools/keyring/test_prompter.py
morf test --no-dbus examples/shells/caelestia/tests/keyring_spec.lua
morf test --no-dbus examples/shells/caelestia/tests/auth_monitor_spec.lua
```

Protocol test clients need Python GObject introspection and GCR typelibs;
they are not dependencies of the running bridge.
