-- Claude Code's token usage, from its own transcripts.
--
-- Every assistant turn Claude Code writes to `~/.claude/projects/**/*.jsonl`
-- carries the token counts the API returned. Summed by hour they give the
-- tokens spent in the current five-hour billing block and in the last seven
-- days, which is what a panel wants to show next to the clock. A port of
-- impasto's `claude_usage.py`, reading with `morf.fs` instead of Python.
--
--   local claude_usage = require("lib.claude_usage")
--   local usage = claude_usage.new {}
--   ui.Text { text = function()
--     local u = usage:get()
--     return u.available and ("%dk this block"):format(u.block_tokens // 1000) or ""
--   end }
--
-- Transcripts only grow, and some grow to hundreds of megabytes, so they are
-- read incrementally: each file's offset, size and mtime are remembered (on
-- disk, across restarts), an unchanged file is not opened, and a changed one
-- is read from where the last pass stopped. The reading is a job, in slices,
-- and a pass reads at most `pass_bytes`; a pass that stopped short is followed
-- by another a second later, most recently changed files first, so the
-- current block is right before the backlog is done.
--
-- `morf.fs.read` reads a whole file. A file larger than `max_read` is read a
-- chunk at a time by running `dd` (directly, read-only) for the byte range.

local morf = require("morf")
local poll = require("lib.poll")

local fs = morf.fs

local claude_usage = {}

local BLOCK_HOURS = 5
local WEEK_HOURS = 24 * 7

-- ---------------------------------------------------------------------------
-- Hours

local function day_number(year, month, day)
  if month <= 2 then year = year - 1 end
  local era = (year >= 0 and year or year - 399) // 400
  local yoe = year - era * 400
  local mp = (month + 9) % 12
  local doy = (153 * mp + 2) // 5 + day - 1
  local doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
  return era * 146097 + doe - 719468
end

--- Hours since the epoch for a UTC ISO 8601 stamp, or nil.
function claude_usage.hour_of(stamp)
  local y, m, d, h = stamp:match("^(%d%d%d%d)%-(%d%d)%-(%d%d)T(%d%d)")
  if not y then return nil end
  return day_number(tonumber(y), tonumber(m), tonumber(d)) * 24 + tonumber(h)
end

-- ---------------------------------------------------------------------------
-- Lines

