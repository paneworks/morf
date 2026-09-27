-- Edge intent, tested without the full dashboard's rendering work.
local test = morf.test
local HOST = [[
  morf.surface.anchors = { top = true, bottom = true, left = true, right = true }
  morf.surface.width = 0
  morf.surface.height = 0
  local ui = require("morf.ui")
  local drawer = require("drawer")
  local hover = require("hover")
  local config = require("config")
  local enabled = morf.signal("hover.enabled", true)
  local nodes, drawers, shapes = {}, {}, {}
  for _, edge in ipairs { "top", "bottom", "left", "right" } do
    local d = drawer.new { name = edge, edge = edge, width = 260, height = 180,
      content = ui.MouseArea { width = 260, height = 180 } }
    shapes[#shapes + 1] = d.shape
    drawers[edge] = d
    nodes[#nodes + 1] = d.panel
    nodes[#nodes + 1] = hover.edge { name = edge, drawer = d, edge = edge,
      length = function() return 300 end, from = function() return 150 end,
      setting = "bottom.hover", enabled = function() return enabled:get() end }
  end
  shapes.anchors = { fill = true }
  ui.Item { anchors = { fill = true }, ui.Sdf(shapes), table.unpack(nodes) }
  morf.ipc.open = function(edge, on)
    if on ~= nil then drawers[edge].set(on) end
    return drawers[edge].open:get()
  end
  morf.ipc.enabled = function(on) enabled:set(on) end
  morf.ipc.setting = function(on) config.set("bottom.hover", on) end
]]
local function load(scale)
  test.load("../shell/hover.lua", { source = HOST, size = { 800, 600 }, scale = scale or 1 })
end
local positions = {
  top = { 400, 5, 400, 0.5 }, bottom = { 400, 595, 400, 599.5 },
  left = { 5, 300, 0.5, 300 }, right = { 795, 300, 799.5, 300 },
}
test.describe("caelestia edge intent", function()
  for _, edge in ipairs { "top", "bottom", "left", "right" } do
    test.it("requires a dwell even at the final pixel on the " .. edge .. " edge", function()
      load()
      local p = positions[edge]
      test.move(p[1], p[2]) test.advance(500)
      test.falsy(test.ipc("open", edge), "opened while just crossing the trigger")
      test.leave() test.advance(500)
      test.falsy(test.ipc("open", edge), "pending open survived pointer leave")
      test.move(p[1], p[2]) test.advance(500)
      test.falsy(test.ipc("open", edge), "the dwell did not reset after leaving")
      test.advance(120)
      test.truthy(test.ipc("open", edge), "a deliberate dwell did not open it")
      test.leave() test.advance(800)
      test.falsy(test.ipc("open", edge), "hover-opened drawer did not close")
      test.move(p[1], p[2]) test.advance(100)
      test.move(p[3], p[4]) test.advance(16)
      test.falsy(test.ipc("open", edge), "the last pixel bypassed the dwell")
      test.advance(504)
      test.truthy(test.ipc("open", edge), "moving within the trigger restarted the dwell")
      test.leave() test.advance(800)
      test.move(p[3], p[4]) test.advance(500)
      test.falsy(test.ipc("open", edge), "direct entry at the last pixel bypassed the dwell")
      test.leave() test.advance(500)
      test.falsy(test.ipc("open", edge), "pending last-pixel open survived pointer leave")
      test.move(p[3], p[4]) test.advance(620)
      test.truthy(test.ipc("open", edge), "dwelling at the last pixel did not open it")
      test.leave() test.advance(800)
      test.falsy(test.ipc("open", edge))
      test.eq(#test.logs("error"), 0)
    end)
  end
  test.it("cancels pending opening when disabled and leaves command-opened drawers alone", function()
    load()
    test.move(400, 595) test.advance(150)
    test.ipc("enabled", false) test.advance(800)
    test.falsy(test.ipc("open", "bottom"))
    test.move(400, 599.5) test.advance(16)
    test.falsy(test.ipc("open", "bottom"), "disabled last pixel opened it")
    test.leave()
    test.ipc("enabled", true)
    test.move(400, 595) test.advance(150)
    test.ipc("setting", false) test.advance(800)
    test.falsy(test.ipc("open", "bottom"))
    test.leave()
    test.ipc("setting", true)
    test.ipc("open", "bottom", true) test.advance(800)
    test.truthy(test.ipc("open", "bottom"), "hover policy closed a command-opened panel")
  end)
end)
