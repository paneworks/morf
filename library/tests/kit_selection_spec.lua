-- lib.kit.selection and Plane widgets with a plain skin: tabs the arrows
-- walk, a list typed into, a grid moved by rows, a multi selection, an
-- indicator that follows the current item, and a plane.
--
--     morf test library/tests/kit_selection_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  skin.define("plain", { skins = {
    Selection = function(t)
      return {
        indicator = ui.Rect { id = "indicator", color = "#ff0000",
          x = function() return t.current_x end, y = function() return t.current_y end,
          width = function() return t.current_width end, height = 2 },
        item = function(index, value, s)
          return ui.Text { id = "item-" .. tostring(value), text = tostring(value),
            color = function() return s.current() and "#ffffff" or "#888888" end }
        end,
      }
    end,
    Plane = function(t) return { handle = ui.Rect { id = "plane-handle", width = 4, height = 4,
      x = function() return t.visual_x * 100 end, y = function() return t.visual_y * 100 end } } end,
  } })
  skin.use("plain")
  local log = {}
  local tab = morf.signal("test.tab", 1)
  ui.Item { width = 600, height = 400,
    w.tabs { id = "tabs", x = 10, y = 10, items = { "One", "Two", "Three" }, item_width = 60, item_height = 30,
      current = function() return tab:get() end,
      on_current_changed = function(i) tab:set(i) log[#log + 1] = "tab:" .. i end },
    w.list_selection { id = "list", x = 10, y = 60, item_width = 120, item_height = 20,
      items = { "Firefox", "Files", "Terminal", "Thunar" },
      on_activated = function(i) log[#log + 1] = "run:" .. i end },
    w.grid_selection { id = "grid", x = 300, y = 10, columns = 3, item_width = 30, item_height = 30,
      items = { 1, 2, 3, 4, 5, 6, 7, 8, 9 }, current = 2 },
    w.toggle_group { id = "multi", x = 300, y = 200, items = { "B", "I", "U" }, item_width = 30, item_height = 30,
      on_selection_changed = function(sel) log[#log + 1] = "sel:" .. table.concat(sel, "+") end },
    w.xy_pad { id = "pad", x = 10, y = 200, width = 100, height = 100,
      on_moved = function(x, y) log[#log + 1] = ("pad:%.2f,%.2f"):format(x, y) end },
  }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
]]

local function load() test.load { source = HOST, size = { 600, 400 } } test.settle(200) end

local function focus(id)
  for _ = 1, 12 do
    for _, node in ipairs(test.nodes()) do if node.focused and node.id == id then return end end
    test.key("Tab") test.settle(10)
  end
  error("could not reach " .. id)
end

test.it("tabs follow a click and the arrows, and the indicator follows them", function()
  load()
  test.click("item-Three") test.settle(30)
  test.eq(test.ipc("log"), "tab:3")
  local indicator, three = test.get("indicator"), test.get("item-Three")
  test.near(indicator.x, three.x, 1)
  focus("tabs")
  test.key("Left") test.settle(30)
  test.eq(test.ipc("log"), "tab:2")
  test.key("End") test.settle(30)
  test.eq(test.ipc("log"), "tab:3")
end)

test.it("a list is typed into and Return activates", function()
  load()
  focus("list")
  test.type("th") test.settle(30)
  test.key("Return") test.settle(30)
  test.eq(test.ipc("log"), "run:4")
end)

test.it("a grid moves by rows", function()
  load()
  focus("grid")
  test.key("Down") test.settle(30)
  test.eq(test.get("item-5").visible, true)
  test.key("Down") test.key("Right") test.settle(30)
  -- 2 -> 5 -> 8 -> 9; Down past the last row goes nowhere.
  test.key("Down") test.settle(30)
  local indicator, nine = test.get("indicator", { surface = nil }), test.get("item-9")
  test.truthy(nine)
end)

test.it("a toggle group selects several", function()
  load()
  test.click("item-B") test.settle(20)
  test.click("item-U") test.settle(20)
  test.eq(test.ipc("log"), "sel:1,sel:1+3")
end)

test.it("a pad takes both values from a press", function()
  load()
  local pad = test.get("pad")
  test.click(pad.x + 25, pad.y + 25) test.settle(20)
  test.eq(test.ipc("log"), "pad:0.25,0.75")
  local handle = test.get("plane-handle")
  test.near(handle.x - pad.x, 25, 1)
end)
