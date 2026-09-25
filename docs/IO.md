# Processes and sockets in morf

A configuration talks to the rest of the system through child processes
and sockets. Both are event-driven: you start one whenever you like — at
the top level, in a click handler, in a timer, in another callback — and
morf calls you back on the main loop as output arrives. Nothing polls, so
an idle shell costs nothing and a line is seen when it is written.

```lua
local morf = require("morf")   -- also on require("morf.io")
```

## Running a command: `morf.run`

For a command that prints something and exits.

```lua
morf.run({ "git", "status", "--short" }, { cwd = repo }, function(result)
  if result.ok then show(result.stdout) else warn(result.stderr) end
end)
```

`result` is `{ ok, code, signal, stdout, stderr, timed_out, truncated, error }`:

- `ok` — exited with 0, in time.
- `code` — the exit status, `nil` when a signal ended it; `signal` the signal number.
- `stdout`, `stderr` — everything printed, both together capped at
  `max_output` bytes (default 8 MiB); `truncated` says the cap was hit.
- `timed_out` — `timeout_ms` passed: the child got `TERM`, then `KILL`
  two seconds later.
- `error` — only when the program would not start at all (not found, not
  executable, bad `cwd`); `code` is `nil` then.

Options (all optional): `env`, `clear_env`, `cwd`, `stdin` (a string written
and then closed), `timeout_ms`, `max_output`. The callback may also be given
without options: `morf.run(argv, function(result) end)`. The call returns
the same handle `morf.spawn` does, so a run can be killed.

## Streaming a process: `morf.spawn`

For a child that lives on and talks: `pactl subscribe`, `nmcli monitor`,
`cava`, or one you write to.

```lua
local child = morf.spawn {
  command = { "pactl", "subscribe" },
  on_stdout = function(line) if line:find("sink") then refresh() end end,
  on_exit = function(code, signal, timed_out) restart_later() end,
}
```

| option       | meaning |
|--------------|---------|
| `command`    | the argv, a list of strings. Never a shell: to use one, ask for it — `{ "sh", "-c", line }`. |
| `env`        | `{ NAME = "value" }`, added to what the child inherits. |
| `clear_env`  | start from an empty environment instead. |
| `cwd`        | the working directory. |
| `stdin`      | `nil` (`/dev/null`), a string (written, then end of file), or `"pipe"` (kept open for `:write`). |
| `lines`      | `true` (default): `on_stdout`/`on_stderr` get one line at a time, without its newline. `false`: raw chunks as they are read. |
| `max_line`   | longest line delivered, default 64 KiB; a longer one is cut there and the rest of it dropped. |
| `on_stdout`  | called with each line or chunk. Absent: stdout goes to `/dev/null`. |
| `on_stderr`  | the same for stderr. Absent: it goes where morf's own stderr goes. |
| `on_exit`    | `(code, signal, timed_out)`, after every line has been delivered. |
| `timeout_ms` | `TERM` when it runs longer, `KILL` two seconds after. |
| `max_output` | stdout and stderr past this many bytes are read and dropped. |
| `detached`   | `true`: survives a reload or exit of morf, in its own process group. |

`morf.spawn` returns `nil, message` when the program cannot be started.
Otherwise the handle has:

- `:write(data)` — to a `stdin = "pipe"` child; `true`, or `false, why`
  (closed, or more than 4 MiB already waiting).
- `:close_stdin()` — end of file once what is queued has been written.
- `:kill(signal)` — `"TERM"` (default), `"KILL"`, `"INT"`, `"HUP"`, … or a
  number; returns whether it was still running.
- `:pid()`, `:running()`.
- `:close()` — stop listening: no callback runs for it again. The child is
  not killed; it loses its pipes.

A process morf holds no handle for -- one an earlier shell started, say --
is signalled by its id: `morf.kill(pid, signal)` (default `TERM`) returns
`true`, or `false` and the reason. Process groups and init are refused.

## Connecting: `morf.connect`

A long-lived connection to a Unix socket (or TCP with `host`, `port`).

```lua
local events
events = morf.connect {
  path = runtime .. "/hypr/" .. signature .. "/.socket2.sock",
  on_connect = function() events:send("subscribe\n") end,
  on_line = function(line) handle(line) end,        -- or on_data = function(chunk)
  on_close = function(reason) morf.timer(1000, reconnect, false) end,
}
```

