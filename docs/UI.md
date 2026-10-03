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
re-run when a frame changes it: one that reads the size, when the node is
resized; one that reads the position, when it moves. (A label inside a
panel that slides or grows moves on every frame and keeps its size; a
binding on its size stays still.)

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

`library/lib/align.lua` is exactly that bar.

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

The outputs are `morf.screens`, index 1 the one this instance drives, a
table kept current in place. `morf.screens_revision()` is its tracked
read: a binding or an effect that calls it runs again when an output is
plugged or unplugged, moves, resizes, rescales or rotates -- and not when
the compositor merely repeats the same list -- so nothing has to compare
lists on a timer:

```lua
morf.effect("follow screens", function()
  morf.screens_revision()
  rebuild_for(#morf.screens)
end)
```

With no output at all -- every screen switched off, the dock unplugged, a
compositor that removes an output on DPMS -- there is nothing to draw on,
and by default nothing runs until an output comes back. A configuration
that has work to do then says so:

```lua
morf.surface.outputless = true
if #morf.screens == 0 then
  -- Only what makes sense with no screen: a timer, an IPC verb, a D-Bus
  -- service, a process -- here, lighting a screen again.
  morf.ipc["screen.on"] = function() morf.spawn { command = { "wlr-randr", "--output", "DP-1", "--on" } } end
  return
end
-- ... the shell as usual ...
```

While the compositor offers no output, the shell runs the file once more, in
one runtime of its own: `morf.screens` is empty, `morf.capabilities.outputless`
is `true`, and every surface, window and layer it declares stays unmapped
(no root is needed). Timers, `morf ipc`, D-Bus, file watches, processes, the
idle notifications, clipboard, output power and gamma all work, and
`morf.screens_revision()` moves as ever. When an output appears that
runtime is stopped and the per-output ones start from scratch, as on any
hotplug; values kept with `morf.reloadable(name, default)` go across in
both directions (a counter bumped with every screen off is where it was
when the screens come back), and anything longer-lived belongs in a file.
A shell started with every screen already off runs the file with no output
once to hear whether it asks for this; one that does not is stopped at once
and the shell waits for an output, as before. `morf check --screens 0` and
`test.load(path, { screens = 0 })` load a configuration this way.

Some duties belong to the shell, not to a screen: a bus name only one
connection can own (a notification server, a tray watcher), a history only
one writer should keep. Exactly one of the runtimes is the primary one, and
`morf.primary()` says whether it is this one -- a tracked read, so a
binding or an effect runs again when the duty moves -- and
`morf.on_primary(function(is_primary) end)` hears each change (not the
value it starts with, which `morf.primary()` already gives from the first
line):

```lua
local server
local function follow(primary)
  if primary and not server then server = start_notification_server() end
  if not primary and server then server.close() server = nil end
end
follow(morf.primary())
morf.on_primary(follow)
```

Which one, and when it moves:

- the output the compositor announced first is primary (the lowest
  `wl_output` global); with no output, the outputless runtime; a runtime
  alone in its process (`morf test`, `morf check`) always is;
- it stays primary while other outputs are plugged, unplugged, moved or
  rescaled, and across a reload -- the new runtime starts as primary;
- only when its own runtime ends -- its output unplugged or switched off,
  every output gone -- does the duty move, to the first-announced output
  still lit (or to the outputless runtime, or to the first output that
  comes back);
- a runtime gives back every bus name `morf.dbus.serve` took when it ends,
  and the next primary is told only after that: the name it asks for is
  free, `"owned"` rather than `"taken"`. A reload gives the old runtime's
  names back before the new file runs, so it finds them free too (a file
  that fails to load leaves the shell without them until the next reload).

Work the primary does for every screen hands its answer over with
`morf.shared(name, initial)`: a signal like `morf.signal`'s (`get`, `set`,
read by bindings), whose writes reach the signal of the same name in every
other runtime of the process on their next turn, waking them for it. A
runtime that asks for the name after a write -- a screen plugged in later,
the copy after a reload -- starts from that write rather than `initial`.
Values are what a signal holds: nil, booleans, numbers, strings, colours
and tables of them; a function cannot cross. Three screens then cost one
sample, not three:

```lua
local load = morf.shared("cpu.load", 0)
if morf.primary() then
  morf.timer(2000, function() load:set(read_load()) end, true)
end
ui.Text { text = function() return ("%d%%"):format(load:get()) end }
```

`lib.poll` sources take `shared = true` for this (`lib.sysinfo` uses it):
the primary samples, the others read its samples, and a read on any screen
keeps the primary sampling.

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

A function assigned to a property later is a binding too, made then:
`node.opacity = function() return shown:get() and 1 or 0 end` runs at
once and follows what it reads, and takes the place of any binding the
property had. A plain value written over a bound property is what it
shows until the binding next runs.

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

`library/lib/component.lua` gives Elm's shape on these pieces: `init(args)`
returns the model's table, `view(model, send)` runs once and returns a
tree of bindings on the model, and `update(model, msg, send)` is the one
place the model changes. `send(msg)` returns a handler; `send_with(fn)`
builds the message from the handler's arguments; `dispatch(msg)` delivers
one from code that is not a handler. A message that is a function runs
and its return is the message. See `examples/demos/desktop/polkit.lua`.

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
A write while the property is still moving goes on from where it is, over
the whole duration again, carrying its speed into the new path;
`retarget = "restart"` drops the speed and runs the whole easing curve
again from there, which is what Qt's `Behavior { NumberAnimation {} }`
does. On the display, an animation starts on the first frame after it was
asked for: a frame that came late -- the turn that asked also built a
panel -- is not charged to it, so its first frame is its first value
rather than a jump part of the way.
`morf.animation.fling` coasts a property.

An `easing` is a name (`"out_cubic"`), a cubic Bézier `{ x1, y1, x2, y2 }` (four numbers in order, or those named fields),
or a spline of several: `{ spline = { x1, y1, x2, y2, x, y, ... } }`, each
six numbers a segment's two control points and its end, from `(0, 0)` to a
last end of `(1, 1)` — Qt's `BezierSpline`. `x` is time and keeps moving
forwards; `y` is free, which is how a curve overshoots and settles in more
than one step. It is accepted wherever an easing is: behaviors, `enter`,
transitions, `morf.animation.play`, loops, `morf.easing.*`, and a theme
token holding one (`morf.theme { emphasized = { spline = { ... } } }`).

```lua
local settle = { spline = { 0.05, 0.7, 0.1, 1.04, 0.62, 1.03, 0.78, 1.01, 0.9, 1.0, 1, 1 } }
ui.Item { behavior = { translate_y = { duration = 460, easing = settle } } }
```

`stretch = { stiffness, damping, scale, max }` (or `stretch = true`) makes
any node squash and stretch with its own motion. The engine measures where
the node is drawn from frame to frame — whatever moves it: a behavior, a
spring, a parent, a layout change — and a damped spring pulls a deformation
towards one set by that velocity: `scale` longer along the motion per 1000
px/s (0.12), and narrower across it so the area stays, never more than
`max` (0.35). When the motion stops the spring overshoots and settles back
to square; `stiffness` (260) and `damping` (16) are the spring's, critical
damping being `2 * sqrt(stiffness)`. The deformation is a symmetric 2×2
matrix about the node's centre on its rendered transform, so the children
bend with it, a pointer is mapped through it, and an `SdfShape` tracking
the node (below) bends the same way. It is nothing per frame to set up and
nothing at all at rest: a spring that has settled asks for no frames.
`node.stretch = false` takes it away. A jump — a node that was somewhere
else a tenth of a second ago — is not speed and does not stretch.

`transform_matrix = { a, b, c, d, tx, ty }` (or just `{ a, b, c, d }`) on
any node is an affine map applied about `transform_origin`, inside
`scale`, `rotation` and `skew` (a stylesheet's `transform` read right to
left): `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`. Children and hit
testing follow it; it animates like any list of numbers.

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

### Loader

`ui.Loader { active = ..., source = function() return node end }` builds
its item by calling `source` when `active` turns true, and destroys it when
`active` turns false. Two properties change when that work happens:

- `keep = true`: let go, the item is hidden (its `visible` set false) rather
  than destroyed, and shown again as it was the next time `active` turns
  true. Its bindings keep running while it is hidden.
- `preload = true`: while `active` is false, the item is built ahead of
  time -- one Loader per turn, only while nothing animates (or once it has
  waited 1.5 s for that) -- and held hidden. Turning `active` true shows it
  without calling `source`. Without `keep` it is still destroyed when let
  go, and the next one is built ahead the same way, so each opening gets a
  fresh item and pays nothing for it. A `source` that fails while
  preloading turns `preload` off and is left to fail where it is asked for.

A preloaded or kept item's root `visible` belongs to the Loader. It is laid
out while hidden, so its text is shaped, and the renderer makes its glyphs
while the shell is idle: the frame that shows it has nothing left to do but
draw.

### Destruction

A node is destroyed when a `Loader` lets it go, when its `Repeater` row
leaves the model, when `ui.destroy(node)` is called, or when any of its
ancestors goes the same way -- after its `exit` animation, if it declared
one (see *Leaving*, section 6). `on_destroyed = function() end` on any node
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

`morf.color.hct(hue, chroma, tone)` makes a colour in HCT, the space
Material Design's schemes are built in: hue and chroma are CAM16's, tone
is L\* (0 black to 100 white), so two colours of the same tone have the
same contrast against a third whatever their hues. A chroma sRGB cannot
show at that hue and tone is given up and the hue and tone kept, which is
what makes "tone 40 of this hue" always a usable colour. `c:hct()` gives
back `hue, chroma, tone`; `c:with { t = 30 }` changes the tone alone, and
`{ h, c, t }` is a colour table like the others.
`morf.color.tonal_palette(hue, chroma)` — or `tonal_palette(colour)`, for
that colour's hue and chroma — is the colour at every tone: `p(40)`,
`p[90]` and `p:tone(99)` are colours, `p.hue` and `p.chroma` what it was
made from. The colour science is a port of Google's Material Color
Utilities (Apache-2.0) and gives its published values.

```lua
local primary = morf.color.tonal_palette(morf.color "#6750a4")
local theme = { primary = primary(40), on_primary = primary(100), container = primary(90) }
```

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
differs, text's antialiasing included. `examples/demos/sdf/blend-compare.lua` draws
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

### Frosted glass

`backdrop_blur` on a `Rect` or `ClipRect` takes a radius, in logical
pixels: whatever the same surface drew beneath the shape — a wallpaper
drawn inline, the panels under a popover — is blurred and drawn back
inside it, corners and all, with the rect's own `color` over it as the
tint. It needs nothing from the compositor, so it looks the same under
cage, GNOME or a compositor with no blur at all. `backdrop_saturation`
(1 by default) greys the backdrop towards 0 or deepens it above 1.

```lua
ui.Rect {
  radius = 22,
  color = morf.color("#1a1a1e"):alpha(0.55),  -- the tint over the blur
  backdrop_blur = 24,
  backdrop_saturation = 1.4,
}
```

The radius is a Gaussian's standard deviation, as in a stylesheet's
`blur()`, and stops at 96. The blur reads twice the radius past the edge,
so the rim pulls in what lies beside the glass rather than darkening. It
is done at half resolution and below, and kept: a panel blurs again only
when something drawn beneath it within that reach changes, so a desk whose
clocks tick on top of its panels blurs nothing after the first frame. A
change under the glass repaints the whole panel.

It only sees this surface. Over a separate wallpaper layer, or over other
windows, the shape has nothing beneath it to blur; for that, `backdrop_blur
= true` asks the compositor to blur behind the node instead, where it can
(`morf.capabilities.backdrop_blur` says whether it can). One node takes one or
the other; wrap it in an `Item` with `backdrop_blur = true` for both.

### Masks

`mask` on any node multiplies the alpha of everything the node and its
subtree draw by the alpha of something else at the same point: what Qt's
`MultiEffect` does with a `maskSource`, or an `OpacityMask`. It is one of
two things.

- **A node**, whose drawing is the mask: a rounded `Rect`, an `Sdf` of
  shapes, a `Text` or an `Icon`, or any subtree. It is moved under the
  node it masks and laid out in that node's box -- filling it when it asks
  for no size and no anchors, otherwise placed by its own `x`, `y`, size
  and anchors as a child of a plain `Item` is, whatever kind of container
  the owner is. It moves, scales and animates with the owner, but it is
  never drawn on its own and takes no input, and a positioner gives it no
  place; a `Flickable`'s mask does not scroll. Colour does not matter,
  only alpha: an opaque white rect keeps everything it covers. `mask =
  nil` (or a table) takes it away and removes it; a hidden mask
  (`visible = false`) masks nothing.
- **A table with a `gradient`**, the same gradient a `Rect` takes (see
  above), across the node's own box. Only the stops' alpha counts, and a
  stop may be a bare number, which is that alpha: `stops = { 0, { 1, 0.1
  }, { 1, 0.9 }, 0 }` fades in over the first tenth and out over the last.

`mask_invert = true` keeps what the mask does not cover and cuts out what
it does.

```lua
-- A list that fades out at its top and bottom edges, whatever it scrolls.
ui.Flickable {
  width = 320, height = 400,
  mask = { gradient = { stops = { 0, { 1, 0.08 }, { 1, 0.92 }, 0 } } },
  list,
}

-- An avatar cut to a circle, and a badge punched out of it.
ui.Image {
  source = avatar, width = 64, height = 64,
  mask = ui.Sdf {
    ui.SdfShape { shape = "circle", anchors = { fill = true } },
    ui.SdfShape { shape = "circle", x = 44, y = 44, width = 24, height = 24,
                  operation = "subtract" },
  },
}

-- Text as a stencil over a gradient.
ui.Rect {
  width = 300, height = 80,
  gradient = { angle = 90, stops = { "#ff5f6d", "#ffc371" } },
  mask = ui.Text { text = "morf", font_size = 64, anchors = { center_in = true } },
}
```

The masked subtree is drawn into an offscreen layer, and the mask into a
second one covering exactly the same pixels; both are sized to what the
frame's damage reads of the node, never the whole surface, and share their
atlases and passes with the frame's other layers. A node with no mask
costs nothing. The mask composes with the node's `opacity`, transforms,
rounded clip and `layer` settings: a `layer.shadow_color` or an effect
shader on a masked node is masked with it. Animating the mask repaints
where it changed; toggling `mask_invert` repaints the node.

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

`options.transition = { duration = ms, easing = ... }` eases a colour
written to a token there from the one on show (in OkLab), frame by frame,
instead of jumping: every reader follows the whole way, so a new scheme
cross-fades the shell at once. Written again mid-way, it sets out from
where it is. Other tokens change at once.

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

A variable font's axes are `axes = { FILL = 1, GRAD = 0, opsz = 24, wght =
500 }`: any four-letter OpenType tag the face defines, in its own units, on
`Text` and `TextInput`. It is a map of numbers, so a `behavior` on `axes`
moves every axis in it at once, the way Material Symbols fills an icon in
when it is selected; give each state the same keys, or the map jumps
rather than moves. `wght` is the weight (it wins over `font_weight`). Every
axis is shaped as well as drawn, so one that changes advances -- `wdth`, or
`opsz` on a face like Google Sans Flex or Roboto Flex -- changes the width
the text measures, and what is measured is what is drawn; an axis that
animates re-lays the text out, and its parent with it. A tag the face does
not have is ignored, and a face with none of them is drawn as it always
was. Glyphs are shaped and drawn per point of the design space, quantised
to 1/64 of the way from an axis's default to either end, so an animation
through an axis costs at most 65 pictures (and layouts) of each glyph
however many frames it takes. `morf.font_axes(family)` lists what an
installed family can move: `{ tag, min, default, max }` for each axis.

Optical sizing is automatic, as CSS's `font-optical-sizing: auto`: a face
with an `opsz` axis is set at `opsz` equal to the font size in pixels (a
run of `spans` with a size of its own at that size), so small labels get
the face's wider, looser small-size design and large ones its tighter
display cut. `axes = { opsz = ... }` names a size of its own;
`optical_sizing = "none"` (or `false`) keeps the face's default.

```lua
ui.Text {
  text = "home", font_family = "Material Symbols Rounded", font_size = 24,
  axes = function() return selected() and { FILL = 1, wght = 600 } or { FILL = 0, wght = 400 } end,
  behavior = { axes = { duration = 250, easing = "out_cubic" } },
}
```

See `examples/demos/text/font_axes.lua`.

Text is smoothed in subpixels (LCD, "ClearType") where that is safe, and
in greyscale everywhere else. `morf.surface.subpixel_text` is `"auto"` by
default: the stripe order comes from fontconfig's `rgba` (else the
output's `wl_output.subpixel`), the fringe softening from its `lcdfilter`,
and `"none"` or vertical stripes there mean greyscale. `"off"` turns it
off; `"rgb"` or `"bgr"` name the order outright. Even then a glyph is drawn
in subpixels only when all of these hold, because its fringes need a solid
colour beneath them to mix with:

- it is drawn onto an opaque `ui.Rect` (solid fill, no gradient, blur or
  shader; inside its rounded corners and a translucent border) of the
  same surface -- or the surface is `morf.surface.opaque` -- and, inside a
  rounded `ui.ClipRect` or another offscreen layer, of that same layer:
  the panel's own opaque fill counts, the surface beneath it does not;
- no layer it is in is translucent (an `opacity` below one, so also
  while one fades), blurred, or wears an effect shader, and it stays
  clear of the rounded corners that clip it;
- it is only moved, not scaled, rotated or skewed; it is not mid-morph and
  has no outline;
- the surface is drawn at a whole-number scale on an output that is not
  rotated or flipped, and the GPU can blend two colours per pixel
  (dual-source blending; `MORF_NO_DUAL_SOURCE=1` pretends it cannot).

A translucent card, a panel fading in, text over a picture: greyscale. The
same label on a solid background or a solid rounded panel: sharper, in
colour fringes a third of a pixel wide. Nothing about the text itself
changes -- its size, its metrics, where it wraps.

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

### Fuzzy matching

`morf.text.fuzzy(query, items, opts)` ranks a list the way a launcher or a
picker wants it: items whose characters contain the query's, in order,
best first. `items` are strings, or tables with `opts.key` naming the
field to match — or a list of fields, each `"field"` or `{ "field",
weight }`, where an item scores its best weighted field. `opts.limit`
keeps the best few. Each result is `{ item, index, score, key, positions
}`: the item itself, its index in `items`, the score, the field that
matched, and the 1-based byte offsets (as `string.sub` counts) of the
matched characters.

```lua
local hits = morf.text.fuzzy(query:get(), apps, { key = { "name", { "exec", 0.5 } }, limit = 30 })
for _, hit in ipairs(hits) do
  ui.Text { spans = morf.text.highlight(hit.item.name, hit.positions, { bold = true, color = theme.accent }) }
