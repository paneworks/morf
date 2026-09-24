# Writing UI in morf

How a configuration puts things on screen: the node kinds, how they are
sized and placed, how state reaches them, and what makes a frame. Every
section has a snippet you can paste into a config and run.

Four questions have four separate answers, and every feature here
belongs to exactly one of them:

- **Where does a node go?** Layout. Decided by the node's *parent*, and by
  only one kind of parent at a time.
- **Where does a property's value come from?** State. Decided per property:
  a literal, a binding, or a state block.
- **How is a piece of UI organised?** Structure: a function returning a
  node, a component with a model, a list with a delegate.
- **What does a node look like?** Appearance. Numbers and colours on the
  node itself, every one of which animates by existing.

Layout kinds compete with each other for one node's placement. Value
sources compete with each other for one property. Everything else nests.

A configuration can be checked, drawn and tested without a compositor --
`morf check`, `morf render`, `morf test` -- as [TESTING.md](TESTING.md)
describes; give a node an `id` to find it from a test.

## 1. Nodes

`local ui = require("morf.ui")`. A node is a table constructor: named keys
are properties, the array part is the children.

```lua
ui.Rect {
  width = 200, height = 40, radius = 8, color = "#1b2128",
  ui.Text { x = 12, y = 10, text = "hello", color = "#ffffff" },
}
```

| group | kinds |
|---|---|
| containers | `Item`, `Inset`, `Flickable`, `Loader`, `Layout` |
| painting | `Rect`, `ClipRect`, `Text`, `Image`, `Icon`, `Path`, `Sdf`, `SdfShape` |
| input | `MouseArea`, `TextInput` and `Terminal` (the kinds the pointer can hit), `DropArea` (the only kind a drag can) |
| positioners | `Row`, `Column`, `Grid` with `columns` |
| layouts | `Flex`, `Grid` with tracks |
| lists | `Repeater`, `ListView`, `GridView`, `each` |
| other | `Timer`; `reparent`, `spring`, `smoothed` |

Every property of every kind is in one schema
(`crates/morf-scene/src/schema.rs`); an unknown name is an error at
construction, with the kind and the name in it.

A node without a parent is a root, and a surface draws its roots. A child
of a node is drawn inside it, after it. Paint order among siblings is tree
order, then `z`: a higher `z` paints over, and is hit before, its siblings
whatever its place in the tree.

## 2. Sizes

A node's requested size is, in order: its own `width`/`height` when
positive; `layout.width`/`layout.height`; its
`implicit_width`/`implicit_height` when positive; else its measured
implicit size. Then it is clamped by `layout.minimum_*` and `maximum_*`.

Implicit sizes: text is shaped (at its own width when it has one, else
unconstrained, then again at the width its parent gave it if it wraps or
elides); an image is its pixel size, overridable by `source_width` and
`source_height`; a positioner is the sum or max of its children; anything
else is the bounding box of its children's `x + width` and `y + height`.

`width = 0` and no width are the same thing. A node cannot ask to be zero
wide; it can be `visible = false`.

What the frame actually gave a node is readable as `node.layout_x`,
`layout_y`, `layout_width`, `layout_height`. A binding that reads one is
re-run when a frame moves the node:

```lua
local box = ui.Item { anchors = { fill = true } }
ui.Rect { height = 10, width = function() return (box.layout_width or 0) / 2 end }
```

Text: `wrap`, `elide = "left" | "middle" | "right"` for a single line,
`max_lines` for wrapped text (it keeps that many lines and elides the
last), `horizontal_alignment`, `vertical_alignment`. `max_lines` without
`wrap`, or `elide` with it, is an error rather than nothing.

## 3. Placement

Each container kind owns the placement of its direct children, and reads
particular keys on them. The wrong kind's keys are errors.

| parent kind | what places the child | child-side keys |
|---|---|---|
| `Item`, `Rect`, any plain node | the child's own `x`, `y`, `anchors` | `anchors` |
| `Row`, `Column`, `Grid` (fixed `columns`) | packing order, `gap`, `align`, `justify` | `layout.align_self` |
| `Flex`, `Grid` with `template_columns/rows` | flexbox, grid tracks | `layout.grow`, `shrink`, `basis`, `align_self`, `margin`, `width`, `height`, `minimum_*`, `maximum_*`, `column`, `row`, `column_span`, `row_span` |
| `Inset` | margins around its one child | — |
| `Layout { measure, place }` | your two functions | whatever they read |

### Anchors

Relative to the parent only: `fill`, `center_in`, `left`, `right`, `top`,
`bottom` as booleans, `margins` and `left_margin` etc. as numbers.
`left` and `right` together stretch the width; likewise `top`/`bottom`.

```lua
ui.Rect { anchors = { left = true, right = true, top = true, margins = 8 }, height = 30 }
```

Anchors inside a positioner on the axis it packs, or anywhere inside a
`Flex` or a track `Grid`, are errors: one kind places a node.

### One vocabulary

Every container that packs children reads the same three words. `gap` is
the space between children. `align` places them across the packed axis:
`start`, `center`, `end`, `stretch`. `justify` distributes leftover room
along it: `start`, `center`, `end`, `space_between`, `space_around`,
`space_evenly`. A child overrides `align` with `layout = { align_self =
"end" }`, and asks for leftover room with `layout = { grow = 1 }`.

### Positioners

`Row` and `Column` pack children at their own sizes: `gap` between,
`align` across, `justify` along. They never resize a child (`align =
"stretch"` is the one exception). `Grid` with a numeric `columns` fills
row-major; tracks are as wide as their widest cell. A child with `visible
= false` takes no room, no gap and no grid cell, as in a `Flex`: hide a
card and the ones after it close up, and the container shrinks to what it
shows. To keep a hidden child's place, fade it with `opacity = 0` instead.

```lua
ui.Row {
  gap = 10, align = "center", justify = "space_between",
  ui.Text { text = "Password:", width = 90 },
  ui.Rect { width = 300, height = 30, radius = 6 },
}
```

Reach for `Flex` the moment a child should take leftover space.

### Flex and track grids

`ui.Flex` is flexbox: `direction = "row" | "column" | "row_reverse" |
"column_reverse"`, `wrap`, `gap`, `padding`, `align` (across the
direction: `start`, `center`, `end`, `stretch`, `baseline`), `justify`
(along it: those plus `space_between`, `space_around`, `space_evenly`),
`align_content` for wrapped lines.

