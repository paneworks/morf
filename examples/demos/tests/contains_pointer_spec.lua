-- `contains_pointer`: true while the pointer is inside a node's box,
-- whatever is drawn over it, and a binding reading it re-runs only when it
-- changes.
--
--     morf test examples/demos/tests/contains_pointer_spec.lua

local test = morf.test

-- A panel with a button on it and an area behind everything (so the
-- panel is in the surface's input region), a clipped strip whose row
-- reaches past it, and a label that says what the panel's
-- `contains_pointer` is and counts how often it was worked out.
local CONFIG = [[
local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "contains"
morf.surface.anchors = {}
morf.surface.width = 400
morf.surface.height = 300

local runs = 0
local panel = ui.Rect {
  id = "panel", x = 100, y = 50, width = 200, height = 120, color = "#222222",
  ui.MouseArea { id = "back", anchors = { fill = true } },
  ui.MouseArea { id = "button", x = 20, y = 20, width = 60, height = 40 },
}
local strip = ui.Item {
  id = "strip", x = 0, y = 200, width = 100, height = 40, clip = true,
  ui.Rect { id = "row", x = 50, width = 150, height = 40, color = "#444444" },
}
local root = ui.Item {
  anchors = { fill = true },
  ui.MouseArea { id = "everywhere", anchors = { fill = true } },
  panel,
  strip,
  ui.Text {
    id = "label", x = 4, y = 4, color = "#ffffff",
    text = function()
      runs = runs + 1
      return panel.contains_pointer and "in" or "out"
    end,
  },
}
morf.ipc.runs = function() return runs end
-- A binding made while the pointer is already somewhere: it is answered
-- where the pointer is, not left false until the pointer next moves.
morf.ipc.late = function()
  ui.reparent(ui.Text {
    id = "late", x = 4, y = 20, color = "#ffffff",
    text = function() return strip.contains_pointer and "in" or "out" end,
  }, root)
end
]]

local function label() return test.get({ id = "label" }).text end

test.describe("contains_pointer", function()
  test.before_each(function()
    test.source(CONFIG)
    test.settle()
  end)

  test.it("is false until the pointer arrives", function()
    test.eq(label(), "out")
    test.falsy(test.get({ id = "panel" }).contains_pointer)
  end)

  test.it("stays true over what is on top of the node", function()
    test.move(150, 100)
    test.eq(label(), "in")
    -- Onto the button: the panel's own area is no longer hovered, and the
    -- panel still contains the pointer.
    test.move(140, 90)
    test.eq(label(), "in")
    test.truthy(test.get({ id = "panel" }).contains_pointer)
    test.truthy(test.get({ id = "button" }).contains_pointer)
    test.move(50, 100)
    test.eq(label(), "out")
  end)

  test.it("re-runs a binding only when it changes", function()
    local before = test.ipc("runs")
    for x = 110, 290, 10 do test.move(x, 100) end
    test.eq(test.ipc("runs"), before + 1, "one run for going in")
    test.move(20, 20)
    test.move(30, 30)
    test.eq(test.ipc("runs"), before + 2, "and one for coming out")
  end)

  test.it("goes false when the pointer leaves the surface", function()
    test.move(150, 100)
    test.eq(label(), "in")
    test.leave()
    test.eq(label(), "out")
  end)

  test.it("answers a node first asked about where the pointer already is", function()
    test.move(50, 220)
    test.ipc("late")
    test.eq(test.get({ id = "late" }).text, "in")
    test.move(50, 100)
    test.eq(test.get({ id = "late" }).text, "out")
  end)

  test.it("is clipped by an ancestor that clips", function()
    test.move(75, 220)
    test.truthy(test.get({ id = "row" }).contains_pointer)
    test.move(150, 220)
    test.falsy(test.get({ id = "row" }).contains_pointer)
  end)
end)
