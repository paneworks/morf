# The kit contract

A visual theme is a **style**, never a layout. Every theme draws the same
layouts (`themes/layouts/views/*`, `themes/layouts/lock.lua`,
`themes/layouts/greet.lua`); what differs is how each element looks and moves.

Rules:

1. **Layouts draw only through the kit** (`require("kit")`) and the theme's
   tokens (`require("theme")`): no `radius = 24`, no fixed colours, no
   theme-specific shapes in a layout. A layout may compose kit calls into
   bigger pieces (see `themes/layouts/parts.lua`), and may place, size and
   group them freely: that arrangement is shared by every theme.
2. **No theme uses another theme's style code.** A theme's `components.lua`,
   `motion.lua`, tokens, auth style, frame and rail never `require` another
   theme's modules. Non-visual helpers live in `themes/kit_common.lua`
   (numbers, bytes, clamp, collect, signal plumbing) and may be shared.
3. **Every function below exists in every theme.** A theme may make a
   decoration (`kit.decor`, `kit.code`) draw nothing, but must accept the
   call and keep the reserved space, so the layout is identical.
4. **Same geometry contract.** A function given `width`/`height` occupies
   exactly that box in every theme. Text sizes come from theme tokens, so a
   layout reserves the box, not the glyph run.
5. **Nothing animates at rest**; values ease on change; decorative motion is
   finite. Per-frame-changing paths (spectra) are one path, not N nodes.

Per-theme views are limited to the screen chrome: `frame` and `rail` (the
border around the screen and the workspace strip in it). Their geometry
(`V.insets`, `V.geometry`, rail item/gap sizes) must match across themes.

## Colour and ink

| function | returns |
|---|---|
| `kit.signal(kind)` | fn → colour. `accent`, `ok`, `warn`, `alert`, `info`, `extra`. Status colours come from `theme.lule` / palette roles, never literals. |
| `kit.ink(kind)` | fn → text colour: `hi`, `lo`, `accent`. |
| `kit.stroke(strength, color?)` | fn → line colour: `faint`, `quiet`, `idle`, `mark`, `hot`. |
| `kit.level(value, warn?, alert?)` | colour fn for a 0..100 level: accent, then warn ≥ `warn` (70), alert ≥ `alert` (90). |

## Text

`kit.text(props)`, `kit.heading(props)`, `kit.subtitle(props)`,
`kit.section_label(props)`, `kit.menu_label(props)`, `kit.icon(name, size,
color, props)` keep their current meaning.

| function | spec |
|---|---|
| `kit.label(props)` | micro label (caps, codes, units): Text props + `size`. |
| `kit.readout(spec)` | big reading: `value` (fn → string), `size`, `unit`, `color`. |
| `kit.code(key, shape)` | decorative code string, stable per key ("SD. 249/10.63"); `""` allowed. |
| `kit.caption(spec)` | section caption row across `width`: `text`, `note`. |
| `kit.facts(rows, width, row_h, label_w)` | label/value lines (`rows` = `{label, value}`). |

## Containers

| function | spec |
|---|---|
| `kit.surface(props)`, `kit.card(props)` | as today. |
| `kit.panel(spec)` | framed region: `width`, `height`, `title`, `header` (bool), `status`, `color`, children. |
| `kit.header(spec)` | the chrome strip a panel wears: `width`, `key`, `title`, `status`, `color`. Height 22. |
| `kit.chip(spec)` | small tag: `text`, `color`, `filled`, `width`. Height 13. |
| `kit.decor(name, spec)` | theme decoration or `nil`: `brackets`, `corners`, `ticks`, `scale`, `hatch`, `grid`. Each theme defines its own spec fields. |

## Controls

`kit.action`, `kit.hover`, `kit.pill`, `kit.switch`, `kit.slider`,
`kit.tabs`, `kit.tabbed`, `kit.loading`, `kit.shape`, `kit.shape_path`,
`kit.svg`, `kit.sdf_shape`, `kit.media_progress`, `kit.morph_number`,
`kit.with_viewport` keep their current meaning. Themes without a concept
(e.g. `with_viewport`) implement it as a pass-through.

`kit.tabs(spec)`: `id`, `tabs` (`{ key, name, icon | icon_build }`), `tab`
(a signal), `width` (a number or a binding), `height`, `pad`, and two
explicit options a kit must honour instead of guessing the caller:

| option | meaning |
|---|---|
| `ids` | `"name"`: each tab is `<id>-tab-<name:lower()>` (the dashboard); `"key"` (default): `<id>-tab-<key>` (side panels). Icons are `<that>-icon`; the indicator `<id>-tab-indicator`. |
| `growing` | `true`: the row follows a drawer whose width eases between sizes (`width` a binding); tabs share it evenly and grow with it. `false`/nil: fixed slots across `width`. |

## Readings

| function | spec |
|---|---|
| `kit.gauge(spec)` | small arc gauge (as today). |
| `kit.ring(spec)` | large gauge with its reading: `size`, `value` (0..1), `color`, `text`, `label`, `sweep`. |
| `kit.mini_ring(spec)` | small gauge + caption under it: `size`, `value`, `text`, `label`, `color`. |
| `kit.bar(spec)` | thin linear level (as today). |
| `kit.meter(spec)` | segmented level: `width`, `height`, `count`, `value`, `color`, `track`. |
| `kit.fill(spec)` | emphasised fill bar: `width`, `height`, `value`, `color`. |
| `kit.vmeter(spec)` | vertical channel: `width`, `height`, `value`, `color`, `count`. |
| `kit.chart(spec)` | history chart: `width`, `height`, `samples`, `first`, `second`, `top`, `bottom`, `floor`, `color`, `columns`, `hatch`/`emphasis`, `caption`, `scale`, `id`. Returns node, top fn. |
| `kit.spectrum(spec)` | bars per value: `width`, `height`, `values`, `color`, `gap`, `mirror`. One path. |
| `kit.radar(spec)` | polygon over a web: `size`, `values`, `axes`, `color`. |
| `kit.dial(spec)` | reticle/compass dial: `size`, `value`, `color`. |
| `kit.cell(spec)` | boxed number with a level mark: `width`, `height`, `value` (0..100), `text`, `label`. |
| `kit.stat(spec)` | readout card: `width`, `height`, `label`, `value`, `level`, `color`. |
| `kit.triplet(spec)` | NOW/AVG/PEAK: `width`, `series`, `top`, `format`, `color`. |

