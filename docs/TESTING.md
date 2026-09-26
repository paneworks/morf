# Running a configuration without a compositor

`morf` normally runs a configuration on a Wayland compositor: its loop is
the compositor's frame callbacks, its sizes come from configure events, and
its input is the seat's. Three commands run one with none of that, for CI,
for whoever is writing a configuration, and for tests that have to give the
same answer every time:

| command       | what it does |
|---------------|--------------|
| `morf check`  | loads it, runs it briefly, lays out every surface, and reports what went wrong |
| `morf render` | draws a surface (or the whole screen) to a PNG with the real GPU renderer |
| `morf test`   | runs spec files that load configurations and drive them: clicks, keys, time, IPC |

All three share one way of running a configuration, *headless*:

- **Nothing connects to Wayland.** There is no client at all. The runners
  also remove `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`,
  `HYPRLAND_INSTANCE_SIGNATURE`, `NIRI_SOCKET` and `SWAYSOCK` from their own
  environment before anything runs, so neither the configuration nor a
  program it starts can find the session they were launched from.
- **Screens are made up.** `--size WxH` (default `1920x1080`) is every
  screen's logical size; `morf.screens` lists `HEADLESS-1`, `HEADLESS-2`, …
  side by side. `morf.capabilities.headless` is `true`.