end
local score, positions = morf.text.fuzzy_score("ffx", "Firefox")   -- nil when it does not match
```

The scoring is fzf's kind: a match at the start of a word, a camelCase
hump or a path component is worth more than one in the middle of a word,
a run of consecutive matches keeps the bonus its first character earned,
skipped characters cost a little, and the first character's bonus counts
twice, so a prefix wins. It is case-insensitive unless the query has an
uppercase letter. Spaces split the query into terms that must all match.
It works on characters, so accents and CJK match as whole characters; it
does not fold accents (`e` does not find `é`). Ties go to the shorter
text, then to the earlier item; an empty query keeps every item in order.
Ranking 10,000 entries of seventy characters takes a few milliseconds, so it
can run on every keystroke. `morf.text.highlight(text, positions, style)`
turns a match into `spans` for a `ui.Text`: the matched characters in
runs carrying `style` (bold when none is given), the rest plain.

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
does: a click, a Tab or Shift+Tab, or writing `focus = true` moves it, and
`on_focus_changed(focused)` says so (see [Focus](#focus)). A field -- or any node with
`on_key_pressed` -- that sets `tab_navigation = false` keeps Tab while it
has the keyboard: Tab and Shift+Tab go to its `on_key_pressed` (for a
completion, say) instead of moving focus. While it has it, the compositor's
input method (text-input-v3) is enabled for it and what the input method
commits is typed into the field.

`highlights` colours runs of what is typed, as a code editor's syntax
does: `{ start, stop, color, underline, strike }` tables over byte
offsets (`text:find` gives them), in any order, overlaps cut back to the
earlier. Only what leaves every glyph where it was may be set, so the
caret, the selection and a click land where they would without it; a
wavy error is `underline = true` in the error's colour. They show over
typed text, never over the placeholder or a password.

```lua
editor.highlights = function()
  local marks = {}
  for start, word, stop in editor.text:gmatch("()(%a+)()") do
    if KEYWORDS[word] then marks[#marks + 1] = { start = start - 1, stop = stop - 1, color = theme.keyword } end
  end
  return marks
end
```

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
`on_key_pressed(keysym, text, modifiers, repeat, name)` returns `true`, which
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
reload ends them all. `examples/demos/text/terminal.lua` runs btop in a panel;
`examples/demos/text/fzf_launcher.lua` is an application launcher that is fzf.

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
  pixels and returns their source. `bytes` is a string (what `morf.dbus`
  gives for an `ay`) or a list of byte values; `stride` is the bytes from
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
screen. `examples/demos/sdf/path.lua` has one of each.

`morf.geometry` builds reusable path data in Rust. `shape_path(name,
{size=100, segments=72})` supplies the named expressive shapes;
`shape_curves(shape, segments)` returns normalized cubic coordinates.
`polygon(vertices, opts)`, `regular(sides, opts)`, `star(points, inner, opts)`
and `lobes(count, inner, opts)` build custom outlines. `lib.m3shapes` wraps these
operations with the existing `Shape` node and morph animation controls.

`graph_series(values, {width, height, samples, bottom=0, top, closed=false})`
draws a bounded history with its newest sample at the right edge; `closed=true`
adds the fill under it. `graph_grid(width, height, columns, rows)` builds the
grid. Sampling intervals, history ownership, colors and UI structure remain
configuration choices; only numeric path generation moves into the engine.

The marks a style draws with, as path data (angles in degrees, clockwise
from twelve o'clock):

| function | |
|----------|---|
| `arc(cx, cy, r, from, sweep)` | an arc in pieces of at most 90°, so a full turn too |
| `sector(cx, cy, r0, r1, from, sweep)` | a ring's slice, closed; a pie's when `r0` is 0 |
| `hatch(width, height, gap = 6)` | `/` stripes across a box, cut to it |
| `hatch_under(x0, dx, ys, width, height, gap = 6)` | the stripes under a stepped series, step `i` at height `ys[i]` |
| `ticks(cx, cy, r0, r1, { from, sweep, count \| angles, major, major_r0 })` | radial ticks; every `major`-th from `major_r0` |
| `ruler(length, size, { pitch = 8, major = 5, minor = size / 2, min_count = 4, vertical })` | a tick ruler along an edge |
| `segments(width, height, count, gap = 2, { vertical })` | a segmented bar's cells |
| `plot(values, plot)` | what a path reading a channel with that `plot` draws (below) |

### Data channels

A chart that changes every frame should not build its outline in Lua. A
data channel is a run of numbers a producer writes and a `ui.Path` draws:

```lua
local load = morf.channel { size = 60 }                 -- a ring: the newest 60 pushed
local bars = morf.channel { size = 56, mode = "frame" } -- a frame: replaced whole
load:push(0.4)        bars:set({ 0.1, 0.8, ... })
ui.Path { width = 300, height = 80, view_box = { 0, 0, 300, 80 },
  series = load.id, plot = { kind = "area", smooth = true }, fill_color = accent }
```

`ch:get()`, `ch:last()`, `ch:peak()` and `ch:len()` read it, and read
reactively: a binding that called one runs again when the channel is
written, by Lua or by a Rust producer. A path with `series` makes its
outline from the channel's numbers where it is painted (`d` is not read),
and the loop repaints for a write only when such a path is on show.

`plot` says how: `kind` -- `"line"`, `"area"`, `"steps"`, `"steps_area"`,
`"hatch_steps"` (the stripes under the steps, `hatch` apart) or `"bars"`;
`width`, `height` (the view box's by default); `samples` across the width
(a ring's size by default), newest at the right edge; `bottom`, `top`
(a frame defaults to 0..1, a ring to its peak times `headroom`, never
under `bottom + floor`); `pad_top`, `pad_bottom`; `smooth` (a curve
through the samples); and for bars `gap`, `radius` (the most their ends
round), `min_bar` (the least one stands, no more than its width) and
`mirror` (grown from the middle). `with = other.id` puts another channel
on the same automatic scale.

More kinds read a channel's numbers in layouts of their own, and a chart
with several tones draws one path per tone over the same channel:
`"cells"` (column by column, `rows` to a column; the places in `[lo, hi)`
-- one tone of a heatmap, a spectrogram, a calendar), `"scatter"` (`x, y`
pairs across `left..right`, dots of radius `point`), `"candles"` (`open,
high, low, close` fours; `direction = "up" | "down"`), `"boxes"` (`min,
q1, median, q3, max` fives), `"stack"` and `"stack_bars"` (`layers`
interleaved series, this path layer `layer`; `smooth` curves a stack),
`"histogram"` (raw values into `bins`), `"radial"` (a band per value,
from `inner` out, `sweep` degrees from `start`; `arcs = true` draws each
band's middle as an open arc to stroke with round caps), `"states"` (the
runs of samples in state `state`, a timeline's colour) and `"state_cells"`
(status history squares), and `"wave"` (an amplitude envelope).

A table property holding bindings among its fields -- `accessible`,
`plot`, `view_box` -- is bound as a whole: `plot = { kind = "bars", top =
function() return peak:get() end }`.

`lib.channel.from(values)` gives a channel for a channel, a list or a
function returning one -- copied in by an effect owned by the drawing
node -- so a component can draw whatever it is handed through a channel.
`morf.audio.monitor { channel = bars, spectrum = { bars = 56 } }` writes
filtered bars with no Lua per frame, and `lib.sysinfo.channel(name)` is
a history as a channel.

### Fields

A `ui.Sdf` is one surface composed from the shapes beneath it — every
`SdfShape`, and every `Rect` however deeply the positioners nest — resolved
per pixel as distance fields, so shapes union, subtract and morph into one
another and a seam between two can be smooth. Each `SdfShape` says
`shape` (`circle`, `box`, `capsule`, `star`, `ring`, …), `operation`
(`union`, `subtract`, `intersect`, `smooth_union`, `smooth_subtract`,
`smooth_intersect`, `xor`) and `blend` (the seam's radius; the field's own
`blend` when it names none).

- `blend_profile` on the `Sdf`: `"quadratic"` (the default) is a soft
  polynomial seam that also swells where two shapes merely pass close;
  `"circular"` makes the seam a true arc of the blend radius tangent to
  both shapes and leaves them exact everywhere else — where an edge meets
  another square on, a quarter-circle fillet. Both work for smooth unions
  and smooth subtractions.
- `blend_group` on an `SdfShape`: two layers in different non-zero groups
  meet with a hard edge whatever their operation; group 0 (the default)
  blends with everything. Two panels in groups 1 and 2 each fillet into a
  frame in group 0 and do not bridge to one another.
- `matrix = { a, b, c, d }` on an `SdfShape`: a linear map the shape is
  drawn through about its centre, after `rotation`. The distance is scaled
  so the edge stays one pixel soft however the map stretches it.
- `track = node` on an `SdfShape`: the layer's rectangle is wherever that
  node is drawn this frame — its layout box through every transform above
  it and its own, animated values and a `stretch` included — worked out by
  the renderer as it paints, with nothing running in Lua. A scale along the
  axes becomes the layer's size, so a stretched box keeps round corners; a
  turn or a shear rides in the layer's matrix. A hidden node takes its layer
  with it. `shape.track = nil` lets go.
- `opacity` on an `SdfShape` (or a `Rect` in a field, or any node between
  a layer and its field, multiplied down) fades that layer and nothing else.
  The field is mixed between the composition without the layer and the one
  with it, weighted by the opacity: at 0 it is as if the layer were not
  there, at 1 it is whole, and in between the layer's shape, the seam it
  makes with the others and its colour fade together, while the rest of the
  field is untouched. A fading `subtract` half fills its hole; a drawer's
  background filleted into a frame fades in with its contents and the frame
  stays as it is. It animates like any number (`behavior`, `enter`,
  `morf.animation`), and a frame of the fade repaints only where the layer
  reaches. Several layers fading at once are each there or not
  independently, every combination weighted; a field draws up to three
  fading at the same time exactly (a fourth is drawn whole until it
  settles), and a fading layer costs its field's pixels one composition per
  combination while it fades, none once it has settled. The `Sdf`'s own
  `opacity` still fades the whole field as one picture.

A field whose layers alone moved repaints only where those layers were and
are, widened by the seam: a panel sliding in a fullscreen frame costs the
panel, not the screen.

A drawer growing out of a frame round the screen is all of these: the
frame is the screen minus a rounded inner box, the drawer's background is
a box tracking the drawer, joined by a circular seam, and the drawer
slides and stretches:

```lua
local THICK, SEAM = 10, 18
local panel = ui.Item {
  anchors = { top = true, horizontal_center = true }, width = 420, height = 150,
  translate_y = function() return opened:get() and 0 or -(150 + THICK + SEAM + 2) end,
  behavior = { translate_y = { duration = 460, easing = "out_back" } },
  stretch = { stiffness = 240, damping = 13, scale = 0.16 },
  ui.Text { x = 22, y = 18, text = "hello", color = "#e6e1f0" },
}
ui.Item {
  anchors = { fill = true },
  ui.Sdf {
    anchors = { fill = true }, fill_color = "#1c1b22",
    blend = SEAM, blend_profile = "circular",
    ui.SdfShape { shape = "box", anchors = { fill = true } },
    ui.SdfShape { shape = "box", anchors = { fill = true, margins = THICK }, radius = 22, operation = "subtract" },
    ui.SdfShape { shape = "box", radius = 18, operation = "smooth_union", blend_group = 1, track = panel },
  },
  ui.Item { anchors = { fill = true, margins = THICK }, clip = true, panel },
}
```

Tuck a closed drawer further out than the seam (`size + THICK + SEAM`),
or the fillet of its far edge still dimples the frame. `examples/demos/motion/drawers.lua`
puts one on every edge, opened over IPC.

### Keys

A node with `on_key_pressed` or `on_key_released` is somewhere keys can
go: the focused one of its surface (a click, a Tab, `focus = true`), or
else the first. `on_key_pressed(keysym, text, modifiers, repeat, name)`
runs for a press and for each of the keyboard's repeats of a held key, and
`repeat` says which it is; `on_key_released(keysym, text, modifiers, nil,
name)` runs when the key comes up, on whatever has focus by then. `name`
is the key's X name (`"Down"`, `"Return"`, `"Escape"`, `"BackSpace"`,
`"Page_Up"`, `"F5"`, `"space"`, a character for a printable key), so a
handler compares names rather than numbers; `morf.keys` has every named
key's keysym (`keysym == morf.keys.Down`). Something that
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

### Focus

Each surface has at most one focused node. `focused` says which, and
`visual_focus` says a keyboard put it there: a theme draws its focus ring
from `visual_focus`, so the ring follows Tab and never a click. Both are
the runtime's to write; a configuration moves focus with
`morf.focus.set(node, keyboard)` (true draws the ring), `morf.focus.clear(node)`,
`morf.focus.next()` and `morf.focus.previous()`, and asks with
`morf.focus.get()`. Those settle at the next turn of the loop. Writing
`focus = true` on a node asks for focus as well, and `false` gives it back,
which is how a panel that opens says it wants the keys.

`focus_policy` says how a node takes focus:

| policy | Tab stops at it | a click focuses it |
|---|---|---|
| `"auto"` (default) | when it takes keys | when it takes keys |
| `"tab"` | yes | no |
| `"click"` | no | yes |
| `"strong"` | yes | yes |
| `"none"` | no | no |

A node takes keys when it has `on_key_pressed` or `on_key_released`, or is
a text input or a terminal. A button made focusable by a theme
(`kit.focusable`) says `"tab"`: Tab reaches it, and a click on it leaves a
search field typing. Tab walks the surface's tree in order and Shift+Tab
walks back, skipping any subtree that is hidden, disabled or leaving; a
click focuses the nearest node, itself or an ancestor, whose policy takes
a click, and a click on nothing that does leaves focus where it was.

A key goes to the focused node. When that node takes no keys of its own --
a button -- Return, the keypad's Enter and Space click it, and any other
key goes up to the nearest ancestor that takes keys, so a launcher's
arrows reach the launcher while one of its rows has focus.

`focus_scope = true` makes a group that remembers which of its nodes last
had focus. Tab into the group lands there, and Tab out of it goes to what
is beside the group rather than through the rest of it. When the focused
node is removed, hidden or disabled, focus goes to what its nearest
surviving scope remembers, or to that scope's first node, and to nothing
when it was in no scope. When the keyboard leaves a surface its node stops
showing focus (`focused` goes false) and shows it again when the keyboard
comes back; `on_focus_changed(focused)` runs on a node as it gains and
loses focus.

### Shortcuts

`shortcuts` on any node maps key sequences to functions:

```lua
ui.Item {
  shortcuts = {
    ["ctrl+b"] = toggle_bold,
    ["ctrl+k ctrl+s"] = save_all,      -- a sequence: chords apart by spaces
    ["back"] = go_back,                -- a mouse's back button, or XF86Back
  },
  ...
}
```

A key goes to shortcuts before it goes to the node with focus: first those
on the focused node and its ancestors, nearest first, then those of any
shown node on the surface whose table says `scope = "surface"`. A
shortcut's function is called with its sequence (`"ctrl+b"`); returning
`false` passes the key on as though nothing had matched. A chord is
modifiers and a key joined by `+`: `ctrl`, `shift`, `alt`, `super`, and any
name `morf.keys` knows or a single character (`"ctrl+,"`). Shift folds
letters, so `"ctrl+shift+k"` matches however the keyboard reports the
capital. A key that begins a longer sequence is held for a second and a
half for the next; a key that breaks the sequence then goes on alone.

A modifier alone -- `"alt"`, `"super"`, `"ctrl"`, `"shift"` -- names its
tap: the key pressed and let go with nothing between, no other key and
no click. It reaches the shortcut even from a text field, as a menu bar's
Alt does everywhere; Alt held with a letter is still that letter's chord
and no tap.

Typing comes first. While a text input has focus its plain keys (no Ctrl,
Alt or Super) and its editing chords (Ctrl with A, C, X, V, Z, Y, or with a
key that moves or deletes) never reach shortcuts; a terminal keeps every
key but Super-chords. Function and media keys still do. A mouse's side
buttons are the keys `back` and `forward` on the surface under the
pointer, so Alt+Left and the back button can share a function.

### Gestures

Gestures are events like any other, on any node that takes the pointer:

| handler | when |
|---|---|
| `on_double_clicked(x, y, local_x, local_y)` | a second click within 400 ms and 8 px of the first, after that click's `on_clicked` |
| `on_long_pressed(x, y, local_x, local_y)` | a press held within 8 px for half a second; the click its release would make is not delivered |
| `on_swiped(direction, velocity_x, velocity_y)` | a press moved over 24 px and let go faster than 400 px/s; `direction` is `left`, `right`, `up` or `down` |
| `on_pinched(scale, phase, x, y)` | two fingers spreading or closing: `scale` against their first spread, `phase` `update` then `end`, `(x, y)` their midpoint |
| `on_edge_swiped(edge)` | on a surface's root: a finger landing within 20 px of an edge and moving 48 px in |

Two fingers moving together where nothing takes a pinch scroll what lies
under them, as a touchpad does. A finger is the left button throughout: it
presses, drags and clicks as the pointer does, so a swipe and a long press
work under a finger and a mouse alike.

### Overlays

`morf.overlay.open(content, options)` shows `content` over everything else
on its surface: in the surface's overlay layer, the root's last child.
What opens there stacks, newest on top, and Escape and a press outside
close the top one first.

```lua
local menu = build_menu()            -- any node
morf.overlay.open(menu, {
  anchor = button,                   -- beside this node
  placement = "bottom-start",        -- top|bottom|left|right|center[-start|-end]
  on_close = function(reason) end,   -- "escape", "outside", "closed", "gone"
})
morf.overlay.close(menu)
morf.overlay.is_open(menu)
```

An anchored overlay sits `gap` px (4) from its anchor by `placement`,
flips to the other side when its own has no room, and shifts along to stay
`margin` px (8) inside the surface; with no anchor (give `root` then) it is
centred. `dim = true` (or a colour) lays a scrim over the surface under it,
and `modal` -- true when it dims -- keeps what is under it from taking
input; Tab then walks only the overlay. `escape = false` and
`outside = false` keep it open on those; a press on the anchor never counts
as outside, so a button that toggles its menu works. Focus moves to the
first node in it Tab would reach as it opens (`focus = false` leaves focus
where it is; focus already inside it, put there by what opened it, stays),
with the ring when the focus it took over had one, and goes back to the
node that had it -- the control that opened it -- when it closes
(`restore = false` leaves it alone). Closing hides the content in the
layer; opening it again shows it there, and destroying it closes it.

A kit popup (`lib.kit.popup`) closes at once -- focus back, what is under
it live again -- and then plays its theme's exit: it stays in the layer
for the theme's `linger` (`popup_motion`'s) as a ghost that takes no input
and gives no focus back, and is taken back if it opens again meanwhile.

### Right to left

`layout_direction = "rtl"` (or `"ltr"`) on any node sets the direction of
its subtree; `""`, the default, takes the parent's, and the root takes the
locale's (`LC_ALL`, `LC_MESSAGES`, `LANG`: Arabic, Hebrew, Persian, Urdu,
... write right to left), or `MORF_DIRECTION` for a run. A right-to-left
subtree mirrors what is placed against its parent's sides: rows, columns
and grids pack from the right, an inset swaps its margins, a flex row runs
right to left, a child anchored `left` is anchored right, and a text's
`left` or `right` alignment swaps so it keeps to its start. A child placed
by its own `x` keeps it -- a drawing positioned by number is the
drawing's -- so a layout meant to mirror is anchored to its start
(`anchors = { left = true, left_margin = 12 }`) rather than placed at
`x = 12`. `node.effective_direction` reads `"ltr"` or `"rtl"`. Every kit
control reads it into its archetype's `mirrored`: the arrow keys turn
round, a slider's `visual_position` runs from the right, a switch's thumb
starts on the right, and the Shell's sidebar stands on the right.

### Applications

`morf app app.lua` runs a configuration as an application rather than a
shell: one runtime whatever the outputs, its own surface shrunk to nothing
on the background layer, its windows the interface, and the process ended
when the main window closes. `lib.kit.app` is the side of it a
configuration writes:

```lua
local app = require("lib.kit.app")
local kit = app.kit()          -- the configuration's `kit` module, else the default look
app.application {
  title = "Settings", app_id = "dev.morf.Settings", width = 980, height = 660,
  minimum_width = 340, minimum_height = 480,
  build = function(win) return root end,   -- laid out at win.width x win.height
}
```

The window decorates itself: edges that resize (`start_system_resize`) and,
from `lib.kit.composites.header_bar { window = win, title, subtitle,
start, ["end"] }`, a bar whose empty part moves the window and whose
controls minimise, maximise and close it. `lib.kit.shell.make` (the Shell
archetype) arranges the header bar, sidebar, content, inspector, bottom
bar, banner and toasts and adapts at its `breakpoints`: under the first
the sidebar becomes a drawer (F9 or Ctrl+B opens it, Escape or a press
outside shuts it), under the last the inspector hides; F6 walks the
regions, which are landmarks to a screen reader. `app.is_app()` says
whether `morf app` is running it. `examples/apps/settings/app.lua` is a
complete one.

### The default look

`lib.kit.skins.default` is a whole kit -- every contract function, a skin
for every archetype, the display widgets and instruments -- in an
Adwaita-like look, so an application needs no theme: `make { variant =
"dark" | "light" | "high_contrast", reduced_motion }`, following the
desktop's colour scheme, contrast and motion preferences when not told
(`MORF_KIT_VARIANT` overrides for a run). `morf check --kit default`
checks it against the contract.

### Accessibility

A screen reader is told a tree of the shown nodes that mean something:
every node with an `accessible_role`, every `Text` with text (a
`"label"`), every `TextInput` (a `"text_field"`), and every `MouseArea`
Tab reaches (a `"button"` unless it says otherwise). The boxes between
them are left out, and so is a subtree under `accessible_hidden = true`.

```lua
ui.MouseArea { accessible_role = "switch", accessible_name = "Wi-Fi",
  accessible_description = "Off while flying",
  accessible = { checked = function() return on:get() end },
  on_accessible_action = function(action, value) ... end,
  ... }
```

| property | |
|----------|---|
| `accessible_role` | what the node is: `button`, `toggle_button`, `check_box`, `radio_button`, `switch`, `link`, `menu_item`, `slider`, `spin_button`, `progress`, `tab_list`, `tab`, `tab_panel`, `list_box`, `list_box_option`, `list`, `list_item`, `grid`, `grid_cell`, `tree`, `tree_item`, `table`, `row`, `cell`, `text_field`, `password_text`, `search_field`, `dialog`, `alert_dialog`, `alert`, `status`, `tooltip`, `menu`, `group`, `label`, `heading`, `image`, and the landmarks `navigation`, `main`, `complementary`, `region`, `banner`, `search`, `log`, ... (`morf_scene::ACCESSIBLE_ROLES` lists them all) |
| `accessible_name` | what it is called; a button, tab, label or list item with none is named by the text under it, a field by its `placeholder` |
| `accessible_description` | a longer word on it |
| `accessible` | a table of the rest: `value` (a number or text), `minimum`, `maximum`, `step`, `checked` (`true`, `false`, `"mixed"`), `expanded`, `selected`, `disabled`, `pressed`, `read_only`, `modal`, `orientation`, `placeholder`, `level` |
| `accessible_hidden` | leaves the node and everything under it out |

A control's children are presentational: a button, a slider or a field is
read as one thing, and what is under it is not offered separately. A
range's text is its reading, so a slider is never named by it: name it.

What a screen reader asks -- `"click"`, `"focus"`, `"increment"`,
`"decrement"`, `"expand"`, `"collapse"`, `"set_value"` (with the value) --
goes to the node's `on_accessible_action(action, value)` first; returning
anything but `false` ends it. Otherwise it is the key a keyboard user
would press, with focus given to the node: Space, Up, Down, Right, Left.
Every kit control (`lib.kit.control`) sets its role from its archetype and
widget, its name from its `label`, `title` or `placeholder` (or the
`accessible_name` given), its states from its live state, and takes a
slider's or a field's value directly.

On Linux the tree goes to AT-SPI through AccessKit, one window per surface
(the `a11y` feature of `morf`, on by default). Nothing is built until a
screen reader asks: an adapter waits on the accessibility bus, and only
once it is wanted does each turn of the loop rebuild the tree -- when the
scene has changed, or at most four times a second for layout alone -- and
send the nodes that changed. `MORF_NO_A11Y=1` turns it off for a run.

`except = { node, ... }` names more nodes a press on which is not outside
-- the other controls that open it. `morf.overlay.track(node, options)`
gives a node the layer's behaviour where it already stands -- a drawer, a
panel a layout placed itself: the stack, Escape, focus in and back, and a
press anywhere else on its surface closing it, for which the engine puts a
catcher behind everything on that surface while it is open. A press is
outside by where it lands, not by what it hits: empty space inside the
node is inside. Its `on_close(reason)` shuts it; `morf.overlay.close(node)`
ends the tracking. `lib.kit.popup` builds menus, dialogs, tooltips and
toasts on both, and `popup.track` is how a shell's drawers become popups.

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

`enter` may time itself instead: with a `duration` (and an `easing` and a
`delay`, as a behavior has) every property it names travels to its
declared value on that timing, whatever the node's behaviors say.

### Leaving

`exit = { opacity = 0, scale = 0.9, y = 10, duration = 200, easing =
"in_cubic" }` on any node is how it leaves. When whatever holds it lets go
-- a `Loader` turning inactive, its `Repeater` row removed from the model,
`ui.destroy(node)` -- the node is not removed. It stays in the tree and is
drawn as it animates to those values, and only when the animation ends is
it removed, `on_destroyed` hooks and all. While it leaves it is out of the
layout: its parent is sized and packed without it, so what it was pushing
closes up at once, and it keeps the box it had relative to its parent
(moved by whatever the exit does to its `x` and `y`, so `y = 10` drops it
ten pixels from where it sat in a column). It takes no input. `duration`
is 200 when not given; `delay` waits first; any property that animates can
be named, the rest are simply set as it starts to leave.

Put back before it has gone -- the `Loader` turns active again, the same
row (an equal value) returns to the model, `ui.reparent` puts it
somewhere -- the node is taken back rather than built anew: it rejoins
the layout and every property its exit moved animates back to where it
was aimed, on the exit's timing. `ui.destroy(node, true)` removes at once,
exit or not. A node held by a `core.retainable` lock stays until both the
lock and its exit let go. A reload drops the old runtime, exits and all.

```lua
ui.Repeater {
  model = notifications,
  delegate = function(n)
    return ui.Rect {
      width = 300, height = 64, radius = 14,
      enter = { opacity = 0, translate_x = 40, duration = 260, easing = "out_cubic" },
      exit = { opacity = 0, scale = 0.9, y = 10, duration = 240, easing = "in_cubic" },
      ui.Text { x = 16, y = 12, text = n.title },
    }
  end,
}
```

See `examples/demos/motion/exit.lua`.

### Hover and press

`on_pressed(surface_x, surface_y, local_x, local_y, button, modifiers)`
and `on_released` and `on_clicked` the same: `button` is `left`, `right`,
`middle`, `back` or `forward` (nil from a touch), `modifiers` what the
keyboard holds, `"shift"`, `"ctrl+alt"` or `""`. `on_dragged`,
`on_drag_started` and `on_position_changed` (the pointer moving over the
area) are told `(surface_x, surface_y, delta_x, delta_y, local_x,
local_y, modifiers)`. A Shift-click extends a selection, a Ctrl-click
toggles one, a Shift-drag holds to an axis: the modifiers come with the
event, as they were when it happened. Tests press with them:
`test.click(x, y, { modifiers = "shift" })`, and `test.press`,
`test.drag` and `test.wheel` take the same option.

A `MouseArea` keeps `hovered` (the pointer is over it, and it is the
topmost area there) and `pressed` (a button or a touch went down on it and
has not come up), both read-only. A binding follows them like any other
property, so hover needs no signal and no `on_entered`:

```lua
local area = ui.MouseArea { anchors = { fill = true } }
ui.Rect { color = function() return area.pressed and "#444" or area.hovered and "#333" or "#222" end }
```

Every node, of any kind, has `contains_pointer`, also read-only: the
pointer is inside the node's box, whatever is drawn over it. `hovered` is
one area at a time; `contains_pointer` is "the pointer is somewhere on this
panel", buttons and all, which is what a panel that shuts when the pointer
leaves it asks (Qt's `HoverHandler`, or `containsMouse` with propagation;
a `MouseArea`'s own `contains_pointer` is the hover that looks through
what is above it).

```lua
local panel = ui.Rect { width = 400, height = 300,
  ui.MouseArea { anchors = { fill = true } },   -- takes the pointer here
  ui.MouseArea { x = 20, y = 20, width = 80, height = 32 },
}
morf.effect("panel.leave", function()
  if not panel.contains_pointer then close() end
end)
```

- The box is the node's laid-out rectangle taken through every transform
  above it and its own, animated values and a `stretch` included, and cut
  by every ancestor that clips. A hidden or leaving node, and a node used
  as a mask, contains nothing; a disabled one is still where it is drawn.
- It follows the pointer the compositor sends. A surface hears the pointer
  only over its input region -- its `MouseArea`s (and text inputs,
  terminals, drop areas, links), or the `input_regions` it set -- so a
  panel the pointer should be seen on needs an area under it, as above.
  Off the region, or off the surface, it is false. Touch is not the
  pointer.
- It changes when the pointer moves, enters or leaves, as `hovered` does:
  a panel sliding under a pointer that stays still is seen at the next
  motion. A node read for the first time is answered at the end of that
  turn, where the pointer is then.
- It costs nothing for a node nobody reads: reading it once enrols the
  node, and only enrolled nodes are tested, once per pointer motion. A
  binding reading it re-runs only when it turns.

### The wheel

`on_wheel(surface_x, surface_y, pixel_x, pixel_y, step_x, step_y,
local_x, local_y, modifiers)` runs for a wheel turn or a touchpad scroll;
`modifiers` is what is held, as `"ctrl+shift"` (Ctrl with the wheel
zooms, by custom). The wheel
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
once the shell has connected. `examples/demos/desktop/clipboard-history.lua` is all of
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

`win:close()` only hides a window; it can be opened again. `win:destroy()`
ends it for good, for a popup, a floating window or a layer surface alike:
the surface is torn down, its root and everything under it are removed as
`ui.destroy` removes a node (their `on_destroyed` hooks run, a terminal in
it is hung up), and `on_closed` runs once if the window was on screen — not
again if it was already closed. After that every method on the handle
raises `window destroyed`; a second `destroy()` does nothing. A window made
per use — a dialog, a terminal — is destroyed when done with instead of
being kept hidden in a pool. A child window hanging off a destroyed parent
is not destroyed with it, but with no parent it is never shown.

```lua
local dialog = morf.window.floating {
  root = build_dialog(), width = 420, height = 200, visible = true,
  on_closed = function() dialog:destroy() end,   -- closed by the compositor
}
```

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

### How much Lua may run at once

Every piece of Lua runs on a budget of VM instructions, so a loop that
never ends stops the piece it is in, not the shell. The budgets:

| what | default | `MORF_LIMITS` key |
|------|---------|-------------------|
| the configuration file itself | 50 000 000 | `load` |
| one module (`require`) | 20 000 000 | `module` |
| one view delegate (a row, a panel built on demand) | 20 000 000 | `delegate` |
| one handler, binding or effect | 1 000 000 | `handler` |
| every effect of one pass together | 8 000 000 | `frame` |

Building is loading's kind of work and gets loading's room; a handler
answers something and must be quick. A handler that runs out stops with
"Lua handler fuel exhausted after N instructions"; do heavy work in
pieces (`morf.timer(1, ...)`) or off the loop. `MORF_LIMITS` takes
comma-separated `key=N` (`MORF_LIMITS=module=40000000,handler=2000000`).

### What things cost

Measured on the machine this repository is built on (Intel Core
i7-11800H), release build, with luna's native tier off and on (the `jit`
Cargo feature; `MORF_JIT=off` switches a jit build back to the
interpreter). The VM figures come from
`cargo test --release -p morf-lua --lib [--features jit] bench_vm -- --ignored --nocapture`.

| what | interpreter | native tier |
|---|---|---|
| a pure Lua loop, 10 000 iterations (numbers and a table) | 1241 µs | 833 µs |
| a host-to-Lua call (an IPC verb that returns) | 0.68 µs | 0.78 µs |
| one binding re-run by a signal change (1000 of them per change) | 6.3 µs | 6.8 µs |
| a 64-row table written to a signal | 78 µs | 71 µs |
| one node built with three bindings (200 per build) | 16.2 µs | 16.6 µs |

The native tier compiles loops, arithmetic and table access; calls,
metamethods and coroutines still run through the interpreter, so only the
pure loop gains. A binding's cost is almost all the flush around it --
dependency bookkeeping, entering the VM, converting values -- not its few
instructions: a thousand bindings re-running is about a third of a 60 Hz
frame.

caelestia (Tsugumori) at 3840x2160 in the sealed sandbox, CPU time per
painted frame under `MORF_FRAME_LOG`:

| phase | interpreter, median / p90 | native tier, median / p90 |
|---|---|---|
| opening the dashboard | 19.2 / 26.9 ms | 18.9 / 25.7 ms |
| the media tab, a player on | 21.6 / 28.5 ms | 20.4 / 26.6 ms |
| the performance tab | 24.1 / 32.6 ms | 24.1 / 32.3 ms |

About a tenth of the shell's Lua instructions ran native. A frame there
is layout and painting, not Lua: the native tier changes it by a few per
cent. `MORF_JIT_LOG=1` prints the native tier's counters every ten
seconds.

### Compiled bindings: measured, not built

Compiling simple bindings -- property reads, arithmetic, conditionals over
signals -- to a native form that runs without entering Lua was planned
for when binding evaluation shows up next to rendering. It does not:

- a binding's own instructions are a small part of what it costs. Of the
  6.3 µs a re-run takes (above), most is the flush around it: finding what
  depends on the signal, entering the VM, converting the value, writing
  the property and marking what it dirties. A compiled expression still
  needs all of that but the middle step;
- caelestia (Tsugumori) at 3840x2160 in the sealed sandbox, opening and
  closing the dashboard under `MORF_FRAME_LOG=2 MORF_PROFILE=1`: of the
  frames over 16 ms (148 of them, median 18.6 ms), rendering took 74 % of
  the time and layout 4 %; every binding and effect that ran in the whole
  run took about 0.2 s of its 4.5 s of frame time -- under 5 %, so at best
  that much would be won, before the bookkeeping that stays.

So the work goes where the time is -- painting (cached path textures,
transforms of static drawings, fewer layers) and layout -- and binding
evaluation stays in Lua. Should a configuration's profile ever show
bindings past a fifth of its frames, this is where to look again.

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
| `MORF_WAKE_LOG=1` | every wake and its cause: `compositor`, `wake fd` (a service thread), or `deadline: timer`, `caret`, `image`, `dbus-timeout`, `terminal`, `tray-retry`, `clock-seconds`, `clock-minutes`, `clock-hours`, `fallback`, `pending` (the last turn left work), with how long it slept; each timer as it fires, named by the `file:line` that made it (`ui.Timer at …` for a node); and, every two seconds while anything animates, what is moving (`path.property`, marked `(loops)` when it never ends) |
| `MORF_FRAME_LOG=1` | every painted frame and what it cost; `=2` also splits any frame over 16 ms into layout, render and the rest |
| `MORF_SLOW_MS=N` | any stage of a turn that held the output longer than N ms (default 150) |
| `MORF_PROFILE=1` | with each slow stage, whose work filled it (below) |

Every line these print starts with the time, `[12345.678]`: milliseconds
on the monotonic clock (`CLOCK_MONOTONIC`, Python's `time.monotonic()`), so
a log can be lined up with a screen recording or another process's log.

`MORF_PROFILE=1` names what a slow stage spent its time on, the costliest
first, each with its own time (what it ran itself), its total (with what
it called), and how often it ran:

```
[3302451.528] morf: output DP-1: services, timers and callbacks took 74 ms
      10.98 ms own   69.51 ms total    1x  loader build … > ClipRect > Item > Loader (bar.island:359)
       9.20 ms own   20.34 ms total  251x  construct ui.Rect
       4.12 ms own    4.12 ms total   42x  binding Rect.color (bar.controls.calendar_card:153)
     126.46 ms own  126.46 ms total    1x  handler services.stats:44
```

A `binding` is named by the node path and property it drives, an `effect`
by the name `morf.effect` gave it, a `handler` (a timer, a callback, a
click) by the `file:line` of its function; a `timer`, `loader build`,
`ipc` verb or `construct ui.X` holds whatever ran inside it. `blocking
D-Bus call`, `get` and `set` are the synchronous `morf.dbus` calls -- a
turn that waits on the bus says so. `engine: …` is the engine's own
bookkeeping between them. Off, it costs nothing.

A shell that wakes more than it should says why under `MORF_WAKE_LOG`:
a `clock-seconds` every second is a binding reading `morf.clock` where
the minute clock would do, a `timer` is a timer still running.

### What a frame costs the GPU

A frame repaints what changed and hands the compositor only that. On
Wayland the renderer presents through buffers of its own (dmabufs it
attaches and commits itself), each of which remembers the frame it last
showed: a buffer coming back into use is brought up to date by copying
only what changed while the compositor held it, and the commit declares
only the frame's damage. A clock ticking on a 4K screen costs a tenth of a
millisecond of GPU time, not the full-screen copy a swapchain needs every
frame. When the device or the compositor cannot (no dmabuf export, no
`zwp_linux_dmabuf_v1`, no common modifier, GLES) it presents through a
swapchain as before.

| Variable | Does |
|---|---|
| `MORF_GPU_PROFILE=1` | writes GPU timestamps between a frame's stages and prints them: offscreen layers, the surface pass, the copy onto what is presented; with the pixels the damage made the commands shade and the heaviest of them. Each frame waits for its timestamps, so only for measuring. The stages are wall time on the GPU, so a GPU the compositor keeps busy inflates them; the smallest of many frames is the frame's own cost |
| `MORF_GPU_WAIT=1` | waits for each frame on the GPU and prints how long it took from submission: mostly time queued behind other clients' work on a busy GPU |
| `MORF_PRESENT_LOG=1` | each frame's buffer, how many the compositor held, how long getting one took, what was copied into it, and every commit and release |
| `MORF_PRESENT=swapchain` | presents through the swapchain instead (`MORF_PRESENT_MODE=fifo\|mailbox\|immediate` picks its mode) |
| `MORF_PRESENT_BUFFERS=N` | how many buffers of its own a surface uses at most (default 4) |
| `MORF_PRESENT_WAIT_MS=N` | how long a frame waits for the compositor to give a buffer back before it is skipped (default 250); a skipped frame's changes go with the next one |

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

## 9. Widgets

A widget is three things kept apart: an **archetype**, the behaviour with no
look (a Rust state machine in `crates/morf-kit`: what a press, a drag or a
key does to its state); a **skin**, the look with no behaviour (Lua, per
theme: slot trees drawn from the archetype's live state); and the
**settings** that make a widget what it is (a switch is a checkable press,
a seek bar a range whose value waits for the release). A theme is a set of
skins and display functions; it contains no interaction code, so every
theme's controls behave the same, take the same keys and tell a screen
reader the same things.

On top of the archetypes: **display widgets** take no input at all (text,
status, readings, charts, structure -- a kit function each); **composites**
combine archetypes into bigger controls (a combo box, a date picker, a file
chooser) shared by every theme; **domain instruments** (aviation, HUD,
audio, editors) are display widgets and composites for a field.

### Making a control

```lua
local control = require("lib.kit.control")
local node, t, ctl = control.make("Press", "push", {
  label = "Apply", width = 96, height = 34,
  on_clicked = function() apply() end,
  enabled = function() return dirty:get() end,   -- a setting given as a binding follows it
})
```

`control.make(archetype, widget, spec, extra)` makes the archetype, builds
the node (a `MouseArea` whose pointer, focus, keys and wheel go to the
archetype), and asks the current skin for the widget's slots. The
archetype's answers update `t`, a reactive table of its state the skin
reads (`t.hovered`, `t.down`, `t.checked`, `t.visual_position`, ...), and
raise the spec's handlers (`on_clicked`, `on_toggled`, `on_moved`, ...).
Spec fields that are the archetype's settings (`checked`, `value`, `from`,
`mode`, ...) configure it, as bindings when given as functions; node
fields (`id`, `x`, `anchors`, `visible`, ...) go to the node. `extra` takes
the node's own `props`, its `children`, `builders` (slots that build parts
rather than nodes: a list's row delegate) and a `voice` (the node a screen
reader knows the control by). `ctl.send(event, ...)` drives the archetype
directly. Most code uses the constructors in `lib.kit.widgets`
(`widgets.switch { ... }`, `widgets.slider { ... }`, `widgets.area { ... }`
for a layout's own pressable area) and the glue modules named under each
archetype below.

### Skins and slots

A skin is a function of the live state returning slot trees, or a table of
one such function per slot; a slot given as a function is built lazily, the
first time the control is hovered, pressed or focused.

```lua
local skin = require("lib.kit.skin")
skin.define("plain", { skins = {
  Press = function(t, spec)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = 6,
        color = function() return t.down and "#2a2a2a" or (t.hovered and "#3a3a3a" or "#333333") end },
      label = ui.Text { anchors = { center_in = true }, text = spec.label or "" },
      indicator = function() return focus_ring(t) end,   -- built when first focused
    }
  end,
} })
skin.use("plain")      -- every control rebuilds its slots in it
```

Every archetype has `background` (beneath the control's own children) and
`content`; each adds its own (a range's `track`, `fill`, `handle`; a
selection's `item` and `place` builders; a navigation's `transition`).
`skin.use(name)` switches every live control at once, keeping its state.
A theme's kit carries its skins as `kit.skins.<Archetype>`.

A widget may have a look of its own: `kit.skins.<widget>` (`skins.knob`,
`skins.data_table`) is asked before its archetype's, slot by slot, so it
fills what it draws differently and the archetype's skin the rest. The
three looks shipped (the default kit, caelestia's Material and Tsugumori)
keep them in `widgets/<archetype>.lua` beside their `skins.lua`, one file
per archetype, each `function(S, theme, M) S.<widget> = ... end`. Every
widget of every archetype is drawn by the widget galleries
(`library/tests/default_widgets_gallery_spec.lua`,
`examples/shells/caelestia/tests/kit_widgets_gallery_spec.lua`) from its
sample in `library/lib/kit/samples/<archetype>.lua`; `KIT_WIDGETS="Canvas
Dock"` draws only those archetypes.

