-- Synced lyrics for what is playing.
--
-- Lyrics come as LRC: a line of text after the time it is sung,
-- `[01:23.45]like this`, sometimes several times before one line, with an
-- `[offset:+250]` to shift them all. This reads LRC, finds the line for a
-- moment, and finds lyrics for a track: an `.lrc` beside the file being
-- played, then lrclib.net (keyless, by artist, title, album and length),
-- cached on disk, so a song is asked for once.
--
--   local lyrics = require("lib.lyrics")
--   local media = require("lib.mpris").connect()
--   local follow = lyrics.follow(media)
--   ui.Text { text = function() return follow.line:get() end }
--
-- `follow` keeps signals for the whole song (`lines`), the line being sung
-- (`index`, `line`, `next_line`) and where the search got to (`status`:
-- "none", "searching", "synced", "plain", "missing").

local morf = require("morf")
local poll = require("lib.poll")

local lyrics = {}

lyrics.LRCLIB = "https://lrclib.net"

-- --------------------------------------------------------------------- LRC

local function stamp(m, s)
  return tonumber(m) * 60 + tonumber(s)
end

--- LRC text as `{ lines = { { time, text }, ... } (by time), tags = {},
--- synced = bool }`. Lines without a time make a plain (unsynced) lyric;
--- word timings (`<01:02.30>`) are dropped; `[offset:ms]` is applied (a
--- positive offset makes lines come sooner, as players read it).
function lyrics.parse(text)
  local lines, tags, plain = {}, {}, {}
  local offset = 0
  for raw in (text or ""):gmatch("[^\r\n]*") do
    local rest, times = raw, {}
    while true do
      local m, s, tail = rest:match("^%s*%[(%d+):(%d+%.?%d*)%](.*)$")
      if not m then break end
      times[#times + 1] = stamp(m, s)
      rest = tail
    end
    if #times > 0 then
      rest = rest:gsub("<%d+:%d+%.?%d*>", ""):gsub("^%s+", ""):gsub("%s+$", "")
      for _, t in ipairs(times) do lines[#lines + 1] = { time = t, text = rest } end
    else
      local key, value = raw:match("^%s*%[(%a+):(.-)%]%s*$")
      if key then
        key = key:lower()
        tags[key] = value
        if key == "offset" then offset = (tonumber(value) or 0) / 1000 end
      elseif raw:match("%S") then
        plain[#plain + 1] = raw
      end
    end
  end
  if #lines == 0 then
    for i, text in ipairs(plain) do lines[i] = { text = text } end
    return { lines = lines, tags = tags, synced = false }
  end
  for _, line in ipairs(lines) do line.time = math.max(0, line.time - offset) end
  -- Stable by time: lines at the same moment keep their order.
  for i, line in ipairs(lines) do line.order = i end
  table.sort(lines, function(a, b)
    if a.time ~= b.time then return a.time < b.time end
    return a.order < b.order
  end)
  for _, line in ipairs(lines) do line.order = nil end
  return { lines = lines, tags = tags, synced = true }
end

--- The index of the line being sung at `seconds` (0 before the first).
function lyrics.index_at(parsed, seconds)
  local lines = parsed.lines
  if not parsed.synced or #lines == 0 or seconds < lines[1].time then return 0 end
  local lo, hi = 1, #lines
  while lo < hi do
    local mid = (lo + hi + 1) // 2
    if lines[mid].time <= seconds then lo = mid else hi = mid - 1 end
  end
  return lo
end

-- ---------------------------------------------------------------- finding

local function key_of(track)
  return morf.encoding.sha1(table.concat({
    track.artist or "", track.title or "", track.album or "", tostring(math.floor(track.length or 0)),
  }, "\n")):sub(1, 16)
end

-- The `.lrc` beside a local file: /music/song.flac -> /music/song.lrc.
local function beside(url)
  if type(url) ~= "string" or not url:match("^file://") then return nil end
  local path = url:sub(8):gsub("%%(%x%x)", function(h) return string.char(tonumber(h, 16)) end)
  local lrc = path:gsub("%.[^./]+$", "") .. ".lrc"
  local ok, text = pcall(morf.fs.read, lrc)
  if ok and type(text) == "string" and text ~= "" then return text end
  return nil
end

--- Finds lyrics for a track (`{ title, artist, album, length, url }`) and
--- answers `on_done(parsed, source)` on a later turn, or `on_done(nil,
--- why)`. Options: `cache_dir`, `ttl` (a week), `base` (lrclib's address),
--- `offline` (no network).
function lyrics.find(track, on_done, opts)
  opts = opts or {}
  if (track.title or "") == "" then
    morf.timer(1, function() on_done(nil, "no title") end, false)
    return
  end
  local local_text = beside(track.url)
  if local_text then
    morf.timer(1, function() on_done(lyrics.parse(local_text), "file") end, false)
    return
  end
  local cache = (opts.cache_dir or poll.cache_dir()) .. "/lyrics-" .. key_of(track) .. ".json"
  local cached = poll.cache_read(cache, opts.ttl or 7 * 86400)
  if cached then
    morf.timer(1, function()
      if cached.missing then return on_done(nil, "missing") end
      on_done(lyrics.parse(cached.text), "cache")
    end, false)
    return
  end
  if opts.offline then
    morf.timer(1, function() on_done(nil, "offline") end, false)
    return
  end
  local query = morf.http.query {
    artist_name = track.artist or "",
    track_name = track.title,
    album_name = track.album or "",
    duration = (track.length or 0) > 0 and math.floor(track.length + 0.5) or nil,
  }
  local headers = { ["user-agent"] = "morf lib/lyrics (https://github.com/bresilla/morf)" }
  local function answer(record, source)
    -- A missing field is `morf.json.null`, not a string.
    local text = record and (type(record.syncedLyrics) == "string" and record.syncedLyrics ~= ""
      and record.syncedLyrics or record.plainLyrics) or nil
    if type(text) ~= "string" or text == "" then
      poll.cache_write(cache, { missing = true })
      return on_done(nil, "missing")
    end
    poll.cache_write(cache, { text = text })
    on_done(lyrics.parse(text), source)
  end
  local base = opts.base or lyrics.LRCLIB
  morf.http.get(base .. "/api/get?" .. query, { headers = headers, timeout_ms = 15000 }, function(response)
    if response.ok then return answer(response.json(), "lrclib") end
    if response.status ~= 404 then return on_done(nil, response.error or ("lrclib answered " .. response.status)) end
    -- Not by exact match: the best of a search by artist and title.
    local search = morf.http.query { track_name = track.title, artist_name = track.artist or "" }
    morf.http.get(base .. "/api/search?" .. search, { headers = headers, timeout_ms = 15000 }, function(found)
      local list = found.ok and found.json() or nil
      if type(list) ~= "table" then return on_done(nil, found.error or "lrclib search failed") end
      local best
      for _, record in ipairs(list) do
        if type(record.syncedLyrics) == "string" then best = record break end
        best = best or record
      end
      answer(best, "lrclib")
    end)
  end)
end

-- --------------------------------------------------------------- following

--- Follows a `lib.mpris` connection: finds lyrics whenever the active track
--- changes and the line as it plays. Returns `{ lines, index, line,
--- next_line, status, source }` as signals, and `stop()`. Options: those of
--- `find`, `tick_ms` (100: how often the line is looked up while playing),
--- `name` (the signals' prefix).
function lyrics.follow(media, opts)
  opts = opts or {}
  local name = opts.name or "lyrics"
  local f = {
    lines = morf.signal(name .. ".lines", {}),
    index = morf.signal(name .. ".index", 0),
    line = morf.signal(name .. ".line", ""),
    next_line = morf.signal(name .. ".next_line", ""),
    status = morf.signal(name .. ".status", "none"),
    source = morf.signal(name .. ".source", ""),
  }
  local parsed = { lines = {}, synced = false }
  local asked = nil
  local function show(index)
    if index == f.index:get() then return end
    f.index:set(index)
    local line = parsed.lines[index]
    local following = parsed.lines[index + 1]
    f.line:set(line and line.text or "")
    f.next_line:set(following and following.text or "")
  end
  local effect = morf.effect(name .. ".track", function()
    local a = media.state.active
    local track = { title = a.title, artist = a.artist, album = a.album, length = a.length, url = a.url }
    local key = key_of(track)
    if key == asked then return end
    asked = key
    parsed = { lines = {}, synced = false }
    f.lines:set({})
    f.index:set(-1)
    show(0)
    if track.title == "" then return f.status:set("none") end
    f.status:set("searching")
    lyrics.find(track, function(found, source)
      if asked ~= key then return end -- the track changed meanwhile
      if not found then
        f.status:set("missing")
        f.source:set(source or "")
        return
      end
      parsed = found
      local texts = {}
      for i, line in ipairs(found.lines) do texts[i] = { time = line.time or -1, text = line.text } end
      f.lines:set(texts)
      f.status:set(found.synced and "synced" or "plain")
      f.source:set(source)
      show(lyrics.index_at(parsed, media.position()))
    end, opts)
  end)
  -- The line is looked up only while synced lyrics play: an idle shell is
  -- not woken ten times a second for a song that is not there.
  local timer
  local ticking = morf.effect(name .. ".tick", function()
    local run = f.status:get() == "synced" and media.state.active.playing == true
    if run and not timer then
      timer = morf.timer(opts.tick_ms or 100, function()
        show(lyrics.index_at(parsed, media.position()))
      end, true)
    elseif not run and timer then
      timer:cancel()
      timer = nil
    end
  end)
  function f.stop()
    if timer then timer:cancel() timer = nil end
    if ticking and ticking.dispose then ticking:dispose() end
    if effect and effect.dispose then effect:dispose() end
  end
  return f
end

return lyrics
