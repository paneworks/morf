# Prek shell controller receiver

[eva](https://github.com/bresilla/eva) exposes ordinary same-user IPC verbs `prek-control` and `prek-status`.
They reuse its existing drawers, menus, keyboard and workspace preview. They do
not authenticate a Prek broker, prove rendered geometry, or transfer ownership
from the native global recognizer. The status response explicitly reports
`recognizer_ownership=false`. Native binding, broker grants and acknowledged
neutral handoff must be implemented and accepted together before enabling exclusive Prek routing.
The existing native/lisgd driver choices are unchanged.

The separate [native shell provider](NATIVE_SHELL_PROVIDER.md) supplies real
Wayland connection/configure identity to a verifying broker. It does not turn
these ordinary controller IPC verbs into authenticated broker grants.

`prek-control` takes one JSON string argument:

```json
{"version":1,"output":"DP-1","epoch":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","gesture":"00000000000000000000000000000001","ordinal":1,"action":"dashboard","phase":"begin","dx":0,"dy":-80,"vx":0,"vy":0}
```

All fields are required; unknown fields and payloads over 2 KiB are refused.
`epoch` and `gesture` are exactly 32 lowercase hexadecimal characters. They are
deduplication identifiers, not credentials. `ordinal` is an integer from 1 to
2,147,483,647. `output` names the frozen output, with at most 256 bytes and no
control characters; a different output returns no result and does nothing.
Selection never follows a later focus change. Motion values must be finite and
at most 1,000,000 in absolute value. Displacements are cumulative surface logical
coordinates since begin, velocities surface logical units per second. The
adapter must convert compositor coordinates through Morf's actual density.

Discrete actions are `dashboard`, `notifications`, `quick_settings`, `apps`,
`web`, `keyboard_show`, `keyboard_hide` and `keyboard_toggle_layout`. Opening a
panel closes other ordinary panels, preserving the keyboard. Keyboard show
selects `full`; toggle switches `full`/`dev` only while visible and resets local
key ownership through the existing keyboard controller. Password/form reveal,
authentication, arbitrary IPC verbs and synthetic injection are not operations
of this receiver.

Continuous actions are `dashboard`, `notifications`, `quick_settings` and
`workspace_preview`. A gesture begins at ordinal 1, then accepts only the next
ordinal with the same epoch, gesture and action. Phases are `begin`, `update`,
`end` and `cancel`; discrete actions use `discrete`, ordinal 1. Drawers follow
their existing horizontal or vertical axis. Workspace progress drives the same
preview used by native gestures, preserving its partial return, reversal,
snapshots and release-time compositor switch. Acceptance means the controller
accepted the request, not that an asynchronous animation completed.

One continuous request may be active per output. A 500 ms inactivity timer and
a two-second hard lifetime cancel abandoned progress to its original state.
Authentication and capture inhibition also cancel progress without committing a
workspace switch. Duplicate, late and out-of-order requests cannot restart or
repeat an effect. Failed controller calls retain their deduplication identity.
The current receiver retains at most 4,096 starting identities per Lua runtime,
then refuses new starts with `capacity`; it never evicts identities and silently
replays old requests. Native generation retirement is still required for an
unbounded production session. No transport timeout permits retrying a mutation.

Replies contain `version`, `output`, `epoch`, `gesture`, `ordinal`, `status`
(`accepted` or `rejected`) and, for refusals, `error`: `invalid_request`,
`blocked`, `duplicate`, `busy`, `capacity`, `unsupported_phase`, `unavailable`,
`retired`, `out_of_order` or `controller_failed`. Malformed requests cannot
provide valid correlation fields. A caller must match all correlation fields
before using a response. Ordinary Morf IPC carries a returned table as a JSON
string in its scalar `result` array.

The native endpoint can deliver the same typed payload to the fixed internal
`prek-native-control` handler with a random, live native authorization token.
Calling this handler through ordinary IPC without that token returns
`unauthorized`. Native IDs are deduplicated by the bounded native protocol,
separately from the ordinary 4,096-entry ledger. Native expiry/revocation and
token replacement cancel progress; workspace completion checks authority again
after its animation, immediately before sending a command. Authentication also
cancels previews that are already settling after an `end` request. See the
[native protocol](NATIVE_SHELL_PROVIDER.md#root-controller-protocol) for limits,
generation retirement and the remaining handoff requirements.

`prek-status OUTPUT` returns ordinary controller state for that exact output:
version, output, ownership=false, blocked, retained identity count, active
progress, panel visibility/selection, keyboard visibility/layout and workspace
preview activity. This is Lua controller state, not native visibility, native
lock evidence or a broker-authorized context snapshot.

Shell and lock IPC calls request a repaint when visible scene properties or
primary surface settings actually change. Reading status and changing hidden
or nonvisual state do not force a frame. A handler that fails after a visible
write still requests repaint; this does not roll back or retry its effects.
Normal animation, service and compositor-driven frames remain independent.

The local Cage helper records ordinary IPC durations in `rpc_timings`, bounded
to 256 samples, with the total call count recorded separately. Each entry names
the verb, records `reply` or `error`, and gives `elapsed_ns`. These are client
round-trip timings, not render completion or action grants. A failed trial
writes `morf-controls-failure.json`, preserving controller refusals, timings and
process status before and after cleanup. Inactivity and hard gesture limits
remain enforced; a slow or retired request is not replayed.

Run the real controller/layout tests for both themes:

```sh
morf test --no-dbus tests/prek_controls_spec.lua   # in eva
```

Prek's optional local Cage `--morf-shell` acceptance also starts the real shell
on its owned nested compositor, checks the actual compositor layer, and drives
these IPC verbs. Use an optimized Morf binary for the full rendered shell; a
debug build can miss the existing IPC deadline while laying out large panels.
Neither check establishes root/logind authentication, recognizer handoff or
physical input delivery through Prek.
