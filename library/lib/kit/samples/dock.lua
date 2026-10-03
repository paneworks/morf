-- Gallery samples for the Dock archetype's widgets: each a dock as it is
-- used -- an editor's area of file tree, documents and console, a
-- workshop's shelves round a viewport, a settings box of tabs, a row of
-- open documents, an IDE's tool windows. Panel contents are plain kit
-- text: the dock is what is shown.
local ui = require("morf.ui")

local S = { span = {} }

local function mono(kit) return kit.theme and kit.theme.mono or nil end

-- A column of lines, `{ text, dim }` each, `size` px apart.
local function lines(kit, list, opts)
  opts = opts or {}
  local col = ui.Column { anchors = { fill = true, margins = 12 }, gap = opts.gap or 6 }
  for _, line in ipairs(list) do
    local text, dim, icon = line, false, nil
    if type(line) == "table" then text, dim, icon = line[1], line.dim, line.icon end
    local row = ui.Row { gap = 8, align = "center", height = opts.row or 20 }
    if icon then ui.reparent(kit.icon(icon, 16, dim and kit.ink("lo") or nil), row) end
    ui.reparent(kit.text { text = text, font_size = opts.size or 14, font_family = opts.mono and mono(kit) or nil,
      color = dim and kit.ink("lo") or nil, height = opts.row or 20, vertical_alignment = "center" }, row)
    ui.reparent(row, col)
  end
  return ui.Item { anchors = { fill = true }, clip = true, col }
end

local function files(kit)
  return lines(kit, {
    { "src", icon = "folder_open" }, { "  canvas.lua", icon = "description" }, { "  dock.lua", icon = "description" },
    { "  widgets.lua", icon = "description" }, { "tests", icon = "folder" }, { "README.md", icon = "article" },
  })
end
local function code(kit)
  return lines(kit, {
    { "local dock = require(\"lib.kit.dock\")", dim = false },
    { "", dim = true },
    { "-- Panels in splits and tab stacks.", dim = true },
    "function M.make(widget, spec)",
    "  local tree = layout(spec)",
    "  return root, dock",
    "end",
  }, { mono = true, size = 13, gap = 4 })
end
local function console(kit)
  return lines(kit, {
    { "$ morf test library/tests", dim = true }, "ok 1 - a tab drags onto an edge", "ok 2 - a wire is pulled",
    { "# 2 passed, 0 failed", dim = true },
  }, { mono = true, size = 13, gap = 2 })
end
local function outline(kit)
  return lines(kit, { { "M.make", icon = "function" }, { "tab_node", icon = "function" },
    { "stack_node", icon = "function" }, { "split_node", icon = "function" } })
end

function S.dock_area(kit, w)
  return w.dock_area { id = "sample-dock-area", width = 600, height = 480,
    panels = {
      files = { title = "Files", icon = "folder", content = files(kit) },
      outline = { title = "Outline", icon = "list", content = outline(kit) },
      editor = { title = "dock.lua", icon = "code", content = code(kit), closable = false },
      readme = { title = "README.md", icon = "article", content = lines(kit, { "# morf", { "A UI engine.", dim = true } }) },
      console = { title = "Console", icon = "terminal", content = console(kit) },
    },
    layout = { orientation = "horizontal", ratios = { 0.3, 0.7 }, children = {
      { id = "side", panels = { "files", "outline" } },
      { orientation = "vertical", ratios = { 0.66, 0.34 }, children = {
        { id = "docs", panels = { "editor", "readme" } },
        { id = "bottom", panels = { "console" } } } } } } }
end
S.span.dock_area = { 2, 2 }

function S.shelf_dock(kit, w)
  return w.shelf_dock { id = "sample-shelf-dock", width = 600, height = 480,
    panels = {
      layers = { title = "Layers", icon = "layers", content = lines(kit, {
        { "Background", icon = "visibility" }, { "Sketch", icon = "visibility" }, { "Ink", icon = "visibility_off", dim = true } }) },
      assets = { title = "Assets", icon = "photo_library", content = lines(kit, { { "brushes/", icon = "folder" } }) },
      viewport = { title = "Viewport", icon = "crop_free", closable = false, content = ui.Item { anchors = { fill = true } } },
      props = { title = "Properties", icon = "tune", content = lines(kit, {
        { "Opacity  100%" }, { "Blend  Normal" }, { "Locked  No", dim = true } }) },
    },
    layout = { orientation = "horizontal", ratios = { 0.26, 0.48, 0.26 }, children = {
      { id = "left", panels = { "layers", "assets" } },
      { id = "centre", panels = { "viewport" } },
      { id = "right", panels = { "props" } } } } }
end
S.span.shelf_dock = { 2, 2 }

function S.tabbed_container(kit, w)
  return w.tabbed_container { id = "sample-tabbed-container", width = 600, height = 480,
    panels = {
      general = { title = "General", closable = false, content = lines(kit, { "Name  Untitled", "Size  1920 × 1080" }) },
      export = { title = "Export", closable = false, content = lines(kit, { "Format  PNG", "Scale  2×" }) },
      about = { title = "About", closable = false, content = lines(kit, { { "Created today", dim = true } }) },
    },
    layout = { id = "tabs", panels = { "general", "export", "about" }, current = "general" } }
end
S.span.tabbed_container = { 2, 2 }

function S.document_tabs(kit, w)
  return w.document_tabs { id = "sample-document-tabs", width = 600, height = 480,
    panels = {
      main = { title = "main.lua", icon = "code", content = code(kit) },
      canvas = { title = "canvas.lua", icon = "code", content = code(kit) },
      readme = { title = "README.md", icon = "article", content = lines(kit, { "# morf" }) },
      notes = { title = "notes.txt", icon = "description", content = lines(kit, { "todo: dock skins" }) },
    },
    layout = { id = "docs", panels = { "main", "canvas", "readme", "notes" }, current = "canvas" } }
end
S.span.document_tabs = { 2, 2 }

function S.tool_windows(kit, w)
  return w.tool_windows { id = "sample-tool-windows", width = 600, height = 480,
    panels = {
      project = { title = "Project", icon = "account_tree", content = files(kit) },
      editor = { title = "app.lua", icon = "code", closable = false, content = code(kit) },
      terminal = { title = "Terminal", icon = "terminal", content = console(kit) },
      problems = { title = "Problems", icon = "error", content = lines(kit, { { "No problems", dim = true } }) },
      output = { title = "Output", icon = "output", content = lines(kit, { { "Build finished", dim = true } }) },
    },
    layout = { orientation = "vertical", ratios = { 0.62, 0.38 }, children = {
      { orientation = "horizontal", ratios = { 0.32, 0.68 }, children = {
        { id = "left", panels = { "project" } }, { id = "main", panels = { "editor" } } } },
      { id = "bottom", panels = { "terminal", "problems", "output" } } } } }
end
S.span.tool_windows = { 2, 2 }

return S