```lua
ui.Flex {
  direction = "row", gap = 8, align = "center", justify = "space_between",
  anchors = { fill = true },
  ui.Text { text = function() return morf.clock:get() end },
  ui.Rect { layout = { grow = 1, minimum_width = 0 }, height = 4, color = "#333" },
  ui.Image { source = "battery.svg", width = 16, height = 16 },
}
```

`ui.Grid` with `template_columns` or `template_rows` is a CSS grid. A
track is `"1fr"`, `"auto"`, a number, `"min_content"`, `"max_content"`,
`{ min = 40, max = "1fr" }`, or `"repeat(3, 1fr)"`. Children place
themselves with `layout = { column = 2, row = 1, column_span = 2 }`;
without placement they flow in order. `gap`, or `column_gap` and
`row_gap`, are the gaps. Sizes in the child's `layout` may be a number, a percent string
or `"auto"`.

```lua
ui.Grid {
  template_columns = { "repeat(3, 1fr)" }, gap = 20,
  cell(), cell(), cell(), cell(), cell(),
}
```

A layout stretches children across its axis by default, as CSS does; a
positioner does not. A flex item never shrinks below its content unless
it says `layout = { minimum_width = 0 }`, also as CSS does.

Everything under a `Flex` or track `Grid` that is itself one is laid out
in the same pass; anything else is a leaf, sized by the rules above, whose
own children then follow the ordinary rules. A card in a grid cell still
anchors its label.


### Your own container

`ui.Layout` takes two functions. `measure(available, children)` returns
width and height; `place(bounds, children)` returns one `{ x, y, width,
height }` per child (width and height optional). `available` and `bounds`
are `{ width, height }` (`available` may be `math.huge`), `children` a
list of measured `{ width, height }` in tree order. A child is measured
once, before either is asked. The functions get numbers and return
numbers; writing to a node from inside one is refused.

```lua
ui.Layout {
  anchors = { fill = true },
  measure = function(available, children)
    local h = 0
    for _, c in ipairs(children) do h = math.max(h, c.height) end
    return available.width, h
  end,
  place = function(bounds, children)
    local out = {}
    for i, c in ipairs(children) do
      local x = i == 1 and 0 or i == #children and bounds.width - c.width
        or (bounds.width - c.width) / 2
      out[i] = { x = x, y = (bounds.height - c.height) / 2 }
    end
    return out
  end,
  left, middle, right,
}
```

`examples/lib/align.lua` is exactly that bar.

## 4. Lists

`morf.list_model(rows)` holds rows with stable identity. `model:replace(rows,
"id")` matches by that field, so rows that stayed keep their nodes; without
a key it matches by value. `insert`, `remove`, `move`, `set`, `get`, `len`
as expected; `#model` and `model[i]` (so `ipairs(model)`) read like `len`
and `get`.

A binding that reads a model -- `len`, `get`, `index_of`, `#`, an index --
depends on it, and runs again when any change lands on it:

```lua
local items = morf.list_model({})
ui.Text { text = function() return items:len() .. " items" end }
```

`ui.Repeater { model, delegate }` builds one node per row and follows the
model: rows that go are destroyed, rows that come are built, rows that
move are moved, and the children's order is the model's. A delegate may
return a second value, an updater `function(row, index)`, in which case a
changed row is patched in place rather than rebuilt: a caption changes, a
thumbnail stays. `as = "row" | "column" | "grid" | "flex"` lays the delegates
out as that container, with its properties.

```lua
local windows = morf.list_model({})
ui.Repeater {
  as = "grid", columns = 3, gap = 20,
  model = windows,
  delegate = function(window)
    local caption = ui.Text { text = window.title }
    return ui.Item { width = 320, height = 200, caption },
      function(next) caption.text = next.title end
  end,
}
-- later
windows:replace(rows, "identifier")
```

The compositor's own windows come ready-made: `morf.toplevels.model` is a
list model keyed by `identifier` that the engine keeps current, so a dock
is a Repeater over it with no timer. `morf.toplevels.list()`, `.get(id)`
and `.revision()` are tracked reads (a binding that calls one re-runs when
a window opens, closes, or changes title, app id or state), and
`morf.toplevels.on_changed(function(change) end)` hears
`{ opened, closed, changed }` identifier lists. `morf.windows` is still
the plain snapshot table.

A row is `{ identifier, title, app_id, activated, maximized, minimized,
fullscreen, controllable, outputs, output, parent }`. The state flags,
`outputs` (the names of the screens the window is on, as `morf.screens`
names them, in the order it entered them), `output` (the first of them)
and `parent` (the identifier of the window a dialog belongs to) come from
`wlr-foreign-toplevel-management`; on a compositor without it they are
false, empty and absent, and `controllable` is false. A window moving to
another screen is a change like a retitle, so a dock per screen is a
filter:

```lua
local here = morf.screens[1] and morf.screens[1].name   -- the screen this instance drives
ui.Text { text = function()
  local n = 0
  for _, w in ipairs(morf.toplevels.list()) do if w.output == here then n = n + 1 end end
  return n .. " windows here"
end }
```

`ui.ListView` and `ui.GridView` virtualise long lists; scroll them with
`morf.sync_view(node, offset)`. `ui.each(list, delegate, options)` is a
Repeater over a `morf.state` list (below).

## 5. State

### Bindings

Any property given a function is a binding. It runs once at construction
and again whenever something it read changes: a signal, a `morf.state`
field, another node's property (`other.width`, or `other.width_target`
for the animation's destination), `layout_*`, `morf.clock`. It returns a
value: a number, a string, a colour, or a table for the properties that
take one (a gradient, a decoration). It never runs per frame.

The time comes in three grains: `morf.clock` ("HH:MM:SS"),
`morf.minute_clock` ("HH:MM") and `morf.hour_clock` ("HH"). Read the
coarsest one that shows what you need: the shell wakes every second only
while some binding reads `morf.clock`, every minute while one reads the
minute clock, and not at all for the time when nothing does. A
`morf.system_clock` binds at its `precision`, or at the grain its `format`
shows if that is coarser — `clock:format("%H:%M")` is a minute binding
even on a clock made at seconds.

A binding or `morf.effect` may itself build nodes with bindings (or make
another effect): those are registered when the running flush ends and get
their first run straight after, before the caller sees the result.

