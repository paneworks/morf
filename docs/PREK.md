# Prek

What morf tells prek (the sibling `paneworks/prek`), the privileged input
broker, and what it takes from it: the native providers in
`crates/frontend/morf-host/src/provider/` -- the shell provider and its
rendered context, the session-lock evidence, and the socket transport they
share -- and the controller verbs a shell answers.

## The shell provider

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

The read-only connection query is newline-delimited JSON:

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
lock evidence: one background poll thread, eight peers, 4 KiB shell requests and two
seconds of idle/partial-request time. Complete valid requests refresh that
peer's expiry; partial trickles do not. Idle providers have no periodic wakeups.
Malformed, pipelined, unknown-field and future-version requests close the peer.
Mutations require native root credentials as described below. Native lock
requests retain their existing 1 KiB bound. The UI thread never waits for socket clients. Failure to publish
optional shell evidence logs a warning without disabling ordinary Morf.

Prek additionally verifies the expected executable and process lifetime, private
endpoint/directory, active logind membership and bound compositor before
including the state in its fresh owning desktop view. It rejects duplicate
output providers and missing/mismatched outputs. The public
`prek.morf.providers()` API copies these verified connection states with its
session fence and exact decimal-string process birth.

The endpoint also implements submitted-frame inspection and the bounded root
controller protocol below. Its connection state alone supplies no recognizer
ownership, native lock completion, keyboard state or arbitrary IPC bridge. Production broker/logind and physical
acceptance remain separate from private-socket/model and local Cage checks.

### Submitted primary frame

`{"version":1,"op":"rendered-context"}` is read-only for owner/root when
`control-capabilities.rendered_context=true`. It returns the connection `epoch`,
decimal-string `ui_generation`, `revision` and monotonic `sampled_ns`,
`coverage="primary_surface_named_bounds"` and `frame`. The frame is null when
unavailable, busy, stale or lacking a matching GPU submission. Reads wake the
UI for a new observation but never refresh the timestamp themselves.

The frame supplies Morf layout `width`, `height`, `scale_120` and up to 256
named `regions` with decimal-string native `node`, scene `id`, clipped
`bounds={x,y,width,height}` and `interactive`. Names may repeat; node identities
are distinct within the endpoint epoch and Lua generation. Hidden, transparent
and exiting subtrees are excluded. These are transformed bounding boxes, not
exact hit shapes or compositor presentation. The entire reply is at most
128 KiB; excessive metadata withdraws the frame instead of truncating it.

With `semantic_regions=true`, `input_context={role,layout?}` on an actual named
node supplies optional `semantic` metadata. Roles are `keyboard`,
`secure_overlay`, `clock_background`, `content_bottom`, `physical_bottom`,
`top_left`, `top_right`, `side_left`, `side_right` and `local_control`. A keyboard
requires its layout name; other roles forbid one. No coordinates, credentials
or ownership claims can be attached through this annotation.

With `layer_space=true`, a configured primary Wayland layer additionally
supplies `surface={namespace,width,height,local_scale}`. The namespace is fixed
at layer creation; the dimensions are the native logical viewport. Multiplying
Morf local coordinates by `local_scale` applies the actual density conversion.
Fallback/window backends omit this field. A compositor layout match can map
these bounds to a global layout box, but cannot prove animated position,
stacking, surface input coverage or presentation.

With `keyboard_registry=true`, `keyboard` supplies
`{scope:"registered_morf_primary",registered,state,node?,layout?}`. The registry
walks every scene root and accepts a known `hidden` or `visible` state only for
one named keyboard under the primary root. Known states include its node and
layout. Other-root declarations, multiple/no declarations, exiting nodes or
incomplete visible bounds yield `unknown`. Changes outside the primary root
also invalidate the registry cache. This coverage cannot establish the absence
of another program's keyboard or grant input authority.

### Root controller protocol

`{"version":1,"op":"control-capabilities"}` is read-only for owner/root. It
returns `version`, connection `epoch`, exact decimal-string `ui_generation`,
`root_only=true`, `queue=8`, `receipts=64`, `lease_ms=1000`, `grant_ms=250` and
`recognizer_ownership=false`, with `cancel_gesture=true` for gesture-scoped
cancellation. An absent cancellation field on an older provider means unsupported.
A new Lua VM increments the UI generation and
retires queued work and authorization tokens without changing connection epoch.

All mutations require SO_PEERCRED UID 0; JSON never supplies credentials. The
first issuer is pinned by PID, process birth and pidfd. A different issuer may
take over only after that process dies. Each request contains `version=1`,
`op`, exact `epoch`, `ui_generation` and a fence object with exactly `broker`
(two u64 words, not both zero), positive `configuration` and positive `session`.
Same-issuer fence changes require the same broker words, nondecreasing
generations and at least one advancing generation. An explicit revocation
requires a newer fence before that issuer can grant again.

- `lease` adds `expires_ns`, a canonical decimal CLOCK_MONOTONIC nanosecond
  string strictly in the future and at most one second away. Renewal after
  expiry rotates the private UI token, preventing old gestures from reviving.