-- An assistant turn: `"usage":{...}` inside its message, then the request id,
-- the type and the timestamp. Only what comes after the usage object is read,
-- because what comes before it -- the message content -- can be megabytes.
local function scan_text(text, from, file, spend)
  local pos = from
  local seen = file.recent_set
  while true do
    local start = text:find('"usage":{', pos, true)
    -- Nothing more; keep the last few bytes, which could be the start of a
    -- marker the next read completes.
    if not start then return math.max(from - 1, #text - 16), nil end
    local stop = text:find("\n", start, true)
    if not stop then
      -- The line is still being written, or the chunk ends mid-line: come
      -- back to it.
      return start - 1, start
    end
    -- Some two dozen calls for a turn: finds, matches, conversions.
    spend(24)
    local line = text:sub(start, stop - 1)
    local request = line:match('"requestId":"([^"]+)"')
    if request and line:find('"type":"assistant"', 1, true) and not seen[request] then
      local stamp = line:match('"timestamp":"([^"]+)"')
      local hour = stamp and claude_usage.hour_of(stamp)
      local usage = line:match('^"usage":(%b{})')
      if hour and usage then
        -- The per-iteration breakdown repeats the same counts; leave it out,
        -- wherever in the object it sits.
        usage = usage:gsub('"iterations":%b[]', "")
        -- Cache reads are left out, as the original does: they re-send the
        -- same context on every turn and would dwarf the tokens processed.
        local tokens = (tonumber(usage:match('"input_tokens":(%d+)')) or 0)
          + (tonumber(usage:match('"output_tokens":(%d+)')) or 0)
          + (tonumber(usage:match('"cache_creation_input_tokens":(%d+)')) or 0)
        local slot = file.buckets[hour]
        if not slot then
          slot = { 0, 0 }
          file.buckets[hour] = slot
        end
        slot[1] = slot[1] + tokens
        slot[2] = slot[2] + 1
        -- A turn with several content blocks is written as several lines
        -- with the same request and the same usage; count it once.
        seen[request] = true
        file.recent[#file.recent + 1] = request
        -- Repeats are adjacent, so a short memory is enough.
        if #file.recent > 8 then
          seen[table.remove(file.recent, 1)] = nil
        end
      end
    end
    pos = stop + 1
  end
end

-- ---------------------------------------------------------------------------
-- State, kept on disk: per file its offset, size, mtime, recent request ids
-- and hourly buckets. Buckets are a string ("hour,tokens,messages;...") so a
-- file costs one JSON entry however long it has been used.

local function pack(file, spend)
  local parts = {}
  for hour, slot in pairs(file.buckets) do
    spend(2)
    parts[#parts + 1] = hour .. "," .. slot[1] .. "," .. slot[2]
  end
  return {
    offset = file.offset, size = file.size, mtime = file.mtime,
    buckets = table.concat(parts, ";"),
    recent = table.concat(file.recent, " "),
  }
end

local function unpack_file(entry, spend)
  local file = {
    offset = tonumber(entry.offset) or 0,
    size = tonumber(entry.size) or 0,
    mtime = tonumber(entry.mtime) or 0,
    buckets = {},
    recent = {},
    recent_set = {},
  }
  for hour, tokens, messages in (entry.buckets or ""):gmatch("(%d+),(%d+),(%d+)") do
    spend(5)
    file.buckets[tonumber(hour)] = { tonumber(tokens), tonumber(messages) }
  end
  for request in (entry.recent or ""):gmatch("%S+") do
    spend(2)
    file.recent[#file.recent + 1] = request
    file.recent_set[request] = true
  end
  return file
end

local function fresh_file()
  return { offset = 0, size = 0, mtime = 0, buckets = {}, recent = {}, recent_set = {} }
end

-- ---------------------------------------------------------------------------
-- Totals

--- The block and week sums, and the peaks, over `hours` (hour -> {tokens,
--- messages}) at `now_hour`. The block starts on the hour of the first message
--- within reach of a five-hour window, as the original reckons it.
function claude_usage.summarise(hours, now_hour, spend)
  spend = spend or function() end
  local first, last
  for hour in pairs(hours) do
    if not first or hour < first then first = hour end
    if not last or hour > last then last = hour end
  end
  if not first then return nil end
  local function total_over(from, to)
    local tokens, messages = 0, 0
    for hour = from, to do
      local slot = hours[hour]
      if slot then
        tokens = tokens + slot[1]
        messages = messages + slot[2]
      end
    end
    return tokens, messages
  end
  local block_start = now_hour
  for hour = now_hour - BLOCK_HOURS + 1, now_hour do
    if hours[hour] and hour < block_start then block_start = hour end
  end
  while block_start - 1 > now_hour - BLOCK_HOURS and hours[block_start - 1] do
    block_start = block_start - 1
  end
  local block_tokens, block_messages = total_over(block_start, now_hour)
  local week_tokens, week_messages = total_over(now_hour - WEEK_HOURS + 1, now_hour)
  spend(WEEK_HOURS // 20)
  -- The busiest block and week on record, by sliding sums: one pass each.
  local peak_block, peak_week, window_block, window_week = 0, 0, 0, 0
  for hour = first, last + WEEK_HOURS do
    local add = hours[hour] and hours[hour][1] or 0
    local drop_block = hours[hour - BLOCK_HOURS] and hours[hour - BLOCK_HOURS][1] or 0
    local drop_week = hours[hour - WEEK_HOURS] and hours[hour - WEEK_HOURS][1] or 0
    window_block = window_block + add - drop_block
    window_week = window_week + add - drop_week
    if window_block > peak_block then peak_block = window_block end
    if window_week > peak_week then peak_week = window_week end
    if hour % 4 == 0 then spend(5) end
  end
  return {
    block_start = block_start * 3600,
    block_end = (block_start + BLOCK_HOURS) * 3600,
    block_tokens = block_tokens,
    block_messages = block_messages,
    week_tokens = week_tokens,
    week_messages = week_messages,
    peak_block_tokens = peak_block,
    peak_week_tokens = peak_week,
  }
end

-- ---------------------------------------------------------------------------
-- A reader

local Usage = {}
Usage.__index = Usage

--- Options:
---   dir          -- transcripts (default $CLAUDE_CONFIG_DIR/projects or
---                   ~/.claude/projects)
---   state_path   -- where offsets and buckets are kept (default in the cache)
---   interval     -- ms between passes while read (default 60 s)
---   keep_hours   -- buckets older than this are dropped, files not touched
---                   since are skipped (default five weeks)
---   pass_bytes   -- most bytes one pass reads (default 32 MiB)
---   max_read     -- files up to this are read whole (default 4 MiB)
---   chunk        -- bytes per `dd` read of a larger file (default 2 MiB)
---   read_range(path, offset, length, on_done) -- how a range of a large file
---                   is read; `dd` unless a test says otherwise
---   now          -- a function returning the time, for tests
function claude_usage.new(options)
  options = options or {}
  local home = fs.home() or ""
  local config = morf.env("CLAUDE_CONFIG_DIR")
  local self = setmetatable({
    dir = options.dir or ((config and config ~= "" and config or (home .. "/.claude")) .. "/projects"),
    state_path = options.state_path or (poll.cache_dir() .. "/claude-usage.json"),
    keep_hours = options.keep_hours or 24 * 35,
    pass_bytes = options.pass_bytes or 32 * 1024 * 1024,
    max_read = options.max_read or 4 * 1024 * 1024,
    chunk = options.chunk or 2 * 1024 * 1024,
    now = options.now or morf.time.now,
    read_range = options.read_range,
    files = nil,
  }, Usage)
  if not self.read_range then
    local dd = poll.which("dd")
    if dd then
      self.read_range = function(path, offset, length, on_done)
        poll.run({ dd, "if=" .. path, "bs=64K", "iflag=skip_bytes,count_bytes",
          "skip=" .. offset, "count=" .. length, "status=none" }, function(result)
          on_done(result.ok and result.stdout or nil, result.error or result.stderr)
        end, { max_bytes = length + 65536, timeout_ms = 60000 })
      end
    end
  end
  local interval = options.interval or 60 * 1000
  self.source = poll.source {
    name = "claude_usage",
    interval = interval,
    initial = { available = false, scanning = true, block_tokens = 0, week_tokens = 0,
      block_messages = 0, week_messages = 0, peak_block_tokens = 0, peak_week_tokens = 0 },
    sample = function(done) self:_pass(done) end,
  }
  return self
end

--- The usage, read so a binding follows it: `{ available, block_start,
--- block_end (epoch seconds), block_tokens, block_messages, week_tokens,
--- week_messages, peak_block_tokens, peak_week_tokens, files, scanning,
--- skipped, updated }`. `scanning` is true while older transcripts are still
--- being read; the numbers grow until it is false.
function Usage:get() return self.source:get() end

--- Starts a pass now.
function Usage:refresh() self.source:refresh() end

function Usage:_load(spend)
  if self.files then return end
  self.files = {}
  local text = fs.read(self.state_path)
  local ok, state = false, nil
  if text then ok, state = pcall(morf.json.decode, text) end
  if ok and type(state) == "table" and state.version == 1 and type(state.files) == "table" then
    for path, entry in pairs(state.files) do
      spend(3)
      self.files[path] = unpack_file(entry, spend)
    end
  end
end

function Usage:_save(spend)
  local files = {}
  for path, file in pairs(self.files) do
    spend(4)
    files[path] = pack(file, spend)
  end
  local ok, text = pcall(morf.json.encode, { version = 1, files = files })
  if ok then fs.write(self.state_path, text) end
end

-- Reads the part of one file past its offset, calling `spend` as it goes;
-- `budget` is how many bytes it may read. Returns the bytes read, and whether
-- the file has more.
function Usage:_read_file(path, file, stat, budget, spend, wait)
  if stat.size < file.offset then
    -- Smaller than where we were: replaced, not appended to.
    local reset = fresh_file()
    for key, value in pairs(reset) do file[key] = value end
  end
  local read = 0
  if stat.size <= self.max_read then
    local text = fs.read(path, self.max_read)
    if not text then return 0, false end
    local stop, pending = scan_text(text, file.offset + 1, file, spend)
    read = #text - file.offset
    -- The whole file is here, so all of it is read unless a line is still
    -- being written.
    file.offset = pending and stop or #text
    return read, false
  end
  if not self.read_range then
    return 0, false, "no way to read past " .. self.max_read .. " bytes (no dd)"
  end
  while file.offset < stat.size and read < budget do
    local length = math.min(self.chunk, stat.size - file.offset)
    local at_end = file.offset + length >= stat.size
    local text, err = wait(function(resume) self.read_range(path, file.offset, length, resume) end)
    -- A reader that fails is not asked again until the file changes.
    if not text or #text == 0 then return read, false, err or "nothing read" end
    local stop, pending = scan_text(text, 1, file, spend)
    read = read + length
    if at_end and not pending then
      file.offset = stat.size
      break
    end
    -- A chunk read through a pipe comes back as text, where a character cut
    -- in two by a read boundary becomes a three-byte replacement; so the
    -- bytes consumed can be a little fewer than the text says. Step back by
    -- the most that could be, and let the request ids already counted keep
    -- the overlap from counting twice.
    local replaced, at = 0, 1
    while true do
      at = text:find("\239\191\189", at, true)
      if not at then break end
      replaced, at = replaced + 1, at + 3
      spend(2)
    end
    local consumed = pending and pending - 1 or stop
    if pending == 1 and not at_end then
      -- One line's tail longer than a chunk: skip past its start rather
      -- than read the same bytes forever.
      consumed = 9
    end
    local advance = consumed > 0 and math.max(1, consumed - 2 * replaced) or 0
    file.offset = file.offset + advance
    if advance == 0 then break end
  end
  return read, file.offset < stat.size
end

function Usage:_pass(done)
  if self._running then return done(self.source.value) end
  self._running = true
  local paths = fs.glob(self.dir .. "/**/*.jsonl")
  local now = self.now()
  local now_hour = math.floor(now / 3600)
  local oldest_hour = now_hour - self.keep_hours
  poll.job(function(spend)
    -- Waits for a callback-style call inside the job: the job simply is not
    -- resumed until the answer arrives.
    local function wait(start)
      local answer, err, arrived = nil, nil, false
      start(function(value, message) answer, err, arrived = value, message, true end)
      while not arrived do coroutine.yield() end
      return answer, err
    end
    self:_load(spend)
    local candidates = {}
    for _, path in ipairs(paths) do
      spend(3)
      local stat = fs.stat(path)
      if stat and stat.is_file and stat.modified >= oldest_hour * 3600 then
        -- Whole milliseconds, which survive the trip through JSON exactly.
        stat.modified = math.floor(stat.modified * 1000)
        candidates[#candidates + 1] = { path = path, stat = stat }
      end
    end
    -- Files written within the block first: the current block is in those.
    -- Two buckets rather than a sort, which would cost a comparison call per
    -- step with nowhere to yield.
    local ordered, later = {}, {}
    local recent = (now_hour - BLOCK_HOURS) * 3600 * 1000
    for _, candidate in ipairs(candidates) do
      local bucket = candidate.stat.modified >= recent and ordered or later
      bucket[#bucket + 1] = candidate
    end
    for _, candidate in ipairs(later) do ordered[#ordered + 1] = candidate end
    spend(#candidates // 4)
    local budget = self.pass_bytes
    local backlog, skipped, changed = false, {}, false
    local present = {}
    for _, candidate in ipairs(ordered) do
      local path, stat = candidate.path, candidate.stat
      present[path] = true
      local file = self.files[path]
      if not file then
        file = fresh_file()
        self.files[path] = file
      end
      spend(4)
      if file.failed ~= stat.size
        and (file.mtime ~= stat.modified or file.size ~= stat.size or file.offset < stat.size) then
        if budget <= 0 then
          backlog = true
        else
          local read, more, err = self:_read_file(path, file, stat, budget, spend, wait)
          budget = budget - read
          changed = true
          if err then
            skipped[#skipped + 1] = path .. ": " .. tostring(err)
            -- Not tried again until it changes size.
            file.failed = stat.size
          end
          if more then
            backlog = true
          else
            file.mtime, file.size = stat.modified, stat.size
          end
        end
      end
    end
    -- Forget files that are gone or too old, and buckets past the window.
    local hours = {}
    for path, file in pairs(self.files) do
      spend(2)
      if not present[path] then
        self.files[path] = nil
        changed = true
      else
        for hour, slot in pairs(file.buckets) do
          spend(2)
          if hour < oldest_hour then
            file.buckets[hour] = nil
          else
            local total = hours[hour]
            if not total then
              total = { 0, 0 }
              hours[hour] = total
            end
            total[1] = total[1] + slot[1]
            total[2] = total[2] + slot[2]
          end
        end
      end
    end
    if changed then self:_save(spend) end
    local summary = claude_usage.summarise(hours, now_hour, spend) or {
      block_start = now_hour * 3600, block_end = (now_hour + BLOCK_HOURS) * 3600,
      block_tokens = 0, block_messages = 0, week_tokens = 0, week_messages = 0,
      peak_block_tokens = 0, peak_week_tokens = 0,
    }
    summary.available = next(hours) ~= nil
    summary.scanning = backlog
    summary.files = #ordered
    summary.skipped = skipped
    summary.updated = now
    return summary
  end, function(value, err)
    self._running = false
    if value and value.scanning then
      -- More to read: the next pass soon, not a minute from now.
      morf.timer(1000, function() self.source:refresh() end, false)
    end
    done(value, err)
  end)
end

-- ---------------------------------------------------------------------------
-- The plan's limits

--- The 5-hour and 7-day utilisation the API reports, as `/usage` shows it.
---
--- Nothing but the `anthropic-ratelimit-unified-*` response headers carries
--- them, so this sends the smallest request there is (the smallest model, one
--- output token) with Claude Code's own OAuth token and reads the headers.
--- It is a real request against the account, so it is never made unless
--- called: call it on a slow timer if at all. The token is read for this
--- request only and never stored.
---
--- `on_done(result)` gets `{ available, session = { used, resets, status },
--- week = {...}, claim, status }` or `{ available = false, error }`.
--- Options: `credentials` (path), `url`, `model`.
function claude_usage.limits(options, on_done)
  options = options or {}
  local config = morf.env("CLAUDE_CONFIG_DIR")
  local credentials = options.credentials
    or ((config and config ~= "" and config or ((fs.home() or "") .. "/.claude")) .. "/.credentials.json")
  local text = fs.read(credentials, 1024 * 1024)
  local ok, data = false, nil
  if text then ok, data = pcall(morf.json.decode, text) end
  local token = ok and type(data) == "table" and type(data.claudeAiOauth) == "table" and data.claudeAiOauth.accessToken
  if type(token) ~= "string" then
    morf.timer(1, function() on_done({ available = false, error = "no Claude Code credentials at " .. credentials }) end, false)
    return
  end
  morf.http.post(options.url or "https://api.anthropic.com/v1/messages", {
    model = options.model or "claude-haiku-4-5",
    max_tokens = 1,
    messages = { { role = "user", content = "." } },
  }, {
    headers = {
      authorization = "Bearer " .. token,
      ["anthropic-version"] = "2023-06-01",
      ["anthropic-beta"] = "oauth-2025-04-20",
    },
    timeout_ms = 12000,
  }, function(response)
    local headers = response.headers or {}
    local function window(prefix)
      local used = tonumber(headers["anthropic-ratelimit-unified-" .. prefix .. "-utilization"])
      if not used then return nil end
      return {
        used = used,
        resets = tonumber(headers["anthropic-ratelimit-unified-" .. prefix .. "-reset"]) or 0,
        status = headers["anthropic-ratelimit-unified-" .. prefix .. "-status"] or "",
      }
    end
    local session, week = window("5h"), window("7d")
    if not session and not week then
      on_done({ available = false, error = response.error or ("no rate limit headers (status " .. response.status .. ")") })
      return
    end
    on_done({
      available = true,
      session = session,
      week = week,
      claim = headers["anthropic-ratelimit-unified-representative-claim"] or "",
      status = headers["anthropic-ratelimit-unified-status"] or "",
    })
  end)
end

return claude_usage
