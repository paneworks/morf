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

## 2. Optical size that changes advances

The reference sets its text in Google Sans Flex, and Qt follows the face's
`opsz` axis from the point size: small labels come out wider and more
loosely spaced. At the same size ours are about 15 % narrower (a launcher
description measures 330 px against the reference's 390 px). Variable axes
now exist (`axes = { ... }`), and the port uses them for weights (`wght`)
and filled icons (`FILL`), but an axis other than `wght` is applied where
glyphs are drawn and "keeps the default advances", so `opsz` cannot give
the wider setting. Needs `opsz` (ideally automatic from `font_size`, as
CSS's `font-optical-sizing: auto` and Qt do) to reach shaping like `wght`.

## 3. Positional cubic Bézier easing

`docs/UI.md` writes a Bézier easing as `{ x1, y1, x2, y2 }`; the parser only
accepts the named-field table (`{ x1 = 0.05, y1 = 0.7, ... }`) and rejects a
positional list of four numbers with "easing x1 must be a finite number".
Either accept the array form (it is what every motion spec lists) or say
"named fields" in the docs. `theme.lua` uses named fields.

## 4. Opacity of one layer in a field

The reference fades a drawer's background in with its contents as it
opens. The background here is a layer of the frame's field (so it can
fillet into the frame), and a layer has no opacity of its own that blends
with the rest of the field; fading the whole `Sdf` would fade the frame.
The port fades only the contents. Needs `opacity` on `SdfShape` (the
layer's coverage scaled before it is composed with the others).

## Engine fixes made in this branch

- Headless runs (`morf check`/`render`/`test`) stretched a layer anchored at
  both ends of an axis to the screen even when it asked for a size; a
  compositor keeps the asked size, centred. The port's wallpaper layer,
  missing `height = 0`, was whole headless and a 32 px band on Hyprland.
  Headless now does what layer shell does (`crates/morf-cli`).
- `morf render --surface screen` (and `test.snapshot(..., { surface =
  "screen" })`) composed surfaces in declaration order, so a background
  layer declared after the shell's own surface covered it. They are now
  stacked by layer-shell layer (background, bottom, top, overlay), keeping
  declaration order within a layer (`crates/morf-cli`).