`morf.effect(name, fn, options)` is a binding with no property: it runs
for what it does, and again whenever what it read changes. It returns a
handle; `handle:dispose()` takes it out of the graph for good, and
`handle:alive()` says whether it still is. `options.owner = node` ties it
to a node: removing the node disposes it, so an effect made while building
a panel goes with the panel. If its first run fails, `morf.effect` returns
`false, message, handle`.

```lua
local panel = ui.Item {}
morf.effect("panel.follow", function() panel.visible = open:get() end, { owner = panel })
```

### Signals and state tables

`morf.signal(name, value)` holds one value with `get`/`set`: a scalar, a
colour, or a JSON-like table (string keys or a dense array, of those, at
most 16 deep). A table is copied in on `set` and out on `get`, so changing
what `get` returned changes nothing until it is `set`, and an equal table
is no change: nothing re-runs. Over `morf ipc` a table is its JSON text.

```lua
local player = morf.signal("player", { title = "", artists = {} })
player:set({ title = "Song", artists = { "A" } })
ui.Text { text = function() return player:get().title end }
```

Signals and state tables may be made anywhere, a binding included: a
module that holds state can be `require`d for the first time from inside
one, and its signals join the flush that is running.

`morf.state(table)` keeps a shape: each named field is a signal read and
written through the proxy, a nested table is nested, an array is a list
model. Fields are fixed at creation; an unknown name is an error.
`morf.state(table, { reloadable = "name" })` keeps the scalar fields
across a configuration reload under that name, like `morf.reloadable`.

```lua
local model = morf.state { screen = "list", typed = 0, user = { name = "" }, rows = {} }
ui.Text { text = function() return model.user.name end }
model.screen = "prompt"          -- one field
model.rows = { { id = "a" } }    -- a list, replaced whole (by value)
model.rows:replace(rows, "id")   -- or by key
```

### One flush per handler

Every handler the host runs -- a click, a key, a timer, an IPC verb, a
D-Bus call, a capture -- is one flush: write as many signals, fields and
node properties as you like, and the bindings that depend on them run
once, when the handler returns. A bare `node.text = "x"` in a handler
reaches its readers before the next frame. Top-level configuration code
flushes on each write, as it always did.

### Components

`examples/lib/component.lua` gives Elm's shape on these pieces: `init(args)`
returns the model's table, `view(model, send)` runs once and returns a
tree of bindings on the model, and `update(model, msg, send)` is the one
place the model changes. `send(msg)` returns a handler; `send_with(fn)`
builds the message from the handler's arguments; `dispatch(msg)` delivers
one from code that is not a handler. A message that is a function runs
and its return is the message. See `examples/polkit.lua`.

### States

A node's `states` are named tables of `property_changes`,
`anchor_changes` and `parent_change`; `transitions` say how to animate
between them. Select a state by writing `state = "name"`, by a `state`
binding, or let states choose themselves with `when`:

```lua
ui.Rect {
  color = "#222",
  states = {
    default = { property_changes = { color = "#222" } },
    hovered = { when = function() return hover:get() end, property_changes = { color = "#333" } },
    pressed = { when = function() return down:get() end, property_changes = { color = "#444", scale = 0.97 } },
  },
  transitions = { { from = "*", to = "*", duration = 120, easing = "out_quad" } },
}
```

States with `when` are asked by `order` (lowest first, ties by name); the
first true wins; none true selects `default` if there is one, else the
node stays as it is.

### Animation

A `behavior = { property = { duration = 200, easing = "out_quad" } }`
makes writes to that property animate; `ui.spring { stiffness = 300 }`
and `ui.smoothed { velocity = 1000 }` are the other kinds. Behaviors are
installed after construction, so nothing animates its own creation;
`enter` (section 6) says where a first frame starts instead.
Animated *current* values do not re-run bindings; read `_target` for the
destination, or use a `morf.transform_watcher` for the moving value.
`morf.animation.fling` coasts a property.

`morf.animation.play { ... }` runs a group: its array part is a sequence of
steps, each `{ node, property, to, from, duration, easing, delay }`, a
`{ pause = ms }`, a `{ keyframes = { { at, value, easing }, ... } }` track,
or a nested `{ sequence = { ... } }` / `{ parallel = { ... } }`. On the group
itself, `loops = n | "forever"` repeats the whole schedule, `alternate =
true` runs every other pass backwards (the last step first, each from its
target back to where it set out), and `delay` waits once before the first
pass. It returns a handle with `:stop()`, `:finish()`, `:pause()`,
`:resume()` and `:active()`; `on_finished(reason)` hears `"completed"`,
`"stopped"`, or `"canceled"` when a node it moves is removed, and may start
the next group.

```lua
morf.animation.play {
  delay = 120, loops = 3, alternate = true,
  { node = face, property = "translate_y", to = -6, duration = 400, easing = "out_quad" },
  { node = face, property = "scale_y", to = 0.9, duration = 120 },
  on_finished = function() morf.animation.play { { node = face, property = "opacity", to = 0, duration = 200 } } end,
}
```

Motion a node keeps up on its own — a bob, a spin, a pulse — is `loop`,
keyed by property like `behavior`: `from` (default: where it is), `to`,
`duration`, `easing`, `alternate`, `loops` (default forever) and `delay`.
It runs in Rust between frames and ends with the node. `loop` may be a
binding: returning another table restarts what changed, returning nil ends
the loops, and a property whose loop ends goes back to its `from` (through
its `behavior`, if it has one) — unless the loop says `hold = true`, when
it stays wherever the motion had it, and a loop started on it again (with
no `from`) goes on from there: a spinner that stops keeps its angle. A write
to a looping property takes it over, as any write takes over an animation.

```lua
ui.Item {
  loop = function()
    if not busy:get() then return nil end
    return { rotation = { to = 360, duration = 900, hold = true } }
  end,
}
```

```lua
ui.Item {
  loop = function()
    if pets.mood() == "asleep" then return nil end
    return { translate_y = { from = 0, to = -4, duration = 1400, easing = "in_out_sine", alternate = true } }
  end,
  face,
}
```

### Destruction

A node is destroyed when a `Loader` lets it go, when its `Repeater` row
leaves the model, when `ui.destroy(node)` is called, or when any of its
ancestors goes the same way. `on_destroyed = function() end` on any node
runs once then, deepest node first, so what the node's Lua made for it —
a `morf.clipboard.watch`, a service subscription, a timer outside the tree
— can be let go of. It runs as a handler once nothing else is running (never
in the middle of a flush), with a handler's fuel; its writes flush when it
returns, and an error in it is logged. Groups, loops, bindings and handlers
that belong to a destroyed node end with it.

