-- lib.kit.canvas and lib.kit.dock with a plain skin: items picked, moved
-- on a grid and banded; the view panned by the middle button and zoomed
-- by Ctrl and the wheel; a wire pulled between ports; a polygon drawn; a
-- dock's tab dragged onto another stack's edge, closed, and walked by keys.
--
--     morf test --no-dbus library/tests/kit_canvas_dock_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  skin.define("plain", { skins = {} })
  skin.use("plain")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local items = morf.signal("items", {
    { id = "a", x = 20, y = 20, w = 60, h = 40 },
    { id = "b", x = 200, y = 120, w = 60, h = 40 },
  })
  local canvas, view = w.node_graph { id = "canvas", x = 0, y = 0, width = 400, height = 300,
    items = function() return items:get() end,
    ports = { { id = "a.out", item = "a", x = 80, y = 40, kind = "out" },
      { id = "b.in", item = "b", x = 200, y = 140, kind = "in" } },
    delegate = function(item, s) return ui.Rect { id = "item-" .. item.id, anchors = { fill = true }, color = "#446688" } end,
    on_moved = function(ids, dx, dy)
      local next = {}
      for _, it in ipairs(items:get()) do
        local moved = false
        for _, id in ipairs(ids) do if id == it.id then moved = true end end
        next[#next + 1] = moved and { id = it.id, x = it.x + dx, y = it.y + dy, w = it.w, h = it.h } or it
      end
      items:set(next)
      note(("moved:%s:%g:%g"):format(table.concat(ids, "+"), dx, dy))
    end,
    on_connected = function(from, to) note("wire:" .. from .. ">" .. to) end,
    on_drawn = function(tool, points) note(("drawn:%s:%d"):format(tool, #points // 2)) end,
    on_deleted = function(ids) note("deleted:" .. table.concat(ids, "+")) end,
    resizable = true,
    on_resized = function(id, x, y, w, h)
      local next = {}
      for _, it in ipairs(items:get()) do
        next[#next + 1] = it.id == id and { id = id, x = x, y = y, w = w, h = h } or it
      end
      items:set(next)
      note(("resized:%s:%g:%g:%g:%g"):format(id, x, y, w, h))
    end }
  local dock_node, dock = w.dock_area { id = "dock", x = 420, y = 0, width = 400, height = 300,
    panels = { files = { title = "Files", content = ui.Rect { id = "files-body", width = 10, height = 10 } },
      search = { title = "Search", content = ui.Rect { id = "search-body", width = 10, height = 10 } },
      editor = { title = "Editor", closable = false, content = ui.Rect { id = "editor-body", width = 10, height = 10 } } },
    layout = { orientation = "horizontal", ratios = { 0.4, 0.6 }, children = {
      { id = "left", panels = { "files", "search" } }, { id = "main", panels = { "editor" } } } },
    on_closed = function(p) note("closed:" .. p) end }
  ui.Item { width = 840, height = 320, canvas, dock_node }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.view = function() return ("%.3f,%.3f,%.3f"):format(view.t.view_x, view.t.view_y, view.t.zoom) end
  morf.ipc.selection = function() return table.concat(view.selection(), "+") end
  morf.ipc.tool = function(name) view.set_tool(name) end
  morf.ipc.size = function(id) for _, it in ipairs(items:get()) do if it.id == id then return ("%gx%g"):format(it.w, it.h) end end end
  morf.ipc.item = function(id) for _, it in ipairs(items:get()) do if it.id == id then return ("%g,%g"):format(it.x, it.y) end end end
  morf.ipc.layout = function()
    local tree = dock.layout()
    local function show(n)
      if n.kind == "stack" then return "[" .. table.concat(n.panels, " ") .. "]" end
      local parts = {}
      for _, c in ipairs(n.children) do parts[#parts + 1] = show(c) end
      return (n.orientation == "vertical" and "V(" or "H(") .. table.concat(parts, ",") .. ")"
    end
    return tree and show(tree) or ""
  end
  morf.ipc.state = function() return ("%s|%s|%s"):format(view.t.tool, view.t.gesture, tostring(view.t.draft)) end
  morf.ipc.focused = function() return dock.t.focused_panel end
]]

local function load() test.load { source = HOST, size = { 840, 320 } } test.settle(100) end

test.it("a press picks an item, a drag moves it on the grid, Shift adds", function()
  load()
  test.drag({ 40, 30 }, { 75, 52 }, { steps = 6 }) test.settle(50)
  test.eq(test.ipc("log"), "moved:a:32:16")
  test.eq(test.ipc("item", "a"), "52,36")
  test.click(220, 130, { modifiers = "shift" }) test.settle(30)
  test.eq(test.ipc("selection"), "a+b")
  test.key("Delete") test.settle(30)
  test.eq(test.ipc("log"), "deleted:a+b")
end)

test.it("a selected box resizes by its corner handle, on the grid", function()
  load()
  test.click(40, 30) test.settle(30)
  test.eq(test.ipc("selection"), "a")
  -- The south-east corner of (20, 20, 60, 40), at (80, 60): its edges land
  -- on the grid of 16, at 112 and 96.
  test.drag({ 80, 60 }, { 117, 91 }, { steps = 6 }) test.settle(50)
  test.eq(test.ipc("log"), "resized:a:20:20:92:76")
  test.eq(test.ipc("size", "a"), "92x76")
end)

test.it("a band on nothing selects what it crosses", function()
  load()
  test.drag({ 150, 100 }, { 390, 290 }, { steps = 6 }) test.settle(30)
  test.eq(test.ipc("selection"), "b")
end)

test.it("the middle button pans and Ctrl with the wheel zooms about the pointer", function()
  load()
  test.drag({ 300, 250 }, { 250, 200 }, { button = "middle", steps = 5 }) test.settle(30)
  test.eq(test.ipc("view"), "50.000,50.000,1.000")
  test.move(100, 100) test.settle(10)
  test.wheel(0, -1, { x = 100, y = 100, modifiers = "ctrl" }) test.settle(30)
  local x, y, z = test.ipc("view"):match("([^,]+),([^,]+),([^,]+)")
  test.eq(z, "1.200")
  -- The world point under the pointer stayed there: 150 = x + 100 / 1.2.
  test.truthy(math.abs(tonumber(x) + 100 / 1.2 - 150) < 0.01, x)
end)

test.it("a wire is pulled from an out port to an in port", function()
  load()
  test.drag({ 80, 40 }, { 201, 141 }, { steps = 8 }) test.settle(30)
  test.eq(test.ipc("log"), "wire:a.out>b.in")
end)

test.it("a polygon is clicked out and closes on its first point", function()
  load()
  test.ipc("tool", "polygon")
  test.click(100, 200) test.settle(450)
  test.click(160, 200) test.settle(450)
  test.click(160, 260) test.settle(450)
  test.click(101, 201) test.settle(30)
  test.eq(test.ipc("log"), "drawn:polygon:3")
end)

test.it("a dock's tab dragged onto another stack's bottom edge splits it", function()
  load()
  test.eq(test.ipc("layout"), "H([files search],[editor])")
  -- The second tab of the left stack, onto the bottom of the main one.
  test.drag({ 420 + 100, 17 }, { 420 + 260, 290 }, { steps = 10 }) test.settle(50)
  test.eq(test.ipc("layout"), "H([files],V([editor],[search]))")
  test.truthy(test.get("search-body").visible)
end)

test.it("a closable tab closes by the middle button and the keys walk the tabs", function()
  load()
  test.click(420 + 30, 17) test.settle(30)
  test.eq(test.ipc("focused"), "files")
  test.key("Page_Down", { "ctrl" }) test.settle(30)
  test.eq(test.ipc("focused"), "search")
  test.click(420 + 100, 17, { button = "middle" }) test.settle(30)
  test.eq(test.ipc("log"), "closed:search")
  test.eq(test.ipc("layout"), "H([files],[editor])")
end)
