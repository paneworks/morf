-- The example Editor application (examples/apps/editor/app.lua): it
-- builds its dock of file tree, documents, map and log; a tab dragged
-- onto another stack's edge splits it and the layout is kept in the
-- application's state; a node in the graph moves; the toolbar's tool
-- reaches the canvases and a stroke drawn with it lands on the board.
--
--     morf test --no-dbus examples/apps/editor/tests/editor_app_spec.lua
--     EDITOR_SNAPSHOT=1 nixVulkanIntel morf test --no-dbus --snapshots DIR examples/apps/editor/tests/editor_app_spec.lua

local test = morf.test
local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end

local function load()
  test.load("../app.lua", { size = { 1600, 1000 } })
  test.settle(1500)
end

-- A node's centre and the surface it is on, as input options.
local function at(query, dx, dy)
  local node = test.get(query)
  return node.x + node.width / 2 + (dx or 0), node.y + node.height / 2 + (dy or 0), { surface = node.surface }
end

test.it("opens with a header, tools, a file tree, the graph, the map and the log", function()
  load()
  test.truthy(test.find { id = "editor-header", visible = true }, "no header bar")
  test.truthy(test.find { id = "editor-tools", visible = true }, "no toolbar")
  test.truthy(test.find { id = "editor-files", visible = true }, "no file tree")
  test.truthy(test.find { id = "editor-graph", visible = true }, "no node graph")
  test.truthy(test.find { id = "editor-map", visible = true }, "no map")
  test.truthy(test.find { id = "editor-log", visible = true }, "no log")
  test.eq(test.ipc("editor-layout"), "H([files],V(H([graph board],[map]),[log]))")
  test.eq(#test.logs("error"), 0)
  if env("EDITOR_SNAPSHOT") then test.snapshot("editor.png") end
end)

test.it("a tab dragged onto the bottom of another stack splits it, and the layout is kept", function()
  load()
  -- The dock tells the application its layout from the start.
  test.truthy(test.ipc("editor-saved"), "the dock's layout was not kept")
  -- The Map tab: the top-left of the tools stack.
  local map = test.get("editor-map")
  local surface = { surface = map.surface }
  local tab_x, tab_y = map.x + 40, map.y - 17
  -- Onto the bottom quarter of the documents stack.
  local graph = test.get("editor-graph")
  local to_x, to_y = graph.x + graph.width / 2, graph.y + graph.height - 20
  test.press(tab_x, tab_y, surface) test.settle(30)
  for i = 1, 12 do
    test.move(tab_x + (to_x - tab_x) * i / 12, tab_y + (to_y - tab_y) * i / 12, surface) test.settle(16)
  end
  test.release(to_x, to_y, surface) test.settle(300)
  -- (The split the map left had one part and went; the new one joins the column.)
  test.eq(test.ipc("editor-layout"), "H([files],V([graph board],[map],[log]))")
  test.truthy(test.find { id = "editor-map", visible = true }, "the map went missing")
end)

test.it("a node in the graph moves with a drag", function()
  load()
  test.eq(test.ipc("editor-node", "blur"), "280,80")
  local graph = test.get("editor-graph")
  local surface = { surface = graph.surface }
  -- The Blur node's header, wherever the fitted view shows it; moved by
  -- two grid steps across and one down.
  local at_screen = test.ipc("editor-node-screen", "blur")
  local x, y = graph.x + at_screen.x, graph.y + at_screen.y
  local dx, dy = 32 * at_screen.zoom, 16 * at_screen.zoom
  test.press(x, y, surface) test.settle(30)
  for i = 1, 8 do test.move(x + dx * i / 8, y + dy * i / 8, surface) test.settle(16) end
  test.release(x + dx, y + dy, surface) test.settle(100)
  test.eq(test.ipc("editor-node", "blur"), "312,96")
  test.truthy(test.ipc("editor-state").status:find("Moved blur", 1, true), "the move was not logged")
end)

test.it("the toolbar's tool reaches the canvases and a stroke is drawn on the board", function()
  load()
  test.click("editor-tool-freehand") test.settle(200)
  local s = test.ipc("editor-state")
  test.eq(s.tool, "freehand")
  -- The graph has no pen: it keeps selecting.
  test.eq(s.graph_tool, "select")
  test.ipc("editor-activate", "board") test.settle(300)
  test.eq(test.ipc("editor-state").board_tool, "freehand")
  local before = test.ipc("editor-state").strokes
  local board = test.get("editor-board")
  local surface = { surface = board.surface }
  local x, y = board.x + 120, board.y + board.height - 80
  test.press(x, y, surface) test.settle(30)
  for i = 1, 10 do test.move(x + i * 20, y - math.sin(i / 2) * 30, surface) test.settle(16) end
  test.release(x + 200, y, surface) test.settle(100)
  test.eq(test.ipc("editor-state").strokes, before + 1)
  test.click("editor-tool-select") test.settle(200)
  test.eq(test.ipc("editor-state").board_tool, "select")
  test.eq(#test.logs("error"), 0)
end)
