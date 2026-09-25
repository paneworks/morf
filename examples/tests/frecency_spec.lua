-- `examples/lib/frecency.lua`: launches remembered, decaying, and ranked.
--
--     morf test examples/tests/frecency_spec.lua

local test = morf.test

local HOST = [[
  local frecency = require("lib.frecency")
  local clock = 1000000
  local used = frecency.open {
    path = morf.env("XDG_STATE_HOME") .. "/launches.json",
    half_life = 10, limit = 3, now = function() return clock end,
  }
  local apps = {
    { id = "firefox", name = "Firefox" }, { id = "foot", name = "Foot" },
    { id = "files", name = "Files" }, { id = "fractal", name = "Fractal" },
  }
  morf.ipc.record = function(id) used.record(id) return "ok" end
  morf.ipc.days = function(n) clock = clock + tonumber(n) * 86400 return "ok" end
  morf.ipc.score = function(id) return string.format("%.3f", used.score(id)) end
  morf.ipc.rank = function(query)
    local out = {}
    for _, hit in ipairs(used.rank(query, apps, { key = "name", id = "id" })) do out[#out + 1] = hit.item.id end
    return table.concat(out, ",")
  end
  morf.ipc.flush = function() used.flush() return "ok" end
]]

local function path() return morf.env("XDG_STATE_HOME") .. "/launches.json" end

test.describe("frecency", function()
  test.before_each(function() pcall(morf.fs.remove, path()) end)

  test.it("counts launches and halves them every half-life", function()
    test.load { source = HOST }
    test.eq(test.ipc("score", "foot"), "0.000")
    test.ipc("record", "foot")
    test.ipc("record", "foot")
    test.eq(test.ipc("score", "foot"), "2.000")
    test.ipc("days", 10)
    test.eq(test.ipc("score", "foot"), "1.000")
  end)

  test.it("puts the most used first when nothing is typed", function()
    test.load { source = HOST }
    test.ipc("record", "files")
    test.ipc("record", "fractal")
    test.ipc("record", "fractal")
    test.eq(test.ipc("rank", ""), "fractal,files,firefox,foot")
  end)

  test.it("lifts a used match among equals but not over a better one", function()
    test.load { source = HOST }
    for _ = 1, 50 do test.ipc("record", "fractal") end
    -- "f": every name starts with it; the used one comes first.
    test.eq(test.ipc("rank", "f"):match("^[^,]+"), "fractal")
    -- "fire": only Firefox matches well; Fractal does not match at all.
    test.eq(test.ipc("rank", "fire"), "firefox")
    -- "fo": Foot is a prefix match; Fractal is not even a candidate.
    test.eq(test.ipc("rank", "fo"):match("^[^,]+"), "foot")
  end)

  test.it("keeps the file and forgets the least used past the limit", function()
    test.load { source = HOST }
    test.ipc("record", "firefox")
    test.ipc("record", "firefox")
    test.ipc("record", "foot")
    test.ipc("record", "files")
    test.ipc("record", "files")
    test.ipc("record", "fractal") -- a fourth: foot, the least used, goes
    test.ipc("flush")
    local saved = morf.json.decode(morf.fs.read(path()))
    test.eq(saved.entries.foot, nil)
    test.truthy(saved.entries.firefox and saved.entries.fractal)
    -- A new runtime reads it back.
    test.load { source = HOST }
    test.eq(test.ipc("score", "firefox"), "2.000")
  end)
end)
