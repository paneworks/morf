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

The endpoint also implements the bounded root controller protocol below. It
supplies no recognizer ownership, native lock completion, keyboard state,
rendered regions or arbitrary IPC bridge. Production broker/logind and physical
acceptance remain separate from private-socket/model and local Cage checks.

## Root controller protocol

`{"version":1,"op":"control-capabilities"}` is read-only for owner/root. It
returns `version`, connection `epoch`, exact decimal-string `ui_generation`,
`root_only=true`, `queue=8`, `receipts=64`, `lease_ms=1000`, `grant_ms=250` and
`recognizer_ownership=false`. A new Lua VM increments the UI generation and
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
  `request`, the typed controller object in [PREK_CONTROLS.md](PREK_CONTROLS.md).
  Its deadline must be within 250 ms and within the lease. IDs strictly increase
  within a fence; new gesture starts also require strictly increasing 32-hex
  gesture identifiers. The payload output must match this native endpoint.
  Native shell control submission additionally requires the acknowledged owned
  handoff below; ordinary same-user controller IPC remains separate.
- `receipt` adds only `id`. It reads an existing result and never executes work.
- `revoke` adds no fields. It invalidates authorization and rejects queued work.

Replies contain `version`, `epoch`, `ui_generation`, `status` and nullable
`error`; queued/completed receipts also contain `id`. Status is `accepted` for
lease/revoke, `queued` for admitted work, `dispatched` for UI controller
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
These contracts do not enable production Prek routing: verified rendered
context, acknowledged neutral handoff and actual root/logind acceptance remain
required. Positive grant tests currently model credentials around real pidfds;
private-socket and Cage tests exercise actual unprivileged refusal.

## Global phone-touch handoff

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

Caelestia cancels pending global panels/workspace previews and hides their edge
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
