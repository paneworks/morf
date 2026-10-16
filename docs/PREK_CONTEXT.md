# Native submitted-frame context

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

Caelestia annotates its real keyboard panel with its current layout, its ordinary
and authentication drawers, and six separate noninteractive Items for shell
geometry. These Items remain laid out when native global recognizers are
inhibited. They do not take input or affect the existing controller behavior.
Only visible primary-frame nodes are described; missing keyboard annotations
cannot establish hidden state or complete output coverage. The broker retains
unknown keyboard knowledge until a complete verified mapping/coverage provider
can supply it. No semantic annotation changes native lock or handoff authority.