### The contract

`library/lib/kit/contract.lua` is the contract every theme's kit is held
to: the base functions (`kit.text`, `kit.icon`, `kit.card`, ...), every
archetype (roles, state, signals, keys, slots, widgets), every display
widget, every composite and domain instrument, each with the stage it
arrived at, and the owner of every row of the element catalogue
(`library/lib/kit/catalogue.lua`). `morf check --kit` checks a
configuration's `require("kit")` against what is due (`morf check --kit
default` checks the default look); the galleries
(`examples/shells/caelestia/tests/kit_gallery_spec.lua`,
`composites_gallery_spec.lua`, `library/tests/default_kit_spec.lua`) draw
every widget in every theme and fail on anything that spills its cell or
logs an error.

### Focus, keys, popups, lists, channels

The pieces controls stand on are documented where the engine provides
them: focus policies, Tab, focus scopes and the ring under *Focus*;
shortcuts under *Shortcuts*; long press, double click, swipes and pinches
under *Gestures*; popups and the overlay layer under *Overlays*;
recycled lists under *4. Lists*; data channels and plot kinds under *Data
channels*; screen readers under *Accessibility*; mirroring under *Right to
left*; windows under *Applications*; and the theme-free kit under *The
default look* (all in chapter 6). Each archetype's default focus policy:
a press and a disclosure by Tab, a range, plane, selection and collection
strongly (click and Tab), a text field through its input, a popup, scroll,
navigation and shell not at all.

#### Control (the base)

Every archetype is a `Control`: state `hovered`, `down`, `focused`,
`visual_focus` (a keyboard gave it focus: the ring shows), `enabled`,
`mirrored` (right to left: keys and visual positions turn round),
`highlighted`; geometry `padding` and `insets` (and per side), the implicit
size `max(background + insets, content + padding)`; slots `background` and
`content`; `focus_policy`; and `accessible_role`, `accessible_name`,
`accessible_description` with states from its live state.

### The archetypes

Each archetype below: its accessible roles, its state (fields of `t`), the signals
it raises, the keys it answers, the slots its skin fills and the widgets that
are it -- then how its glue is used.

#### Press

| | |
|---|---|
| roles | button, toggle_button, check_box, radio_button, switch, link, menu_item |
| state | checkable, checked, tristate, partial, auto_repeat, delay, interval, pressed_at, group |
| signals | on_clicked, on_pressed, on_released, on_toggled, on_long_pressed, on_double_clicked |
| keys | space, return, arrows_in_group |
| slots | background, content, indicator, icon, label, badge |
| widgets | push, suggested, destructive, flat, raised, outlined, text, tonal, elevated, pill, circular, icon, link, close, copy, loading, toggle, toggle_group_member, switch, checkbox, radio, chip_assist, chip_filter, chip_input, chip_suggestion, tag, tile, card_action, row_activation, menu_item, check_menu_item, radio_menu_item, keycap, fab, extended_fab, speed_dial_item, segment, rating_star, help, disclosure_button, repeat_button, back, forward, hold_button |
| arrived | stage 6 |

Every widget the Press, Range, Plane and Selection archetypes make
(contract.lua), as a
constructor: `widgets.switch { checked = on, on_toggled = set }`,
`widgets.slider { value = level, on_moved = set_level }`. A widget is its
archetype with the settings that make it what it is -- a switch is a
checkable press, a radio button a press in an exclusive group, a seek
bar a range whose value waits for the release -- and the theme's skin
for it (lib.kit.skin; the skin is told which widget through
`spec.widget`).

#### Range

| | |
|---|---|
| roles | slider, spin_button, scroll_bar, progress |
| state | from, to, value, step, page_step, snap, live, orientation, inverted, logarithmic, position, visual_position, wrap, range, first, second |
| signals | on_moved, on_value_changed, increase, decrease |
| keys | arrows, page_up, page_down, home, end |
| slots | background, content, track, fill, handle, second_handle, ticks, value_label, increase, decrease |
| widgets | slider, vertical_slider, range_slider, discrete_slider, log_slider, angle_slider, knob, bipolar_knob, stepped_knob, fader, spin_button, scrubber, scroll_bar, seek_bar, volume, brightness, zoom, rating, level_control, osd_level |
| arrived | stage 6 |

Made through `lib.kit.widgets` like a press: `widgets.slider { ... }`.

#### Plane

| | |
|---|---|
| roles | slider |
| state | x_from, x_to, y_from, y_to, x, y, step_x, step_y, visual_x, visual_y, constraint |
| signals | on_moved, on_value_changed |
| keys | arrows, page_up, page_down |
| slots | background, content, field, handle, crosshair |
| widgets | colour_plane, hue_wheel, xy_pad, pan_pad, envelope_point, joystick, minimap_viewport, crop_handle |
| arrived | stage 7 |

Made through `lib.kit.widgets` like a press: `widgets.colour_plane { ... }`.

#### Selection

| | |
|---|---|
| roles | tab_list, list_box, radio_group, tree, grid |
| state | model, current, selected, mode, wrap, orientation, follow_focus |
| signals | on_current_changed, on_selection_changed, on_activated |
| keys | arrows, home, end, typeahead, space_toggles, ctrl_a |
| slots | background, content, item, indicator, separator |
| widgets | tabs, segmented, view_switcher, inline_view_switcher, radio_group, toggle_group, list_selection, grid_selection, carousel_dots, pagination, stepper_header, sidebar_list, breadcrumbs, day_grid, swatch_grid, emoji_grid, icon_chooser, transfer_side, rating_items, radial_menu, pie_menu, tumbler |
| arrived | stage 7 |

Selections (the Selection archetype): tabs, segmented choices, list and
grid selection, swatch grids. The archetype keeps the current item and
the selected set and answers the keys; this lays out an item per entry
of `spec.items` with the skin's `item` delegate and keeps the current
item's box in the live state, so a skin's `indicator` can travel to it.

```lua
selection.make("tabs", {
  items = { "Overview", "Media", "Weather" },     -- or a function
  current = function() return tab:get() end,
  on_current_changed = function(i) tab:set(i) end,
  orientation = "horizontal",                     -- vertical, grid
  columns = 4, gap = 0, item_width = 80, item_height = 36,
})

