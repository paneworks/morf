-- Right to left (`layout_direction`): rows pack from the right, anchors
-- swap sides, a flex row runs right to left, text keeps to its start, an
-- `ltr` subtree inside stays left to right, and a slider's handle and
-- arrow keys mirror.
--
--     morf test library/tests/rtl_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local w = require("lib.kit.widgets")
  require("lib.kit.skin").define("plain", { skins = {
    Range = function(t) return {
      track = ui.Item { x = 0, y = 0, width = 200, height = 20 },
      handle = ui.Rect { id = "handle", y = 0, width = 4, height = 20,
        x = function() return 200 * t.visual_position - 2 end } } end,
  } })
  require("lib.kit.skin").use("plain")
  local level = morf.signal("rtl.level", 0.25)
  ui.Item { width = 600, height = 400, layout_direction = "rtl",
    ui.Row { id = "row", width = 300, height = 20, gap = 10,
      ui.Rect { id = "first", width = 50, height = 20 }, ui.Rect { id = "second", width = 50, height = 20 } },
    ui.Rect { id = "start", y = 40, width = 40, height = 20, anchors = { left = true, left_margin = 8 } },
    ui.Text { id = "words", y = 80, width = 200, text = "hello", horizontal_alignment = "left" },
    ui.Item { y = 120, width = 600, height = 40, layout_direction = "ltr",
      ui.Rect { id = "kept", width = 40, height = 20, anchors = { left = true } } },
    ui.Flex { id = "flex", y = 170, width = 300, height = 20, direction = "row",
      ui.Rect { id = "f1", width = 50, height = 20 }, ui.Rect { id = "f2", width = 50, height = 20 } },
    ui.Item { y = 220, width = 200, height = 20,
      w.slider { id = "slider", width = 200, height = 20, step = 0.25,
        value = function() return level:get() end, on_moved = function(v) level:set(v) end } },
  }
  morf.ipc.level = function() return level:get() end
]]

test.it("mirrors rows, anchors, flex rows and text, and keeps an ltr subtree", function()
  test.load { source = HOST, size = { 600, 400 } } test.settle(100)
  test.eq(test.get("first").x, 300 - 50)
  test.eq(test.get("second").x, 300 - 50 - 10 - 50)
  test.eq(test.get("start").x, 600 - 8 - 40)
  test.eq(test.get("kept").x, 0)
  test.eq(test.get("f1").x, 300 - 50)
  test.eq(test.get("f2").x, 300 - 100)
end)

test.it("mirrors a slider's handle and its arrow keys", function()
  test.load { source = HOST, size = { 600, 400 } } test.settle(200)
  -- A quarter of the way, from the right.
  test.near(test.get("handle").x, 200 * 0.75 - 2, 1)
  test.click("slider") test.settle(50)
  local before = test.ipc("level")
  test.key("Right") test.settle(50)
  test.truthy(test.ipc("level") < before, "Right did not move toward the start")
end)
