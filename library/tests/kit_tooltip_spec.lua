-- lib.kit.archetypes.popup.tooltip on a plain node and on a MouseArea: the tip opens
-- after the pointer rests on its target and closes when it leaves.
--
--     morf test library/tests/kit_tooltip_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local popup = require("lib.kit.archetypes.popup")
  skin.define("plain", { skins = {
    Popup = function() return { background = ui.Rect { anchors = { fill = true }, color = "#202020" } } end,
  } })
  skin.use("plain")
  local plain = ui.Rect { id = "plain", x = 20, y = 20, width = 100, height = 40, color = "#404040" }
  local area = ui.MouseArea { id = "area", x = 200, y = 20, width = 100, height = 40 }
  ui.Item { width = 400, height = 300, plain, area }
  local plain_tip = popup.tooltip(plain, "Plain", 200)
  local area_tip = popup.tooltip(area, "Area", 200)
  morf.ipc.plain_open = function() return plain_tip.is_open() end
  morf.ipc.area_open = function() return area_tip.is_open() end
]]

local function load() test.load { source = HOST, size = { 400, 300 } } test.settle(100) end

test.it("a tooltip on a node with no hover of its own opens and closes", function()
  load()
  test.falsy(test.ipc("plain_open"))
  test.move(60, 40) test.advance(250) test.settle(100)
  test.truthy(test.ipc("plain_open"), "the tip did not open over its target")
  test.move(60, 200) test.settle(400)
  test.falsy(test.ipc("plain_open"), "the tip stayed open after the pointer left")
end)

test.it("a tooltip on a MouseArea opens on hover and closes when it leaves", function()
  load()
  test.move(250, 40) test.advance(250) test.settle(100)
  test.truthy(test.ipc("area_open"), "the tip did not open over its area")
  test.leave() test.settle(400)
  test.falsy(test.ipc("area_open"), "the tip stayed open after the pointer left")
end)