```
A skin's `item(index, value, s)` builds one entry; `s` reads its state:
`s.current()`, `s.selected()`, `s.hovered()`, `s.down()`, and `s.area`
(the entry's MouseArea). A skin may also give `place(index, value)`, the
entry area's own properties (`x`, `width`, `layout`, ...), and
`container()`, the node the entries go in (a Row, a Column or a Grid by
`orientation` otherwise). `spec.item_id(index, value)` names each
entry's area; `spec.delegate(index, value, s)`, when given, draws the
entries instead of the skin (a layout's own swatches); and
`spec.press_activates` makes one press activate an entry, not only
choose it. The live state adds `current_x`, `current_y`,
`current_width`, `current_height`: the current entry's box within the
control.

`geometry` lays the entries out other than in a row:

  "radial" (a radial menu, a pie menu): item 1 at twelve o'clock, the
  rest clockwise on a circle of `radius` (half way between the hub and
  the rim) in a `size` px square. The pointer over it points from the
  centre (the archetype's "point": the entry in that sector is current,
  none within `dead_radius` of the centre) and a release past the dead
  radius activates -- a marking menu's flick. The live state adds
  `outer`, `inner` and `radius` for the skin's sectors.

  "tumbler" (a spinning wheel picker): the entries on a drum, `rows`
  (5) tall. A vertical drag turns it continuously and a release
  snaps it to the nearest entry, with a little of the flick's speed
  carried on; a press on an entry off the centre turns it there; the
  wheel and the arrows step. Entries away from the centre fold over the
  drum (their area's `translate_y`, `scale_y`, `opacity`). The live
  state adds `rows` (visible) and `row` (an entry's height).

`selection.pie(spec)` makes a pie menu that opens over everything else:
see there.

#### Popup

| | |
|---|---|
| roles | dialog, alert_dialog, menu, tooltip, list_box_popup |
| state | open, modal, dim, close_policy, placement, anchor, side, align, flip, shift, focus_on_open, restore_focus |
| signals | on_opened, on_closed, on_about_to_close |
| keys | escape, tab_trap |
| slots | background, content, dim, enter, exit |
| widgets | menu, context_menu, menu_bar_menu, submenu, popover, tooltip, rich_tooltip, hover_card, dropdown_list, autocomplete_list, command_palette, dialog, alert_dialog, message_dialog, preferences_dialog, about_dialog, shortcuts_dialog, bottom_sheet, side_sheet, drawer, toast, snackbar, banner, notification_popup, lightbox, tour_step |
| arrived | stage 8 |

Popups (the Popup archetype) on the engine's overlay layer: menus,
tooltips, dialogs, toasts, popovers -- and a shell's drawers, tracked
where they already are.

```lua
local menu = popup.make("menu", { items = {
  { label = "Copy", icon = "content_copy", on_clicked = copy },
  { label = "Wrap", checked = function() return wrap:get() end, on_toggled = set_wrap },
} })
menu.open(button)            -- beside its anchor; menu.close(), menu.toggle(button)

popup.make("dialog", { title = "Discard?", body = "...", width = 360,
  buttons = { { label = "Cancel" }, { label = "Discard", on_clicked = discard } } }).open()

popup.toast { text = "Copied", timeout = 2500, root = node }
popup.tooltip(target, "Mute")