```lua
local function clock_face()
  local unsubscribe = services.time.subscribe(function(t) ... end)
  return ui.Item { on_destroyed = unsubscribe, ... }
end
```

## 6. Appearance

One rule covers every property in this section: it is a number, a colour,
or a table of those, and a `behavior` on it animates it. Nothing here has
a second mechanism.

### Colour values

`morf.color(x)` reads any notation: hex, `rgb()`, `hsl()`, `hwb()`,
`lab()`, `oklch()`, a named colour, `transparent`, a `/ alpha` at the end
of any of them. Every property that takes a colour takes a string or a
value, and reads back as a value. A value has fields `r g b a h s l`, and
methods for nearly everything a colour can do:

```lua
local accent = morf.color "#3366cc"
accent:lighten(0.1)  accent:darken(0.1)  accent:saturate(0.2)  accent:rotate(30)
accent:alpha(0.5)    accent:complement() accent:gray()         accent:invert()
accent:mix(other, 0.5, "oklch")          accent:composite(over)
accent:luminance()   accent:is_light()   accent:contrast(paper)  accent:text_color()
accent:distance(other, "ciede2000")      accent:blind("deuteranopia")
accent:with { l = accent.l - 0.1, space = "oklch" }
accent:hex()  accent:oklch_string()  accent:rgb8()  accent:nearest_name()
```

Constructors live beside it: `morf.color.rgb`, `hsl`, `hsv`, `lab`,
`oklab`, `lch`, `oklch`, `xyz`, `cmyk`, `gray`, `named`, `random("vivid")`,
`mix(a, b, t, space)`, `scale { stops }:sample(t)`, and
`distinct(n, { fixed, metric, iterations })` for a palette whose members
stay apart. `c:ansi_style { bold = true }` and `c:paint(text)` colour a
terminal.

A colour animates in a space. The default is OkLab, which is what a
crossfade between two saturated colours should look like; `space` and
`hue` on the behavior choose otherwise:

```lua
ui.Rect { color = accent, behavior = { color = { duration = 300, space = "oklch", hue = "longer" } } }
```

### Blending

A translucent colour is mixed with what is under it in linear light, which
is how light mixes and what morf does unless told otherwise. Browsers, Qt
and GTK mix the sRGB-encoded values instead, and the two disagree on every
translucent pixel: 50% white over black is `#bcbcbc` here and `#808080`
there, and a 3.5% white hairline that is a hint in a browser is a visible
line here. A design made in one of those, or ported from one, looks right
only when mixed the same way:

```lua
morf.surface.blend = "srgb"              -- the shell's own surface
morf.window.popup { root = menu, blend = "srgb" }   -- or any window surface
```

It is per surface, `"linear"` by default, and may change at any time; the
surface rebuilds its pipelines when it does. Opaque colours, images and
gradients' stops land on the same pixels either way — only the mixing
differs, text's antialiasing included. `examples/blend-compare.lua` draws
one design both ways.

### Gradients

`gradient` on a `Rect` or an `Sdf` is one table: a `kind` (`linear`,
`radial`, `conic`), an `angle` (0 points up, 90 right, as a stylesheet's
does), a centre `at = { x, y }` in fractions, a `radius` for radial, the
`space` neighbouring stops mix in, and up to sixteen `stops`. A stop is a
colour, a `{ color, position }` pair, or a `{ color = , position = }`
table; a bare list spreads evenly, a missing position sits between its
neighbours, and two stops at one position are a hard edge.

```lua
ui.Rect {
  gradient = { angle = 135, space = "oklch", stops = { "#e6f7fa", { accent, 0.5 }, "#5fa8d3" } },
  behavior = { gradient = { duration = 400 } },  -- every stop moves
}
```

The whole table is one property: a binding may return it, and a behavior
on it moves every stop's colour and position at once.

### Themes and preferences

`morf.theme(tokens, options)` is a `morf.state` for appearance. A string
that names a colour becomes one. A function field is derived: read
inside a binding, whatever it touches is what the binding follows, so
`hover` below re-derives when `accent` changes with no wiring. A `source`
is a JSON file whose leaf keys are tokens -- the file a palette generator
writes -- read now and again whenever it is rewritten.

```lua
local theme = morf.theme({
  accent = "#3366cc", paper = "#f6f5f4",
  hover = function(t) return t.accent:alpha(0.12) end,
  ink = function(t) return t.paper:text_color() end,
}, { source = "~/.cache/wal/colors.json" })
ui.Rect { color = function() return theme.hover end }
theme.accent = "#ff6600"   -- and every reader of hover follows
```

`morf.prefers` is the desktop's own settings, read from the settings
portal and kept current: `color_scheme` (`"dark"`, `"light"`, `"none"`),
`contrast`, `accent_color` (a colour or nil), `reduced_motion`, and the
driven output's `scale`. Each is a field a binding follows. A derived
token that reads one switches palette with the desktop. When
`reduced_motion` is on, every behavior, group and spring lands on its
target on the next tick: a configuration written with motion ends up in
the same place, without the travel.

`Text` takes `color = "inherit"`: the nearest ancestor with a colour.
An `Item` carries one for the purpose without painting anything.

### Text

Beyond `font_family`, `font_size` and `font_weight`: `line_height` is a
multiple of the size, or a `"24px"` size; `letter_spacing` and
`word_spacing` are pixels; `font_style` is `normal`, `italic` or
`oblique`; `font_stretch` runs from `ultra_condensed` to
`ultra_expanded`. A `decoration = { line, thickness, offset, color }`
draws a band `under`, `over` or `through` the text from the face's own
metrics; thickness and colour default to the face's and the text's.

```lua
ui.Text {
  text = "Sorry, that didn't work", line_height = 1.4, font_style = "italic",
  decoration = function() return refused:get() and { line = "under", color = theme.alert } or {} end,
}
```

### Text in runs, and links

A `Text` may be set in runs of their own style. `spans` is a list whose
entries are strings, in the node's own style, or tables: `text`, `bold`,
`weight`, `italic`, `underline`, `strike`, `color` (any notation), `size`,
`family`, and `link`. `markup` is the part of HTML the desktop
notification spec allows — `<b>`, `<i>`, `<u>`, `<s>`, `<a href="…">`,
`<br>` and entities (`&amp;`, `&lt;`, `&#33;`, …); an unknown tag is
dropped and its content kept, so a notification body can be drawn as it
came. `markup` wins over `spans`, which win over `text`.

