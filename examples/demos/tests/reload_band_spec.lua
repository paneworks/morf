-- A hot reload that fails: the running configuration keeps painting, and
-- the engine shows the failure on the primary surface in a band of its own
-- unless the configuration reports it itself.
--
--     morf test examples/demos/tests/reload_band_spec.lua

local test = morf.test

local BROKEN = "local ok = 1\nlocal y =\n"
local PARSE_ERROR = "unexpected end of token stream"

-- A shell with a `root` of that kind, `width` wide on a surface `height`
-- tall: an old label, a click counter over the whole surface, and whatever
-- `extra` adds before the root and `after` adds once it is built.
local function shell(root, options)
  options = options or {}
  return ([[
local ui = require("morf.ui")
morf.surface.anchors = { top = true, left = true, right = true }
morf.surface.height = %d
local clicks = 0
morf.ipc.clicks = function() return clicks end
morf.ipc.version = function() return 1 end
%s
local root = ui.%s {
  id = "root", width = %d, height = 300,
  ui.MouseArea {
    id = "everywhere", width = 1920, height = 300,
    on_clicked = function() clicks = clicks + 1 end,
  },
  ui.Text { id = "label", text = "v1", color = "#ffffff" },
  ui.Rect { id = "old", width = 40, height = 20, color = "#336699" },
}
%s
]]):format(options.height or 300, options.extra or "", root, options.width or 1920,
    options.after or "")
end

local FIXED = [[
local ui = require("morf.ui")
morf.surface.anchors = { top = true, left = true, right = true }
morf.surface.height = 300
morf.ipc.version = function() return 2 end
ui.Item { id = "root", ui.Text { id = "label", text = "v2", color = "#ffffff" } }
]]

local function band() return test.find({ id = "morf-reload-error" }) end
local function primary_width() return test.surfaces()[1].width end
local function band_text() return test.get({ id = "morf-reload-error-text" }) end

-- `inner` is drawn inside `outer`'s box.
local function inside(inner, outer)
  test.truthy(inner.x >= outer.x - 0.5 and inner.y >= outer.y - 0.5, "starts inside")
  test.truthy(inner.x + inner.width <= outer.x + outer.width + 0.5, "ends inside across")
  test.truthy(inner.y + inner.height <= outer.y + outer.height + 0.5, "ends inside down")
end

local function fail_reload()
  local ok, err = test.reload({ source = BROKEN })
  test.falsy(ok)
  test.contains(err, PARSE_ERROR)
  return err
end