```
A popup's own content is `spec.content` (a node), or what its widget
builds from `items`, `title`, `body` and `buttons`; the theme's skin
draws its `background`. `on_opened`, `on_closed(reason)` and
`on_about_to_close(reason)` -- returning false keeps it open -- follow
it. `popup.track(node, spec)` gives a node that stays where it is (a
drawer) the same behaviour: Escape, a press outside, the stack.

#### TextField

| | |
|---|---|
| roles | text_field, password_text, text_area, search_field |
| state | text, placeholder, echo, read_only, max_length, validator, acceptable, multiline, wrap, selected_text |
| signals | on_edited, on_accepted, on_text_changed, on_invalid |
| keys | editing, return_accepts, escape_reverts |
| slots | background, content, field, leading, trailing, placeholder, counter, error |
| widgets | entry, password, search, text_area, url, email, numeric_entry, otp, tag_input, mentions, inline_rename, entry_row, filter_field, code_input, shortcut_recorder |
| arrived | stage 9 |

Text fields (the TextField archetype) around the engine's text input.

```lua
local node, input = text_field.make("entry", {
  id = "search", width = 300, height = 40, placeholder = "Search",
  on_accepted = run, validator = "number", clear = true,
})

```
`node` is the control, the one to place; `input` the `ui.TextInput`
inside it, which keeps the spec's `id` and every text input property
(`placeholder`, `font_size`, `on_text_changed`, ...). The archetype
validates (`validator`, `minimum`, `maximum`, `required`,
`max_length`), reverts on Escape when asked (`revert_on_escape`), and
reveals a password; the skin draws the field around the input
(`background`, `placeholder`, `trailing` -- a clear or reveal press --,
`counter`, `error`). `inset` (px, or `{ l, t, r, b }`) keeps the input
clear of the skin's edges. `on_edited(text)`, `on_accepted(text)` --
only when acceptable -- and `on_invalid(text)` follow it.

#### Scroll

| | |
|---|---|
| roles | scroll_pane |
| state | content_x, content_y, content_width, content_height, scroll_policy, snap, at_start, at_end |
| signals | on_scrolled, on_reached_start, on_reached_end |
| keys | arrows, page_up, page_down, home, end, space |
| slots | background, content, scroll_bar_x, scroll_bar_y, edge_fade, overscroll |
| widgets | scroll_view, scroll_area, pager, shelf, infinite_scroll |
| arrived | stage 9 |

Scrolled views (the Scroll archetype) around the engine's flickable.

```lua
local node, flick = scroll.make("scroll_view", {
  id = "page", width = 400, height = 300, clip = true,
  ui.Column { ... },                        -- the content
})

```
`node` is the control, the one to place -- it takes no pointer input of
its own --; `flick` the `ui.Flickable`
inside it, which keeps the spec's `id` and flickable properties
(`content_y`, `interactive`, ...). The archetype follows where it has
scrolled (`t.position_y`, `t.size_y`, `t.at_end`, ...), answers the
arrows, Page keys, Home, End and Space, and snaps (`snap = "items" |
"pages"`); the skin draws `scroll_bar_x`/`scroll_bar_y` from that,
`edge_fade` and `overscroll`. `scroll_policy_x`/`_y` say when a bar
shows. `on_scrolled(x, y)`, `on_reached_start`, `on_reached_end` follow
it.

A snapping view settles once the scrolling pauses: the archetype picks
the item or page, and the view glides there. A `pager` scrolls sideways
a page at a time (its skin draws the page dots), a `shelf` sideways an
item at a time (`item_size`; its skin draws edge fades and arrows), and
the wheel turns either sideways. An `infinite_scroll` that reaches its
end sets `t.loading` and calls `on_load_more(done)`; `done()` clears it
(the skin shows a loading row meanwhile). A skin moves the view with
`spec.glide(x, y)`, which eases there.

#### Collection

| | |
|---|---|
| roles | list, grid, table, tree, tree_grid |
| state | model, delegate, layout, columns, expanded, section, item_size |
| signals | on_row_activated, on_sort_changed, on_expanded_changed, on_end_reached |
| keys | selection_keys, left_right_tree |
| slots | background, content, row, header, section_header, footer, empty, loading, placeholder_row |
| widgets | list, boxed_list, list_box, virtual_list, grid_view, flow_box, data_table, tree_view, tree_table, file_list, timeline, feed, chat_log, kanban_column, transfer_list |
| arrived | stage 10 |

Collections (the Collection archetype): lists, grids, tables and trees
over a model, through the engine's list view -- only the rows in sight
are built, and a row scrolled out is rebound to the row scrolled in.

```lua
collection.make("list", {
  rows = model,                 -- a morf.list_model (or a plain array)
  width = 300, height = 400, row_height = 36,
  size_field = "height",        -- rows that say how tall they are
  kind_field = "kind",          -- delegates only reused within a kind
  on_activated = function(i) end, on_end_reached = load_more,
})
collection.make("table", { rows = files, columns = {
  { key = "name", title = "Name", width = 220, sortable = true },
  { key = "size", title = "Size", width = 90, sortable = true } },
  on_sort_changed = function(key, ascending) end })
collection.make("tree", { tree = { { key = "a", label = "A", children = { ... } } } })

```
The skin draws each row with its `row(row, s)` builder -- returning a node
and an updater `function(row)` that rebinds it -- where `s` reads the
row's state as it is now bound: `s.index()`, `s.current()`,
`s.selected()`, `s.hovered()`, `s.down()`, and in a tree `s.depth()`,
`s.expandable()`, `s.expanded()`; a table's cells come from the skin's
`cell(row, column, s)` and its header from `header(column, t)`. A spec's
own `delegate(row, s)` (returning node and updater too) draws the rows
instead.

#### Disclosure

| | |
|---|---|
| roles | button_expanded, group |
| state | expanded, group, animated |
| signals | on_expanded, on_collapsed |
| keys | space, return, left_right_tree |
| slots | background, content, header, indicator, content |
| widgets | expander, expander_row, accordion, collapsible_header, collapsible_section, details, tree_node, show_more, collapsible_card, fold_out |
| arrived | stage 11 |

Disclosures (the Disclosure archetype): expanders, accordions,
collapsible sections, "show more".

```lua
local node, t = disclosure.make("expander", {
  title = "Advanced", width = 320, header_height = 40,
  content = ui.Column { ... },         -- shown while expanded
  expanded = false, group = "settings", -- one open at a time
  on_toggled = function(open) end,
})

```
The header is the control: a press, Space or Return toggles, Left and
Right close and open. The skin draws `header` (the row's look, given
`spec.title`), `indicator` (the chevron) and `background`; the content
is the spec's, clipped under the header, the whole growing and shrinking
with it.

#### Drag

| | |
|---|---|
| roles | splitter, grip, draggable |
| state | axis, bounds, threshold, active, delta, target, mode |
| modes | move, resize, split, reorder, swipe, transfer |
| signals | on_drag_started, on_dragged, on_dropped, on_swiped |
| keys | arrows, alt_arrows_reorder |
| slots | background, content, handle, ghost, drop_indicator |
| widgets | split_pane, resizable_panel, resize_grip, reorderable_rows, reorderable_tabs, sortable_grid, swipe_dismiss, swipe_actions, pull_to_refresh, sheet_handle, window_move, drag_source, drop_zone, slide_to_confirm |
| arrived | stage 11 |

Drags (the Drag archetype): split panes, resize grips, reorderable rows,
swipe-to-dismiss, window move.

```lua
drag.make("grip", { mode = "resize", axis = "x", minimum = 200, maximum = 600,
  value = function() return width:get() end, on_moved = function(w) width:set(w) end,
  width = 8, height = 300 })
drag.split { first = list, second = detail, width = 800, height = 500,
  ratio = 0.35, minimum = 0.2, maximum = 0.8 }
drag.swipe(card, { on_swiped = function(direction) dismiss() end })  -- on a node of the layout's

```
The control is the handle; the skin draws `handle`, `ghost` and
`drop_indicator`. `drag.swipe` gives a layout's own node the swipe: a
headless Drag it feeds the pointer to, which moves the node with the
finger, lets it go past its distance or speed, and springs it back
otherwise.

#### Navigation

| | |
|---|---|
| roles | tab_panel, group |
| state | pages, current, mode, can_go_back, history |
| signals | on_pushed, on_popped, on_current_changed |
| keys | alt_left, back, ctrl_tab, arrows_carousel |
| slots | background, content, page, transition, back, indicator |
| widgets | navigation_view, view_stack, tab_pages, carousel, onboarding, wizard, settings_subpages, master_detail |
| arrived | stage 11 |

Navigations (the Navigation archetype): a navigation view's push and
pop, a view stack, a carousel, a wizard.

```lua
local node, nav = navigation.make("navigation_view", {
  width = 400, height = 600, current = "settings",
  pages = { settings = build_settings, sound = build_sound },   -- builders or nodes
})
nav.push("sound") ; nav.pop() ; nav.go("settings")

```
Alt+Left and a mouse's back button pop, wherever focus is inside it (its
shortcuts); Ctrl+Tab cycles a switcher; a carousel's arrows walk it. A
page is built when first shown and kept -- its scroll and focus with it
-- while it is in the stack. The skin's `transition(from, to, direction)`
moves the pages; without one the new page slides in from its side.

#### Shell

| | |
|---|---|
| roles | application, navigation, main, complementary |
| state | breakpoints, collapsed, layout, sidebar_width, content_width, toolbar_style |
| signals | on_breakpoint, on_collapsed |
| keys | f9, ctrl_b, f6 |
| slots | background, content, header_bar, sidebar, content, inspector, bottom_bar, toolbar_top, toolbar_bottom, banner, toasts |
| widgets | window_layout, header_bar, toolbar_view, split_view, overlay_split_view, navigation_split_view, multi_pane, breakpoint_bin, clamp, bottom_bar |
| arrived | stage 16 |

Application shells (the Shell archetype): a window's header bar, sidebar,
content, inspector, bottom bar, banner and toasts, arranged for the
window's width and rearranged as it narrows.

```lua
local node, app = shell.make("window_layout", {
  width = function() return win.width end, height = function() return win.height end,
  header_bar = header, sidebar = list, content = page, inspector = details,
  bottom_bar = tabs, banner = notice, toasts = stack,
  sidebar_width = 280, inspector_width = 300, breakpoints = { 600, 900 },
  on_breakpoint = function(layout) end,
})
app.toggle_sidebar()

```
Wide, the sidebar stands beside the content and the inspector beside it
on the other side; under the first breakpoint the sidebar becomes a
drawer over the content with a scrim (Escape or a press outside shuts
it) and the bottom bar takes its place; under the last the inspector
hides. F9 and Ctrl+B toggle the sidebar, F6 and Shift+F6 move between
the regions. The regions are landmarks: navigation, main and
complementary to a screen reader. The skin draws `background` and may
decorate `sidebar`'s edge; the parts are the configuration's.

#### Canvas

| | |
|---|---|
| roles | group, image |
| state | view_x, view_y, zoom, tool, selection, hovered, pointer_x, pointer_y, gesture, band, draft, connect_from, connect_to |
| tools | select, pan, point, line, rect, ellipse, polyline, polygon, freehand, connect, brush, zoom |
| signals | on_view_changed, on_selection_changed, on_moved, on_drawn, on_connected, on_activated, on_context, on_brushed, on_deleted |
| keys | arrows_nudge, plus_minus_zoom, zero_reset, home_fit, ctrl_a, delete, escape, tab_items |
| slots | background, content, grid, item, wires, selection, draft, band, crosshair, overlay |
| widgets | zoomable_canvas, node_graph, whiteboard, diagram, map_view, image_viewer, chart_inspector, timeline_track, drawing_board |
| arrived | stage 21 |

Canvases (the Canvas archetype): a world a viewport looks into, panned
and zoomed, holding items that are picked, selected, moved, connected
and drawn. A node graph, a whiteboard, a map, an image viewer, a
zoomable chart and a timeline are each one drawn differently.

```lua
local node, view = canvas.make("node_graph", {
  width = 800, height = 600,
  items = function() return graph.nodes end,   -- { id, x, y, w, h, shape?, ... }, world units
  ports = function() return graph.ports end,   -- { id, item, x, y, kind = "in" | "out" }
  wires = function() return graph.edges end,   -- { id, from = port id, to = port id }
  delegate = function(item, s) return ui.Rect { anchors = { fill = true }, ... } end,
  content = world_drawing,                     -- nodes in world units, under the items
  overlay = zoom_buttons,                      -- nodes in screen units, over everything
  tool = "select", grid = 16, snap = true,
  on_moved = function(ids, dx, dy) end, on_connected = function(from, to) end,
  on_drawn = function(tool, points) end, on_deleted = function(ids) end,
})
view.fit()  view.zoom_by(2)  view.center_on(x, y)  view.select { "a" }
view.to_screen(x, y)  view.to_world(x, y)

```
The world is one node under a transform, so panning and zooming move a
drawing that stays drawn; the items are laid in it at their world
boxes (a rect or an ellipse at x, y, w, h; a line, polygon or point at
the origin, drawing its own points). A delegate draws an item -- it
takes no pointer: the canvas picks -- and is told `s.item()`,
`s.selected()`, `s.hovered()` and `s.zoom()`. Without a delegate the
skin's `item` builder draws it from its fields (`fill`, `stroke`,
`label`). Selected items move with a drag before the configuration
hears `on_moved` and moves them.

The skin draws `background` and `grid` under the world and `wires`
(a builder: a wire between two world points), `selection` (a builder:
the outline round a selected item, in screen units), `draft`, `band`,
`crosshair` and `overlay` over it.

#### Dock

| | |
|---|---|
| roles | group, tab_list, tab, tab_panel, splitter |
| state | focused, focused_panel, maximized, dragging, drop_target, drop_zone, panel_count |
| signals | on_layout_changed, on_activated, on_closed, on_maximized, on_focus_changed |
| keys | ctrl_page, ctrl_w, ctrl_shift_m, f6, escape |
| slots | background, content, tab, stack, divider, floating, drop_indicator |
| widgets | dock_area, shelf_dock, tabbed_container, document_tabs, tool_windows |
| arrived | stage 21 |

Docks (the Dock archetype): panels in a tree of splits and tab stacks
that the user rearranges -- a tab dragged onto a stack's middle joins
it, onto an edge splits beside it, out of the dock floats -- and
maximises, closes, and walks by keys.

```lua
local node, dock = lib.kit.dock.make("dock_area", {
  width = 1200, height = 800,
  panels = {
    files = { title = "Files", icon = "folder", content = file_tree },
    editor = { title = "main.lua", content = editor, closable = false },
    log = { title = "Log", content = function() return log_view() end },  -- built when first shown
  },
  layout = { orientation = "horizontal", ratios = { 0.2, 0.8 }, children = {
    { panels = { "files" } },
    { orientation = "vertical", ratios = { 0.7, 0.3 }, children = {
      { panels = { "editor" } }, { panels = { "log" } } } } } },
  on_layout_changed = function(tree, floating) save(tree, floating) end,
  on_closed = function(panel) end,
})
dock.activate("log")  dock.close("log")  dock.maximize("editor")  dock.float("files", x, y, w, h)
dock.dock("files", stack_id, "left")  dock.layout()

```
A panel's content is made once and kept: a stack shows its current and
parks the rest, so a panel moved, hidden or floated keeps its state.
The skin draws `tab` (a builder: one tab, told `s.title`, `s.icon`,
`s.current()`, `s.focused()`, `s.hovered()`, `s.closable`, `s.close()`),
`stack` (a builder: the frame round a stack, told `s.focused()`),
`divider` (a builder: the handle between two parts, told
`s.orientation`, `s.hovered()`, `s.dragging()`), `floating` (a builder:
a floating panel's frame, told `s.title`; its top `tab_height` is the
bar it is moved by) and `drop_indicator` (where a dragged tab would
land: a node over the whole dock, shown while a tab is dragged over a
stack, that puts its plate at `spec.drop_box()` -- x, y, w, h -- and
may move it there as it likes).

#### Transform

| | |
|---|---|
| roles | group, dialog |
| state | x, y, box_width, box_height, angle, active, handle, maximized, minimized |
| signals | on_changed, on_committed, on_maximized, on_minimized |
| keys | arrows_move, ctrl_arrows_resize, alt_arrows_turn, return_maximize, escape |
| slots | background, content, frame, handle, rotate_handle, guide |
| widgets | floating_panel, image_cropper, resize_box, pip_window, event_block |
| arrived | stage 22 |

Transforms (the Transform archetype): a box the user moves by its body,
resizes from its edges and corners, and turns -- a floating panel, an
image cropper's frame, the handles round a canvas item, a
picture-in-picture window, a calendar's event.

```lua
local node, box = lib.kit.transform.make("floating_panel", {
  title = "Inspector", x = 40, y = 40, width = 320, height = 240,
  container = { 1280, 800 },          -- what it floats in (the area it fills, by default)
  content = inspector,                -- or the spec's array part
  on_committed = function(x, y, w, h, angle) save(x, y, w, h) end,
  on_close = function() end,
})
box.maximize()  box.minimize()  box.restore()  box.set(x, y, w, h)  box.t.box_width

