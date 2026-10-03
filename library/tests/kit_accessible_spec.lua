-- What a screen reader is told of kit controls (lib.kit.control sets each
-- control's role, name and live states; morf builds the tree), and what it
-- may do: press, toggle, set a value, step it, open and shut, pick a tab.
--
--     morf test library/tests/kit_accessible_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  local disclosure = require("lib.kit.disclosure")
  skin.define("plain", { skins = {} })
  skin.use("plain")
  local log = {}
  local level = morf.signal("test.a11y.level", 0.5)
  local tab = morf.signal("test.a11y.tab", 1)
  ui.Item { width = 600, height = 400,
    w.push { id = "push", x = 10, y = 10, width = 80, height = 30, label = "Apply",
      on_clicked = function() log[#log + 1] = "push" end },
    w.switch { id = "sw", x = 100, y = 10, width = 50, height = 30, accessible_name = "Wi-Fi" },
    w.slider { id = "sl", x = 10, y = 60, width = 200, height = 20, step = 0.1, accessible_name = "Volume",
      value = function() return level:get() end, on_moved = function(v) level:set(v) end },
    w.tabs { id = "tabs", x = 10, y = 100, width = 300, height = 30, items = { "One", "Two", "Three" },
      current = function() return tab:get() end, on_current_changed = function(i) tab:set(i) end },
    disclosure.make("expander", { id = "more", x = 10, y = 150, width = 200, title = "More",
      content = ui.Rect { width = 200, height = 40 } }),
    ui.Text { x = 10, y = 300, text = "Plain words" },
  }
  morf.ipc.log = function() local s = table.concat(log, ",") log = {} return s end
  morf.ipc.level = function() return level:get() end
  morf.ipc.tab = function() return tab:get() end
]]

local function load() test.load { source = HOST, size = { 600, 400 } } test.settle(100) end
local function row(query) return test.accessible(query)[1] end

test.it("names every control by its role and states", function()
  load()
  test.eq(row({ role = "window" }).children > 0, true)
  local push = row { id = "push" }
  test.eq(push.role, "button") test.eq(push.name, "Apply") test.truthy(push.focusable)
  local sw = row { id = "sw" }
  test.eq(sw.role, "switch") test.eq(sw.name, "Wi-Fi") test.eq(sw.checked, false)
  local sl = row { id = "sl" }
  test.eq(sl.role, "slider") test.near(sl.value, 0.5, 1e-6) test.eq(sl.minimum, 0) test.eq(sl.maximum, 1)
  test.eq(row({ id = "tabs" }).role, "tab_list")
  local tabs = test.accessible { role = "tab" }
  test.eq(#tabs, 3) test.eq(tabs[1].name, "One") test.eq(tabs[1].selected, true) test.eq(tabs[2].selected, false)
  local more = row { id = "more" }
  test.eq(more.role, "button") test.eq(more.name, "More") test.eq(more.expanded, false)
  test.eq(row({ name = "Plain words" }).role, "label")
end)

test.it("does what a screen reader asks as the control's own keys would", function()
  load()
  test.truthy(test.accessible_action(row { id = "push" }, "click")) test.settle(50)
  test.eq(test.ipc("log"), "push")
  test.accessible_action(row { id = "sw" }, "click") test.settle(50)
  test.eq(row({ id = "sw" }).checked, true)
  test.accessible_action(row { id = "sl" }, "set_value", 0.8) test.settle(50)
  test.near(test.ipc("level"), 0.8, 1e-6)
  test.accessible_action(row { id = "sl" }, "decrement") test.settle(50)
  test.near(test.ipc("level"), 0.7, 1e-6)
  test.accessible_action(row { id = "more" }, "expand") test.settle(300)
  test.eq(row({ id = "more" }).expanded, true)
  test.accessible_action(test.accessible({ role = "tab" })[3], "click") test.settle(50)
  test.eq(test.ipc("tab"), 3)
  test.eq(test.accessible({ role = "tab" })[3].selected, true)
end)
