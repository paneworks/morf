# What the caelestia port needs from the engine

Found while porting phase 1 (the frame, the bar, the launcher and the
dashboard). Each entry says what is missing, the API that would cover it,
and why Lua cannot do it well. Work already under way elsewhere is marked
**in progress** and only left as a TODO in the port.

## 1. Optical size that changes advances

The reference sets its text in Google Sans Flex, and Qt follows the face's
`opsz` axis from the point size: small labels come out wider and more
loosely spaced. At the same size ours are about 15 % narrower (a launcher
description measures 330 px against the reference's 390 px). Variable axes
now exist (`axes = { ... }`), and the port uses them for weights (`wght`)
and filled icons (`FILL`), but an axis other than `wght` is applied where
glyphs are drawn and "keeps the default advances", so `opsz` cannot give
the wider setting. Needs `opsz` (ideally automatic from `font_size`, as
CSS's `font-optical-sizing: auto` and Qt do) to reach shaping like `wght`.

## Engine fixes made in this branch

- A node's hover including its descendants: every node has a read-only
  `contains_pointer`, true while the pointer is inside its box whatever is
  on top (docs/UI.md, "Hover and press"). The dashboard shuts on
  `panel.contains_pointer` going false instead of a hand-kept list of its
  areas.
- Opacity of one layer in a field: `opacity` on an `SdfShape` fades that
  layer, seam and all, without fading the rest of the field (docs/UI.md,
  "Fields"). A drawer's background, a layer of the frame's field, now fades
  in with its contents as the reference's does.
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