- **Surfaces are sized as a compositor would size them**, from
  `morf.surface` (and each `morf.window` surface's settings): anchored to
  both edges of an axis is the screen's extent, otherwise the size asked
  for, or the root's own `width`/`height` when it asked for none. A layer
  surface is placed by its anchors and margins, centred on an axis it is
  anchored to neither or both ends of.
- **Time is virtual.** Every `morf.timer` and `ui.Timer` runs on a clock
  that starts at zero and moves only when the runner moves it; animations
  advance by the frames the runner steps (16 ms each). Ten seconds of a
  configuration's time take as long as the work in them, and fire the same
  timers in the same order every run. Things outside the process -- a
  child's output, a D-Bus reply, a file -- still arrive on the wall clock.
- **Input goes through the shell's own paths.** A click is the same
  `LayerEvent` the pointer code handles in a running shell, hit-tested
  against the layout the surface was last laid out with; a key goes to the
  focused node of the surface exactly as it would from a keyboard.

## Keeping away from the session

A configuration may talk to the session bus, write its settings, and start
programs. The runners have three switches for how much of that reaches the
machine:

| option          | effect |
|-----------------|--------|
| `--no-dbus`     | the session and system bus addresses point nowhere: every bus call fails at once, as on a machine with no bus |
| `--private-bus` | starts a `dbus-daemon` of the run's own, with no services, as the session bus (the system bus points nowhere); stopped when the run ends. For a configuration that owns a name -- a notification server, a tray watcher -- without taking it from the real one |
| `--isolate`     | points `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` and `XDG_CACHE_HOME` at a scratch folder under `$TMPDIR`, removed afterwards. **On by default for `morf test`** (`--no-isolate` turns it off) |

Without `--no-dbus` or `--private-bus` a configuration that asks for the
bus gets the real one, which is what a check of "does it load on this
machine" usually wants and what a test never should. Programs started with
`morf.run` and `morf.spawn` are real; `morf test` can stub `morf.run` (see
below).

## `morf check`

```sh
morf check shell.lua [--size WxH] [--screens N] [--ipc 'VERB ARGS']...
                     [--after MS] [--wait MS] [--strict]
                     [--no-dbus | --private-bus] [--isolate] [-- args...]
```

Loads the configuration once per screen, makes any `--ipc` calls (each
followed by a settle), advances `--after` milliseconds of virtual time
(default 500), and prints:

- each surface: its label, size, position, whether it is shown, and how
  many nodes it holds -- hidden surfaces are laid out too, so a panel's
  problems are found before anyone opens it;
- the IPC verbs the configuration registered;
- **errors**: the load failing (with `file:line`, including the line of a
  `require` that found nothing), a Lua error in any callback, binding or
  handler that ran, a line the configuration logged at `error` level, a
  layout that failed, a surface that could not be worked out;
- **warnings**: everything else logged at `warn` -- lint such as
  `lint: Rect > Column laid out to nothing and has 3 children that will
  never be seen` among it --, a layout whose bindings never settle, and
  bindings that read a property while it animates.

The exit status is 1 when there are errors, and with `--strict` when there
are warnings too. `--wait MS` spreads that much real time over the run, for
a configuration whose first screen depends on a process or a bus answering.

```
$ IMPASTO_DRY_RUN=1 morf check examples/shells/impasto/shell/init.lua --private-bus --isolate --size 1280x720
examples/shells/impasto/shell/init.lua on HEADLESS-1 (1280x720, 1 screen)
  surface primary                   1280x720   at 0,0  157 nodes
  surface layer:impasto-dock        1280x131   at 0,589  32 nodes
  surface layer:impasto-picker      1280x720   at 0,0  hidden  13 nodes
  ...
  ipc     appearance bluetooth board ...
0 errors, 0 warnings
```

## `morf render`

```sh
morf render shell.lua -o out.png [--size WxH] [--scale S]
                      [--surface NAME|INDEX|screen] [--ipc 'VERB ARGS']...
                      [--after MS] [--wait MS] [--no-dbus | --private-bus] [--isolate] [-- args...]
```

Draws with the same `RenderEngine` and `WgpuBackend` the shell paints with,
on an offscreen target: the configuration's shaders are built, the frame is
drawn twice (the second one incremental, as every frame after the first
is), and the pixels are written as a PNG with straight alpha.

- `--surface` picks what is drawn: `primary` (the default), an index into
  the list `morf check` prints, a kind (`floating`), a namespace or title,
  or a label (`layer:impasto-dock`, with `#id` added when two share one).
  `screen` draws the primary and every shown layer surface where their
  anchors put them, composed as a compositor would.
- `--scale S` draws at S times the logical size.
- `--after MS` (default 250) is virtual time run before the picture, so a
  timer or an animation can be caught finished -- or halfway.

It needs a Vulkan driver. From the Nix shell that means one of the wrappers:

```sh
nixVulkanIntel morf render examples/demos/text/terminal.lua -o terminal.png
```

With no adapter it stops with `no GPU to render with (...)` and exit status 1.

## `morf test`

```sh
morf test spec.lua... [--filter PATTERN] [--size WxH] [--scale S]
                      [--snapshots DIR] [--no-dbus | --private-bus] [--no-isolate]
```

A spec file is Lua with `morf.test` in it (also `require("morf.test")`):

```lua
local test = morf.test

test.describe("counter", function()
  test.before_each(function()
    test.stub_run("uname", { stdout = "testbox\n" })
    test.load("counter.lua", { size = { 1280, 720 } })
  end)

  test.it("counts clicks on the button", function()
    test.click { id = "increment" }
    test.click { id = "increment" }
    test.eq(test.get({ id = "count" }).text, "count: 2")
    test.eq(test.ipc("count"), 2)
  end)

  test.it("is ready after a second of its own time", function()
    test.advance(999)
    test.eq(test.get({ id = "status" }).text, "starting")
    test.advance(1)
    test.eq(test.get({ id = "status" }).text, "ready")
  end)
end)
```

The spec runs in a runtime of its own; the configuration it loads runs in
another, exactly as the shell would run it. Running the file registers its
tests; each is then run on its own, so one that fails -- an assertion, a
Lua error, a configuration that would not load, a loop that never ends --
fails alone and the rest still run. Output is TAP:

```
TAP version 13
# examples/demos/tests/counter_spec.lua
ok 1 - counter counts clicks on the button (100 ms)
not ok 2 - counter is ready after a second of its own time (104 ms)
#   examples/demos/tests/counter_spec.lua:58: expected "ready", got "starting"
#   the configuration said:
#     warn: timer callback: runtime error: counter.lua:21: ...
1..2
# 1 passed, 1 failed, 0 skipped in 0.31 s
```

A failure prints the assertion with its `file:line` and the warnings and
errors the configuration logged during that test. The exit status is 1 when
any test failed. `--filter PATTERN` runs only tests whose full name (the
`describe` names and the test's, joined by spaces) contains `PATTERN`.

### Structure

| call | |
|------|---|
| `test.describe(name, fn)` | a group; its name prefixes its tests' |
| `test.it(name, fn)` | a test |
| `test.skip(name, reason)` | a test written down and not run (`# SKIP` in the output) |
| `test.before_each(fn)`, `test.after_each(fn)` | run around every test in the enclosing `describe` (and those nested in it) |

### Assertions

Each raises an error at the spec's line when it fails, with both values
shown; `message` is put in front.

| call | passes when |
|------|-------------|
| `test.eq(actual, expected, message)` | equal; tables compared deeply |
| `test.ne(actual, other, message)` | not equal |
| `test.truthy(value, message)`, `test.falsy(value, message)` | |
| `test.near(actual, expected, tolerance, message)` | within `tolerance` (default `1e-6`) |
| `test.contains(haystack, needle, message)` | a string containing a substring, or a table containing a value (compared deeply) |
| `test.matches(text, pattern, message)` | a Lua pattern matches |
| `test.raises(fn, pattern, message)` | `fn` errors, with a message matching `pattern` if given |
| `test.fail(message)` | never |

### The configuration under test

| call | |
|------|---|
| `test.load(path, options)` | loads a configuration, replacing the one loaded before (whose children are killed and reaped first). `path` is looked for beside the spec, then from the working directory |
| `test.load { source = [[...]] }`, `test.source(text, options)` | a configuration written in the spec |
| `test.surfaces()` | `{ label, kind, name, width, height, x, y, visible }` for each surface, the primary first |

`options`: `size = { w, h }`, `screens = n`, `args = { ... }` (the
configuration's own arguments, what follows `--`: `morf.args`,
`morf.options`, `morf.operands`), and `env = { NAME = "value" }` --
what `morf.env` answers for those names (`false` for unset). `env` is seen
by the configuration only, not by programs it starts.

### Time

| call | |
|------|---|
| `test.advance(ms)` | moves the virtual clock: a frame every 16 ms (services polled, animations ticked, surfaces laid out) and a stop at every timer deadline in between, so each timer fires at its own time |
| `test.settle(limit_ms)` | steps frames until nothing moves -- no animation running, no layout converging, no service with news -- or `limit_ms` (default 5000) has passed; returns the virtual milliseconds it took |
| `test.wait(predicate, timeout_ms, message)` | for answers from real processes: steps frames while waiting up to `timeout_ms` (default 2000) of **wall** time for `predicate()` to return something true, and returns it |
| `test.now()` | the virtual clock, in milliseconds since load |
| `test.shortcuts_inhibited()` | whether the configuration asks the compositor to hold its shortcuts off it now (`morf.shortcuts.inhibit`) |

Every input call and `test.ipc` is followed by a frame of no time, so the
layout a following `test.find` reads already shows what it did; effects run
as the shell runs them, once per handler.

### Input

Points are surface-local logical pixels on the primary surface unless
`options.surface` names another (as `morf render --surface` does). Where a
point is taken, a node or a query for one may be given instead, meaning its
centre and its surface. Keys go where a compositor would send them: to the
surface a button was last pressed on (a settings window clicked into), or
the primary before anything was.

| call | |
|------|---|
| `test.click(x, y [, { button, surface }])`, `test.click(query)` | motion, press and release; `button` is `left` (default), `right`, `middle`, `back`, `forward` or a Linux button code |
| `test.move(x, y)`, `test.press(x, y)`, `test.release(x, y)` | the pieces of a click |
| `test.leave([{ surface }])` | the pointer leaving the surface it is on (or the one named), as a compositor says when it moves off the input region: `hovered` and every `contains_pointer` there go false |
| `test.drag({ x1, y1 }, { x2, y2 }, { steps, button })` | press, move in steps, release |
| `test.wheel(dx, dy [, { x, y, surface }])` | a wheel turn at the pointer (or `x, y`); positive `dy` scrolls down |
| `test.key(name [, modifiers])` | one key pressed and released: an X keysym name (`Return`, `Escape`, `Tab`, `BackSpace`, `Left`, `Page_Down`, `F5`, …) or one character. `modifiers` is a list or a string: `"ctrl+shift"` |
| `test.type(text)` | each character as a key; `\n` is Return |

### Finding nodes

A node is a table: `handle`, `element` (`"Text"`, `"MouseArea"`, …), `id`,
`text` (for text nodes), `x`, `y`, `width`, `height` (its box on its
surface, through every transform above it), `visible` (it, every ancestor
and its surface shown, and not fully transparent), `opacity` (its own),
`exiting` (it is playing its `exit`: drawn, but out of the layout and
taking no input), `contains_pointer` (the pointer, where the last
`test.move` left it on this surface, is inside the node's box -- what the
node's own `contains_pointer` says once something reads it), `depth`, `parent` (a
handle), `surface` (the label) and `surface_kind`. A node's `id` is an
ordinary property every element has and nothing in the engine reads:
`ui.Rect { id = "panel", ... }`.

| call | |
|------|---|
| `test.find(query)` | the first node, in paint order, that matches, or `nil` |
| `test.get(query)` | the same, failing the test when there is none |
| `test.find_all(query)` | every match |
| `test.nodes()` | every node on every surface |
| `test.text_of(node)` | the node's text and its descendants', joined by spaces |

A query is a table of fields that must all equal the node's (`{ id =
"count" }`, `{ element = "Text", visible = true }`, `{ text = "add one" }`,
`{ text_contains = "add" }`, `{ surface = "floating" }`), a string (an `id`
or an exact `text`), or a function of the node returning true.

### Talking to it

| call | |
|------|---|
| `test.ipc(verb, ...)` | calls a `morf.ipc` handler and returns what it returned |
| `test.ipc_verbs()` | the verbs it registered |
| `test.logs(level)` | `{ level, message }` for each line logged since load (or since `test.clear_logs()`) at `level` (`debug`, `info`, `warn`, `error`) or above |
| `test.clear_logs()` | |
| `test.stub_run(program, result)` | from now on, `morf.run` of `program` (its `argv[1]`, or that path's last part) does not start anything: the callback gets `result` -- `{ ok, code, stdout, stderr }`, missing fields filled in (`ok` from `code`), or a string meaning `stdout` -- on a later turn of the loop, as a real run's would come. Stubs outlast `test.load` within a spec file |
| `test.clear_stubs()` | |
| `test.runs()` | the argv of every `morf.run` since load, stubbed or not |
| `test.snapshot(name [, { surface }])` | renders to `name` under `--snapshots DIR` (default `snapshots/` beside the spec); returns `true, path`, or `false, reason` when there is no GPU -- the test goes on, with the reason noted in the output |
| `test.note(text)`, `test.log(...)` | a line printed under the test's result |

## Examples

`examples/tests/` holds specs for configurations in `examples/`:

- `counter.lua` and `counter_spec.lua`: a small configuration and a spec
  that uses every kind of call -- clicks, the wheel, keys, typed text, a
  timer, a stubbed command, IPC, finding by id, text and predicate.
- `terminal_spec.lua`: `examples/demos/text/terminal.lua`, with btop (or top) really
  running on its pseudo-terminal, waited for on the wall clock.
- `watch_destroy_spec.lua`: `morf.fs.watch` hearing a file in the run's
  scratch state directory (waited for on the wall clock), and
  `window:destroy()` taking a floating window's surface and tree.
- `impasto_terminal_spec.lua`: a `Terminal=true` desktop entry written into
  the run's scratch data folder and started through impasto's launcher: a
  window per program, destroyed when it exits, kept up (with the exit code)
  when it fails.
- `contains_pointer_spec.lua`: a configuration written in the spec, and a
  panel's `contains_pointer` through pointer moves, a leave, a clip, and a
  binding made while the pointer was already there.
- `impasto_spec.lua`: loads `examples/shells/impasto/shell/init.lua` with
  `IMPASTO_DRY_RUN`, opens every panel over IPC and closes it, and checks
  that nothing landed in `morf ipc call failed`. Run it with
  `--private-bus`: impasto's notification server owns a bus name.

```sh
morf test examples/demos/tests/counter_spec.lua examples/demos/tests/terminal_spec.lua
morf test --private-bus examples/shells/impasto/tests/impasto_spec.lua
nixVulkanIntel morf test examples/demos/tests/counter_spec.lua   # with snapshots
```

## What is not simulated

- No compositor answers anything: screencopy, toplevel lists, the
  clipboard, drag and drop, idle and session-lock requests are accepted and
  go nowhere. A popup is sized but not positioned against its parent the way
  `xdg_positioner` would place it; a floating window has no position.
- Timers are virtual; `morf.elapsed_timer`, the text caret's blink and the
  wall-clock time of day (`morf.system_clock`, `os.time`) are not.
- One runtime is driven: `morf test` loads the configuration for the first
  screen (the rest are in `morf.screens`), where a shell runs one per
  screen. `morf check --screens N` does load it once per screen.
- Keys are keysyms with the text they type; there is no keymap, so a
  configuration that reads raw keycodes sees none.