```lua
ui.Text {
  font_size = 14, color = theme.ink, link_color = theme.accent, wrap = true,
  spans = { "Build ", { text = "failed", bold = true, color = "#e5484d" },
            " — ", { text = "see the log", link = "file:///tmp/build.log" } },
  on_link = function(href) morf.spawn { command = { "xdg-open", href } } end,
}
ui.Text { markup = notification.body, wrap = true, max_lines = 4, on_link = open }
```

Anything a run leaves out is the node's own: family, size, weight, slant,
colour, spacing. A link is underlined unless it says `underline = false`,
and drawn in `link_color` when it names no colour of its own. A run's size
changes the height of the line it is on. The runs are shaped together, so
kerning and wrapping go across them; `elide` and `max_lines` keep the runs
of what is left.

`on_link(href)` hears a click on a link. The pointer finds a link where it
was laid out — the rest of the text lets clicks through to whatever is
beneath — and takes the node's `cursor` (`"pointer"`) over it. Where each
link landed is the read-only `links`, a list of `{ href, x, y, width,
height }` in the node's own space, kept current after every layout.

### Text input

`ui.TextInput` is text you can edit. It is set by the same shaper as
`Text` and takes the same type properties (`font_family`, `font_size`,
`font_weight`, `line_height`, `letter_spacing`, `word_spacing`,
`font_style`, `font_stretch`, `horizontal_alignment`); everything it
draws — `color`, `placeholder_color`, `selection_color`,
`selected_text_color`, `caret_color`, `caret_width` — animates like any
other number or colour.

```lua
local search -- declared first: the field's own callback names it
search = ui.TextInput {
  width = 320, height = 40, font_size = 16, color = "#e6e8ee",
  placeholder = "Search…", placeholder_color = "#8b90a0", focus = true,
  on_text_changed = function(text) query:set(text) end,
  on_accepted = function(text) launch(text) end,
  on_escape = function() search.text = "" end,
}
```

`text` goes both ways: assigning it replaces what the field holds (and
its undo history), and every edit the field makes assigns it — so a
binding on `search.text` follows the keyboard. `on_text_changed(text)`
fires for the field's own edits (typing, pasting, `:insert`), not for an
assignment. `placeholder` shows while `text` is empty. `multiline = true`
takes Enter as a new line (Ctrl+Enter accepts) and wraps at the width
unless `wrap = false`; a single line never wraps and scrolls sideways under
the caret instead. `password = true` draws each character as
`password_char` and refuses copy and cut. `max_length` caps the
characters (zero is no limit), `read_only` leaves it selectable but not
editable, `caret_blink_interval` is the blink's half period in ms (zero
holds still), and `vertical_alignment` places a single line in a taller
box.

The keys are the ones every text box has: arrows, Home/End (Ctrl for the
whole text), Ctrl for a word at a time, Shift to select, Up/Down and
PageUp/PageDown between lines, Ctrl+A, Ctrl+C/X/V (and Shift+Delete,
Ctrl/Shift+Insert) through the compositor clipboard -- Shift+Delete cuts
only a selection; with nothing selected it deletes nothing and goes to
`on_key_pressed`, so a launcher can bind it -- Ctrl+Z and
Ctrl+Shift+Z or Ctrl+Y, Enter to `on_accepted(text)`, Escape to
`on_escape()`. A click places the caret, a double click selects a word, a
triple click the line, and a drag selects. A key the field has no use for
— Up in a single line, Ctrl+Q — goes to its own `on_key_pressed(keysym,
text, modifiers)`, which is how a launcher moves its list from the search
box. `modifiers` is a string such as `"ctrl+shift"`; every
`on_key_pressed` receives it.

A field has the keyboard when `focus` is true, and one field at a time
does: a click, a Tab, or writing `focus = true` moves it, and
`on_focus_changed(focused)` says so. A field -- or any node with
`on_key_pressed` -- that sets `tab_navigation = false` keeps Tab while it
has the keyboard: Tab and Shift+Tab go to its `on_key_pressed` (for a
completion, say) instead of moving focus. While it has it, the compositor's
input method (text-input-v3) is enabled for it and what the input method
commits is typed into the field.

The caret and the selection are properties too, as byte offsets —
the number of bytes before them, so `text:sub(1, cursor_position)` is
what lies left of the caret: `cursor_position`, `selection_start` and
`selection_end`, each writable. `scroll_x`/`scroll_y` are how far the
content has scrolled to keep the caret in view, and `content_width` /
`content_height` how big it is, for a scroll bar to follow. Methods:
`:select(start, stop)`, `:select_all()`, `:deselect()`, `:insert(text)`,
`:selected_text()`, `:undo()`, `:redo()`.

### Terminal

`ui.Terminal` is a terminal emulator: a program on a pseudo-terminal, its
screen drawn as a grid of cells, the keyboard and the pointer going to it.
Anything that runs in a terminal window runs in one — a shell, `btop`,
`nvim`, `fzf` — inside the shell's own surfaces, with ordinary UI around it.

```lua
local term
term = ui.Terminal {
  command = { "btop" },            -- the argv; default: $SHELL
  font_family = "monospace", font_size = 13, padding = 8,
  colors = { foreground = "#d7dae0", background = "#0f1218", cursor = "#7aa2f7",
             palette = { "#1d202f", "#f7768e", --[[ ... sixteen ]] } },
  anchors = { fill = true }, focus = true,
  on_exit = function(code, signal) panel.visible = false end,
  on_title = function(title) end,
  on_bell = function() end,
}
ui.Text { text = function() return term.title .. "  " .. term.columns .. "×" .. term.rows end }
```

How the program is started, all optional, read once when the node is made:
`command` (never a shell unless it names one), `cwd`, `env` (added to what
it inherits), and `scrollback`, the lines of history kept (10000, at most
100000). The program starts the first time the node is laid out, at its
real size; nothing runs for a terminal that is never on screen. It gets
the terminal as its controlling tty in a session of its own (so `^C`, `^Z`
and job control work), `TERM=xterm-256color`, `COLORTERM=truecolor`, and
never morf's `LD_LIBRARY_PATH` (see [IO.md](IO.md)).

