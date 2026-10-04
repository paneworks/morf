-- Editor: an example application on the widget kit's Canvas and Dock, in
-- the manner of mara (an editor kit of docked shelves, tabbed containers,
-- floating panes and canvases).
--
--     morf app examples/apps/editor/app.lua
--
-- A window with a header bar and a toolbar of canvas tools over a dock: a
-- file tree on the left shelf, the documents in the middle as tabs -- a
-- node graph and a whiteboard --, a map as a tool window beside them and
-- a log along the bottom. Tabs drag between stacks, onto an edge to split
-- one, out to float; the layout the user makes is kept in the
-- application's state as the dock reports it, and the dock is built from
-- it. The toolbar's tool goes to the documents; what the canvases do --
-- a node moved, a wire pulled, a stroke drawn -- lands in the log. No
-- theme of its own: it draws with whatever `kit` is installed, else the
-- default look.
local morf = require("morf")
local ui = require("morf.ui")
local app = require("lib.kit.app")
local kit = app.kit()
local composites = require("lib.kit.composites")
local widgets = require("lib.kit.widgets")
local samples = require("lib.kit.samples.canvas")

local HEADER, TOOLS = 46, 44

-- What the application holds: the tool, the open document, and the
-- dock's layout as it last reported it (nil: the one below).
local state = morf.state { tool = "select", document = "graph", status = "Ready" }
local layout = morf.signal("editor.layout", nil)
local floating = morf.signal("editor.floating", {})

-- The layout survives a restart: kept in the state directory
-- (EDITOR_STATE names another file), read at start, written a moment
-- after the dock last changed.
local STATE_FILE = (morf.env and morf.env("EDITOR_STATE")) or morf.state_path("editor.json")
do
  local ok, text = pcall(morf.fs.read, STATE_FILE)
  local ok2, saved = false, nil
  if ok and type(text) == "string" and text ~= "" then ok2, saved = pcall(morf.json.decode, text) end
  if ok2 and type(saved) == "table" and type(saved.layout) == "table" then
    layout:set(saved.layout)
    floating:set(type(saved.floating) == "table" and saved.floating or {})
  end
end
local save_timer
local function save()
  if save_timer then save_timer:cancel() end
  save_timer = morf.timer(500, function()
    save_timer = nil
    local ok, err = pcall(morf.fs.write, STATE_FILE, morf.json.encode { layout = layout:get(), floating = floating:get() })
    if not ok then morf.log.warn("editor: could not save the layout: " .. tostring(err)) end
  end, false)
end

local DEFAULT_LAYOUT = { orientation = "horizontal", ratios = { 0.2, 0.8 }, children = {
  { id = "shelf", panels = { "files" } },
  { orientation = "vertical", ratios = { 0.7, 0.3 }, children = {
    { orientation = "horizontal", ratios = { 0.68, 0.32 }, children = {
      { id = "documents", panels = { "graph", "board" }, current = "graph" },
      { id = "tools", panels = { "map" } } } },
    { id = "bottom", panels = { "log" } } } } } }

-- ------------------------------------------------------------------ log --

local log = morf.list_model({})
local serial = 0
local function note(text)
  serial = serial + 1
  log:insert(1, { key = "line-" .. serial, label = ("%03d  %s"):format(serial, text) })
  state.status = text
end

-- --------------------------------------------------------------- tools --

-- Each tool: the canvases it means something to.
local TOOLS_LIST = {
  { id = "select", icon = "arrow_selector_tool", label = "Select", graph = true, board = true },
  { id = "pan", icon = "pan_tool", label = "Pan", graph = true, board = true },
  { id = "connect", icon = "polyline", label = "Connect", graph = true },
  { id = "freehand", icon = "draw", label = "Pen", board = true },
  { id = "rect", icon = "rectangle", label = "Box", board = true },
  { id = "ellipse", icon = "circle", label = "Ellipse", board = true },
  { id = "line", icon = "pen_size_2", label = "Line", board = true },
}
local BY_TOOL = {}
for _, tool in ipairs(TOOLS_LIST) do BY_TOOL[tool.id] = tool end
local function tool_for(canvas)
  local tool = BY_TOOL[state.tool]
  return tool and tool[canvas] and state.tool or "select"
end

-- ----------------------------------------------------------- documents --

local graph = samples.graph({
  { id = "image", title = "Image", tone = "info", x = 40, y = 60, outputs = { "Color", "Alpha" } },
  { id = "noise", title = "Noise", tone = "extra", x = 40, y = 280, inputs = { "Scale" }, outputs = { "Value" } },
  { id = "blur", title = "Blur", tone = "accent", x = 280, y = 80, inputs = { "Image", "Radius" }, outputs = { "Result" } },
  { id = "mix", title = "Mix", tone = "warning", x = 520, y = 200, w = 160, inputs = { "A", "B", "Factor" },
    outputs = { "Out" } },
  { id = "output", title = "Output", tone = "success", x = 760, y = 220, inputs = { "Image" } },
}, {
  { from = "image.Color", to = "blur.Image" }, { from = "noise.Value", to = "blur.Radius" },
  { from = "blur.Result", to = "mix.A" }, { from = "image.Alpha", to = "mix.Factor" },
  { from = "mix.Out", to = "output.Image" },
})