The connect never blocks: `on_connect` runs when it is up, or `on_close`
runs with the reason when it is not (`"timed out"` after
`connect_timeout_ms`, default 5000, or the error). `on_close` gets `"eof"`
when the other side hung up. `on_line` splits on `\n` (bounded by
`max_line`); `on_data` hands over chunks as read. The handle has
`:send(data)` (queued until connected; `false, why` past 4 MiB waiting),
`:connected()` and `:close()`, after which no callback runs.

## Asking a socket: `morf.request_socket`

For request/reply sockets that answer and close — Hyprland's `.socket.sock`,
and anything shaped like it.

```lua
morf.request_socket(path, "j/monitors", function(reply, err)
  if reply then monitors = morf.json.decode(reply) end
end, { timeout_ms = 5000, max_bytes = 8 * 1024 * 1024 })
```

It connects, writes the request, collects until the other side closes, and
calls back once: `(reply, nil)`, or `(nil, err)` on a refused connection,
`"timed out"`, or a reply over `max_bytes`. It returns a handle whose
`:close()` abandons the request.

## Reading off the loop: `morf.fs.read_async`

`morf.fs.read` answers at once, which is right for almost everything. A
few files are not memory but hardware: a hwmon sensor under
`/sys/class/hwmon` asks the firmware, and on a laptop one read can take
tens of milliseconds -- a dozen of them, sampled every few seconds, held
the loop for 120 ms each time, and every animation on screen with it.

```lua
morf.fs.read_async({ a, b, c }, function(ok, contents)
  -- contents[i]: the text of the i-th path, or false where it could not
  -- be read (missing, unreadable, or larger than the limit)
end, limit)
```

The files are read on a worker thread, in order, and the callback runs on
a later turn of the loop -- never inside the call. `paths` is a list (at
most 256) or one path; `limit` is the most bytes a file may have (default
16 MiB) -- a sysfs attribute reports a size of 4096 whatever it holds, so
leave it out there. Returns `true`, or `nil, message` when too much work
is already queued; a call that is wrong (not a path, a bad limit) raises.

## Watching files: `morf.fs.watch`

For a file someone else writes — a settings file another screen saves, a
history, a theme — or a folder whose contents come and go.

```lua
local w = morf.fs.watch(path, function(event)
  -- event.path  the path that changed
  -- event.kind  "changed" | "created" | "deleted" | "moved"
  -- event.name  its name: for a folder's entry, relative to the folder
end, { recursive = false })
w:close()   w:closed()   w:path()
```

- **A file** is followed by name, through its folder, so a file that does
  not exist yet can be watched: it is `created` when it appears. When its
  folder does not exist either, the nearest one that does is watched and
  the watch follows the folders down as they are made. `changed` is a
  write, or the file replaced by a rename (how editors save); `deleted` and
  `moved` are it going.
- **A folder** reports its entries: made (`created`), written (`changed`),
  removed or moved away. The folder itself going is `deleted` or `moved`
  with its own path, after which it is watched as a path that does not
  exist, until it comes back. `recursive = true` takes in every folder
  below it too (at most 4096), and those made later.
- **Bursts are coalesced**: what happened to one path between two turns of
  the loop is one callback — a hundred writes are one `changed`, an editor's
  move-aside-and-write is one `changed`, a file made and removed before the
  loop looked is nothing, one removed and made again is `changed`. At most
  64 callbacks per watch per turn; the rest wait for the next.

It costs nothing while nothing changes. Every watch in the process shares
one inotify descriptor and one thread, asleep in `poll` until the kernel
has news, which then rings the loop the way a child's output does. Nothing
is polled and there is no thread per watch.

A watch lasts as long as its handle: `:close()` ends it (no callback runs
after it, not even one already gathered), and so does the handle being
collected — keep it in a variable that lives as long as the watch should —,
a reload, and the runtime ending. `morf.fs.watch` raises for a call that
is wrong (no path, no function, a bad option, more than the limit), and
returns `nil, message` when the kernel refuses the watch.

The older `morf.file(path):watch()` (`watcher:next(timeout)`) and
`morf.file_view { watch_changes = true }` still work and sit on the same
shared watcher, pulled rather than pushed; prefer `morf.fs.watch`.

## D-Bus values