test.describe("a failed reload", function()
  test.it("keeps the old scene painting and shows the error band", function()
    test.source(shell("Item", { width = 100 }))
    fail_reload()
    local old = test.get({ id = "old" })
    test.truthy(old.visible)
    test.eq({ old.width, old.height }, { 40, 20 })
    test.eq(test.get({ id = "label" }).text, "v1")
    test.eq(test.ipc("version"), 1)
    local shown = band()
    test.truthy(shown, "the band is shown")
    test.eq(shown.surface, "primary")
    test.eq({ shown.x, shown.y, shown.width }, { 0, 0, primary_width() })
    local text = band_text()
    test.contains(text.text, PARSE_ERROR)
    test.contains(text.text, "inline.lua")
    test.contains(text.text, "at line 3")
    inside(text, shown)
  end)

  test.it("is outside the configuration's tree and survives its rebuild", function()
    test.source(shell("Item", { after = [[
local content = ui.Item { id = "content" }
ui.reparent(content, root)
morf.ipc.rebuild = function()
  ui.destroy(content)
  content = ui.Item { id = "content", ui.Text { id = "rebuilt", text = "rebuilt" } }
  ui.reparent(content, root)
end
]] }))
    fail_reload()
    local overlay = test.get({ id = "morf-reload-error-overlay" })
    test.eq(overlay.depth, 0)
    test.eq(overlay.parent, nil)
    local rooted = test.find_all({ id = "root" })
    test.eq(#rooted, 1)
    test.eq(test.find({ id = "morf-reload-error" }).parent, overlay.handle)
    test.ipc("rebuild")
    test.eq(test.get({ id = "rebuilt" }).text, "rebuilt")
    test.truthy(band(), "the band outlives the rebuild")
    test.eq(test.problems(), {})
  end)

  test.it("names the line a runtime error points at", function()
    test.source(shell("Item"))
    local ok, err = test.reload({ source = "local t = nil\nlocal x = 1\nprint(t.field)\n" })
    test.falsy(ok)
    local text = band_text().text
    test.contains(text, "at line 3")
    test.contains(text, err)
  end)

  test.it("is removed by a fixed reload", function()
    test.source(shell("Item"))
    fail_reload()
    test.truthy(band())
    test.eq(test.reload({ source = FIXED }), true)
    test.eq(band(), nil)
    test.eq(test.get({ id = "label" }).text, "v2")
    test.eq(test.ipc("version"), 2)
  end)

  for _, root in ipairs({ "Item", "Row", "Column", "Grid", "Flex" }) do
    test.it("leaves a shell with a " .. root .. " root running", function()
      test.source(shell(root))
      fail_reload()
      test.advance(100)
      test.truthy(band(), "the band is shown")
      test.truthy(test.get({ id = "old" }).visible)
      test.eq(test.ipc("version"), 1)
      test.eq(band().width, primary_width())
      test.eq(test.problems(), {})
    end)
  end
end)

test.describe("a configuration's own reporting", function()
  test.it("takes the place of the band", function()
    test.source(shell("Item", { extra = [[
local reported = ""
morf.on_reload(function(ok, error) if not ok then reported = error return true end end)
morf.ipc.reported = function() return reported end
]] }))
    fail_reload()
    test.eq(band(), nil)
    test.contains(test.ipc("reported"), PARSE_ERROR)
  end)

  test.it("leaves the band to a handler that does not take the failure", function()
    test.source(shell("Item", { extra = [[
morf.on_reload(function(ok) end)
]] }))
    fail_reload()
    test.truthy(band(), "the band is shown")
  end)

  test.it("does not count when the handler throws", function()
    test.source(shell("Item", { extra = [[
morf.on_reload(function() error("handler broke") end)
]] }))
    fail_reload()
    test.truthy(band(), "the band is shown")
    local said = {}
    for _, line in ipairs(test.logs("warn")) do said[#said + 1] = line.message end
    test.contains(table.concat(said, "\n"), "handler broke")
    test.clear_logs()
  end)

  test.it("registered later is given the error, and the band comes down", function()
    test.source(shell("Item", { extra = [[
local reported = ""
morf.ipc.listen = function()
  morf.on_reload(function(ok, error) if not ok then reported = error return true end end)
end
morf.ipc.reported = function() return reported end
]] }))
    fail_reload()
    test.truthy(band())
    test.ipc("listen")
    test.advance(16)
    test.eq(band(), nil)
    test.contains(test.ipc("reported"), PARSE_ERROR)
  end)
end)

test.describe("a press on the primary surface", function()
  test.it("on the band takes it down and goes no further", function()
    test.source(shell("Item"))
    fail_reload()
    test.click(band())
    test.eq(band(), nil)
    test.eq(test.ipc("clicks"), 0)
  end)

  test.it("elsewhere takes the band down and reaches the shell", function()
    test.source(shell("Item"))
    fail_reload()
    local shown = band()
    test.click(400, shown.y + shown.height + 50)
    test.eq(band(), nil)
    test.eq(test.ipc("clicks"), 1)
  end)
end)

test.describe("a long error", function()
  local LONG = "error(string.rep('a long and winding error ', 120))"

  test.it("wraps inside a band of capped height", function()
    test.source(shell("Item"))
    test.falsy(test.reload({ source = LONG }))
    local shown = band()
    test.truthy(shown.height <= 240, "capped: " .. shown.height)
    test.truthy(shown.height > 40, "more than one line: " .. shown.height)
    test.eq(shown.width, primary_width())
    inside(band_text(), shown)
  end)

  test.it("fits a bar's height", function()
    test.source(shell("Item", { height = 32 }))
    test.falsy(test.reload({ source = LONG }))
    local shown = band()
    test.truthy(shown.height <= 32, "within the bar: " .. shown.height)
    inside(band_text(), shown)
  end)
end)
