-- Polling, shared by the libraries that watch the machine and the web.
--
-- A panel that shows the CPU wants a number that moves; the machine does not
-- say when its numbers move, so somebody has to ask it every so often. The
-- asking should cost nothing when nobody is looking: a hidden popup with a
-- process list must not walk /proc every two seconds. So a *source* here polls
-- only while something reads it.
--
-- A source is a value plus a revision signal. `source:get()` reads the signal
-- -- so a binding that calls it is re-run when a new sample lands -- and
-- returns the plain Lua value. The first read starts the timer. Every sample
-- bumps the revision, which re-runs the bindings that read it, which read the
-- source again; a sample that nobody re-reads means every reader is gone, and
-- after `linger` such samples the timer stops. The next read starts it again.
-- `pin(true)` keeps a source polling without readers, for code that reacts
-- from a handler instead of a binding.
--
-- A handler here gets a hundred thousand Lua instructions, and any call --
-- a Lua function, `string.match`, `morf.fs.read` -- costs about twenty-five
-- of them, so a handler has room for some four thousand calls. Work that is proportional to the machine (a
-- process per pid, a line per transcript) runs as a *job*: a coroutine that
-- calls `spend(n)` as it goes and is resumed on the next tick when it has
-- spent its share. Nothing here blocks: processes and HTTP answer later.

local morf = require("morf")

local poll = {}

-- ---------------------------------------------------------------------------
-- Jobs

--- Runs `body(spend)` in slices across timer ticks and hands its return value
--- to `on_done(value)` (or `on_done(nil, message)` when it raised).
---
--- `spend(n)` counts work in calls (a unit is one call, about twenty-five
--- instructions; `spend` is one itself) and yields once the slice
--- (`options.slice`, default 1500 -- under half a handler) is used up; the
--- job resumes a millisecond later. The returned handle has `cancel()` and
--- `done()`.
function poll.job(body, on_done, options)
  options = options or {}
  local slice = options.slice or 1500
  local used = 0
  local finished = false
  local timer
  local thread = coroutine.create(function()
    return body(function(cost)
      used = used + (cost or 1)
      if used >= slice then
        used = 0
        coroutine.yield()
      end
    end)
  end)
  local handle = {}
  local function step()
    if finished then return end
    used = 0
    local ok, value = coroutine.resume(thread)
    if not ok then
      finished = true
      if timer then timer:cancel() end
      if on_done then on_done(nil, tostring(value)) end
    elseif coroutine.status(thread) == "dead" then
      finished = true
      if timer then timer:cancel() end
      if on_done then on_done(value) end
    elseif not timer then
      timer = morf.timer(options.every_ms or 1, step, true)
    end
  end
  function handle.cancel()
    if finished then return false end
    finished = true
    if timer then timer:cancel() end
    return true
  end
  function handle.done() return finished end
  if options.now then step() else morf.timer(1, step, false) end
  return handle
end

-- ---------------------------------------------------------------------------
-- Sources

local Source = {}
Source.__index = Source

local next_source = 0

--- Makes a source. `spec`:
---   sample(done)   -- takes a sample; call `done(value)` when it is ready
---                     (now or later), or `done(nil, message)` on failure,
---                     which keeps the previous value and sets `error`.
---   interval       -- milliseconds between samples (default 2000)
---   linger         -- samples nobody read before the timer stops (default 2)
---   initial        -- the value before the first sample
---   name           -- for the revision signal, which a log may name
function poll.source(spec)
  next_source = next_source + 1
  local self = setmetatable({
    interval = spec.interval or 2000,
    linger = spec.linger or 2,
    value = spec.initial,
    error = nil,
    updated = nil,
    samples = 0,
    _sample = spec.sample,
    _revision = morf.signal((spec.name or ("poll.source" .. next_source)) .. ".revision", 0),
    _count = 0,
    _reads = 0,
    _idle = 0,
    _published = false,
    _running = false,
    _busy = false,
    _pinned = false,
  }, Source)
  return self
end

--- The value, read so that a binding follows it. Starts polling.
function Source:get()
  self._revision:get()
  self._reads = self._reads + 1
  if not self._running then self:_start() end
  return self.value
end

--- Only the revision: a binding that wants to re-run on every sample.
function Source:revision()
  self:get()
  return self._count
end

--- Replaces the value from outside the timer: a setter that knows the new
--- state (a brightness it just wrote) need not wait for the next sample.
function Source:publish(value)
  self.value = value
  self.error = nil
  self.updated = morf.time.now()
  self.samples = self.samples + 1
  self._count = self._count + 1
  self._published = true
  self._reads = 0
  self._revision:set(self._count)
end

function Source:_fail(message)
  self.error = message
  self._count = self._count + 1
  self._published = true
  self._reads = 0
  self._revision:set(self._count)
end

function Source:_take()
  if self._busy then return end
  self._busy = true
  local settled = false
  local ok, message = pcall(self._sample, function(value, err)
    if settled then return end
    settled = true
    self._busy = false
    if value == nil and err ~= nil then
      self:_fail(err)
    else
      self:publish(value)
    end
  end)
  if not ok then
    self._busy = false
    self:_fail(tostring(message))
  end
end

function Source:_tick()
  if not self._pinned and self._published then
    if self._reads == 0 then self._idle = self._idle + 1 else self._idle = 0 end
    if self._idle >= self.linger then
      self:_halt()
      return
    end
  end
  self:_take()
end