`morf.dbus` converts as the bus's types suggest: numbers are numbers,
`s`/`o`/`g` are strings, arrays and structures are lists, dictionaries are
tables, a variant is its value.

**Byte arrays (`ay`) are strings.** An image's pixels, an SSID, a path sent
NUL-terminated arrive as one Lua string of those bytes — `#data`,
`data:byte(i)`, `data:sub(a, b)` — not a list of numbers (which is what
they were before, one table slot per byte). `morf.image.from_dbus` and
`morf.image.from_rgba` take the string as it comes. When sending, an `ay`
takes a string or a list of byte values:

```lua
proxy:call_with("AddConnection", {
  { signature = "a{sa{sv}}", value = {
      ["802-11-wireless"] = { ssid = { signature = "ay", value = "cafe" } },
  } },
})
```

A string that is not valid UTF-8 given where no signature says otherwise
goes as `ay`; given for an `s`, it is made valid, as always.

## What is bounded, and what is cleaned up

- A runtime runs at most 64 children and holds at most 64 connections;
  a call past that raises.
- A runtime holds at most 256 file watches (`MORF_LIMITS=watches=N`); a
  call past that raises. Closed watches do not count.
- Each handle has at most 64 callbacks run per turn of the loop; the rest
  wait for the next turn, so one chatty child cannot starve the rest.
- A child that prints faster than its callbacks keep up with is not read
  until they catch up (past 1 MiB undelivered): it waits on its pipe, and
  memory stays flat.
- `LD_LIBRARY_PATH` is never passed to a child unless `env` names it. morf
  may be started through a wrapper (nixGL and the like) that points it at
  libraries a system binary must not load.
- Every child is reaped. On a reload, or when morf exits, each child the
  configuration started is killed and reaped, except `detached` ones, and
  every connection is closed and every watch dropped; nothing from the old
  configuration calls into the new one.
- morf's own reload-on-save follows the configuration's `.lua` files
  through the same shared watcher: it sleeps until one is written, waits
  for 50 ms of quiet, and reloads when the files' sizes or times actually
  differ — never by looking at every file on a timer.

## Compressed bytes and archives: `morf.encoding`, `morf.archive`

A package manager's sync database, a downloaded tarball, a `.gz` log: bytes
a configuration read with `morf.fs.read` or `morf.http`, inflated in memory.

```lua
local db = morf.fs.read("/var/lib/pacman/sync/core.db")      -- a gzip or zstd tar
for _, member in ipairs(morf.archive.tar(db, { contents = true })) do
  if member.name:match("/desc$") then parse(member.data) end
end
```

- `morf.encoding.decompress(bytes, format, { max_size })` — `format` is
  `"gzip"`, `"zlib"`, `"deflate"`, `"zstd"`, `"xz"` or `"lzma"`; `nil` or
  `"auto"` goes by the magic number (gzip, zstd, xz and zlib have one).
  Returns the bytes, or `nil, why` for corrupt input or an output longer
  than `max_size` (default 64 MiB, at most 512 MiB) — the cap is checked
  while inflating, so a small bomb never becomes a large allocation.
- `morf.encoding.compression(bytes)` — the format a magic number names, or `nil`.
- `morf.archive.tar(bytes, { contents, max_size, max_entries })` — every
  member of a tar archive (ustar, GNU long names, pax paths) as `{ name,
  type, size, mode, mtime, link, data }`: `type` is `file`, `directory`,
  `symlink`, `hardlink`, `char`, `block` or `fifo`; `link` is there for
  links; `data` only when `contents = true`. A gzip, zstd or xz archive is
  inflated first, under `max_size`. At most `max_entries` members (default
  100000); more, a bad header checksum, or a member running past the end is
  `nil, why`.
- `morf.archive.tar_read(bytes, name, options)` — one file's bytes, or
  `nil, why`.

All of it runs on the main loop: a few milliseconds for a small database,
about a tenth of a second for 36 MB of tar, so read a large one when the
answer is wanted, not in a binding.

## A night light: `morf.gamma`

The colour ramps of an output, through the compositor's
`wlr-gamma-control-unstable-v1` — what wlsunset and hyprsunset do, as a
call a configuration makes when it likes (at sunset, from a slider):

```lua
if morf.gamma.supported() then
  morf.gamma.set { temperature = 3400, brightness = 0.9 }   -- this output
  morf.gamma.set { output = "DP-2", temperature = 4000 }    -- another one
end
morf.gamma.reset()                                         -- every output it changed
```