- `submit` adds canonical positive decimal-string `id`, `deadline_ns` and
  `request`, the typed controller object in [the controller receiver](#the-shell-controller-receiver).
  Its deadline must be within 250 ms and within the lease. IDs strictly increase
  within a fence; new gesture starts also require strictly increasing 32-hex
  gesture identifiers. The payload output must match this native endpoint.
  Native shell control submission additionally requires the acknowledged owned
  handoff below; ordinary same-user controller IPC remains separate.
- `receipt` adds only `id`. It reads an existing result and never executes work.
- `revoke` adds no fields. It invalidates authorization and rejects queued work.
- `cancel-gesture` adds `gesture_epoch` and `gesture`, each exactly 32 lowercase
  hex characters. It retires only that gesture under the already authenticated
  issuer/fence, removes its queued jobs and invalidates its private UI guard.
  Other gestures and the owning lease/handoff remain valid. A retired identity
  cannot be submitted again with a different request ID. At most 4096 retired
  gestures are retained per fence; completed request metadata follows the
  existing 64-receipt bound. A newer fence clears the old histories.

Replies contain `version`, `epoch`, `ui_generation`, `status` and nullable
`error`; queued/completed receipts also contain `id`. Status is `accepted` for
lease/revoke/cancellation, `queued` for admitted work, `dispatched` for UI controller
acceptance, or `rejected`. `dispatched` does not prove animation or compositor
completion. Missing/evicted receipts return `retired`; no timeout authorizes a
mutating retry. Errors include `ui_replaced`, `invalid_deadline`, `owner_alive`,
`stale_fence`, `retired`, `duplicate`, `expired_lease`, `invalid_request`,
`capacity`, `duplicate_gesture`, `expired` and `controller_refused`.

The socket thread only queues primitive data and wakes the owning UI thread.
The UI consumes at most one item per turn, checks its deadline/lease/issuer,
and calls the fixed internal controller. Native authorization is checked again
inside Lua and immediately before a delayed workspace dispatch. Revocation,
issuer death, expiry and local authentication cancel active/settling native
previews; a 25 ms check exists only while native progress is active or settling.
Ordinary IPC callers cannot manufacture the random native authorization token.
Gesture-scoped effect guards combine that random scope token with the payload's
epoch and gesture ID. Native authorization checks the whole guard; cancellation
of one key does not revoke the scope. The shell compares only the scope prefix
when detecting fence replacement, so a different valid gesture does not itself
preempt a running controller. A cancellation acknowledgement establishes native
permission retirement, not rendered cleanup or undoing a call already in flight.
Late completions cannot overwrite a cancelled request's retired receipt.
These contracts do not enable production Prek routing: verified rendered
context, acknowledged neutral handoff and actual root/logind acceptance remain
required. Positive grant tests currently model credentials around real pidfds;
private-socket and Cage tests exercise actual unprivileged refusal.

### Global phone-touch handoff

The read-only request `{"version":1,"op":"handoff-state"}` returns `version`,
connection `epoch`, decimal `ui_generation`, and a `handoff` object. Its fields
are `phase`, decimal `ownership_generation`, `global_enabled`, `native_neutral`,
`contacts`, decimal `contact_revision`, `ui_ack`, `driver`, decimal monotonic
`observed_ns`, `scope="global_phone_touch"` and `recognizer_ownership=false`.
Reading also wakes the owning UI loop for a fresh native contact observation.
The contact tracker includes every native Wayland down/up/cancel before hit
testing, including contacts on empty space, and survives a Lua reload. It is
bounded to 64 contacts; inconsistent input prevents a neutral acknowledgement.
This is the client's native contact boundary, not a physical evdev snapshot.

Root handoff mutations use the same five-field common envelope and live lease:

- `handoff-quiesce` adds positive decimal `ownership_generation`, strictly newer
  than the last handoff. It stops duplicated global phone gestures and persists
  inhibition. Accepted means the request began; wait for `phase="quiesced"`,
  `ui_ack=true` and a fresh native-neutral contact observation.
- `handoff-activate` adds the same ownership generation. It requires that
  quiesced state and a contact observation less than 250 ms old, then reports
  `owned`. The broker must separately prove device readiness and its physical
  neutral/grab boundary before enabling recognition.
- `handoff-release` adds ownership generation and `descriptors_released=true`.
  This is root's assertion that recognition stopped, buffered/held input was
  drained and every owned evdev/uinput descriptor was closed. The receiver
  cannot inspect those descriptors. It retires queued effects, prepares the UI,
  removes inhibition in the native background thread, and resumes globals only
  when the UI loop again observes a neutral boundary. Poll until `native`.

Other phases are `native`, `quiescing`, `releasing` and `lost`. Expiry, root
issuer loss, revoked authority, Lua VM replacement or an unavailable UI receiver
cannot reactivate globals; they retain inhibition. A private per-output marker
under `$XDG_RUNTIME_DIR/morf/handoff/` survives worker/backend restart. Output
names use exact hex directory chunks. Cleanup checks marker device/inode and
preserves replacements. Marker persistence/removal runs outside the UI mutex;
native background work wakes the UI for final acknowledgement. No idle timer
or socket loss enables recognizers. Reload candidates inherit live native
policies before executing configuration.
Queued effects and ownership are retired before a reload candidate executes,
including a failed reload; successful VM replacement also advances UI generation.

eva cancels pending global panels/workspace previews and hides their edge
regions while quiesced. Local panel navigation, keyboard keys and keyboard-local
layout/hide gestures retain their handlers. An unmanaged external `lisgd`
driver cannot acknowledge quiescence; stopping that process and confirming its
descriptor release still requires integration. Root failures include
`handoff_required`, `handoff_unavailable`, `stale_ownership`, `busy`,
`inhibition_failed`, `not_neutral` and `release_unconfirmed`.

The receiver and typed broker transport implement this protocol; production
MorfActions negotiation remains disabled. Native/UI tests and modeled root
credentials do not establish actual root/logind acceptance, physical source
handoff, rendered context or external lisgd shutdown.

## The submitted-frame context

The private native `*-shell-provider.sock` endpoint advertises
`rendered_context=true` in `control-capabilities`. A read-only request is:

```json
{"version":1,"op":"rendered-context"}
```

The response contains `version=1`, the native connection `epoch`, decimal-string
`ui_generation`, `revision` and `sampled_ns`,
`coverage="primary_surface_named_bounds"`, and `frame` (or null).

An available frame contains logical surface `width`, `height`, `scale_120`
and up to 256 `regions`. Each region contains its existing scene name `id`,
an opaque decimal-string generational `node` identity,
`bounds={x,y,width,height}` in surface-local logical coordinates and
`interactive`. Bounds follow ancestor transforms and clipping; hidden,
transparent, masked and exiting subtrees are excluded. Interactivity identifies
an enabled MouseArea, DropArea, TextInput or Terminal itself. Scene names may
repeat (launcher rows, for example); every matching native node remains in the
array under its own identity. Node identities are meaningful only within the
endpoint epoch and UI generation. Names contain no controls and fit 128 bytes.
Invalid or excessive reports are unavailable, without truncation.

The native GPU path records the exact scene paint stamp it submitted. A skipped
frame or layout-only host records no witness. The UI thread observes bounds only
when the current scene, surface size/scale and runtime still match that witness.
Unchanged submitted geometry can be observed again without repainting. An idle
shell with no reader performs no geometry traversal. A query wakes the UI thread
for a fresh observation, while returning only the preceding observation; the
query never timestamps itself. Reports expire after 250 ms, and are unavailable
while the UI thread is mutating state. Backend replacement removes the endpoint;
VM replacement changes the UI generation and cannot reuse old geometry.

This is inspection of a submitted primary scene. Bounds of curved or rotated
items are bounding boxes, not exact hit shapes. They do not acknowledge
compositor presentation, surface stacking or input masks. Auxiliary layers are
outside this coverage. Missing named bounds cannot establish keyboard absence
on the output, authentication or a safe gesture scope. The existing native
ownership/controller protocol remains separate.

Prek verifies the native process/session/compositor binding around its bounded
read and exposes copied fresh reports through `prek.morf.regions()`. This does
not enable production Morf action negotiation or physical input takeover.

When `control-capabilities.semantic_regions` is true, each exported region can
also contain `semantic={role,layout?}`. A configuration declares this using
`input_context` on the actual named scene node. Allowed roles are `keyboard`,
`secure_overlay`, `clock_background`, `content_bottom`, `physical_bottom`,
`top_left`, `top_right`, `side_left`, `side_right` and `local_control`. Keyboard
requires a nonempty layout name of at most 128 bytes; other roles cannot set it.
Unknown fields, caller coordinates and authentication/ownership claims are
rejected. Annotation changes affect the scene paint stamp and must pass through
the same GPU witness before they become observable.

eva annotates its real keyboard panel with its current layout, its ordinary
and authentication drawers, and six separate noninteractive Items for shell
geometry. These Items remain laid out when native global recognizers are
inhibited. They do not take input or affect the existing controller behavior.
Only visible primary-frame nodes are described; missing keyboard annotations
cannot establish hidden state or complete output coverage. The broker retains
unknown keyboard knowledge until a complete verified mapping/coverage provider
can supply it. No semantic annotation changes native lock or handoff authority.

## The shell controller receiver

[eva](https://github.com/bresilla/eva) exposes ordinary same-user IPC verbs `prek-control` and `prek-status`.
They reuse its existing drawers, menus, keyboard and workspace preview. They do
not authenticate a Prek broker, prove rendered geometry, or transfer ownership
from the native global recognizer. The status response explicitly reports
`recognizer_ownership=false`. Native binding, broker grants and acknowledged
neutral handoff must be implemented and accepted together before enabling exclusive Prek routing.
The existing native/lisgd driver choices are unchanged.

The separate [native shell provider](#the-shell-provider) supplies real
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
[native protocol](#root-controller-protocol) for limits,
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

## Lock evidence

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