The grid follows the node: its size in cells is its laid-out size, less
`padding` on every side, over the cell the font makes (the face's advance
by its ascent and descent, in whole pixels). A resize reaches the program
as `SIGWINCH`. The node has no size of its own; give it one, or anchors, or
a `layout.grow`.

Kept by the runtime and read-only, each a property a binding follows:
`columns`, `rows`, `title` (what the program last called itself, OSC 0/2),
`running`, and `exit_code` (nil while it runs; 128 + the signal when a
signal ended it; 127 when it could not be started, which is also said on
the terminal itself). `on_exit(code, signal)` hears the same, and
`on_clipboard(text)` hears a program asking to set the clipboard (OSC 52).
`font_family`, `font_size`, `padding` and `colors` may change at any time.
`colors` takes any colour notation; a colour the program sets itself (OSC
4, 10, 11) wins over it.

Methods:

- `:write(text)` — bytes to the program, as if typed: `true`, or `false,
  why`. What is written before the program has started waits for it.
- `:paste(text)` — the same, wrapped in bracketed-paste markers when the
  program asked for them (a shell, an editor), so it is not run line by line.
- `:kill(signal)` — `"TERM"` unless named; whether it was running.
- `:scroll(lines)` — up into the history by `lines`, down by a negative
  number, back to the bottom with none; whether the view moved.
- `:text()` — the screen as the view shows it, one line per row, trailing
  blanks trimmed. For a test, or for reading what a program printed.
- `:pid()` — the program's process id while it runs.
- `:selection()` — the text selected with the pointer, or nil;
  `:clear_selection()` drops it. When the program has not asked for the
  mouse, a left drag selects cells, a double click a word, a triple click a
  line, drawn inverted; `on_selection(text)` runs when the button is let
  go. Copying is the configuration's: `morf.clipboard.set(term:selection())`
  from an `on_key_pressed` that returns true for Ctrl+Shift+C.

Keys go to the program — it is the terminal's while it has the keyboard,
Tab and Escape included — unless the terminal's own
`on_key_pressed(keysym, text, modifiers, repeat)` returns `true`, which
keeps that key from the program (a panel's Escape, a copy shortcut); any
other return lets it through. They are encoded as xterm does: the arrows,
Home/End, PageUp/PageDown, Insert/Delete and F1–F12 with their modifier
forms, application cursor mode, Ctrl folding a letter to its control code,
Alt as an Escape prefix. A click gives it the keyboard (its cursor is solid
while it has it, an outline otherwise); so does `focus = true`, as for a
text input. When the program asked for the mouse (btop, nvim with `mouse=a`,
fzf) presses, releases, motion and the wheel are reported to it, in SGR
form when it asked for that; otherwise the wheel scrolls the history, or is
sent as arrow keys to a full-screen program that did not ask.

Box drawing, block elements, braille and powerline separators are drawn
from the cell's own geometry rather than a font, so frames join and
graphs line up; everything else is the font's glyph (or a fallback face's)
placed on its cell, whatever the font would have advanced it by. Bold,
italic, dim, underline, strikeout, inverse and truecolor are drawn as the
program asks.

A terminal costs nothing while its program is quiet: output arrives through
the same reactor as `morf.spawn`'s and is fed to the emulator as it comes,
at most 256 KiB a turn of the loop, so `yes` shares the loop with everything
else; a frame is drawn only when the screen changed, and only the rows that
changed are repainted. A runtime has at most 16 terminals (`MORF_LIMITS`
`terminals=N`); destroying the node hangs its program up (`SIGHUP`), and a
reload ends them all. `examples/terminal.lua` runs btop in a panel;
`examples/fzf_launcher.lua` is an application launcher that is fzf.

### Images

`ui.Image { source = ... }` draws a path, a `file://` URI, an SVG written
inline (text starting with `<svg`) or a `data:` URI, or a picture held in
memory under a `memory:` source — a capture's, or pixels the configuration
published itself:

```lua
-- A notification's `image-data` hint, straight from morf.dbus: an
-- (iiibiiay) struct, positional or with the spec's field names.
local cover = morf.image.from_dbus(hints["image-data"], { name = "note-" .. id })
ui.Image { source = cover, width = 48, height = 48, fill_mode = "preserve_aspect_fit" }
-- later, when the notification goes
morf.image.release(cover)
```

- `morf.image.from_rgba(bytes, width, height, stride, options)` publishes raw
  pixels and returns their source. `bytes` is a string or a list of byte
  values (what `morf.dbus` gives for an `ay`); `stride` is the bytes from
  one row to the next (`nil`: packed). Options: `format` (`"rgba"`, the
  default, `"rgb"`, `"bgra"`, `"argb"`), `premultiplied` (divide the colour
  back out of alpha), and `name`. Returns `nil, why` when the sizes do not
  fit the bytes.
- `morf.image.from_dbus(image_data, options)` the same, for a D-Bus
  `(iiibiiay)` image (8 bits a sample, 3 or 4 channels).
- `morf.image.release(source)` lets a published picture go; whether it was
  held.
- `morf.image.encode_png(bytes, width, height, path, options)` writes raw
  pixels (same `stride`, `format`, `premultiplied` options) as a PNG file:
  `true`, or `nil, why`.

What became of the source is on the node, as properties a binding
follows, and in `on_status(status, error)` (given when the image is made):

- `status` is `"none"` with no source, `"loading"` until the frame that
  first draws the node, then `"ready"`, or `"error"` with the reason in
  `error` — a file that is not there, is not a picture, or will not decode.
  A source whose header reads but whose pixels do not (a truncated
  download) turns `"error"` the frame after it was first drawn.
- The node's implicit size is the picture's own, as for any image.

```lua
local cover = ui.Image {
  source = path, width = 64, height = 64, fill_mode = "preserve_aspect_crop",
  on_status = function(status, err) if status == "error" then log(err) end end,
}
ui.Text { visible = function() return cover.status == "error" end, text = "lost" }
```

An error is not retried while the source stays the same: write another
source (or the same one again after writing `""`) to look again.

A GIF, animated PNG or animated WebP plays. `playing` (default `true`)
pauses it where it is; `speed` (1) scales its frame delays; `frame` is the
frame it shows, and writing it seeks; `loops` is `"forever"` or how many
times through, after which it rests on its last frame; `frame_count` is
read-only. It moves only while it is drawn: hidden, fully transparent, off
a surface that stopped painting, it stops where it is and costs nothing,
and nothing wakes the loop but its next frame. Every frame is decoded once
at the picture's own size and kept — at most 64 MiB of frames per picture
(a longer one plays the frames that fit), 16 moving pictures and 256 MiB in
all per surface, the least recently drawn let go first. An inline or
`memory:` source is always drawn still.

