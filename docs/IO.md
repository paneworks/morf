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

## What is bounded, and what is cleaned up

- A runtime runs at most 64 children and holds at most 64 connections;
  a call past that raises.
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
  every connection is closed; nothing from the old configuration calls
  into the new one.

## The older API

`morf.io.process_view`, `morf.process` and `morf.socket` still work: they
are pulled rather than pushed (`:next(timeout)`, `:receive(n, timeout)`),
so something has to ask them on a timer. Prefer the calls above.