local strokes = morf.signal("editor.strokes", {
  { id = "note-1", x = 40, y = 40, w = 170, h = 110, label = "Wire the blur", tone = "warning", kind = "note" },
  { id = "note-2", x = 240, y = 60, w = 170, h = 110, label = "Try the map", tone = "info", kind = "note" },
})
local drawn = 0
local function draw(tool, points)
  drawn = drawn + 1
  local id = "stroke-" .. drawn
  local item
  if tool == "rect" or tool == "ellipse" then
    local x0, y0, x1, y1 = points[1], points[2], points[3], points[4]
    item = { id = id, shape = tool, x = math.min(x0, x1), y = math.min(y0, y1), w = math.abs(x1 - x0),
      h = math.abs(y1 - y0), tone = "accent" }
  else
    item = { id = id, shape = "line", points = points, width = 3, tone = "error" }
  end
  local list = {}
  for _, it in ipairs(strokes:get()) do list[#list + 1] = it end
  list[#list + 1] = item
  strokes:set(list)
  note(("Drew a %s"):format(tool == "freehand" and "stroke" or tool))
end
-- A note or a box dragged by a handle takes its new box.
local function resize_stroke(id, x, y, w, h)
  local list = {}
  for _, it in ipairs(strokes:get()) do
    if it.id == id then
      local copy = {}
      for k, v in pairs(it) do copy[k] = v end
      copy.x, copy.y, copy.w, copy.h = x, y, w, h
      list[#list + 1] = copy
    else
      list[#list + 1] = it
    end
  end
  strokes:set(list)
end

local function move_strokes(ids, dx, dy)
  local moved = {}
  for _, id in ipairs(ids) do moved[id] = true end
  local list = {}
  for _, it in ipairs(strokes:get()) do
    if moved[it.id] then
      local copy = {}
      for k, v in pairs(it) do copy[k] = v end
      copy.x, copy.y = (it.x or 0) + dx, (it.y or 0) + dy
      if it.points then
        copy.points = {}
        for i = 1, #it.points - 1, 2 do copy.points[i], copy.points[i + 1] = it.points[i] + dx, it.points[i + 1] + dy end
      end
      list[#list + 1] = copy
    else
      list[#list + 1] = it
    end
  end
  strokes:set(list)
  note(("Moved %d on the board"):format(#ids))
end

-- --------------------------------------------------------------- files --

local FILES = {
  { key = "graph", label = "shading.graph", icon = "account_tree" },
  { key = "board", label = "ideas.board", icon = "draw" },
  { key = "map", label = "district.map", icon = "map" },
  { key = "assets", label = "assets", icon = "folder", children = {
    { key = "noise", label = "noise.png", icon = "image" },
    { key = "brush", label = "round.brush", icon = "brush" } } },
  { key = "readme", label = "README.md", icon = "article" },
}

-- ------------------------------------------------------------- window --

local views = {}
note("Opened the project")

app.application {
  title = "Editor", app_id = "dev.morf.Editor",
  width = 1280, height = 820, minimum_width = 720, minimum_height = 480,
  build = function(win)
    local function W() return win.width end
    local function H() return win.height end
    local dock_node, dock
    -- Each panel's content fills the body its stack gives it.
    local function fill(make)
      return function()
        local holder = ui.Item { anchors = { fill = true } }
        local node = make(holder)
        if node then ui.reparent(node, holder) end
        return holder
      end
    end
    -- A list that fills its panel: made once, its size bound to the
    -- panel's, so a tree keeps its open folders as the dock is resized.
    local function sized(holder, make)
      local built = make(function() return holder.layout_width or 0 end,
        function() return holder.layout_height or 0 end)
      if built then ui.reparent(built, holder) end
    end

    local panels = {
      files = { title = "Files", icon = "folder", closable = false, content = fill(function(holder)
        sized(holder, function(w, h)
          local node, tree
          node, tree = widgets.tree_view { id = "editor-files", width = w, height = h, row_height = 32, tree = FILES,
            accessible_name = "Files",
            on_activated = function(i)
              local row = tree and tree.model and tree.model:get(i)
              local key = row and row.key
              if key == "graph" or key == "board" or key == "map" then
                dock.activate(key)
                note("Opened " .. row.label)
              elseif row then
                note("Selected " .. row.label)
              end
            end }
          views.tree = tree
          return node
        end)
      end) },
      graph = { title = "shading.graph", icon = "account_tree", closable = false, content = fill(function()
        local node, view = widgets.node_graph { id = "editor-graph", anchors = { fill = true },
          items = function() return graph.items:get() end, ports = graph.ports, wires = function() return graph.wires:get() end,
          tool = function() return tool_for("graph") end,
          on_moved = function(ids, dx, dy) graph.on_moved(ids, dx, dy) note(("Moved %s"):format(table.concat(ids, ", "))) end,
          on_connected = function(from, to) graph.on_connected(from, to) note(("Wired %s to %s"):format(from, to)) end,
          on_deleted = function(ids) graph.on_deleted(ids) note(("Deleted %s"):format(table.concat(ids, ", "))) end }
        views.graph = view
        -- The whole graph in sight once the canvas has its size.
        local fitted = false
        morf.effect("editor.graph.fit", function()
          if not fitted and (view.t.viewport_width or 0) > 0 then fitted = true view.fit() end
        end, { owner = node })
        return node
      end) },
      board = { title = "ideas.board", icon = "draw", closable = false, content = fill(function()
        local node, view = widgets.whiteboard { id = "editor-board", anchors = { fill = true },
          items = function() return strokes:get() end, tool = function() return tool_for("board") end,
          on_drawn = draw, on_moved = move_strokes,
          on_resized = function(id, x, y, w, h) resize_stroke(id, x, y, w, h) note(("Resized %s"):format(id)) end }
        views.board = view
        return node
      end) },
      map = { title = "Map", icon = "map", content = fill(function()
        local node, view = widgets.map_view { id = "editor-map", anchors = { fill = true }, items = samples.district(),
          zoom = 0.45, view_x = 40, view_y = 40, attribution = "© Sample Atlas" }
        views.map = view
        return node
      end) },
      log = { title = "Log", icon = "terminal", content = fill(function(holder)
        sized(holder, function(w, h)
          return (widgets.list { id = "editor-log", rows = log, width = w, height = h, row_height = 28,
            accessible_name = "Log" })
        end)
      end) },
    }

    dock_node, dock = widgets.dock_area { id = "editor-dock", y = HEADER + TOOLS,
      width = W, height = function() return H() - HEADER - TOOLS end,
      panels = panels, layout = layout:get() or DEFAULT_LAYOUT, floating = floating:get(),
      -- The layout is the application's: kept as the dock reports it.
      on_layout_changed = function(tree, floats)
        layout:set(tree)
        floating:set(floats or {})
        save()
      end,
      on_activated = function(panel)
        if panel == "graph" or panel == "board" then state.document = panel end
      end,
      on_closed = function(panel) note("Closed " .. tostring((panels[panel] or {}).title or panel)) end }
    views.dock = dock

    -- The toolbar: the tools, one on at a time, and a fit button.
    local items = {}
    for _, tool in ipairs(TOOLS_LIST) do
      items[#items + 1] = { id = "editor-tool-" .. tool.id, icon = tool.icon, tooltip = tool.label,
        checked = function() return state.tool == tool.id end,
        on_toggled = function(on)
          if on then state.tool = tool.id note("Tool: " .. tool.label) end
        end }
      if tool.id == "connect" then items[#items + 1] = { separator = true } end
    end
    items[#items + 1] = { separator = true }
    items[#items + 1] = { id = "editor-fit", icon = "fit_screen", tooltip = "Fit the document",
      on_clicked = function()
        local view = views[state.document]
        if view then view.fit() end
      end }
    local toolbar = composites.toolbar { id = "editor-tools", y = HEADER, width = W, height = TOOLS, items = items,
      accessible_name = "Tools" }

    local header = composites.header_bar { id = "editor-header", width = W, window = win, title = "Editor",
      subtitle = function()
        local panel = panels[state.document]
        return panel and panel.title or ""
      end }

    -- For tests and scripting.
    morf.ipc["editor-resize"] = function(w, h) win:size(tonumber(w), tonumber(h)) return true end
    return ui.Item { width = W, height = H, dock_node, toolbar, header }
  end,
}

-- Exposed for tests and scripting: the layout as text, the tool, the
-- graph's nodes, the board's items, the log.
local function show(n)
  if not n then return "" end
  if n.kind == "stack" or n.panels then return "[" .. table.concat(n.panels or {}, " ") .. "]" end
  local parts = {}
  for _, c in ipairs(n.children or {}) do parts[#parts + 1] = show(c) end
  return (n.orientation == "vertical" and "V(" or "H(") .. table.concat(parts, ",") .. ")"
end
morf.ipc["editor-layout"] = function() return show(layout:get() or DEFAULT_LAYOUT) end
morf.ipc["editor-saved"] = function() return layout:get() ~= nil end
morf.ipc["editor-state"] = function()
  return { tool = state.tool, document = state.document, status = state.status, log = log:len(),
    strokes = #strokes:get(), board_tool = views.board and views.board.t.tool or "",
    graph_tool = views.graph and views.graph.t.tool or "" }
end
morf.ipc["editor-node"] = function(id)
  for _, n in ipairs(graph.items:get()) do if n.id == id then return ("%g,%g"):format(n.x, n.y) end end
  return ""
end
morf.ipc["editor-tool"] = function(name) state.tool = name return true end
morf.ipc["editor-activate"] = function(panel) views.dock.activate(panel) return true end
-- Where a graph node's header shows, in the canvas's own pixels.
morf.ipc["editor-node-screen"] = function(id)
  for _, n in ipairs(graph.items:get()) do
    if n.id == id then
      local x, y = views.graph.to_screen(n.x + 40, n.y + 15)
      return { x = x, y = y, zoom = views.graph.t.zoom }
    end
  end
end