- `set { output, temperature, brightness, gamma }` — `output` is an
  output's name as `morf.screens` gives it, or `nil` for the one this
  configuration runs on; `temperature` in kelvin, 1000 to 25000, 6500 being
  neutral; `brightness` 0 to 1; `gamma` 0.1 to 10, 1 being linear. Anything
  left out is neutral. The ramps are a black body's white point at that
  temperature (normalised so 6500 K is white), times the brightness,
  through the gamma curve.
- `reset(output)` — that output's own ramps back; with no name, every
  output this configuration changed.
- `supported()` — whether the compositor offers gamma control; false while
  the configuration first loads, before the shell has connected
  (`morf.capabilities.gamma_control` says the same).

Requests are sent on the next turn of the loop, and only the last one per
output in a turn is: a slider dragged through a hundred temperatures sends
one ramp. Only one client may hold an output's gamma at a time; when
another does (a running wlsunset, say), the compositor refuses and the
refusal is logged. The compositor restores an output the moment the shell
lets go of it — on `reset`, on a reload (a new configuration starts from
the outputs' own ramps), and when the shell exits for any reason.

## Levels, bands and beats: `morf.audio.monitor`

A monitor listens to a device — the default sink when `device` is `nil`,
so what is playing — on the sound server's own thread, and hands the
configuration what it measured, between frames:

```lua
local meter = morf.audio.monitor {
  device = nil, rate_hz = 30, bands = 24,
  on_level = function(left, right, bands) end,   -- peaks 0..1, and band energies 0..1
  beat = true,
  on_beat = function(strength) pulse:set(strength) end,
  on_tempo = function(bpm, confidence) end,
}
meter.bpm, meter.confidence   -- the latest tempo estimate, or nil before there is one
meter:stop()
```

`on_level` runs at most once a frame with the loudest peak since the last
one. With `beat = true` the monitor also listens for beats (then
`on_level` may be left out): `on_beat(strength)` runs for every beat, as
soon as the loop wakes for it, `strength` being 0 to 1 against the beats
of the last few seconds; `on_tempo(bpm, confidence)` runs when the tempo
estimate moves, and `meter.bpm` reads the latest. `on_beat` and
`on_tempo` without `beat = true` are an error; without it the audio thread
does exactly what it did before.

Beats are onsets: the spectrum of every 10 ms or so is compared with the
last, and a rise across it that is a local peak and clears a threshold
following the last half second (its mean, plus twice its spread, and at
least half again the mean) is a beat — a kick, a snare, a strummed chord;
steady sound, a drone or noise, makes none. The tempo is where the same
rises repeat over the last six seconds, 60 to 200 BPM: it takes two or
three seconds to appear, drifts to follow a slow change, and jumps only
after a new tempo has been heard for about a second and a half. The
confidence says how regular the beats are at that tempo, 0 to 1 — a click
track is near 1, speech near 0. It costs a fraction of a percent of one
core. An octave is ambiguous by nature: music with a strong half-time feel
may read at half the tempo a dancer would clap.

## A program on a terminal: `ui.Terminal`

A program that wants a terminal rather than pipes — anything that draws a
screen, asks for its size, or reads keys one at a time — runs in a
`ui.Terminal` node instead: a pseudo-terminal, watched by the same kind of
reactor, with an emulator drawing its screen. It is described with the
other nodes, in [UI.md](UI.md#terminal).

## The older API

`morf.io.process_view`, `morf.process` and `morf.socket` still work: they
are pulled rather than pushed (`:next(timeout)`, `:receive(n, timeout)`),
so something has to ask them on a timer. Prefer the calls above.

## Idle: `morf.idle`

```lua
local sub = morf.idle.subscribe(300000, function(idle) end)   -- after 5 minutes without input
morf.idle.subscribe(60000, fn, true)   -- input only: counts even while something keeps the session awake
sub:cancel()
morf.idle.inhibit(true)                -- keep the session awake (a film, a presentation)
morf.idle.inhibited()                  -- what was last asked for
```

`subscribe` calls back with `true` when the session has been idle that
long and `false` when input comes back. `inhibit` holds an idle inhibitor
on the shell's surface; whether the compositor has one to hold is
`morf.capabilities.idle_inhibit`, so a "keep awake" switch can tell "off"
from "cannot here".