function Source:_start()
  if self._running then return end
  self._running = true
  self._idle = 0
  self._published = false
  self._timer = morf.timer(self.interval, function() self:_tick() end, true)
  -- The first sample now rather than an interval from now; a timer rather
  -- than a call, because a read inside a binding must not write signals.
  morf.timer(1, function() self:_take() end, false)
end

function Source:_halt()
  self._running = false
  if self._timer then self._timer:cancel() end
  self._timer = nil
end

--- Whether the timer is running.
function Source:running() return self._running end

--- Keeps polling with no readers (`true`) or lets it idle again (`false`).
function Source:pin(on)
  self._pinned = on ~= false
  if self._pinned then self:_start() end
end

--- Takes a sample now, whether or not the timer runs.
function Source:refresh()
  morf.timer(1, function() self:_take() end, false)
end

--- Changes the interval; a running timer is restarted at the new pace.
function Source:set_interval(ms)
  self.interval = ms
  if self._running then
    self:_halt()
    self._running = true
    self._timer = morf.timer(self.interval, function() self:_tick() end, true)
  end
end

--- Stops polling until the next read.
function Source:stop() self:_halt() end

-- ---------------------------------------------------------------------------
-- History

--- A ring of the last `size` numbers, for a sparkline.
function poll.ring(size)
  local ring = { size = size or 60, items = {}, first = 1, count = 0 }
  function ring.push(value)
    local index = (ring.first + ring.count - 1) % ring.size + 1
    if ring.count < ring.size then
      ring.count = ring.count + 1
    else
      ring.first = ring.first % ring.size + 1
    end
    ring.items[index] = value
  end
  --- Oldest first, as a fresh array.
  function ring.list()
    local out = {}
    for offset = 0, ring.count - 1 do
      out[#out + 1] = ring.items[(ring.first + offset - 1) % ring.size + 1]
    end
    return out
  end
  return ring
end

-- ---------------------------------------------------------------------------
-- Programs

--- Finds `name` on PATH, the way the kernel would when running it. Returns
--- the full path or nil. A name with a slash is taken as given.
function poll.which(name, path)
  if name:find("/", 1, true) then
    return morf.fs.is_file(name) and name or nil
  end
  path = path or morf.env("PATH") or "/usr/local/bin:/usr/bin:/bin"
  for dir in path:gmatch("[^:]+") do
    local candidate = dir .. "/" .. name
    if morf.fs.is_file(candidate) then return candidate end
  end
  return nil
end

--- Runs a program directly (argv, no shell) and collects what it prints.
---
--- `on_done(result)` gets `{ ok, code, stdout, stderr, error }`: `ok` is exit
--- code 0; `error` says why there is no exit code (not found, timed out).
--- `options.timeout_ms` (default 60000) kills a program that hangs;
--- `options.max_bytes` (default 4 MiB) stops collecting past that much.
function poll.run(argv, on_done, options)
  options = options or {}
  local program = argv[1]
  local args = {}
  for index = 2, #argv do args[#args + 1] = argv[index] end
  local ok, process = pcall(morf.process, program, args)
  if not ok then
    morf.timer(1, function()
      on_done({ ok = false, stdout = "", stderr = "", error = tostring(process) })
    end, false)
    return { cancel = function() return false end }
  end
  process:close_stdin()
  local out, err, size = {}, {}, 0
  local max_bytes = options.max_bytes or 4 * 1024 * 1024
  local deadline = morf.time.now_ms() + (options.timeout_ms or 60000)
  local timer
  local finished = false
  local function finish(result)
    if finished then return end
    finished = true
    timer:cancel()
    result.stdout = table.concat(out)
    result.stderr = table.concat(err)
    on_done(result)
  end
  timer = morf.timer(options.poll_ms or 20, function()
    -- A bounded drain: a program that prints a lot is read over a few ticks.
    for _ = 1, 64 do
      local event = process:next()
      if not event then break end
      if event.kind == "exit" then
        finish({ ok = event.success, code = event.code })
        return
      elseif size < max_bytes then
        size = size + #event.data
        if event.kind == "stdout" then out[#out + 1] = event.data else err[#err + 1] = event.data end
      end
    end
    if morf.time.now_ms() > deadline then
      pcall(process.kill, process)
      finish({ ok = false, error = "timed out" })
    end
  end, true)
  return {
    cancel = function()
      if finished then return false end
      finished = true
      timer:cancel()
      pcall(process.kill, process)
      return true
    end,
  }
end

-- ---------------------------------------------------------------------------
-- Caches

--- The folder caches go in: `morf.cache_dir()` when the shell has one, else
--- the XDG cache folder's `morf`.
function poll.cache_dir()
  local ok, dir = pcall(morf.cache_dir)
  if ok and type(dir) == "string" and dir ~= "" then return dir end
  return (morf.fs.dir("cache") or "/tmp") .. "/morf"
end

--- Reads a cached JSON value written by `cache_write`. Returns the value and
--- its age in seconds, or nil when missing, unreadable or older than `ttl`.
function poll.cache_read(path, ttl)
  local text = morf.fs.read(path)
  if not text then return nil end
  local ok, entry = pcall(morf.json.decode, text)
  if not ok or type(entry) ~= "table" or type(entry.at) ~= "number" then return nil end
  local age = morf.time.now() - entry.at
  if ttl and age > ttl then return nil, age, entry.value end
  return entry.value, age
end

--- Writes a value with the time it was fetched. Failure is not fatal: a cache
--- that cannot be written only costs a fetch next time.
function poll.cache_write(path, value)
  local ok, text = pcall(morf.json.encode, { at = morf.time.now(), value = value })
  if not ok then return false end
  return morf.fs.write(path, text) or false
end

return poll
