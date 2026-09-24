-- `morf.fs.watch` and `window:destroy()`, driven as the shell drives them:
-- `morf test examples/tests/watch_destroy_spec.lua`.
--
-- The files live in the run's own scratch state directory (`--isolate` is
-- on by default for `morf test`); the kernel's news arrives on the wall
-- clock, so the watch tests wait for it with `test.wait`.

local test = morf.test

local CONFIG = [[
local ui = require("morf.ui")
local fs = morf.fs
local dir = morf.state_dir() .. "/watched"
fs.mkdir(dir, { parents = true })
local heard = {}
local watch = fs.watch(dir .. "/settings.json", function(event)
  heard[#heard + 1] = event.kind .. ":" .. event.name
end)
local entries = {}
local folder = fs.watch(dir, function(event)
  entries[#entries + 1] = event.kind .. ":" .. event.name
end)

local closed = 0
local win
local function open()
  win = morf.window.floating {
    title = "doomed", width = 300, height = 200, visible = true,
    root = ui.Rect { id = "doomed-root", color = "#333", ui.Text { id = "doomed-text", text = "bye" } },
    on_closed = function() closed = closed + 1 end,
  }
end
open()

ui.Rect { id = "main", width = 100, height = 40, color = "#111" }

morf.ipc.write = function(text) fs.write(dir .. "/settings.json", text) return true end
morf.ipc.remove = function() fs.remove(dir .. "/settings.json") return true end
morf.ipc.heard = function() return table.concat(heard, " ") end
morf.ipc.entries = function() return table.concat(entries, " ") end
morf.ipc.close_watch = function() watch:close() return watch:closed() end
morf.ipc.destroy = function() win:destroy() return true end
morf.ipc.closed = function() return closed end
morf.ipc.after = function()
  local ok, err = pcall(win.title, win, "again")
  return ok, tostring(err)
end
morf.ipc.reopen = function() open() return win:kind() end
]]

test.describe("fs.watch", function()
  test.before_each(function()
    test.source(CONFIG)
  end)

  test.it("hears a file appear, change and go", function()
    test.ipc("write", "{}")
    test.wait(function() return test.ipc("heard"):find("created:settings.json") end, 3000)
    test.ipc("write", "{ }")
    test.wait(function() return test.ipc("heard"):find("changed:settings.json") end, 3000)
    test.ipc("remove")
    test.wait(function() return test.ipc("heard"):find("deleted:settings.json") end, 3000)
    test.matches(test.ipc("entries"), "created:settings.json")
  end)

  test.it("says nothing more once closed", function()
    test.eq(test.ipc("close_watch"), true)
    test.ipc("write", "{}")
    -- The directory's own watch hears it; the closed one must not.
    test.wait(function() return test.ipc("entries"):find("settings.json") end, 3000)
    test.advance(100)
    test.eq(test.ipc("heard"), "")
  end)
end)

test.describe("window:destroy", function()
  test.before_each(function()
    test.source(CONFIG)
  end)

  test.it("takes the surface and its tree, and hears on_closed once", function()
    test.truthy(test.find { id = "doomed-text" })
    local before = #test.surfaces()
    test.ipc("destroy")
    test.eq(test.ipc("closed"), 1)
    test.eq(#test.surfaces(), before - 1)
    test.falsy(test.find { id = "doomed-root" })
    test.falsy(test.find { id = "doomed-text" })
    local ok, err = test.ipc("after")
    test.eq(ok, false)
    test.matches(err, "window destroyed")
    test.advance(100)
    test.eq(test.ipc("closed"), 1)
  end)

  test.it("leaves room for a new window after it", function()
    test.ipc("destroy")
    test.eq(test.ipc("reopen"), "floating")
    test.truthy(test.find { id = "doomed-text" })
  end)
end)
