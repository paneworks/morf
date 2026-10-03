-- lib.kit.widgets on the Press and Range archetypes, with a plain skin:
-- toggles, an exclusive group the arrows walk, a slider driven by the
-- pointer, the keys and the wheel, auto-repeat, and keys a control does
-- not use going on to what is around it.
--
--     morf test library/tests/kit_widgets_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  skin.define("plain", { skins = {
    Press = function(t) return { background = ui.Rect { anchors = { fill = true },
      color = function() return t.checked and "#00ff00" or "#333333" end } } end,
    Range = function(t) return {
      track = ui.Item { x = 10, y = 0, width = 100, height = 20 },
      handle = ui.Rect { y = 0, width = 4, height = 20, color = "#ffffff",
        x = function() return 10 + 100 * t.visual_position - 2 end },
    } end,
  } })
  skin.use("plain")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local level = morf.signal("test.level", 0.5)
  local parent_keys = 0
  ui.Item { width = 600, height = 300,
    on_key_pressed = function(_, _, _, _, name) parent_keys = parent_keys + 1 note("parent:" .. tostring(name)) end,
    w.switch { id = "sw", x = 10, y = 10, on_toggled = function(on) note("switch:" .. tostring(on)) end },
    w.radio { id = "r1", x = 10, y = 60, group = "g", checked = true, on_toggled = function(on) note("r1:" .. tostring(on)) end },
    w.radio { id = "r2", x = 40, y = 60, group = "g", on_toggled = function(on) note("r2:" .. tostring(on)) end },
    w.radio { id = "r3", x = 70, y = 60, group = "g", enabled = false },
    w.slider { id = "sl", x = 10, y = 120, width = 120, height = 20, step = 0.1,
      value = function() return level:get() end,
      on_moved = function(v) level:set(v) note(("moved:%.3f"):format(v)) end, on_pressed = function(x) note("pressed:" .. tostring(x)) end },
    w.repeat_button { id = "rep", x = 200, y = 10, width = 40, height = 40, repeat_delay = 300, repeat_interval = 100,
      on_clicked = function() note("rep") end, on_released = function() note("rel") end, on_pressed = function() note("prs") end },
    w.push { id = "push", x = 260, y = 10, width = 60, height = 30, on_clicked = function() note("push") end },
  }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.level = function() return level:get() end
  morf.ipc.dbg = function() return log end
]]

local function load() test.load { source = HOST, size = { 600, 300 } } test.settle(200) end

test.it("a switch toggles on a click and on Space", function()
  load()
  test.click("sw") test.settle(20)
  test.eq(test.ipc("log"), "switch:true")
  test.key("Tab") test.settle(20)
  test.key("space") test.settle(20)
  test.eq(test.ipc("log"), "switch:false")
end)

test.it("an exclusive group keeps one checked and the arrows walk it", function()
  load()
  test.click("r2") test.settle(20)
  test.eq(test.ipc("log"), "r2:true,r1:false")
  -- Pressing the checked one again leaves it checked.
  test.click("r2") test.settle(20)
  test.eq(test.ipc("log"), "")
  -- From r2 the arrow skips the disabled r3 and comes round to r1.
  for _ = 1, 3 do test.key("Tab") test.settle(10) end
  local focused
  for _, node in ipairs(test.nodes()) do if node.focused then focused = node.id end end
  test.eq(focused, "r2")
  test.key("Right") test.settle(20)
  test.eq(test.ipc("log"), "r2:false,r1:true")
  for _, node in ipairs(test.nodes()) do if node.focused then focused = node.id end end
  test.eq(focused, "r1")
end)

test.it("a slider follows the pointer, the keys and the wheel", function()
  load()
  local slider = test.get("sl")
  -- 30 px into its 100 px track, which starts 10 px in.
  test.click(slider.x + 40, slider.y + 10) test.settle(20)
  test.near(test.ipc("level"), 0.3, 0.001)
  test.key("Right") test.settle(20)
  test.near(test.ipc("level"), 0.4, 0.001)
  test.key("End") test.settle(20)
  test.eq(test.ipc("level"), 1)
  test.wheel(0, 1, { x = slider.x + 40, y = slider.y + 10 }) test.settle(20)
  test.near(test.ipc("level"), 0.9, 0.001)
end)

test.it("auto-repeat clicks while held", function()
  load()
  local b = test.get("rep")
  test.move(b.x + 5, b.y + 5)
  test.press(b.x + 5, b.y + 5)
  test.advance(350)
  test.eq(test.ipc("log"), "prs,rep")
  test.advance(210)
  test.release(b.x + 5, b.y + 5)
  test.settle(20)
  -- More repeats while held, and no click of its own on the release.
  local log = test.ipc("log")
  test.truthy(log:match("^rep,rep") and log:match(",rel$"), log)
end)

test.it("a key a control does not use goes on to its parent", function()
  load()
  for _ = 1, 10 do
    test.key("Tab") test.settle(10)
    local focused
    for _, node in ipairs(test.nodes()) do if node.focused then focused = node.id end end
    if focused == "push" then break end
  end
  test.key("a") test.settle(20)
  test.eq(test.ipc("log"), "parent:a")
  test.key("Return") test.settle(20)
  test.eq(test.ipc("log"), "push")
end)
