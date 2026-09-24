# examples/lib

Pure-Lua libraries a configuration can `require("lib.<name>")`. The engine
stays agnostic: it offers `morf.fs`, `morf.http`, `morf.process`,
`morf.time`, signals and timers, and everything that knows about a particular
file, website or tool lives here, where a shell can read it and change it.

## poll

The shared machinery for the libraries below that watch something.

- `poll.source { sample = function(done) ... end, interval = ms, linger = n, initial = v }`
  makes a value that polls **only while something reads it**. `source:get()`
  is a tracked read (a binding that calls it re-runs on each sample) and
  starts the timer; a sample nobody re-reads counts as idle, and after
  `linger` idle samples the timer stops until the next read. Also
  `pin(bool)`, `refresh()`, `set_interval(ms)`, `running()`, `stop()`,
  `publish(value)`, and the fields `value`, `error`, `updated`, `samples`.
- `poll.job(body, on_done, { slice })` runs `body(spend)` as a coroutine over
  timer ticks. A handler gets 100k Lua instructions and a native call costs a
  few dozen, so anything proportional to the machine calls `spend(n)` and is
  resumed next tick when its slice is used.
- `poll.run(argv, on_done, { timeout_ms, max_bytes })` runs a program
  directly (no shell) and calls back with `{ ok, code, stdout, stderr, error }`.
- `poll.which(name)` finds a program on `PATH`.
- `poll.ring(n)` is a ring buffer (`push`, `list`) for sparklines.
- `poll.cache_dir()`, `poll.cache_read(path, ttl)`, `poll.cache_write(path, value)`
  keep JSON with the time it was fetched.

## sysinfo

The machine, from `/proc` and `/sys` only.

```lua
local sysinfo = require("lib.sysinfo")
ui.Text { text = function() return ("CPU %d%%"):format(sysinfo.cpu().usage) end }
ui.Text { text = function()
  local t = sysinfo.temperatures().cpu
  return t and ("%d°C"):format(t) or ""
end }
sysinfo.configure { intervals = { cpu = 1000, processes = 3000 }, history = 120, top = 8 }
```

| reader | returns | default interval |
| --- | --- | --- |
| `cpu()` | `usage`, `cores[i] = {name, usage, frequency}`, `count`, `frequency` (MHz, mean), `load = {1, 5, 15}`, `running`, `threads`, `model` | 2 s |
| `memory()` | bytes: `total`, `available`, `used`, `free`, `cached`, `percent`, `swap = {total, used, free, percent}` | 3 s |
| `disks()` | real filesystems from `/proc/mounts` (one row per device), `{device, mount, type, total, used, free, available, percent}` | 30 s |
| `temperatures()` | `cpu` (°C, the package sensor when there is one), `cpu_sensor`, `sensors[i] = {chip, label, celsius, critical}` from hwmon and thermal zones | 5 s |
| `gpu()` | `busy` and `cards` where the driver exposes `gpu_busy_percent` (amdgpu); Intel reads as nil | 2 s |
| `network()` | `default` interface (from the routing table), `rx_rate`/`tx_rate` in bytes/s for it, `interfaces[i]` | 2 s |
| `battery()` | `present`, `percent`, `status`, `charging`, `power` (W), `time_left`/`time_to_full` (s), `ac`, `batteries` -- for machines without UPower | 30 s |
| `backlight()` | `brightness`, `max`, `percent`, `writable`, `set(percent)` only when writable, `devices` | 5 s |
| `system()` | `os`, `os_id`, `os_version`, `kernel`, `hostname`, `user`, `uptime` | 60 s |
| `processes()` | `count`, `by_cpu`, `by_memory`: the top few `{pid, name, state, cpu, memory, memory_percent}` | 5 s, in slices |

`sysinfo.history(name)` gives the last samples of `cpu`, `load`, `memory`,
`swap`, `temperature`, `gpu`, `rx`, `tx` or `coreN`, oldest first.
`sysinfo.set_brightness(percent, device)` writes the backlight when its file
is writable (world-writable, root, or group-writable with the user in
`video`); nothing here asks for privileges. `sysinfo.sample(name)` samples
one section now. `sysinfo.sources[name]` are the poll sources, for `pin`.
`sysinfo.configure { root = "/some/folder" }` reads a fake `/proc` and `/sys`
from a folder, which is how the tests run.

The tables handed out are shared and replaced whole on every sample: read
them, do not change them.

## weather

The weather from keyless APIs: Open-Meteo (geocoding and forecast), with
wttr.in as the fallback and as the answer when no place is given (it guesses
from the address).

```lua
local weather = require("lib.weather")
local here = weather.new { location = "Wageningen", units = "metric" }
ui.Text { text = function()
  local now = here:get()
  if not now.available then return "" end
  return ("%s %d%s %s"):format(now.glyph, now.temperature, now.units.temperature, now.condition)
end }
```

`weather.new(options)`: `location` (a name, geocoded once) or `latitude`,
`longitude` and `name`; `units` (`"metric"` or `"imperial"`); `interval` (ms
between refreshes while read, default 30 min); `ttl` (seconds an answer is
fresh, default 30 min); `cache_dir`; `fallback` (default true); and
`geocoding_url`, `forecast_url`, `wttr_url` to point elsewhere.

`here:get()` is a tracked read of `{ available, source, place, temperature,
feels_like, humidity, wind_speed, wind_direction, code, condition, icon,
glyph, is_day, high, low, hourly, daily, units, updated, stale }`. `hourly`
is the next 24 hours and `daily` seven days, each entry with `time`,
temperatures, `precipitation` (%), `code`, `condition`, `icon`, `glyph`.
`icon` is a freedesktop icon name (`weather-showers`, `weather-clear-night`),
`glyph` a Unicode symbol. `here:refresh()` asks again past the cache.
Answers are cached on disk; offline, the last one comes back with `stale =
true`. `weather.condition(code, is_day)` maps any WMO code.

## github

A public user's contribution calendar: a year of days, each with a count and
the 0-4 shade GitHub draws it in.

```lua
local github = require("lib.github")
local me = github.new { user = "torvalds" }          -- or { user = ..., token = "ghp_..." }
ui.Text { text = function()
  local c = me:get()
  return c.available and ("%d contributions, %d-day streak"):format(c.total, c.current_streak) or ""
end }
```

Without a token the calendar is read from the public page
`github.com/users/<user>/contributions`, by what identifies a day (a cell's
`data-date` and `data-level`, the tooltip naming it with the count) rather
than by where it sits in the markup. With a `token` it comes from the GraphQL
API. Either way it is cached on disk (`ttl`, default an hour) and refreshed
every `interval` (default an hour) while read.

`me:get()` is a tracked read of `{ available, user, source, days = { {date,
count, level, weekday} }, weeks, total, current_streak, longest_streak, today,
max, updated, stale }`. `weeks` are columns of seven, Sunday first, the way the
calendar is drawn (the first column padded with nils). `total` is the number
GitHub states when it states one (it includes private contributions). The
current streak runs back from today, or from yesterday while today is still
empty. Options: `user`, `token`, `interval`, `ttl`, `cache_dir`, `today`,
`base_url`, `api_url`. `github.parse_html`, `github.parse_graphql` and
`github.summarise` are there for other uses.