```lua
local spinner = ui.Image { source = "~/.cache/spinner.gif", width = 32, height = 32,
                           playing = function() return busy:get() end }
```

A published picture is drawable on every surface and is held until it is
released, until the same `name` is published again (which answers with a
*new* source, so nothing draws the old pixels by mistake), or until the
configuration is reloaded or exits. A configuration may hold 4096 of them
and 256 MiB of pixels; past that `from_rgba` answers `nil, why`.

### Paths

`ui.Path` is a shape written as SVG path data — a face, a ring gauge, a
sprite, a rounded polyline — and, unlike an SVG in an `Image`, it stays
geometry: every number and colour on it animates, and it is drawn at the
pixels it covers, so it is as sharp scaled up as at rest.

- `d`: the outline, every SVG command, absolute or relative.
- `fill_color` (black, as SVG's default), `fill_rule`: `nonzero` or
  `evenodd`.
- `stroke_color` (none), `stroke_width` (1), `stroke_cap`: `butt`, `round`,
  `square`; `stroke_join`: `miter`, `round`, `bevel`; `miter_limit` (4).
- `dash = { dash, gap, ... }` and `dash_offset`, in path units; an odd list
  is read twice over, as SVG reads it.
- `trim_start`, `trim_end`: the part of the outline that is stroked, as
  fractions of its whole length (across every subpath, in order). A
  progress ring is `trim_end`; a line drawing itself on is `trim_end` going
  from 0 to 1. The fill is always the whole shape.
- `view_box = { x, y, w, h }` (or four numbers): the part of path space
  stretched over the node, and the node's own size when it is given none.
  `fill_mode` fits it the way an `Image` fits: `stretch`,
  `preserve_aspect_fit` (centred), `preserve_aspect_crop`. Without a view
  box, path units are the node's pixels and the node needs a size. A stroke
  may reach past the node's box, by half its width and more at a corner.
- `morph_to` and `morph_progress`: the outline it turns into. When the two
  have the same run of moves, segments and closes — lines and curves count
  alike — the points walk from one to the other; otherwise the outline
  changes over at the halfway mark.

```lua
local mouth = ui.Path {
  anchors = { fill = true }, view_box = { 0, 0, 140, 130 },
  d = "M34 82 C50 102 90 102 106 82",            -- a smile
  morph_to = "M34 96 C50 76 90 76 106 96",       -- a frown: the same curve
  morph_progress = function() return mood() == "sad" and 1 or 0 end,
  fill_color = "transparent", stroke_color = "#2b2118", stroke_width = 6, stroke_cap = "round",
  behavior = { morph_progress = { duration = 300, easing = "out_cubic" } },
}

local ring = ui.Path {
  width = 48, height = 48, view_box = { 0, 0, 100, 100 },
  d = "M50 8 A42 42 0 1 1 49.99 8",
  fill_color = "transparent", stroke_color = theme.accent, stroke_width = 12, stroke_cap = "round",
  trim_end = function() return battery.level / 100 end,
  behavior = { trim_end = { duration = 400 } },
}
```

A path that is not changing costs a texture lookup: what was drawn is kept
under a key of everything that shaped its pixels. One whose numbers move is
drawn again for each frame they move in, on the CPU, at its on-screen size —
cheap for an icon or a gauge, worth knowing for a path the size of the
screen. `examples/path.lua` has one of each.

### Keys

A node with `on_key_pressed` or `on_key_released` is somewhere keys can
go: the focused one of its surface (a click, a Tab, `focus = true`), or
else the first. `on_key_pressed(keysym, text, modifiers, repeat)` runs
for a press and for each of the keyboard's repeats of a held key, and
`repeat` says which it is; `on_key_released(keysym, text, modifiers)`
runs when the key comes up, on whatever has focus by then. Something that
moves while a key is held — a game, a scrubber — tracks the press and the
release and ignores the repeats:

```lua
local held = {}
ui.MouseArea {
  focus = true,
  on_key_pressed = function(keysym, _, _, repeat_)
    if not repeat_ then held[keysym] = true end
  end,
  on_key_released = function(keysym) held[keysym] = nil end,
}
```

A text input types a repeat as it types a press, so a held Backspace
keeps deleting.

### Entering

`enter = { opacity = 0, translate_x = 32 }` on any node is where its
first frame starts. The behaviors carry it from there to the declared
values; a property with no behavior simply arrives. This is the one way
creation animates.

```lua
ui.Rect {
  opacity = 1, translate_x = 0,
  enter = { opacity = 0, translate_x = 32 },
  behavior = { opacity = { duration = 220 }, translate_x = { kind = "spring", stiffness = 260 } },
}
```

### Hover and press

A `MouseArea` keeps `hovered` (the pointer is over it, and it is the
topmost area there) and `pressed` (a button or a touch went down on it and
has not come up), both read-only. A binding follows them like any other
property, so hover needs no signal and no `on_entered`:

```lua
local area = ui.MouseArea { anchors = { fill = true } }
ui.Rect { color = function() return area.pressed and "#444" or area.hovered and "#333" or "#222" end }
```

### The wheel

`on_wheel(surface_x, surface_y, pixel_x, pixel_y, step_x, step_y,
local_x, local_y)` runs for a wheel turn or a touchpad scroll. The wheel
bubbles: it goes to the topmost `MouseArea` under the pointer that has an
`on_wheel`, passing over any that have none, so a switch or a button on a
scrolling page does not swallow the page's scroll. A `Flickable` under the
pointer scrolls itself — `content_x`/`content_y` move by the pixel delta,
kept between zero and how far its children reach past its viewport — and
one with nothing to scroll that way lets the wheel on to what is beneath
it.

### Cursors

`cursor = "pointer"` on a `MouseArea` is the pointer's shape while it is
over the area, drawn by the compositor from its own theme. The names are
the cursor-shape protocol's: `default`, `pointer`, `text`, `grab`,
`grabbing`, `move`, `not_allowed`, `crosshair`, `ew_resize`, and the
rest, spelled with underscores.

### Drops and the clipboard

A `DropArea` is where a drag from another application can land. `keys`
lists what it takes, best first: exact types, `image/*`, `*`, or the
shorthands `text`, `image` and `files` (a `text/uri-list`); empty takes
anything. It is hit-tested like a `MouseArea` but separately from one, so a
button painted over it neither blocks a drag nor is blocked by it.

```lua
ui.DropArea {
  anchors = { fill = true },
  keys = { "files", "text" },
  on_entered = function(info) end,   -- info.mime_types, info.accepted (nil: refused), info.x, info.y
  on_moved = function(x, y) end,     -- local, then surface coordinates
  on_exited = function() end,        -- also after a drop, so a highlight clears in one place
  on_dropped = function(drop)        -- drop.uris, drop.paths, drop.text already fetched
    drop:read("image/png", function(bytes, err) end)  -- anything else, inside this handler
  end,
}
```

`morf.drag.start { text =, uris =, paths =, data = { [mime] = bytes } }`
from an `on_pressed` or `on_drag_started` handler drags out of the shell;
its optional second argument is told `true` if the drag was dropped.

`morf.clipboard.watch(function(offer) end)` hears every copy, whether or
not the shell has focus, through data control (`ext-data-control-v1`, or
`zwlr-data-control-v1`); pass `{ primary = true }` for middle-click
selections too. `offer.mime_types` says what is on offer and nothing is
read until `offer:read(mime, function(bytes, err) end)` — `mime` may be
`text` or `image` for the best of either. `morf.clipboard.set(data, mime)`
owns the clipboard (`{ mime =, primary = true }` for options; no type means
text), and `morf.clipboard.supported()` says whether data control is there
once the shell has connected. `examples/clipboard-history.lua` is all of
it together.

### Window size and closing

A popup or floating window is drawn at the size the compositor configures
it to, which is not always the size it asked for: a tiling compositor fills
a tile, a person drags an edge. Its root is laid out at that size, and
`win.width` / `win.height` read it — the asked-for size until the first
configure — so a binding follows every resize. `on_resize(width, height)`
hears the same change.

A compositor's close button or keybinding asks a floating window to close.
`on_close_requested` hears the request, and the window is hidden unless it
returns `false`. `on_closed` runs once a popup or floating window is off
screen, whatever took it there: a close request, `win:close()`, a hidden
parent, a dismissed popup. Each may be given in the constructor's table or
set later with the method of the same name; `nil` clears it.

Every surface also hears the keyboard and the pointer come and go:
`on_focus_changed(focused)` when the keyboard comes to it or leaves it (a
click on another window or surface takes it away from one with
`keyboard_focus = "on_demand"`), and `on_pointer_changed(inside)` when the
pointer comes over it or leaves it. Popups, floating windows and layer
surfaces (`morf.window.layer`) take them in the constructor's table or by
method; the shell's own surface takes them as `morf.surface.on_focus_changed`
and `morf.surface.on_pointer_changed` (assign `nil` to stop). An arrange
mode that should end on a click elsewhere:

```lua
morf.surface.keyboard_focus = "on_demand"
morf.surface.on_focus_changed = function(focused)
  if not focused then arranging:set(false) end
end
```

```lua
local root = ui.Item {}
local win = morf.window.floating {
  root = root, width = 900, height = 640, title = "Settings",
  on_resize = function(w, h) wide:set(w >= 1200) end,
}
win:on_close_requested(function()
  if dirty:get() then confirm:set(true); return false end
end)
ui.reparent(ui.Grid { columns = function() return win.width // 280 end }, root)
```

## 7. What makes a frame

- A property write that lands on a new value marks the surface dirty.
- Anything but transforms, opacity, colours, radii and blur also marks the
  *layout* dirty; the next paint re-lays the whole surface (a `Flex` or
  `Grid` subtree through Taffy, a `Layout` through your functions).
  Animate `translate_x` rather than `x` when only the picture moves.
- Bindings run on invalidation, never per frame. Animations run in Rust
  per frame and run no Lua but `on_finished`.
- A handler gets 100k Lua instructions; effects share a frame budget of
  1M. Exhaustion is logged, not fatal.
- A terminal's program writing is a frame only when it changed the screen,
  and the frame repaints the rows it changed.

### What wakes a shell

An idle shell sleeps until something happens, with no poll of its own: the
compositor sending an event, a service thread ringing the loop (a D-Bus
signal or reply, a child's output, an HTTP answer, a decoded image, a
command over IPC), or the first thing that comes due on the clock — a
`morf.timer` or running `ui.Timer`, a caret blink, the next frame of a
moving picture, a D-Bus call's `timeout_ms`, and the clock at the finest
grain a binding reads. Motion runs on the compositor's frame callbacks;
only when a surface gets none (a hidden window in a nested compositor)
does the wall clock tick it instead.

Three environment variables look inside a running output's loop, each
printed on stderr:

| Variable | Prints |
|---|---|
| `MORF_WAKE_LOG=1` | every wake and its cause: `compositor`, `wake fd` (a service thread), or `deadline: timer`, `caret`, `image`, `dbus-timeout`, `terminal`, `tray-retry`, `clock-seconds`, `clock-minutes`, `clock-hours`, `fallback`, `pending` (the last turn left work), with how long it slept |
| `MORF_FRAME_LOG=1` | every painted frame and what it cost |
| `MORF_SLOW_MS=N` | any stage of a turn that held the output longer than N ms (default 150) |

A shell that wakes more than it should says why under `MORF_WAKE_LOG`:
a `clock-seconds` every second is a binding reading `morf.clock` where
the minute clock would do, a `timer` is a timer still running.

## 8. Idioms to prefer

- Reach for a container before a coordinate: a `Row` with `align`, a
  `Flex` with `gap`, a `Grid` with tracks, a `Layout` of your own.
- Read `layout_width` instead of recomputing a parent's arithmetic.
- Keep a shape in one `morf.state`, change it in one place, and let
  `when` states and bindings do the reading.
- Keep lists in a model with a key, and let the `Repeater` follow it.
- Keep a palette in a `morf.theme` and derive from it; let
  `morf.prefers` choose the scheme.
- A secret stays a plain local. Signals are named and observable.
- Let processes and sockets call you: `morf.run`, `morf.spawn`,
  `morf.connect` and `morf.request_socket` (see [IO.md](IO.md)) deliver
  output as it arrives; a timer that polls them costs every idle second.
- Check before you run: `morf check shell.lua` lays out every surface --
  hidden ones too -- and names what failed and where; `morf test` drives
  it with clicks, keys and virtual time ([TESTING.md](TESTING.md)).
