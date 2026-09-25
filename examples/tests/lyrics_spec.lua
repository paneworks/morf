-- `examples/lib/lyrics.lua`: LRC read, the line for a moment, and lyrics
-- found beside the file, in the cache, or from lrclib (a stand-in served
-- by `python3 -m http.server` on this machine: nothing reaches the net).
--
--     morf test examples/tests/lyrics_spec.lua

local test = morf.test
local lyrics = require("lib.lyrics")

local SONG = table.concat({
  "[ar:Someone]",
  "[ti:A Song]",
  "[offset:+500]",
  "[00:10.50]first line",
  "[00:05.00][00:20.00]twice <00:20.40>sung",
  "[00:30.00]",
  "[01:02.25]last",
}, "\n")

local function wait_for(get, message)
  local value
  test.wait(function() value = get() return value ~= nil end, 5000, message)
  return value
end

test.describe("lyrics", function()
  test.it("reads LRC: every time a line has, sorted, offset, word times dropped", function()
    local p = lyrics.parse(SONG)
    test.truthy(p.synced)
    test.eq(p.tags.ar, "Someone")
    local got = {}
    for i, line in ipairs(p.lines) do got[i] = string.format("%.2f %s", line.time, line.text) end
    test.eq(got, { "4.50 twice sung", "10.00 first line", "19.50 twice sung", "29.50 ", "61.75 last" })
  end)

  test.it("keeps unsynced lyrics as plain lines", function()
    local p = lyrics.parse("one\n\ntwo\n")
    test.eq(p.synced, false)
    test.eq(#p.lines, 2)
    test.eq(lyrics.index_at(p, 10), 0)
  end)

  test.it("finds the line being sung", function()
    local p = lyrics.parse(SONG)
    test.eq(lyrics.index_at(p, 0), 0)
    test.eq(lyrics.index_at(p, 4.5), 1)
    test.eq(lyrics.index_at(p, 15), 2)
    test.eq(lyrics.index_at(p, 61.74), 4)
    test.eq(lyrics.index_at(p, 500), 5)
  end)

  -- The rest runs in a configuration of its own, whose loop the runner
  -- steps: `result` answers what the last `find` gave.
  local HOST = [[
    local lyrics = require("lib.lyrics")
    local got = nil
    morf.ipc.find = function(json, opts_json)
      got = false
      lyrics.find(morf.json.decode(json), function(p, source)
        got = p and (source .. "|" .. p.lines[1].text) or ("none|" .. tostring(source))
      end, opts_json and morf.json.decode(opts_json) or {})
      return true
    end
    morf.ipc.result = function() return got or "" end
    local position = 0
    local media = {
      state = morf.state { active = { title = "", artist = "", album = "", length = 0, url = "", playing = true } },
      position = function() return position end,
    }
    local follow = lyrics.follow(media, { offline = true, tick_ms = 50 })
    morf.ipc.play = function(title, url) media.state.active.title = title media.state.active.url = url return true end
    morf.ipc.seek = function(seconds) position = tonumber(seconds) return true end
    morf.ipc.follow = function()
      return follow.status:get() .. "|" .. follow.line:get() .. "|" .. follow.next_line:get()
    end
  ]]

  local function find(track, opts)
    test.ipc("find", morf.json.encode(track), morf.json.encode(opts or {}))
    local answer
    test.wait(function() answer = test.ipc("result") return answer ~= "" end, 10000, "an answer")
    return answer
  end

  test.it("uses the .lrc beside a local file first", function()
    test.load { source = HOST }
    local dir = morf.env("XDG_CACHE_HOME") .. "/music dir"
    morf.fs.mkdir(dir, { parents = true })
    morf.fs.write(dir .. "/song.lrc", "[00:01.00]from the file")
    local url = "file://" .. dir:gsub(" ", "%%20") .. "/song.flac"
    test.eq(find({ title = "x", url = url }, { offline = true }), "file|from the file")
    test.eq(find({ title = "y" }, { offline = true }), "none|offline")
  end)

  test.it("asks lrclib, then answers from its cache", function()
    test.load { source = HOST }
    local root = morf.env("XDG_CACHE_HOME") .. "/lrclib"
    morf.fs.mkdir(root .. "/api", { parents = true })
    morf.fs.write(root .. "/api/get", morf.json.encode {
      trackName = "A Song", syncedLyrics = "[00:02.00]served", plainLyrics = "served",
    })
    local port = 20000 + math.random(0, 20000)
    local server = assert(morf.spawn {
      command = { "python3", "-m", "http.server", tostring(port), "--bind", "127.0.0.1", "--directory", root },
      on_stderr = function() end,
    })
    local base = "http://127.0.0.1:" .. port
    local track = { title = "A Song", artist = "Someone", album = "", length = 180 }
    -- The server takes a moment to listen: ask until it answers.
    local answer
    for _ = 1, 50 do
      answer = find(track, { base = base, ttl = 0 })
      if answer:match("^lrclib") then break end
      pcall(test.wait, function() return false end, 100)
    end
    server:kill()
    test.eq(answer, "lrclib|served")
    -- Asked again, the cache answers, with no server at all.
    test.eq(find(track, { base = base }), "cache|served")
  end)

  test.it("follows a player: the line moves with the position", function()
    test.load { source = HOST }
    test.matches(test.ipc("follow"), "^none|")
    local dir = morf.env("XDG_CACHE_HOME") .. "/follow"
    morf.fs.mkdir(dir, { parents = true })
    morf.fs.write(dir .. "/t.lrc", "[00:01.00]one\n[00:03.00]two\n[00:05.00]three")
    test.ipc("play", "T", "file://" .. dir .. "/t.mp3")
    test.wait(function() return test.ipc("follow"):match("^synced") end, 3000, "synced")
    test.ipc("seek", "3.5")
    test.advance(100)
    test.eq(test.ipc("follow"), "synced|two|three")
    test.ipc("seek", "0.2")
    test.advance(100)
    test.eq(test.ipc("follow"), "synced||one")
  end)
end)
