# Native lock evidence

Morf's real Wayland lock loop publishes a separate native provider for a
privileged input broker such as Prek. It reports backend outcomes; it is not a
Lua IPC verb, an authentication method, or a way to request an unlock.

The provider takes compositor PID/UID from the connected Wayland socket's
`SO_PEERCRED` and records its `/proc/PID/stat` process birth. It never accepts a
PID or lock phase from Lua or an environment variable. The owning private
`XDG_RUNTIME_DIR` must have mode 0700, and its `morf/` directory must belong to
the same user without group/other writes. The socket has mode 0600 and a unique
`<pid>-<epoch>-lock-provider.sock` name. Public `morf list` skips these endpoints.

The persistent newline-delimited JSON protocol is version 1:

```json
{"version":1,"op":"state"}
```

The response contains exactly `version`, `role="lock"`, a random 32-digit hex
`epoch`, `revision`, `phase`, `compositor_pid`, `compositor_start` and
`compositor_uid`. Phases advance only in native code:

| Phase | Revision | Evidence |
| --- | --- | --- |
| `pending` | 1 | The native lock loop is preparing its request. This conservatively inhibits desktop actions. |
| `locked` | 2 | The Wayland backend delivered its compositor session-lock acknowledgement. |
| `unlock_dispatched` | 3 | The existing lock controller requested unlock, and the backend accepted/flushed that request. |

Only the owner and root may connect. Observers cannot write phases. Root alone
can acknowledge an exact completed epoch/revision:

```json
{"version":1,"op":"release","epoch":"0123456789abcdef0123456789abcdef","revision":3}
```

Release does not unlock the compositor. A broker must independently verify the
producer executable, process lifetime, active logind session, owning compositor
and actual compositor unlock before retiring its inhibition. Provider exit,
socket loss and an unlock request alone do not prove a safely unlocked desktop.
The existing configuration/authentication controller remains responsible for
deciding when Morf requests unlock.

After unlock, rendering, Wayland and ordinary lock IPC resources are dropped.
If an allowed observer sampled the provider, only the small native endpoint
remains for at most two seconds or until the exact root acknowledgement. A lock
with no observer exits without that wait. Dropping the provider wakes its poll
thread, closes peers, and removes only its own socket device/inode.

One background poll loop serves at most eight peers, with 1 KiB requests and a
two-second peer deadline. It sleeps without periodic wakeups when idle. Invalid,
oversized, pipelined or unauthorized requests close that peer; no connection can
block the lock's UI loop. Unknown versions/fields are refused.

Unit/private-socket tests cover phase/release constraints, process/runtime
binding, malformed/oversized input, capacity, partial input, replacement paths
and bounded teardown. Prek's optional local Cage `--morf /path/to/actual/morf`
test compares these reports with actual owned Hyprland lock/unlock state. Its
timer-authorized fixture does not exercise PAM, root broker registration or
physical authentication isolation. Those remain separate integration gates.
