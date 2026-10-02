-- `library/lib/settings.lua`: nested defaults, a sparse JSON file, every
-- value a signal, and the file watched.
--
--     morf test library/tests/settings_spec.lua

local test = morf.test

local HOST = [[
  local settings = require("lib.settings")
  local path = morf.env("XDG_CONFIG_HOME") .. "/shell/shell.json"
  local config = settings.open {
    path = path,
    defaults = {
      appearance = { rounding = { scale = 1 }, font = { family = "Rubik" } },
      bar = { persistent = true, workspaces = { shown = 5, labels = { "1", "2" } } },
    },
    legacy = function(t)
      if t.roundingScale then
        t.appearance = t.appearance or {}
        t.appearance.rounding = { scale = t.roundingScale }
        t.roundingScale = nil
      end
      return t
    end,
  }
  local runs = 0
  morf.effect("follow", function() config.get("appearance.rounding.scale") runs = runs + 1 end)
  morf.ipc.get = function(key) return morf.json.encode(config.get(key)) end
  morf.ipc.set = function(key, json)
    local ok, why = config.set(key, morf.json.decode(json))
    return ok and "ok" or why
  end
  morf.ipc.proxy = function()
    config.values.bar.workspaces.shown = 7
    return tostring(config.values.bar.workspaces.shown)
  end
  morf.ipc.flush = function() config.flush() return "ok" end
  morf.ipc.runs = function() return runs end
  morf.ipc.reset = function(key) config.reset(key) return "ok" end
]]

local function path()
  return morf.env("XDG_CONFIG_HOME") .. "/shell/shell.json"
end

test.it("settings can keep local edits without writing from a secondary runtime",function()
  local file=morf.state_path("single-writer-test.json")
  morf.fs.remove(file)
  test.load {source=[[
    local owner=false
    local config=require("lib.settings").open {path=morf.state_path("single-writer-test.json"),
      defaults={volume=30},write_when=function() return owner end}
    morf.ipc.set=function(v) config.set("volume",tonumber(v)) end
    morf.ipc.get=function() return config.get("volume") end
    morf.ipc.owner=function() owner=true end
    morf.ipc.file=function() local ok,text=pcall(morf.fs.read,config.path) return ok and text or nil end
  ]]}
  test.ipc("set","40") test.advance(200)
  test.eq(test.ipc("get"),40) test.eq(test.ipc("file"),nil)
  test.ipc("owner") test.ipc("set","50") test.advance(200)
  test.eq(morf.json.decode(test.ipc("file")).volume,50)
end)

local function file()
  local ok, text = pcall(morf.fs.read, path())
  return ok and text and morf.json.decode(text) or nil
end

test.describe("settings", function()
  test.before_each(function() pcall(morf.fs.remove, path()) end)

  test.it("reads the defaults when there is no file", function()
    test.load { source = HOST }
    test.eq(test.ipc("get", "appearance.rounding.scale"), "1")
    test.eq(morf.json.decode(test.ipc("get", "bar.workspaces")).shown, 5)
  end)

  test.it("writes only what differs, once, a moment later", function()
    test.load { source = HOST }
    test.eq(test.ipc("set", "appearance.rounding.scale", "1.5"), "ok")
    test.eq(test.ipc("set", "bar.persistent", "false"), "ok")
    test.eq(file(), nil)
    test.advance(200)
    local saved = file()
    test.eq(saved.appearance.rounding.scale, 1.5)
    test.eq(saved.bar.persistent, false)
    test.eq(saved.appearance.font, nil)
    test.eq(saved.bar.workspaces, nil)
    -- Back to the default: gone from the file.
    test.ipc("reset", "bar.persistent")
    test.ipc("flush")
    test.eq(file().bar, nil)
  end)

  test.it("refuses a value of the wrong type", function()
    test.load { source = HOST }
    test.matches(test.ipc("set", "bar.persistent", '"yes"'), "should be a boolean")
    test.matches(test.ipc("set", "nope.nothing", "1"), "no setting")
    test.eq(test.ipc("set", "bar.workspaces.labels", '["a","b","c"]'), "ok")
    test.eq(test.ipc("get", "bar.workspaces.labels"), '["a","b","c"]')
  end)

  test.it("writes through the nested table", function()
    test.load { source = HOST }
    test.eq(test.ipc("proxy"), "7")
    test.ipc("flush")
    test.eq(file().bar.workspaces.shown, 7)
  end)

  test.it("takes a file's values, drops wrong ones, and keeps unknown keys", function()
    morf.fs.write(path(), morf.json.encode {
      appearance = { rounding = { scale = 2 }, font = { family = 12 } },
      future = { thing = true },
    })
    test.load { source = HOST }
    test.eq(test.ipc("get", "appearance.rounding.scale"), "2")
    test.eq(test.ipc("get", "appearance.font.family"), '"Rubik"')
    test.ipc("set", "bar.persistent", "false")
    test.ipc("flush")
    test.eq(file().future.thing, true)
  end)

  test.it("follows the file when someone else changes it", function()
    test.load { source = HOST }
    local before = test.ipc("runs")
    morf.fs.write(path(), '{"appearance":{"rounding":{"scale":3}}}')
    test.wait(function() return test.ipc("get", "appearance.rounding.scale") == "3" end, 3000, "re-read")
    test.eq(test.ipc("runs"), before + 1)
    -- A change to another key does not re-run what follows this one.
    morf.fs.write(path(), '{"appearance":{"rounding":{"scale":3}},"bar":{"persistent":false}}')
    test.wait(function() return test.ipc("get", "bar.persistent") == "false" end, 3000, "re-read")
    test.eq(test.ipc("runs"), before + 1)
  end)

  test.it("reads a file in an older shape", function()
    morf.fs.write(path(), '{"roundingScale":0.5}')
    test.load { source = HOST }
    test.eq(test.ipc("get", "appearance.rounding.scale"), "0.5")
  end)
end)
