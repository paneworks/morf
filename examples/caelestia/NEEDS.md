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

## 2. Opacity of one layer in a field

The reference fades a drawer's background in with its contents as it
opens. The background here is a layer of the frame's field (so it can
fillet into the frame), and a layer has no opacity of its own that blends
with the rest of the field; fading the whole `Sdf` would fade the frame.
The port fades only the contents. Needs `opacity` on `SdfShape` (the
layer's coverage scaled before it is composed with the others).

Phase 2 (the other dashboard tabs, the launcher's pickers, the session
menu, the bar's popouts, notifications and the OSD) adds these. Item 1
bites again there: the bar's popouts keep the same list of areas
(`popouts.area`), as the dashboard does.

## 3. A module's instruction budget, stated and adjustable

**What.** Loading a module (each `require`) runs under a fixed budget
(`Limits::effect_fuel`, 1 000 000 instructions), and when it runs out the
whole configuration fails to load: "Lua module fuel exhausted after
1000000 instructions". The dashboard, once it built four tabs in one
module, ran out -- most of it `lib/m3shapes` resampling a dozen outlines
to 72 cubics each for the Media tab's backdrop. Nothing in docs/ mentions
the limit, so the first a configuration hears of it is a shell that will
not start.

**Workaround in the port.** Each tab is its own module, built as it loads
(each `require` gets a fresh budget), and `m3shapes.path(name, {
segments = false })` skips the resampling for shapes that never morph.

**API.** Document the budget in UI.md. Better: a
per-configuration setting (`morf.limits { module_fuel = ... }` read
before the rest loads) or a much larger allowance for the top-level load
than for handlers, which is where a runaway loop matters.

## 4. Bindings written after construction

**What.** `node.opacity = function() ... end` after a node is built fails
the load ("scene properties do not support function values"); only a
constructor takes a binding. The popouts wanted to add a cross-fade to
pages built elsewhere.

**Workaround.** Wrap the page in an `Item` that carries the binding.
Fine here, but it adds a node per page and a component cannot be given a
binding by its user after the fact.

**API.** Accept a function wherever the constructor does (install it as
the constructor would), or `node:bind(property, fn)`.

## 5. Key names in `on_key_pressed`

**What.** `on_key_pressed(keysym, ...)` is handed the X keysym as a
number (65364 for Down), while `test.key` takes names and the docs write
keys by name. Phase 1's launcher compared with `"Up"`/`"Down"`, so its
arrow keys never did anything and no test noticed; phase 2 found it with
the session menu.

**Workaround.** `kit.KEY`/`kit.is_key` map the few names the port uses to
their numbers.

**API.** Hand the name too (`on_key_pressed(keysym, text, modifiers,
repeat, name)`), or a `morf.keys.Down`-style table, so configurations do
not carry keysym tables.

## 6. Watching the scheme change (not an engine gap)

The reference's scheme and light/dark switches go through its own CLI,
which the sandbox stubs, so whether it animates the change could not be
filmed; `> scheme ` there lists nothing. The port applies a new scheme at
once. If the reference cross-fades, every colour binding would need a
colour `behavior` -- a theme-wide transition (`morf.theme(tokens, {
transition = { duration = 400 } })`, every token read through it easing to
its new value) would do it in one place.

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
- An axis other than `wght` reached only the rasteriser, so Google Sans
  Flex's `opsz` could not widen small labels. Every axis is now shaped
  (the vendored cosmic-text takes the axes), optical sizing is automatic
  from the size in pixels as in CSS, and `kit.text` sets `opsz` to the size
  in points (and `ROND` 25), as the reference's font builder does. The
  Alacritty description, at the port's 15 px, measures 366 px where it
  measured 333; at the reference's own `body.small` (12 pt, 16 px) it is
  386 against the reference's 390.