```
The node returned is the area the box floats in (filling its parent, or
`container` px when given); the box itself (`box.node`) sits in it at
`t.x`, `t.y`, `t.box_width` x `t.box_height` and carries the content.
Its body moves it (a floating panel's only by its title bar, the top
`title_height` px, whose double click maximizes); the skin's `handle`
builder draws each grip (told `s.name` -- n, ne, e, se, s, sw, w, nw --
`s.hovered()`, `s.held()`, `s.corner`), and a turned box (`rotatable`)
has a knob `stalk` px over its top (the skin's `rotate_handle`). The
skin's other slots: `background` (under the content: a window's ground,
a cropper's dimming of what is outside), `frame` (over it: an outline,
a title bar), `guide` (thirds while dragging) and `content`; a skin
that sets `t.content_radius` has the content cut to those corners.

Every press tells the archetype the box's centre on the surface, a drag
the pointer on the surface (the box moves under it) and what is held:
Shift keeps the aspect, Alt resizes about the centre. The arrows, Ctrl
with them, Alt with them, Return and Escape are the archetype's.

#### Sheet

| | |
|---|---|
| roles | grid |
| state | row, column, anchor_row, anchor_column, range, editing |
| signals | on_current_changed, on_selection_changed, on_edit_started, on_edited, on_cleared, on_copy, on_cut, on_paste, on_toggled, on_activated |
| keys | arrows, shift_arrows_range, ctrl_arrows_edge, home_end, page, tab, return, f2, type_to_edit, escape, delete, ctrl_a, ctrl_c_x_v |
| slots | background, content, cell, header, range, cursor, editor |
| widgets | spreadsheet, data_grid, step_sequencer, seat_map, cell_grid |
| arrived | stage 22 |

Sheets (the Sheet archetype): a grid the keyboard walks one cell at a
time -- a spreadsheet, a data grid that edits in place, a step
sequencer, a seat map (WAI-ARIA grid).

```lua
local node, sheet = lib.kit.sheet.make("spreadsheet", {
  width = 600, height = 400, rows = 1000, columns = 8,
  cell = function(row, col) return data[row][col] end,   -- a value or a string; may read signals
  on_edited = function(row, col, text) data[row][col] = text end,
  on_paste = function(row, col, grid) end,     -- grid: rows of strings, from TSV
  on_cleared = function(r0, c0, r1, c1) end,
})
sheet.refresh()   -- cell() is read again

```
Spec: `rows`, `columns` (numbers or bindings), `cell(row, col)`,
`headers` (column titles; A, B, C ... by default; false for none),
`row_headers` (a list or `function(row)`; 1..n by default; false for
none), `column_widths` (a list or one number), `row_height`,
`header_height`, `row_header_width`, `disabled(row, col)` (a cell that
takes no toggle or edit: a taken seat), `playhead` (a binding: the
column a step sequencer plays), and the archetype's settings
(`editable`, `toggle`, `read_only`, `page_rows`, `wrap`). Only the rows
in sight are built; columns are not virtual. The header row and the row
headers stay put while the cells scroll under them.

The skin fills `cell` (a builder: `cell(s)` -> node, told `s.column`,
`s.row()`, `s.value()`, `s.text()`, `s.disabled()`, `s.playing()`,
`s.zebra`, `s.widget`), `header` (a builder: `header(h)` -> node, told
`h.kind` -- "column", "row" or "corner" --, `h.index()`, `h.title()`,
`h.current()`, `h.selected()`), `range` and `cursor` (nodes laid over
the cells in their own coordinates: `spec.cursor_box(t)` and
`spec.range_box(t)` give x, y, w, h, and `spec.cell_box(r0, c0, r1,
c1)` any box; `spec.column_box(c)` a column's) and `editor` (a builder:
`editor(props)` -> node, input -- a `ui.TextInput` made with `props`).

#### Roving

| | |
|---|---|
| roles | toolbar, menu_bar |
| state | current, open |
| signals | on_current_changed, on_open, on_close |
| keys | arrows, home_end, menubar_down_opens, escape |
| slots | background, content, indicator, separator |
| widgets | toolbar_group, menubar, button_group, chip_row, icon_bar |
| arrived | stage 22 |

Rovings (the Roving archetype): one Tab stop for a group of controls the
arrows move between -- a toolbar's buttons, a menu bar, a linked button
group, a row of chips, a bar of icons (WAI-ARIA toolbar and menubar).

```lua
local node, group = roving.make("toolbar_group", {
  id = "format", accessible_name = "Format",
  items = {
    { icon = "format_bold", tooltip = "Bold", checked = false, on_toggled = set_bold },
    { icon = "format_italic", tooltip = "Italic", on_clicked = italic },
    { separator = true },
    some_kit_control,                          -- a node as it is
    function(i) return build(i) end,           -- or a builder
  },
})
group.focus() group.current() group.open(2) group.close()

```
A member is a node (a kit control), a builder, or a press described by
`icon`, `label`, `tooltip`, `on_clicked`, `checked`/`on_toggled`,
`enabled` (a value or a binding) and `id` -- drawn as the widget's
member press (a toolbar's flat button, a button group's segment, a chip,
an icon). `{ separator = true }` puts the skin's separator between two
members. The current member alone is a Tab stop ("strong"), the others
take a click ("click"), so Tab stops once and enters where it left; the
arrows (Left/Right, Up/Down, both in a grid), Home and End move it,
passing over disabled members.

A menu bar's members are menus: `{ label = "File", items = { ... } }`,
items as a popup menu's (lib.kit.popup). Down, Return and Space open the
current one's menu; while one is open Left and Right open the next;
Escape closes it and focus goes back to its title. F10 focuses the bar.

Other fields: `orientation` ("horizontal", "vertical", "grid"),
`columns`, `wrap`, `current`, `gap`, `padding`, `item_height`, `x`,
`y`, `anchors`, `on_current_changed(i)`. The skin draws `background`,
`indicator` (riding the current member's box: `t.cur_x`, `t.cur_y`,
`t.cur_w`, `t.cur_h`; `t.within` while focus is inside, `t.keyboard`
when a keyboard put it there, `t.open`) and `separator(vertical)`, a
builder.

#### Form

| | |
|---|---|
| roles | form |
| state | valid, dirty, pending, submitting, error_count, tried, first_invalid |
| signals | on_submitted, on_invalid, on_reset, on_validity_changed, on_dirty_changed, on_show_error, on_hide_error |
| keys | return_submits, ctrl_return |
| slots | background, content, summary, message |
| widgets | form, settings_form, login_form, inline_form |
| arrived | stage 22 |

Forms (the Form archetype): what a group of fields adds up to -- whether
it can be sent, whether anything changed, what is wrong and where -- and
the sending.

```lua
local node, form = form.make("login_form", {
  width = 320, submit_label = "Sign in",
  on_submit = function(values, done) sign_in(values.user, values.password, done) end,
})
form.field("user", { label = "Username", control = { widgets.entry { label = "Username", required = true } },
  message = "Enter your username" })

```
`make` returns the control and a handle:

* `field(name, opts)` adds a field. `opts.control` is what was made for
  it: a node, or the returns of a kit control packed in a table
  (`{ widgets.entry { ... } }`: the control, the input that holds focus,
  its live state, its handle). `valid`, `dirty`, `message` and `value`
  are bindings (a message may be a string); a kit text field gives its
  own (`acceptable`, its text, and what it was made with) when they are
  left out. `validate(value, done)` checks out of line -- a name still
  free on the server --: the field is pending until `done(ok, message)`.
  `label` names it in the summary; `place = false` leaves the node where
  the configuration put it (bind `error(name)` to show its message);
  `reset(value)` puts a control that is not a text field back.
* `error(name)`: the field's message while the policy shows it, else "";
  `shown(name)` whether it shows.
* `submit()`, `reset()`, `remove(name)`, `focus(name)`, and `t`, the
  form's live state (`valid`, `dirty`, `pending`, `submitting`, `tried`,
  `error_count`, `first_invalid`).

`spec.on_submit(values, done)` sends: `done(ok, message)` finishes it; a
failure's message shows in the summary. `spec.flick`, the Flickable the
form scrolls in, is scrolled to a field the form sends the keyboard to.

The widget lays the fields out: `form` a column under its summary with
the submit button at the end, `login_form` the same with a full-width
button, `settings_form` a column that saves itself once what changed is
valid (no button; its summary says unsaved, saving, saved), and
`inline_form` one row, the field and its button. The skin draws the
`summary` (errors, a failed send, the save status) and, through its
`message` builder (name, message, shown, width, id), each field's message
under it; the submit button is a kit Press drawn as `form_submit` (the
theme's suggested action, its loading indicator while sending). What
the policy shows, when the form may send and Return, are the
archetype's.

#### Overflow

| | |
|---|---|
| roles | toolbar |
| state | shown, hidden, overflowing, menu_open |
| signals | on_changed |
| keys | more_return_opens, menu_arrows |
| slots | background, content, more, menu |
| widgets | overflow_toolbar, overflow_tabs, overflow_breadcrumbs, chip_overflow, priority_nav |
| arrived | stage 22 |

Overflows (the Overflow archetype): items sharing a line keep the ones
that fit and put the rest behind a "more" button (priority+) -- a
toolbar's actions, a tab strip, a breadcrumb trail, a row of chips, a
site's navigation.

```lua
local node, bar = overflow.make("overflow_toolbar", {
  id = "actions", width = function() return room:get() end,
  items = {
    { icon = "content_cut", label = "Cut", on_activated = cut, priority = 2 },
    { icon = "share", label = "Share", on_activated = share, pinned = true },
    { node = some_control, label = "Zoom", on_activated = zoom },   -- a node as it is
  },
})
bar.open_menu() bar.close_menu() bar.shown() bar.hidden()

```
Each item's laid-out width is measured and the archetype decides what
fits the control's width: higher `priority` stays longer, `pinned`
never goes, equals go from the end (breadcrumbs: from the middle, so the
first and the last stay). Shown items sit in a row at running x and
spring to their places as the line changes; hidden ones are hidden. The
"more" button (the skin's `more(s)` look on a press; "…" between a
breadcrumb trail's ends) opens a menu listing the hidden items, and a
press there runs the item's `on_activated` (or `on_clicked`).

A tab strip and a navigation take `current` (a value or a binding) and
`on_current_changed(i)`: the current item is chosen and given the
highest priority, so it never goes. Other fields: `mode` ("end",
"start", "middle", "priority"), `gap`, `height`, `menu_width`, `x`,
`y`, `anchors`, `on_changed(shown, hidden)`. Ids: `<id>-item-<n>` (or
the item's own `id`), `<id>-more`, `<id>-menu`, `<id>-menu-<n>`.

### Display widgets

No input, a kit function each (`kit.<function>(spec)`), drawn by every theme;
those a theme does not draw itself come from the shared composition in
`library/lib/kit/display/` through its style (see the head of
`library/lib/kit/display/init.lua`).

| group | widget | function |
|---|---|---|
| charts | chart | `kit.chart` |
| charts | spectrum | `kit.spectrum` |
| charts | bars | `kit.bars` |
| charts | stacked | `kit.stacked` |
| charts | histogram | `kit.histogram` |
| charts | scatter | `kit.scatter` |
| charts | pie | `kit.pie` |
| charts | donut | `kit.donut` |
| charts | heatmap | `kit.heatmap` |
| charts | calendar heatmap | `kit.calendar_heatmap` |
| charts | waveform | `kit.waveform` |
| charts | spectrogram | `kit.spectrogram` |
| charts | candlestick | `kit.candlestick` |
| charts | box plot | `kit.box_plot` |
| charts | state timeline | `kit.state_timeline` |
| charts | gantt | `kit.gantt` |
| charts | treemap | `kit.treemap` |
| charts | sunburst | `kit.sunburst` |
| charts | sankey | `kit.sankey` |
| charts | funnel | `kit.funnel` |
| charts | flame graph | `kit.flame_graph` |
| charts | stacked area | `kit.stacked_area` |
| charts | radial bar | `kit.radial_bar` |
| charts | status history | `kit.status_history` |
| media | icon | `kit.icon` |
| media | image | `kit.image` |
| media | avatar | `kit.avatar` |
| media | thumbnail | `kit.thumbnail` |
| media | video | `kit.video` |
| readings | gauge | `kit.gauge` |
| readings | ring | `kit.ring` |
| readings | mini ring | `kit.mini_ring` |
| readings | bar | `kit.bar` |
| readings | meter | `kit.meter` |
| readings | fill | `kit.fill` |
| readings | vmeter | `kit.vmeter` |
| readings | dial | `kit.dial` |
| readings | radar | `kit.radar` |
| readings | cell | `kit.cell` |
| readings | stat | `kit.stat` |
| readings | triplet | `kit.triplet` |
| readings | thermometer | `kit.thermometer` |
| readings | tank | `kit.tank` |
| readings | led bar | `kit.led_bar` |
| readings | seven segment | `kit.seven_segment` |
| readings | vu meter | `kit.vu_meter` |
| readings | peak meter | `kit.peak_meter` |
| readings | compass | `kit.compass` |
| readings | sparkline | `kit.sparkline` |
| readings | segmented meter | `kit.segmented_meter` |
| status | emblem | `kit.emblem` |
| status | status line | `kit.status_line` |
| status | status | `kit.status` |
| status | chip | `kit.chip` |
| status | spinner | `kit.loading` |
| status | badge | `kit.badge` |
| status | dot | `kit.dot` |
| status | progress bar | `kit.progress` |
| status | progress ring | `kit.progress_ring` |
| status | battery | `kit.battery` |
| status | signal bars | `kit.signal_bars` |
| status | empty state | `kit.empty_state` |
| status | skeleton | `kit.skeleton` |
| status | banner content | `kit.banner` |
| status | status led | `kit.led` |
| status | tag | `kit.tag` |
| status | segmented progress | `kit.segmented_progress` |
| status | semicircle progress | `kit.semicircle` |
| status | status card | `kit.status_card` |
| status | result page | `kit.result_page` |
| status | toast content | `kit.toast` |
| structure | card | `kit.card` |
| structure | panel | `kit.panel` |
| structure | header | `kit.header` |
| structure | surface | `kit.surface` |
| structure | separator | `kit.separator` |
| structure | spacer | `kit.spacer` |
| structure | group box | `kit.group_box` |
| structure | labelled divider | `kit.labelled_divider` |
| structure | frame | `kit.frame` |
| structure | inset | `kit.inset` |
| text | label | `kit.label` |
| text | heading | `kit.heading` |
| text | subtitle | `kit.subtitle` |
| text | caption | `kit.caption` |
| text | readout | `kit.readout` |
| text | facts | `kit.facts` |
| text | body | `kit.text` |
| text | kbd | `kit.keycap` |
| text | markup | `kit.markup` |
| text | code block | `kit.code_block` |
| text | quote | `kit.quote` |
| text | mono | `kit.mono` |
| text | link text | `kit.link_text` |

### Domain instruments

Over the display widgets and the archetypes, in `library/lib/kit/domain/`:

- **audio** (stage 15): automation_lane, audio_visualiser, compressor_curve, eq_bars, lissajous, mixer_strip, piano_keyboard, parametric_eq, tuner
- **aviation** (stage 14): airspeed_tape, altimeter_tape, attitude_indicator, heading_indicator, course_deviation, eicas_strip, flight_path_marker, heading_tape, hsi, nav_display, pitch_ladder, radar_altimeter, range_rings, bank_scale, rolling_digits, turn_coordinator, vertical_speed, weather_radar
- **editor** (stage 15): envelope_editor, node_editor, piano_roll, step_sequencer
- **hud** (stage 14): health_bar, charge_ring, pie_menu, cooldown_sweep, damage_trail_bar, minimap, compass_strip, hotbar, kill_feed, objective_tracker, resource_orb, stamina_ring, xp_bar, achievement_banner, crosshair, shield_bar, ammo_counter, damage_direction, damage_numbers, pip_container, nameplate, buff_row, combo_counter, offscreen_arrow, waypoint_marker, hit_marker, lap_tracker, racing_hud, boss_bar, interaction_prompt, inventory_grid, scoreboard, subtitle_box, tick_ruler, radar_sweep, segmented_arc_ring, target_lock, scan_sweep, waveform_rings, concentric_rings, decode_text, glitch_text, hex_grid, countdown_ring, dot_matrix_progress, biometric_scan, data_stream, striped_loading, signal_noise, crt_scanlines, crosshair_grid, callout, wireframe, telemetry_block, motion_tracker, proximity_ring, bracket_tag, orbit_diagram, starfield, assistant_orb

### Composites

Shared by every theme, built only from archetypes, in
`library/lib/kit/composites/` (`require("lib.kit.composites").<name>(spec)`).

#### about dialog

Built from Popup, Navigation.

An about dialog: an application's name, version, icon and links, with
pages for its credits and its legal text (Popup, dialog + Navigation).

```lua
local node, about = composites.about_dialog {
  id = "about", root = surface_root,
  app_name = "Morf", version = "0.15", icon = "deployed_code",
  comments = "A UI engine for shells and apps.",
  links = { { label = "Website", url = "https://morf.dev" }, { label = "Report an issue", url = "..." } },
  credits = { { title = "Developers", names = { "Ada", "Linus" } }, { title = "Design", names = { "Grace" } } },
  copyright = "© 2026 The Morf authors", license = "MIT",
  legal = "Permission is hereby granted, ...",
  on_link = function(url) end,
}
about.open() about.close() about.show("credits") about.page()

```
The pages -- "about", "credits", "legal" -- are a Navigation switcher
under tabs: a press on a tab, Ctrl+Tab / Ctrl+Shift+Tab, or
`show(page)` changes the page, which slides in from its side. A page
with nothing to say (no credits, no legal text or licence) is left
out. Escape or the close button closes the dialog and focus goes back to
what opened it. `inline = true` returns the dialog as a node to place.
Other fields: `width` (420), `height` (460), `developers`, `designers`,
`artists`, `translators` (lists of names, folded into the credits),
`website` (a first link), `on_closed(reason)`. Ids: `<id>-tabs`,
`<id>-tab-<n>`, `<id>-pages`, `<id>-link-<n>`, `<id>-close`,
`<id>-popup`.

#### calendar

Built from Selection, Navigation.

A calendar (composite: Selection days + Navigation months), inline.

```lua
local node, cal = composites.calendar {
  id = "planner", width = 280,
  value = function() return day:get() end,        -- "YYYY-MM-DD"
  on_changed = function(date) day:set(date) end,  -- the current day moved
  on_picked = function(date) end,                 -- a press or Return chose it
  marked = function(date) return count(date) > 0 end,
}
cal.step(1) ; cal.show("2026-12") ; cal.month() --> "2026-10"