## Status

| function | spec |
|---|---|
| `kit.emblem(spec)` | status mark: `kind` (alert, warn, ok, info; may be fn), `size`, `color`. |
| `kit.status_line(spec)` | mark + big word + trailing emphasis: `kind`, `title`, `subtitle`, `width`, `size`. |
| `kit.status(spec)` | live status strip: `width`, `kind` (fn), `title` (fn), `subtitle` (fn). |

## Motion

`kit.spring`, `kit.elastic`, `kit.bud`, `kit.ride`, `kit.STRETCH` as today.
Each theme's `motion.lua` provides `drawer`, `page_wipe`, `entries` in its
own style.

## Per-theme identity (must not overlap)

- **Material**: rounded tonal containers, pill shapes, M3 expressive shape
  morphs, springy overshoot, soft elevation; no codes or tick marks.
- **Tsugumori**: square mechanical frames, chamfered outlines, corner
  registration marks, call-sign accent curtain, decoding/scramble headings,
  rolling pill labels, mono type.

## Requested additions (not yet in every kit)

Shared layouts that need these use the stand-in named in each row until
every theme's kit provides the function.

| function | spec | stand-in meanwhile |
|---|---|---|
| `kit.round(r)` | a Material radius `r` in the theme's measure, for distance-field boxes and shape morphs a layout builds itself (selection fields, capture buttons): Material `r`, Tsugumori its small chamfer-free radius | `r * theme.ROUNDING / 25` (launcher, capture view) |
| `kit.selection(spec)` | the highlight under the chosen row of a list, riding a `track` item that springs from row to row: `track`, `color`. Material a rounded field box that stretches; Tsugumori a square plate with registration marks. | `ui.Sdf` box with `kit.round` + `kit.decor("corners")` on the tracked item (launcher) |
| `kit.state_surface(spec)` | the background of a stateful control (quick-settings tile, profile choice): `area`, `on` (fn), `height`, `tone`. Material morphs its shape with the state (pill off, rounded square on, tighter pressed); square themes show hover/press in their own way and mark `on`. | `kit.surface` with Material's radius morph + `kit.decor("corners")` shown while on (utilities tiles, power profiles) |
| `kit.keycap(spec)` | a key hint cap: `text`, `height` (22), `width` (fits). | `kit.surface` + `kit.decor("corners")` (launcher footer) |
| `kit.field(spec)` | the well a `ui.TextInput` sits in: `width`, `height`, `focused` (fn), `error` (fn), children. Material a filled/outlined M3 field; square themes a hairline well that lights on focus. | `rows.well`: `kit.surface` + `kit.stroke("faint")` hairline, radius from `theme.ROUNDING` (planner fields, audiogram) |
| `kit.icon_button(spec)` | a small icon toggle (mute): `id`, `width`, `height`, `icon_on`, `icon_off`, `on` (fn; alert tone while on), `on_clicked`. | `rows.toggle`: `kit.action` + `kit.hover` (sound mutes) |
| `kit.term(key, plain)` | a state or status word in the theme's voice. `plain` is the word a person would use, as the shared layout passes it ("Playing", "Charging", "Urgent", or nil/`""` for none); `key` names the state (`media.playing`, `media.paused`, `media.idle`, `media.position.<state>`, `session.<action>`, `load.<ok,warn,alert,asleep>`, `load.note.<...>`, `battery.<ok,warn,alert,info>`, `notice.<kind>`, `notice.verdict.<kind>`, `notice.urgency.<0,1,2>`, `history.empty`, `gpu.asleep`). Material returns `plain`; Tsugumori its call-sign wording. A term may be wider than `plain`: layouts give such words an eliding box. | implemented in all kits |
| `kit.state_ink(spec)` | ink on a `state_surface` ground: `on` (fn), `tone`, `idle`; a theme whose ground is a wash rather than a fill returns the tone itself. | implemented in all kits |
| `kit.keyboard_look(look)` | the shell's on-screen keyboard keys (lib.osk `look`: `key`, `key_dim`, `press`, `radius`) in the theme's style, as each auth style's `keyboard_look` does for the lock. | theme colour roles, `theme.key_radius`, `kit.action`, `kit.decor("corners")` (keyboard view) |

Layouts never spell flavour themselves: no zero-padded indices, codes,
HUD words ("ONLINE", "ARMED", "BUFFER"), tick or ruler geometry, or forced
upper case in a layout. Decorative text is `kit.code`, ornaments are
`kit.decor` (a level marker is `ticks` with `count = 1`), status words are
`L.term` / `kit.term`, casing is the kit's label and heading functions'.

Selectable rows and tiles (device/network rows, route chips, bar and
equalizer choices) use `rows.choice` in `themes/layouts/rows.lua` --
`kit.action` + `kit.hover` + `kit.decor("corners")` lit while chosen -- as
the stand-in for `kit.state_surface` / `kit.selection` above.
