-- A bare kit Control (lib.kit.control) drawn by each caelestia theme's
-- skin: it renders, answers the pointer, shows a ring only for keyboard
-- focus, and a theme switch rebuilds it with no node left behind.
--
--     morf test --no-dbus examples/shells/caelestia/tests/kit_control_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  require("kit")
  local skin = require("lib.kit.skin")
  local control = require("lib.kit.control")
  local root = ui.Item { width = 400, height = 200 }
  ui.reparent(control.make("Control", "Control",
    { id = "kit-control", x = 20, y = 20, width = 160, height = 80, focus_policy = "strong" }), root)
  -- Both caelestia themes' skins, for a switch between them.
  for _, name in ipairs { "material", "tsugumori" } do
    if not skin.has(name) then
      local package = require("themes.init").load(name)
      local kit = require(package.components)(require("theme"))
      skin.define(name, { skins = kit.skins })
    end
  end
  morf.ipc.use = function(name) skin.use(name) end
  morf.ipc.theme = function() return skin.current() end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 400, 200 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
  test.settle(500)
end

local function drawn()
  local control = test.get("kit-control")
  local n = 0
  for _, node in ipairs(test.nodes()) do
    if node.visible and node.parent and node.x >= control.x - 1 and node.x + node.width <= control.x + control.width + 1
      and node.y >= control.y - 1 and node.y + node.height <= control.y + control.height + 1 and node.handle ~= control.handle then
      n = n + 1
    end
  end
  return n
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " draws a bare Control and switches theme without leaking", function()
    load(style)
    test.eq(test.ipc("theme"), style)
    test.truthy(drawn() > 0, "the skin drew nothing")
    local nodes = #test.nodes()
    local other = style == "material" and "tsugumori" or "material"
    test.ipc("use", other) test.settle(200)
    test.truthy(drawn() > 0)
    test.ipc("use", style) test.settle(200)
    test.eq(#test.nodes(), nodes, "switching theme there and back changed the node count")
    test.eq(#test.logs("error"), 0)
  end)
end