```
The days are a kit `day_grid` (a Selection: the arrows walk the days,
typing a number jumps to it, Return picks), one per month, in a kit
Navigation that slides one month in as the other goes. The arrows walk
across a month's edge into the next; Page Up and Page Down turn the
month (Shift: the year), keeping the day; Home and End go to the first
and last of the month.

`month` (a month index -- year * 12 + month - 1 -- or "YYYY-MM", or a
binding to either) makes the shown month the configuration's: the
arrows by the title then only ask, through `on_month(month, delta)`.
`first_weekday` (1 Monday .. 7 Sunday), `cell_height`, `header_height`,
`band` (a binding to `from, to` dates drawn as a run, for a range) and
`press_activates` (a press picks, not only moves) are optional; the
calendar is as tall as the month's weeks unless `fit = false`. Ids:
`<id>-previous`, `<id>-next`, `<id>-month-title`, `<id>-day-<date>`.

#### carousel

Built from Navigation, Selection, Drag.

A carousel (composite: Navigation carousel + Selection dots + Drag swipe).

```lua
local node, car = composites.carousel {
  id = "tips", width = 520, height = 300,
  slides = {
    { title = "Welcome", subtitle = "Swipe to go on", icon = "waving_hand" },
    { content = function(w, h) return my_node end },   -- a node or a builder
  },
  current = 1, wrap = false,
  on_changed = function(index) end,
}
car.next() ; car.previous() ; car.go(3) ; car.current()

```
The slides are a kit `carousel` (a Navigation: each slide is built
when first shown and slides in from its side; Left and Right walk it
once it has focus). A swipe across the slide (a Drag swipe: the slide
follows the finger and springs back when let go short) turns it; the
arrows at its sides and the dots under it (a kit `carousel_dots`
Selection) go too. Ids: `<id>-slides`, `<id>-slide-<i>`,
`<id>-swipe`, `<id>-previous`, `<id>-next`, `<id>-dots`, `<id>-dot-<i>`.

#### colour picker

Built from Plane, Range, TextField, Selection, Popup.

A colour picker (composite: Plane + Range hue + Range alpha + TextField
hex + Selection swatches; in a Popup when asked).

```lua
local node, picker = composites.colour_picker {
  id = "tint", width = 320, height = 300,
  value = function() return tint:get() end,     -- any colour notation
  on_changed = function(hex) tint:set(hex) end, -- "#rrggbb" ("#rrggbbaa" with alpha)
  alpha = true, swatches = { "#e53935", ... },  -- or values / bindings
}

```
A kit `colour_plane` holds saturation across and value up, under a hue
slider and an alpha slider (kit Ranges: dragged, or stepped by the
arrows once focused); a hex field (a kit entry: Return takes what is
typed, Escape gives it back) beside a sample of the colour; and a
`swatch_grid` (a kit Selection: the arrows walk it, a press or Return
takes a swatch). Without `swatches` it offers twelve hues and greys.
`trailing` (a node) goes at the end of the hex row; `popup = true`
makes it a press showing the colour that opens it in a popover. Ids:
`<id>-plane`, `<id>-hue`, `<id>-alpha`, `<id>-hex`, `<id>-swatches`,
`<id>-swatch-<i>`, `<id>-field`, `<id>-popup`.

#### combo box

Built from Press, Popup, Selection.

A combo box: a closed field showing the current item that opens a list
of them (Press + Popup + Selection); with `search = true` the field is a
text field that filters the list as it is typed into (TextField + Popup
+ Collection: autocomplete).

```lua
local node, combo = composites.combo_box {
  id = "size", width = 220, items = { "Small", "Medium", "Large" },
  current = 2,                      -- or a function (then follow it in on_changed)
  on_changed = function(index, item) end,
  variant = "dropdown",             -- "select": no ground, for a row's end
  placeholder = "Choose…", search = false,
}
combo.open() combo.close() combo.current() combo.set(3)

```
Items are strings or tables (`label`, `icon`, ...). The field opens on a
press, Space, Return and Alt+Down; the list's arrows move, Return or a
press picks, Escape closes, and focus goes back to the field. Other
fields: `x`, `y`, `height` (40), `icon` (leading), `item_height` (36),
`visible_items` (8), `list_width`, `placement` ("bottom-start"),
`header` (a node over the list), `item_id(index, item)` (each entry's
id; `<id>-item-<index>` otherwise), `except` (nodes a press on which
does not close the list), `text_size`, `accessible_name`,
`on_opened`, `on_closed(reason)`.

The field's look -- a theme's card under a menu row's wash, the label
and a chevron -- is exported as `field(spec)` for the composites that
open something the same way (picker, rows.combo_row).

#### command palette

Built from Popup, TextField, Collection.

A command palette: a search field over a grouped list of commands that
narrows as it is typed into, in a popup (Popup + TextField + Collection).

```lua
local node, palette = composites.command_palette {
  id = "palette", root = surface_root,        -- Ctrl+K and Ctrl+Shift+P open it there
  commands = {
    { title = "Open file", subtitle = "Browse the disk", icon = "folder_open",
      shortcut = "Ctrl+O", group = "File", action = open_file },
    { title = "Toggle sidebar", icon = "side_navigation", shortcut = "Ctrl+B",
      group = "View", keywords = "panel", action = toggle },
  },
  on_run = function(command, index) end,
}
palette.open() palette.close() palette.toggle() palette.is_open()

```
Commands are `{ title, subtitle, icon, shortcut, group, keywords, action,
id }` (or a function returning the list). The query ranks them fuzzily
by title and keywords; they stay under their groups' headers, the groups
in the order of their best match. Up, Down, Page_Up and Page_Down walk
the commands from the field, Return or a press runs one (closing the
palette first), Escape closes it and focus goes back to what had it.
`inline = true` returns the palette as a node to place, not a popup (a
gallery, a launcher's page). Other fields: `width` (520), `list_height`
(320), `placeholder`, `anchor` + `placement` (beside a node; centred on
`root` otherwise), `shortcuts` (false: no Ctrl+K), `on_opened`,
`on_closed(reason)`. The node returned is the inline palette, or the
holder of the shortcuts (already on `root`), or nil. Ids: `<id>-search` (the field), `<id>-list`,
`<id>-command-<n>` (a command by its index in `commands`), `<id>-popup`.

#### dashboard

Built from Collection, Drag.

A dashboard (composite: a grid Collection of tiles + Drag to rearrange).

```lua
local node, dash = composites.dashboard {
  id = "home", width = 520, height = 340,
  tiles = {
    { key = "cpu", title = "CPU", icon = "memory", value = function() return "12%" end, level = 0.12 },
    { key = "net", title = "Network", icon = "wifi", value = "48 Mb/s",
      build = function(w, h) return node end },        -- a tile's own body
  },
  size = "medium",                                       -- small, medium, large
  on_reordered = function(keys) end, on_size = function(size) end,
}
dash.move("cpu", 3) ; dash.order() --> { "net", ... } ; dash.set_size("large")

```
The tiles are a kit `grid_view` (a Collection: the arrows walk the grid,
typing jumps to a tile, Return or a double press calls `on_activated`).
A tile dragged past its threshold (a Drag transfer) leaves a ghost
under the pointer and lands where it is let go, the others making
room; Alt and an arrow moves the current tile. The size switch (a kit
`segmented` Selection) sets every tile's size: small tiles show the
figure, larger ones its level as a bar too and a `build`er's body.
Ids: `<id>-grid`, `<id>-tile-<key>`, `<id>-ghost`, `<id>-size`,
`<id>-size-<size>`.

#### date picker

Built from Press, Popup, Selection, Navigation.

A date picker and a date range picker (composite: Press + Popup +
Selection day grid + Navigation months).

```lua
local node, picker = composites.date_picker {
  id = "due", width = 200,
  value = function() return due:get() end,       -- "YYYY-MM-DD" or ""
  on_changed = function(date) due:set(date) end,
}
composites.date_picker { id = "trip", range = true,
  from = "2026-10-03", to = "2026-10-09",
  on_changed = function(from, to) end }

```
A press shows the date (`format`, strftime's, "%d %b %Y"; `placeholder`
while there is none) and opens a popover of the month around it -- a
kit calendar (lib.kit.composites.calendar): the arrows walk the days
and across months, Page Up and Page Down turn the month, Home and End
go to its ends, Return or a press picks and closes. A range picker
takes two picks, the first its start, and draws the run between.
`inline = true` gives the calendar alone, picking in place; `compact =
true` makes the press an icon (beside a field the date is also typed
in). Ids: `<id>-field` (the press), `<id>-popup`, and the calendar's
under `<id>` (`<id>-day-<date>`, `<id>-next`, ...).

#### emoji picker

Built from TextField, Selection.

An emoji picker (composite: TextField search + Selection category tabs +
Selection grid of emoji).

```lua
local node, picker = composites.emoji_picker {
  id = "emoji", width = 340, height = 320,
  on_picked = function(emoji, name) insert(emoji) end,
}

```
A search field (a kit `search` entry) over a row of categories (a kit
segmented Selection, each its icon) and a kit `emoji_grid` (a
Selection: the arrows walk it, a press or Return picks) in a scrolled
view, with the emoji the grid is on named under it. Typing searches
every category by name and keywords, fuzzily; Down from the search goes
to the grid, Return there picks the first match. `emoji` replaces the
built-in table (below): a list of `{ key, name, icon, list = { { emoji,
name, keywords }, ... } }`. Ids: `<id>-search`, `<id>-categories`,
`<id>-category-<key>`, `<id>-grid`, `<id>-emoji-<i>`, `<id>-name`.

#### file chooser

Built from Shell, Collection, Navigation, TextField.

A file chooser (composite: Shell + Collection of files + Navigation of
folders + TextField path and name).

```lua
local node, chooser = composites.file_chooser {
  id = "open", width = 640, height = 420,       -- numbers or bindings
  mode = "open",                                -- "save", "folder"; or a binding
  root = "/",                                   -- nothing above it is shown
  path = "~/Pictures",                          -- where it starts (root otherwise)
  name = "capture.png",                         -- save: the name it suggests
  filters = { { name = "Images", patterns = { "png", "jpg", "webp" } }, { name = "All files" } },
  places = { { label = "Home", path = "~", icon = "home" }, ... },   -- the sidebar's
  on_accepted = function(path, info) end,       -- info: { folder, name, is_dir }
  on_cancelled = function() end,
}
chooser.navigate("/tmp") ; chooser.path() ; chooser.selected() ; chooser.accept()

```
The window is a kit Shell (`window_layout`): the places in its sidebar,
which narrower than `collapse_below` becomes a drawer over the files
(F9, Ctrl+B or the header's press open it; Escape or a press outside
shuts it; F6 moves between the places and the files). Its header holds
Back, Up, the breadcrumbs (a kit `breadcrumbs` Selection: a press goes
to that folder) and the path (a kit `entry`: Return goes there). Each
folder is a page of a kit `navigation_view` (a Navigation: going into
a folder pushes it, Back and Alt+Left pop, a breadcrumb or a place goes
straight there), and lists its entries in a kit `file_list` (a
Collection: the arrows walk it, typing jumps, a press chooses, Return or
a double press opens a folder or accepts a file; BackSpace goes up).
Under the files: the name to save as (save), the filter (a combo box)
and Cancel and the accepting press. Folders list first, then files the
filter lets through (none in folder mode). The filesystem is read with
`morf.fs.list`; a folder that cannot be read says why.
Ids: `<id>-sidebar-toggle`, `<id>-back`, `<id>-up`, `<id>-crumbs`,
`<id>-crumb-<i>`, `<id>-path`, `<id>-places`, `<id>-place-<i>`,
`<id>-folders`, `<id>-files` (each folder's list), `<id>-entry-<name>`,
`<id>-name`, `<id>-filter`, `<id>-status`, `<id>-cancel`, `<id>-accept`.

#### font picker

Built from TextField, Collection.

A font picker (composite: TextField search + Collection + preview).

```lua
local node, picker = composites.font_picker {
  id = "font", width = 360, height = 320,
  fonts = { "Inter", "IBM Plex Mono", ... },   -- or a binding
  value = function() return font:get() end,
  on_picked = function(family) font:set(family) end,
}

```
A search field (a kit `search` entry) over the families, a kit list (a
Collection: the arrows walk it, typing jumps, Return or a press picks)
whose rows set each family's name in the theme's face and a line in
the family itself, and under them the family the list is on in a
larger sample (`sample = false` leaves it out). The families are
`fonts`, or `morf.text.families()` where the engine lists them. The
search ranks them fuzzily (`filter = false` leaves the list as given
and only calls `on_search(query)` -- a configuration that pages its own
list); Return in the search picks the first. An empty name keeps a
row's place, empty. `status` (a binding) is said while the list is
empty. Ids: `<id>-search`, `<id>-list`, `<id>-option-<i>`,
`<id>-preview-<i>`, `<id>-sample`.

#### header bar

Built from Shell, Press.

A header bar (composite: Shell slot + Press): a window's title, its own
controls at the start and end, and the window controls -- minimise,
maximise, close -- for a window that decorates itself.

```lua
local node = composites.header_bar {
  id = "header", width = function() return win.width end, window = win,
  title = "Settings", subtitle = function() return page:get() end,
  start = { toggle_button }, ["end"] = { menu_button },
}

```
A press on the bar's empty part moves the window (`start_system_move`),
a double press maximises or restores it. The controls are kit icon
presses with names a screen reader reads. `controls = false` leaves the
window controls out (a dialog, a phone). `flat = true` leaves out its
ground and rule: a kit Shell's skin draws them under its header region.

#### input group

Built from TextField, Press.

An input group: a text field with parts before and after it, on one
ground -- "−" [value] "+", "https://" [address] "Go" (TextField + Press).

```lua
local node, input = composites.input_group {
  id = "url", width = 360, height = 40,
  prefix = "https://",                                  -- a label
  suffix = { label = "Go", on_clicked = go },           -- a button
  placeholder = "example.org", on_accepted = go,
}

```
`prefix` and `suffix` are a part or a list of them: a string is a label,
a table with `on_clicked` a button (`label` makes a filled button,
`icon` alone an icon button; `id`, `width`, `auto_repeat`), a node is
placed as it is. The field takes a text field's own fields (`widget`
-- "entry", "search", "numeric_entry", ... --, `text`, `placeholder`,
`validator`, `on_edited` (each edit; `on_text_changed` is heard twice), `on_accepted`, `on_escape`,
`on_key_pressed`, ...); its id is the group's. `variant = "select"`
leaves the ground off (a row draws its own). Returns the group's node,
the text input, and the field's live state and control.

`style(props)` gives a text input the theme's type and inks (its face
read off a kit text, its colours the kit's inks), for the composites
that make inputs of their own.

#### kanban

Built from Collection, Drag.

A kanban board (composite: a Collection per column + Drag to move cards).

```lua
local node, board = composites.kanban {
  id = "work", width = 520, height = 340,
  columns = {
    { key = "todo", title = "To do", cards = { { key = "a", title = "Write docs", tag = "docs" }, ... } },
    { key = "doing", title = "Doing", cards = { ... } },
    { key = "done", title = "Done", cards = {} },
  },
  on_moved = function(card_key, from_column, to_column, index) end,
}
board.move("a", "done") ; board.cards("todo") --> { "b", "c" }

```
Each column is a kit `kanban_column` (a Collection: the arrows walk its
cards, typing jumps to one). A card is dragged (a Drag transfer: past
its threshold a ghost of it follows the pointer) onto another column
or another place in its own, the drop line showing where it lands. The
keyboard moves the current card: Alt+Left and Alt+Right to the column
beside, Alt+Up and Alt+Down within its column. Ids: `<id>-column-<key>`,
`<id>-list-<key>`, `<id>-card-<card key>`, `<id>-ghost`, `<id>-count-<key>`.

#### media controls

Built from Press, Range.

Media controls: transport presses, a seek bar between the position and
the length, and a volume slider (Press + Range).

```lua
local node = composites.media_controls {
  id = "player", width = 480,
  playing = function() return state.playing end,
  position = function() return state.position end,   -- seconds
  length = function() return state.length end,       -- seconds
  volume = function() return state.volume end,       -- 0..1
  on_play_pause = toggle, on_previous = prev, on_next = next,
  on_seek = function(seconds) end, on_volume = function(v) end,
}

```
The presses are `<id>-previous`, `<id>-play`, `<id>-next`; the seek bar
`<id>-seek` (it seeks on the release; the arrows step it) and the volume
`<id>-volume`. Leave out `on_volume` (or `volume = false`) for no volume
slider. `height` is the transport row's (48).

#### menu button

Built from Press, Popup.

A menu button: a button that opens a menu under it (Press + Popup); with
`split = true`, a split button -- a primary action, and an arrow beside
it that opens the menu.

```lua
local node, menu = composites.menu_button {
  id = "file", label = "File", icon = "description", width = 140,
  items = {
    { label = "New", icon = "add", on_clicked = new },
    { label = "Wrap lines", checked = function() return wrap:get() end, on_toggled = set_wrap },
  },
  split = false, on_clicked = save,     -- the primary action, when split
}
menu.open() menu.close() menu.is_open()

```
The button (or the arrow) opens on a press, Space, Return and Alt+Down;
the menu's own keys walk it, Return runs an item, Escape closes it and
focus goes back to the button. Items are a popup menu's (lib.kit.popup:
`label`, `icon`, `on_clicked`, `checked`/`on_toggled`, `group`, `id`;
`<id>-item-<index>` otherwise). Other fields: `x`, `y`, `height` (36),
`widget` (the button's Press widget, "pill"), `menu_width` (200),
`placement` ("bottom-start"), `on_opened`, `on_closed(reason)`,
`accessible_name`.

#### notification stack

Built from Popup, Collection, Drag.

A notification stack: cards that come in at the top, newest first, stay
for a while and go -- by a swipe, their close button or their timeout
-- in a non-modal overlay at a corner of the surface (Popup + Collection
+ Drag, swipe). With `mode = "toast"` it is a toast overlay: slips at
the bottom centre in the theme's toast look, a few seconds each.

```lua
local node, stack = composites.notification_stack {
  id = "notes", root = surface_root, width = 360,
  timeout = 6000,                        -- ms; 0 keeps them until dismissed
  on_activated = function(note) end, on_dismissed = function(note, reason) end,
}
local key = stack.notify { title = "Mail", body = "Three new messages", icon = "mail",
  app = "Mail", actions = { { label = "Open", on_clicked = open_mail } } }
