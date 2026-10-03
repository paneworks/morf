-- lib.kit.transform with a plain skin: a floating panel moved by its title
-- bar, resized from a corner down to its minimum, maximized and restored
-- by a double click, Shift keeping its aspect, the arrows moving it; an
-- event block that resizes only up and down, on its grid; a
-- picture-in-picture that goes to the nearest corner.
--
--     morf test --no-dbus library/tests/kit_transform_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  skin.define("plain", { skins = {} })
  skin.use("plain")
  local log = {}
  local panel_area, panel = w.floating_panel { id = "panel", title = "Panel", x = 100, y = 100, width = 300, height = 200,
    ui.Rect { id = "panel-body", anchors = { fill = true }, color = "#334455" },
    on_committed = function(x, y, bw, bh) log[#log + 1] = ("%g,%g,%g,%g"):format(x, y, bw, bh) end,
    on_maximized = function(on) log[#log + 1] = "max:" .. tostring(on) end }
  local block_area, block = w.event_block { id = "block", grid = 30, x = 0, y = 60, width = 180, height = 60 }
  local pip_area, pip = w.pip_window { id = "pip", x = 20, y = 20, width = 160, height = 90 }
  ui.Item { width = 1460, height = 620,
    ui.Item { width = 800, height = 600, panel_area },
    ui.Item { x = 820, width = 200, height = 600, block_area },
    ui.Item { x = 1040, width = 400, height = 300, pip_area } }
  local function box(h) local t = h.t return ("%g,%g,%g,%g"):format(t.x, t.y, t.box_width, t.box_height) end
  morf.ipc.panel = function() return box(panel) end
  morf.ipc.block = function() return box(block) end
  morf.ipc.pip = function() return box(pip) end
  morf.ipc.maximized = function() return tostring(panel.t.maximized) end
  morf.ipc.log = function() local s = table.concat(log, " "); log = {}; return s end
  morf.ipc.set = function() panel.set(50, 60, 250, 150) end
]]

local function load() test.load { source = HOST, size = { 1460, 620 } } test.settle(400) end

test.it("a floating panel moves by its title bar, not by its body", function()
  load()
  test.eq(test.ipc("panel"), "100,100,300,200")
  test.drag({ 200, 115 }, { 260, 145 }, { steps = 6 }) test.settle(400)
  test.eq(test.ipc("panel"), "160,130,300,200")
  test.eq(test.ipc("log"), "160,130,300,200")
  -- Below the title bar the body is the content's: nothing moves.
  test.drag({ 300, 250 }, { 350, 280 }, { steps = 6 }) test.settle(400)
  test.eq(test.ipc("panel"), "160,130,300,200")
  -- The node is where the box is.
  local node = test.get("panel")
  test.eq(("%.0f,%.0f,%.0f,%.0f"):format(node.x, node.y, node.width, node.height), "160,130,300,200")
end)

test.it("a corner resizes it, down to its minimum", function()
  load()
  test.drag({ 400, 300 }, { 460, 340 }, { steps = 6 }) test.settle(400)
  test.eq(test.ipc("panel"), "100,100,360,240")
  test.drag({ 460, 340 }, { 0, 0 }, { steps = 8 }) test.settle(400)
  test.eq(test.ipc("panel"), "100,100,200,120")
  -- The west edge keeps the east one where it was.
  test.drag({ 100, 160 }, { 60, 160 }, { steps = 6 }) test.settle(400)
  test.eq(test.ipc("panel"), "60,100,240,120")
end)

test.it("Shift keeps the aspect", function()
  load()
  test.drag({ 400, 300 }, { 500, 310 }, { steps = 6, modifiers = "shift" }) test.settle(400)
  local x, y, bw, bh = test.ipc("panel"):match("([^,]+),([^,]+),([^,]+),([^,]+)")
  test.eq(x .. "," .. y .. "," .. bw, "100,100,400")
  test.truthy(math.abs(tonumber(bh) - 400 / 1.5) < 0.01, bh)
end)

test.it("a double click on the title maximizes to its area and another restores", function()
  load()
  test.click(250, 115) test.click(250, 115) test.settle(600)
  test.eq(test.ipc("maximized"), "true")
  test.eq(test.ipc("panel"), "0,0,800,600")
  local node = test.get("panel")
  test.eq(("%.0f,%.0f"):format(node.width, node.height), "800,600")
  test.truthy(test.ipc("log"):find("max:true", 1, true))
  -- Maximized, the title bar does not move it.
  test.drag({ 300, 15 }, { 360, 60 }, { steps = 6 }) test.settle(200)
  test.eq(test.ipc("panel"), "0,0,800,600")
  test.click(300, 15) test.click(300, 15) test.settle(600)
  test.eq(test.ipc("maximized"), "false")
  test.eq(test.ipc("panel"), "100,100,300,200")
end)

test.it("the arrows move it, Shift by ten, and Ctrl with them resizes", function()
  load()
  test.click(250, 115) test.settle(50)
  test.key("Right") test.key("Down") test.settle(30)
  test.eq(test.ipc("panel"), "101,101,300,200")
  test.key("Left", "shift") test.settle(30)
  test.eq(test.ipc("panel"), "91,101,300,200")
  test.key("Right", "ctrl") test.settle(30)
  test.eq(test.ipc("panel"), "91,101,301,200")
  test.key("Return") test.settle(50)
  test.eq(test.ipc("maximized"), "true")
  test.ipc("set")
end)

test.it("an event block resizes only up and down, on its grid", function()
  load()
  -- The south edge: 60 + 50 snaps to 120.
  test.drag({ 910, 120 }, { 910, 170 }, { steps = 6 }) test.settle(200)
  test.eq(test.ipc("block"), "0,60,180,120")
  -- The north edge: up 40 makes 160, which snaps to 150, the bottom kept.
  test.drag({ 910, 60 }, { 910, 20 }, { steps = 6 }) test.settle(200)
  test.eq(test.ipc("block"), "0,30,180,150")
  -- Its east side is no grip: a drag there moves it, on the grid.
  test.drag({ 999, 100 }, { 999, 133 }, { steps = 6 }) test.settle(200)
  test.eq(test.ipc("block"), "0,60,180,150")
end)

test.it("a picture-in-picture goes to the nearest corner when let go", function()
  load()
  test.drag({ 1100, 60 }, { 1300, 230 }, { steps = 8 }) test.settle(600)
  test.eq(test.ipc("pip"), "228,198,160,90")
end)
