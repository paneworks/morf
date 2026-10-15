# Native shell connection provider

Each real named-output shell worker exposes one private native endpoint:
`$XDG_RUNTIME_DIR/morf/<pid>-<32-lowercase-hex-epoch>-shell-provider.sock`.
This is distinct from ordinary shell controller IPC. Headless runners,
outputless workers and the multi-output desktop canvas do not fabricate a
native output binding.

The actual LayerClient supplies its connected Wayland socket and own output
name. Native code reads SO_PEERCRED and the compositor's process birth. Neither
an environment-selected PID nor a Lua callback supplies these identities.
The endpoint begins at `pending`/revision 1. Initial host startup, or an actual
primary configure event after backend replacement, advances it to
`configured`/revision 2. Lua cannot advance the phase. Configuration is a
native configure witness; it does not prove compositor presentation or actual
rendered widget regions.

Only this newline-delimited JSON request is accepted:

```json
{"version":1,"op":"state"}
```

The response has exactly these fields:

```json
{"version":1,"role":"shell","epoch":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","revision":2,"phase":"configured","compositor_pid":123,"compositor_start":456,"compositor_uid":1000,"output":"DP-1"}
```

UIDs, PIDs and process birth come from the native socket/process. Output comes
from the native client. Epoch is 128 random bits generated for this endpoint.
Backend replacement retires the old endpoint before replacing its connection,
then creates a new epoch initially pending. Ordinary Lua configuration reload on
the same backend does not itself replace this connection epoch. Orderly output
shutdown removes the endpoint before the backend drops. Cleanup matches its
socket inode and device and preserves replacement paths.

The runtime must be owned by the process user, a real directory and private
(0700). Its Morf registry permits no group/other writes. Endpoint mode is 0600.
Only the owner or root can query it. The bounded transport is shared with native
lock evidence: one background poll thread, eight peers, 1 KiB requests and two
seconds of idle/partial-request time. Complete valid requests refresh that
peer's expiry; partial trickles do not. Idle providers have no periodic wakeups.
Malformed, pipelined, unknown-field, future-version or mutating requests close
the peer. The UI thread never waits for socket clients. Failure to publish
optional shell evidence logs a warning without disabling ordinary Morf.

Prek additionally verifies the expected executable and process lifetime, private
endpoint/directory, active logind membership and bound compositor before
including the state in its fresh owning desktop view. It rejects duplicate
output providers and missing/mismatched outputs. The public
`prek.morf.providers()` API copies these verified connection states with its
session fence and exact decimal-string process birth.

This endpoint supplies no controller grant, recognizer ownership, native lock
completion, keyboard state, rendered regions or arbitrary IPC bridge. Those
require their own native contracts. Production broker/logind and physical
acceptance remain separate from private-socket/model and local Cage checks.