stack.dismiss(key) stack.clear() stack.count() stack.expand(key)

```
A note is `{ title, body, icon, app, time, urgency ("critical" stays),
timeout, actions, key }`. Its card shows the icon, the app and the time,
the title and the body's first line; the chevron expands it to the
whole body and its actions. A press on the card is `on_activated`. A
drag sideways past `swipe_distance` (80 px) or a fling dismisses it; let
go short and it springs back. The overlay opens with the first note and
closes with the last; it never takes focus or a press outside it.
`inline = true` returns the stack as a node to place instead. Other
fields: `max_visible` (4; toasts 3) -- how many cards the stack is tall
-- `anchor` + `placement` (a corner node of the caller's; by default
the root's top right, "bottom-end", or its bottom centre for toasts),
`notifications` (shown at once). Ids: `<id>-list`, `<id>-note-<key>`,
`<id>-close-<key>`, `<id>-expand-<key>`, `<id>-action-<key>-<n>`.

#### picker

Built from Press, Popup, Selection.

A picker: choose one of a set from a popup grid or list (Press + Popup +
Selection) -- an icon, a size, a mode.

```lua
local node, pick = composites.picker {
  id = "icon", width = 200, columns = 5,
  items = { { icon = "home", label = "Home" }, { icon = "star", label = "Star" }, ... },
  current = 1, on_changed = function(index, item) end,
  layout = "grid",                  -- or "list"
}

```
The closed field shows the current item (its icon, or its label) and
opens on a press, Space, Return and Alt+Down. In the popup the arrows
move (across and down a grid), Return or a press picks, Escape closes,
and focus goes back to the field. Items with an icon are drawn as that
icon; others by the selection's skin. Other fields: `x`, `y`, `height`
(40), `cell` (44, a grid cell's side), `item_height` (36, a list's
rows), `popup_width`, `placement`, `placeholder`, `item_id(index,
item)` (`<id>-item-<index>` otherwise), `accessible_name`.

#### rows

Built from Press, Range, TextField, Disclosure; variants: action_row, switch_row, check_row, combo_row, entry_row, spin_row, expander_row, button_row, property_row, preferences_group, preferences_page.

Preference rows (a row frame with a Press, Range, TextField or
Disclosure in it), the boxed group they stand in, and the scrolled page
of groups.

```lua
local rows = require("lib.kit.composites.rows")
rows.preferences_page { width = 480, height = 400, groups = {
  { title = "Display", description = "How it looks", rows = {
    { kind = "switch_row", title = "Dark mode", active = dark, on_toggled = set_dark },
    { kind = "combo_row", title = "Scale", items = { "100%", "125%" }, current = 1, on_changed = set_scale },
    { kind = "spin_row", title = "Font size", value = 11, from = 6, to = 48, on_changed = set_size },
  } },
} }
rows.make { kind = "action_row", title = "About", on_activated = about }

```
Every row takes `id`, `width` (360), `height`, `title`, `subtitle`,
`icon` (leading) and returns its node and a handle. The kinds:

| row | what is in it |
|---|---|
| action_row | `on_activated` makes the row a press (a chevron when `chevron ~= false`); `suffix`, a node at its end |
| switch_row | a switch: `active` (a value or fn), `on_toggled(on)`; a press on the row toggles it too |
| check_row | a checkbox before the title: `active`, `on_toggled(on)` |
| combo_row | a combo box at its end: `items`, `current`, `on_changed(index, item)` |
| entry_row | the title over a text field: `text`, `placeholder`, `on_changed(text)`, `on_accepted(text)` |
| spin_row | − value +: `value`, `from`, `to`, `step`, `digits`, `on_changed(value)`; arrows step it |
| expander_row | a disclosure: `rows` (nodes or row specs) shown while expanded, `expanded` |
| button_row | a centred press: `on_activated` |
| property_row | a title over a read-only value: `value` (a value or fn) |

`preferences_group { title, description, rows, width }` boxes rows (nodes
or specs with `kind`) on the theme's card with separators between;
`preferences_page { groups, width, height, gap }` scrolls groups (nodes
or group specs). `make(spec)` builds `spec.kind` (a row, a group or a
page).

#### search bar

Built from TextField, Disclosure.

A search bar: a search field that slides into view on Ctrl+F (or its
toggle) and away on Escape (TextField + Disclosure).

```lua
local node, bar = composites.search_bar {
  id = "find", width = 480, placeholder = "Search",
  on_search = function(text) end,           -- as it is typed
  on_accepted = function(text) end,         -- Return
  revealed = false, toggle = true,          -- a search button beside a title
  title = "Files",                          -- the strip the toggle sits in
}
bar.reveal() bar.hide() bar.revealed()

```
Ctrl+F reveals it and puts the keys in it (from anywhere on the surface
unless `scope = "local"`, when only from inside it); Escape clears it,
hides it and gives focus back to what had it. `on_revealed(open)`
follows. Other fields: `x`, `y`, `height` (the field's, 40), `text`.

#### shortcuts window

Built from Popup, Collection, TextField.

A shortcuts window: an application's keys, grouped, with a search that
narrows them, in a dialog (Popup + Collection, grouped + TextField).

```lua
local node, keys = composites.shortcuts_window {
  id = "keys", title = "Keyboard shortcuts", root = surface_root,
  groups = {
    { title = "General", shortcuts = {
      { keys = "Ctrl+K", description = "Command palette" },
      { keys = "Ctrl+K Ctrl+S", description = "Save all" },      -- a sequence
    } },
  },
}
keys.open() keys.close() keys.is_open() keys.filter("save")

```
Each shortcut's `keys` is a chord (`"Ctrl+Shift+P"`), a sequence of
chords apart by spaces, or a list of either (alternatives); every key is
drawn as the theme's keycap. The search ranks the shortcuts fuzzily by
description and keys and keeps them under their groups. Escape closes
the window and focus goes back to what opened it; the list scrolls with
the wheel and walks with the arrows once it has focus (Down from the
search goes to it). `open_key` (e.g. `"ctrl+?"`) on `root` opens it.
`inline = true` returns the window as a node to place. Other fields:
`width` (480), `height` (440), `placeholder`, `on_closed(reason)`. Ids:
`<id>-search`, `<id>-list`, `<id>-row-<g>-<n>` (group, shortcut),
`<id>-close`, `<id>-popup`.

#### sidebar

Built from Selection, Disclosure.

A sidebar (composite: Selection items + Disclosure sections).

```lua
local node, side = composites.sidebar {
  id = "nav", width = 240, height = 340,
  sections = {
    { title = "Library", items = {
      { key = "inbox", label = "Inbox", icon = "inbox", badge = 4 },   -- badge: a count, a word or a binding
      { key = "sent", label = "Sent", icon = "send" } } },
    { title = "Labels", expanded = false, items = { ... } },
  },
  current = "inbox",                     -- a key, or a binding
  on_changed = function(key, item) end,
  collapsed = false,                     -- icon-only; a binding, or the toggle's
  on_collapsed = function(collapsed) end,
}
side.select("sent") ; side.collapse(true) ; side.current() ; side.toggle_section(2)

```
Each section is a kit `collapsible_section` (a Disclosure: a press,
Space or Return folds it, Left and Right close and open) holding a kit
`sidebar_list` (a Selection: the arrows walk it, typing jumps, a press
chooses). An item is its icon, label and badge. The toggle at the top
folds the whole sidebar to its icons -- one list of every item -- and
back, the width easing
between. Ids: `<id>-toggle`, `<id>-section-<s>`, `<id>-list-<s>`,
`<id>-item-<key>`, `<id>-rail`, `<id>-rail-<key>`.

#### status bar

Built from Press.

A status bar: a strip along the foot of a window or a panel with what is
going on -- words, icons, badges, a progress bar -- in a left, a centre
and a right zone; an item with `on_clicked` is a press (display widgets
+ Press items).

```lua
local node, bar = composites.status_bar {
  id = "status", width = 800,
  left = { { icon = "check_circle", text = "Ready" },
           { id = "branch", icon = "commit", text = function() return branch:get() end, on_clicked = pick } },
  center = { { kind = "progress", value = function() return done:get() end, width = 120 } },
  right = { { text = "Ln 12, Col 4", on_clicked = go_to_line }, { kind = "badge", count = 3, kind_tone = "alert" },
            { icon = "notifications", tooltip = "Notifications", on_clicked = show } },
}

```
An item is `{ text, icon, kind, tooltip, on_clicked, id, width }`;
`text` and `icon` may be bindings. `kind` is how it reads: "text" (the
default: an icon and words), "label" (the quieter label face), "badge"
(`count` or `text`, `tone`), "dot" (a status dot, `tone`, `pulse`),
"progress" (`value` 0..1, `width`), "separator", or "node" (`node`, the
caller's own). A pressable item takes a hover wash and the keyboard's
ring: Tab reaches it, Space and Return press it; one with only an icon
shows `tooltip`. The centre zone stays centred whatever the sides hold.
Other fields: `x`, `y`, `anchors`, `height` (30), `ground` (false: no
tonal strip), `accessible_name`. Ids: an item's `id`, or
`<id>-<zone>-<n>`.

#### tab view

Built from Selection, Navigation, Drag, Collection.

A tab view with an overview (composite: Selection tabs + Navigation pages
+ Drag reorder + Collection overview grid).

```lua
local node, tabs = composites.tab_view {
  id = "docs", width = 520, height = 340,
  tabs = { { title = "Notes", icon = "description", content = build_notes }, ... },
  current = 1, closable = true, addable = true,
  on_add = function(n) return { title = "Tab " .. n } end,   -- the new tab (nil: none)
  on_changed = function(index, tab) end,
  on_closed = function(tab) end, on_reordered = function(titles) end,
}
tabs.add { title = "More" } ; tabs.close(2) ; tabs.select(1) ; tabs.toggle_overview()

```
The tab strip is a kit `tabs` Selection (the arrows walk it; Alt+Left
and Alt+Right, or a drag along the strip, move the current tab), each
tab with its icon, title and a close press. The pages are a kit
`view_stack` (a Navigation: the new page slides in from its side; each
is built when first shown and kept). The overview press swaps the page
for a kit `grid_view` (a Collection) of the tabs as cards: a press or
Return opens one. Ctrl+T adds a tab, Ctrl+W closes the current one.
A tab's `content` is a node or a builder `function(width, height)`.
Ids: `<id>-tabs`, `<id>-tab-<i>`, `<id>-close-<i>`, `<id>-add`,
`<id>-overview`, `<id>-pages`, `<id>-page-<i>` (the page of the tab at
`i` when it was built), `<id>-grid`, `<id>-card-<i>`.

#### tag input

Built from TextField, Press.

A tag input: a text field that turns what is typed into removable chips
(TextField + Press).

```lua
local node, tags = composites.tag_input {
  id = "labels", width = 420, tags = { "lua", "ui" },
  placeholder = "Add a tag", on_changed = function(list) end,
  suggestions = nil, unique = true,
}
tags.add("rust") tags.remove(1) tags.list()

```
Return or a comma adds what is typed; Backspace in an empty field
removes the last chip; a press on a chip (or Return/Space on it) removes
it. Chips are `<id>-tag-<index>`. Chips wrap onto new lines as the
width fills; `height` is one line's (40) and the node grows with them.

#### time picker

Built from Range, Popup.

A time picker (composite: Range spins + Popup).

```lua
local node, picker = composites.time_picker {
  id = "alarm", value = function() return alarm:get() end,  -- "HH:MM" (24 h)
  on_changed = function(time) alarm:set(time) end,
  seconds = false, twelve_hour = false, minute_step = 5,
}

```
Each field -- hours, minutes, seconds -- is a spin: a kit Range that
wraps round (23 goes on to 00), the reading between a step up and a
step down. With focus the arrows step it, Page Up and Page Down by a
larger step, Home and End go to its ends, the wheel turns it, and
typing two digits sets it; Tab goes to the next field. `twelve_hour`
reads the hours 1 to 12 beside an AM/PM choice (a kit segmented
Selection); the value is always "HH:MM[:SS]", 24 h. The press shows the
time and opens the spins in a popover; `inline = true` gives the spins
alone. Ids: `<id>-field`, `<id>-popup`, `<id>-hours` (`-minutes`,
`-seconds`; each with `-up` and `-down`), `<id>-meridiem`.

#### toolbar

Built from Press, Popup.

A toolbar: a row of tool buttons, toggles and separators; what does not
fit its width goes into an overflow menu at its end (Press items +
overflow Popup).

```lua
local node, bar = composites.toolbar {
  id = "tools", width = 420,                  -- or a binding: it refits as it changes
  items = {
    { id = "new", icon = "add", label = "New", on_clicked = new },
    { id = "bold", icon = "format_bold", tooltip = "Bold", checked = false, on_toggled = set_bold },
    { separator = true },
    { id = "share", icon = "share", tooltip = "Share", on_clicked = share },
  },
}
bar.shown() bar.overflowed() bar.open_overflow()

```
An item with `label` shows it beside its icon (`show_label = false`
keeps it in the menu only); one without shows its icon and a tooltip.
`checked` (true, false or a binding) makes a toggle: it stays down
while on, and `on_toggled(on)` hears it. When the items are wider than
the toolbar the last ones leave it, a "more" button takes their place
and its menu lists them -- a press there runs the item (or toggles it)
as the button would. The buttons are one Tab stop (the Roving archetype,
headless): Tab enters at the one last used, Left/Right, Home and End
walk the shown ones, passing over disabled ones; Space and Return press
it. What fits is the Overflow archetype's (headless): higher `priority`
stays longer. Other fields: `x`, `y`, `height` (40), `menu_width` (220),
`accessible_name`. Ids: `<id>-item-<n>` (or the item's own `id`),
`<id>-more`, `<id>-menu`, `<id>-menu-<n>`.

#### tour

Built from Popup, Navigation.

A tour (coachmarks): a card beside each thing it points out in turn,
with a ring round that thing, Back / Next / Skip and a dot per step
(Popup anchored to its targets + Navigation, a wizard of steps).

```lua
local node, tour = composites.tour {
  id = "tour", root = surface_root,
  steps = {
    { target = search_field, title = "Search", body = "Find anything from here." },
    { target = function() return sidebar end, title = "Sidebar", body = "..." },
  },
  on_finished = function() end, on_skipped = function(step) end,
}
tour.start() tour.next() tour.back() tour.skip() tour.step() tour.is_open()

```
Each step's card opens beside its `target` (a node, or a function
returning one) by the tour's `placement` ("bottom" -- it flips where
there is no room); a step without a target is centred on `root`. A ring, the theme's
accent, is drawn round the target over everything. Next on the last
step finishes; Skip or Escape ends it early. Right / Left (and Return
for Next) walk it while the card has focus. `inline = true` returns
the card as a node to place (it points at nothing: a gallery's, or a
page's onboarding). Other fields: `width` (300), `ring_padding` (6),
`labels` ({ next, back, skip, done }). Ids: `<id>-card`, `<id>-steps`,
`<id>-next`, `<id>-back`, `<id>-skip`, `<id>-dot-<n>`, `<id>-ring`.

#### transfer list

Built from Collection, Press.

A transfer list (composite: two Collections + Press move buttons).

```lua
local node, transfer = composites.transfer_list {
  id = "columns", width = 520, height = 300,
  items = { "Name", "Size", "Type", "Modified" },   -- strings or { key, label, icon }
  chosen = { "Name" },                             -- keys (or labels) already on the right
  titles = { "Available", "Shown" },
  on_changed = function(chosen_keys) end,
}
transfer.move_right() ; transfer.move_left() ; transfer.chosen() --> { "Name", ... }

```
Each side is a kit `transfer_list` (a Collection in multi mode: a press
toggles an item into the selection, Space too, Ctrl+A takes them all,
the arrows walk it). Between them, presses move the selected items
across (or all of them); a double press or Return moves the current
one. A side's title counts what it holds and what is selected. Ids:
`<id>-left`, `<id>-right`, `<id>-left-<key>`, `<id>-right-<key>`,
`<id>-add`, `<id>-add-all`, `<id>-remove`, `<id>-remove-all`.

#### wizard

Built from Navigation, Selection.

A wizard, or stepper (composite: Navigation wizard + Selection header).

```lua
local node, wiz = composites.wizard {
  id = "setup", width = 520, height = 340,
  steps = {
    { title = "Account", content = function(w, h) return form end,
      validate = function() if name == "" then return false, "Enter a name" end return true end },
    { title = "Theme", content = ... },
    { title = "Done", content = ... },
  },
  on_finish = function() end, on_cancel = function() end,   -- on_cancel: a Cancel press
  on_changed = function(index) end,
}
wiz.next() ; wiz.back() ; wiz.go(2) ; wiz.current() ; wiz.finished()

```
The header is a kit `stepper_header` (a Selection: each step's number,
or a tick once passed, and its title; the arrows walk it). The pages are
a kit `wizard` (a Navigation: each built when first shown, the next
sliding in from its side; Alt+Left goes back). Next and Finish ask the
step's `validate` first: false (and a message) keeps it there and says
why. Going on through the header asks every step on the way; going back
never asks. Alt+Right is Next. Ids: `<id>-header`, `<id>-step-<i>`,
`<id>-pages`, `<id>-page-<i>`, `<id>-back`, `<id>-next`, `<id>-finish`,
`<id>-cancel`, `<id>-error`.
