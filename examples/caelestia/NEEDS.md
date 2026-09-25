# What the caelestia port needs from the engine

Found while porting phase 1 (the frame, the bar, the launcher and the
dashboard). Each entry says what is missing, the API that would cover it,
and why Lua cannot do it well. Work already under way elsewhere is marked
**in progress** and only left as a TODO in the port.

## 1. Hover that includes a node's descendants

**What.** The dashboard opens when the pointer reaches the top edge and
shuts when the pointer leaves it. "Leaves it" means leaves the panel *and
everything on it*. A `MouseArea`'s `hovered` is true only while it is the
topmost area under the pointer, so a full-panel area stops being hovered
the moment the pointer is over a tab, a calendar arrow or a media button.

**Workaround in the port.** `dashboard.lua` registers every `MouseArea` it
makes in a list and treats the panel as hovered while any of them is
(`panel_hovered()`), plus a 150 ms grace timer. That breaks as soon as
someone adds an area without going through the helper, and it cannot see
areas a library builds.

**API.** `contains_pointer` (read-only, like `hovered`) on any node: true
while the pointer is inside the node's box through its transforms,
regardless of what is on top -- Qt's `HoverHandler`/`containsMouse` with
propagation. Or a `MouseArea { hover_through = true }` whose `hovered`
ignores areas above it. The hit test already knows the answer; Lua can
only approximate it.

## 2. Variable font axes -- **in progress**

- **Optical size.** The reference sets its text in Google Sans Flex, whose
  `opsz` axis Qt follows from the point size: small labels come out wider
  and more loosely spaced. At the same size ours are about 15 % narrower
  (launcher descriptions: 330 px against 390 px). Needs automatic `opsz`
  (or `font_variations = { opsz = ... }`).
- **Weight on a variable face.** `font_weight = 500/600` on Google Sans
  Flex (one variable file) has no effect without `wght`.
- **Fill.** The reference's active tab icon and several dashboard icons are
  Material Symbols with `FILL 1`; ours are outlined.

TODO markers: none in code; the type falls back to the default instance.

## 3. Masks -- **in progress**

The reference clips the media cover art and the avatar to M3 shapes
(cookie, circle). The port draws the shape as a `ui.Path` placeholder and
uses a `ClipRect` circle for `~/.face`. Needs `mask = node` with a
`ui.Path` or `SdfShape` as the mask.

## 4. Exit animations -- **in progress**

A drawer that closes is kept visible until its close animation finishes,
then hidden in `on_finished` (`drawer.lua`). An `exit = { ... }` on the
node would make that declarative, and the same for launcher rows leaving
the list (they vanish at once; the reference fades and slides them).

## 5. Positional cubic Bézier easing

`docs/UI.md` writes a Bézier easing as `{ x1, y1, x2, y2 }`; the parser only
accepts the named-field table (`{ x1 = 0.05, y1 = 0.7, ... }`) and rejects a
positional list of four numbers with "easing x1 must be a finite number".
Either accept the array form (it is what every motion spec lists) or say
"named fields" in the docs. `theme.lua` uses named fields.

## 6. Runtime keyboard focus changes -- to verify

The launcher sets `morf.surface.keyboard_focus = "exclusive"` while it is
open and `"none"` otherwise, so the fullscreen frame never takes the
keyboard at rest. Tested headless (where keys reach the focused node
regardless); needs a check on a real compositor that the layer surface is
re-committed with the new interactivity.

## Engine fix made in this branch

- `morf render --surface screen` (and `test.snapshot(..., { surface =
  "screen" })`) composed surfaces in declaration order, so a background
  layer declared after the shell's own surface covered it. They are now
  stacked by layer-shell layer (background, bottom, top, overlay), keeping
  declaration order within a layer (`crates/morf-cli`).
