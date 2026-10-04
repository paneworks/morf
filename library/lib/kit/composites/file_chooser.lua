-- A file chooser (composite: Shell + Collection of files + Navigation of
-- folders + TextField path and name).
--
--     local node, chooser = composites.file_chooser {
--       id = "open", width = 640, height = 420,       -- numbers or bindings
--       mode = "open",                                -- "save", "folder"; or a binding
--       root = "/",                                   -- nothing above it is shown
--       path = "~/Pictures",                          -- where it starts (root otherwise)
--       name = "capture.png",                         -- save: the name it suggests
--       filters = { { name = "Images", patterns = { "png", "jpg", "webp" } }, { name = "All files" } },
--       places = { { label = "Home", path = "~", icon = "home" }, ... },   -- the sidebar's
--       on_accepted = function(path, info) end,       -- info: { folder, name, is_dir }
--       on_cancelled = function() end,
--     }
--     chooser.navigate("/tmp") ; chooser.path() ; chooser.selected() ; chooser.accept()
--
-- The window is a kit Shell (`window_layout`): the places in its sidebar,
-- which narrower than `collapse_below` becomes a drawer over the files
-- (F9, Ctrl+B or the header's press open it; Escape or a press outside
-- shuts it; F6 moves between the places and the files). Its header holds
-- Back, Up, the breadcrumbs (a kit `breadcrumbs` Selection: a press goes
-- to that folder) and the path (a kit `entry`: Return goes there). Each
-- folder is a page of a kit `navigation_view` (a Navigation: going into
-- a folder pushes it, Back and Alt+Left pop, a breadcrumb or a place goes
-- straight there), and lists its entries in a kit `file_list` (a
-- Collection: the arrows walk it, typing jumps, a press chooses, Return or
-- a double press opens a folder or accepts a file; BackSpace goes up).
-- Under the files: the name to save as (save), the filter (a combo box)
-- and Cancel and the accepting press. Folders list first, then files the
-- filter lets through (none in folder mode). The filesystem is read with
-- `morf.fs.list`; a folder that cannot be read says why.
-- Ids: `<id>-sidebar-toggle`, `<id>-back`, `<id>-up`, `<id>-crumbs`,
-- `<id>-crumb-<i>`, `<id>-path`, `<id>-places`, `<id>-place-<i>`,
-- `<id>-folders`, `<id>-files` (each folder's list), `<id>-entry-<name>`,
-- `<id>-name`, `<id>-filter`, `<id>-status`, `<id>-cancel`, `<id>-accept`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local shell = require("lib.kit.shell")

local function get(v) if type(v) == "function" then return v() end return v end

local M = {}

--- `path` made absolute and tidy: `~` is the home folder, no trailing slash.
function M.expand(path)
  path = tostring(path or "")
  local home = morf.env("HOME") or "/"
  if path == "~" then path = home elseif path:sub(1, 2) == "~/" then path = home .. path:sub(2) end
  path = path:gsub("/+", "/")
  if #path > 1 then path = path:gsub("/$", "") end
  return path
end
local function parent(path)
  if path == "/" then return "/" end
  return path:match("^(.*)/[^/]+$") or "/"
end
local function basename(path) return path == "/" and "/" or (path:match("([^/]+)$") or path) end
local function within(path, root)
  if root == "/" then return path:sub(1, 1) == "/" end
  return path == root or path:sub(1, #root + 1) == root .. "/"
end
local function human(bytes)
  bytes = tonumber(bytes) or 0
  if bytes < 1024 then return ("%d B"):format(bytes) end
  local units = { "KB", "MB", "GB", "TB" }
  local v, u = bytes / 1024, 1
  while v >= 1024 and u < #units do v, u = v / 1024, u + 1 end
  return ("%.1f %s"):format(v, units[u])
end
local IMAGES = { png = true, jpg = true, jpeg = true, webp = true, gif = true, svg = true, bmp = true, avif = true }
local function icon_for(row)
  if row.is_dir then return "folder" end
  if IMAGES[row.extension or ""] then return "image" end
  return "description"
end
M.human = human

--- The entries of a folder, folders first: `{ name, path, is_dir, size,
--- extension }`; `nil, why` when it cannot be read. `filter`: a set of
--- extensions files must have (nil: any); `folders_only`.
function M.list(path, options)
  options = options or {}
  local ok, entries, why = pcall(morf.fs.list, path, { follow = true, hidden = options.hidden == true })
  if not ok then return nil, tostring(entries) end
  if not entries then return nil, why or "Cannot read folder" end
  local folders, files = {}, {}
  for _, e in ipairs(entries) do
    local row = { key = e.path, name = e.name, path = e.path, is_dir = e.is_dir == true, size = e.size or 0,
      extension = (e.extension or ""):lower() }
    if row.is_dir then folders[#folders + 1] = row
    elseif not options.folders_only and (not options.filter or options.filter[row.extension]) then
      files[#files + 1] = row
    end
  end
  local function by_name(a, b) return a.name:lower() < b.name:lower() end
  table.sort(folders, by_name)
  table.sort(files, by_name)
  for _, f in ipairs(files) do folders[#folders + 1] = f end
  return folders
end

local function patterns_of(filter)
  if not filter or not filter.patterns or #filter.patterns == 0 then return nil end
  local set = {}
  for _, p in ipairs(filter.patterns) do set[(tostring(p):gsub("^%*?%.?", "")):lower()] = true end
  return set
end

function M.make(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local Wf, Hf = spec.width or 640, spec.height or 420
  local W0, H0 = get(Wf), get(Hf)
  local SB = spec.sidebar_width or 180
  local HEAD = 88
  local FOOT = 96
  local ROW = spec.row_height or 36
  local root_path = M.expand(spec.root or "/")
  local home = M.expand("~")
  local function mode() return get(spec.mode) or "open" end
  local filters = spec.filters or {}
  local st = morf.state { path = "", selected = "", selected_dir = false, name = get(spec.name) or "",
    filter = 1, status = "", revision = 0, ready = false }

  -- Places: the configuration's, or home and its usual folders and the root.
  local places = {}
  if spec.places then
    for _, p in ipairs(spec.places) do
      places[#places + 1] = { label = p.label or basename(M.expand(p.path)), path = M.expand(p.path), icon = p.icon or "folder" }
    end
  else
    if within(home, root_path) then
      places[#places + 1] = { label = "Home", path = home, icon = "home" }
      for _, sub in ipairs { { "Desktop", "desktop_windows" }, { "Documents", "description" },
        { "Downloads", "download" }, { "Pictures", "image" }, { "Music", "music_note" }, { "Videos", "movie" } } do
        local p = home .. "/" .. sub[1]
        if morf.fs.is_dir(p) then places[#places + 1] = { label = sub[1], path = p, icon = sub[2] } end
      end
    end
    places[#places + 1] = { label = root_path == "/" and "Computer" or basename(root_path), path = root_path,
      icon = root_path == "/" and "hard_drive" or "folder" }
  end

  -- Each folder's rows, refreshed every visit.
  local models = {}
  local function model_of(path)
    if not models[path] then models[path] = morf.list_model({}) end
    return models[path]
  end
  local generation = 0
  local function refresh(path)
    local rows, why = M.list(path, { hidden = spec.show_hidden, folders_only = mode() == "folder",
      filter = mode() ~= "folder" and patterns_of(filters[st.filter]) or nil })
    if not rows then
      st.status = tostring(why)
      model_of(path):replace({}, "key")
      return false
    end
    st.status = ""
    -- A fresh `slot` per listing: a row the list only shifted (one before
    -- it filtered out) would keep the index it was bound with.
    generation = generation + 1
    for _, row in ipairs(rows) do row.slot = row.key .. "#" .. generation end
    model_of(path):replace(rows, "slot")
    st.revision = st.revision + 1
    return true
  end

  -- Where it opens: before anything that lists the path is made (a kit
  -- Selection takes its entries as they are when it is made).
  local start = M.expand(get(spec.path) or root_path)
  if not within(start, root_path) or not morf.fs.is_dir(start) then
    if morf.fs.is_dir(parent(start)) and within(parent(start), root_path) then start = parent(start) else start = root_path end
  end
  st.path = start
  refresh(start)
  local nav, nav_node, lists = nil, nil, {}
  local app
  local function cw()
    local w = get(Wf) or W0
    if not st.ready or not app then return w end
    return math.max(120, w - ((app.t.collapsed or not app.t.sidebar_open) and 0 or SB))
  end
  local function accept_entry(row)
    if not row then return false end
    if row.is_dir then return false end
    if spec.on_accepted then spec.on_accepted(row.path, { folder = parent(row.path), name = row.name, is_dir = false }) end
    return true
  end

  local handle = {}
  local function navigate(path)
    path = M.expand(path)
    if not within(path, root_path) then st.status = "Outside " .. root_path return false end
    if not morf.fs.is_dir(path) then
      -- A file: go to its folder and choose it.
      if morf.fs.exists(path) and within(parent(path), root_path) then
        navigate(parent(path))
        st.selected, st.selected_dir = path, false
        if mode() == "save" then st.name = basename(path) end
        return true
      end
      st.status = "No such folder: " .. path
      return false
    end
    if path == st.path then refresh(path) return true end
    model_of(path)
    if not nav then st.path = path refresh(path) return true end
    local current = st.path
    if current ~= "" and parent(path) == current and path ~= current then nav.push(path) else nav.go(path) end
    return true
  end
  handle.navigate = navigate
  local function up() if st.path ~= root_path then navigate(parent(st.path)) end return true end

  local function open_row(row)
    if not row then return end
    if row.is_dir then navigate(row.path) return end
    if mode() == "save" then st.name = row.name handle.accept() return end
    if mode() == "open" then accept_entry(row) end
  end

  -- One folder's page: its list.
  local serial = 0
  local function page(path)
    serial = serial + 1
    local model = model_of(path)
    local LW = W0
    local list
    list = widgets.file_list { id = sid("files"), accessible_name = basename(path), width = LW,
      height = H0 - HEAD - FOOT, rows = model, row_height = ROW,
      current = function()
        local _ = st.revision
        for i = 1, model:len() do if model:get(i).path == st.selected then return i end end
        return 0
      end,
      on_current_changed = function(i)
        local row = i >= 1 and i <= model:len() and model:get(i) or nil
        if not row then return end
        st.selected, st.selected_dir = row.path, row.is_dir
        if mode() == "save" and not row.is_dir then st.name = row.name end
      end,
      on_activated = function(i) if i >= 1 and i <= model:len() then open_row(model:get(i)) end end,
      delegate = function(row, s)
        local function now() return s.row() or row end
        local look
        look = ui.Item { id = sid("entry-" .. row.name), width = LW, height = ROW,
          kit.surface { x = 4, y = 1, height = ROW - 2, radius = kit.round(8),
            width = function() return cw() - 8 end,
            color = function()
              local c = kit.signal("accent")()
              if s.current() then return c:alpha(0.16) end
              return c:alpha(s.hovered() and 0.06 or 0)
            end },
          kit.icon(function() return icon_for(now()) end, 20,
            function() return (now().is_dir and kit.ink("accent") or kit.ink("lo"))() end,
            { x = 14, anchors = { vertical_center = true } }),
          kit.text { x = 44, anchors = { vertical_center = true }, elide = "middle",
            width = function() return cw() - 44 - 96 end, text = function() return now().name end },
          kit.label { anchors = { vertical_center = true }, width = 80, horizontal_alignment = "right",
            x = function() return cw() - 92 end,
            text = function() local r = now() return r.is_dir and "" or human(r.size) end } }
        return look, function(next_row) if id then look.id = id .. "-entry-" .. next_row.name end end
      end }
    lists[path] = list
    return ui.Item { width = LW, height = H0 - HEAD - FOOT, list,
      kit.subtitle { x = 20, y = 24, visible = function() return model:len() == 0 end,
        text = function() return st.status ~= "" and st.status or "This folder is empty" end } }
  end
  local pages = setmetatable({}, { __index = function(_, path) return function() return page(path) end end })

  -- The header: back, up, breadcrumbs; the path under them.
  local function crumbs()
    local _ = st.path
    local out, p = {}, st.path
    while p ~= "" do
      table.insert(out, 1, { label = (p == root_path and root_path == "/") and "/" or basename(p), path = p })
      if p == root_path or p == "/" then break end
      p = parent(p)
    end
    -- The last four, the rest behind an ellipsis.
    while #out > 4 do table.remove(out, 1) end
    return out
  end
  local function crumb_width(label) return math.min(180, (utf8.len(label) or #label) * 9 + 28) end
  local crumb_bar = widgets.breadcrumbs { id = sid("crumbs"), accessible_name = "Path",
    items = crumbs, item_height = 32, gap = 2, height = 32,
    width = function()
      local w = 0
      for _, c in ipairs(crumbs()) do w = w + crumb_width(tostring(c.label)) + 2 end
      return math.max(1, w)
    end,
    current = function() return #crumbs() end,
    item_id = function(i) return sid("crumb-" .. i) end,
    on_current_changed = function(i) local c = crumbs()[i] if c then navigate(c.path) end end,
    delegate = function(i, item, s)
      local label = tostring(item.label)
      local width = crumb_width(label)
      if s.area then s.area.width = width end
      return ui.Item { width = width, height = 32,
        kit.surface { anchors = { fill = true, margins = 2 }, radius = kit.round(8),
          color = function() return kit.signal("accent")():alpha(s.hovered() and 0.08 or 0) end },
        kit.text { anchors = { center_in = true }, width = width - 16, elide = "middle", text = label,
          horizontal_alignment = "center", font_weight = 500,
          color = function() return (s.current() and kit.ink("hi") or kit.ink("lo"))() end } }
    end }
  local function header_width() return get(Wf) or W0 end
  -- The fields set their text in the theme's face, on the theme's field.
  local probe = { text = "" }
  ui.destroy(kit.text(probe), true)
  local typing = morf.state { path = false, name = false }
  local function entry(spec_)
    spec_.font_family, spec_.font_source, spec_.font_size = probe.font_family, probe.font_source, probe.font_size
    spec_.color, spec_.placeholder_color = kit.ink("hi"), kit.ink("lo")
    spec_.caret_color = kit.signal("accent")
    spec_.selection_color = function() return kit.signal("accent")():alpha(0.3) end
    spec_.vertical_alignment = "center"
    return widgets.entry(spec_)
  end
  local path_node, path_input = entry { id = sid("path"), accessible_name = "Location",
    x = 10, width = function() return header_width() - 40 end, height = 36, inset = { 4, 0, 4, 0 },
    text = function() return st.path end,
    on_focus_changed = function(on) typing.path = on end,
    on_accepted = function(text) navigate(text) end,
    on_escape = function() if spec.on_cancelled then spec.on_cancelled() end end }
  local function tool(suffix, icon, name, enabled, action, x)
    return widgets.icon { id = sid(suffix), accessible_name = name, width = 36, height = 36, size = 20,
      icon_off = icon, x = x, y = 4, enabled = enabled, opacity = function() return enabled() and 1 or 0.35 end,
      on_clicked = function() if enabled() then action() end end }
  end
  local header = ui.Item { width = header_width, height = HEAD,
    tool("sidebar-toggle", "side_navigation", "Places", function() return true end,
      function() if app then app.toggle_sidebar() end end, 6),
    tool("back", "arrow_back", "Back", function() local _ = st.path return st.ready and nav ~= nil and nav.t.can_go_back end,
      function() nav.pop() end, 44),
    tool("up", "arrow_upward", "Up", function() return st.path ~= root_path end, up, 82),
    ui.Item { x = 124, y = 6, height = 32, clip = true, width = function() return header_width() - 130 end, crumb_bar },
    kit.field { x = 10, y = 46, width = function() return header_width() - 20 end, height = 36,
      focused = function() return typing.path end, path_node } }

  -- The places.
  local place_list = widgets.sidebar_list { id = sid("places"), accessible_name = "Places", x = 6, y = 8,
    items = places, item_width = SB - 12, item_height = 36, gap = 2, orientation = "vertical",
    item_id = function(i) return sid("place-" .. i) end,
    current = function()
      for i, p in ipairs(places) do if p.path == st.path then return i end end
      return 0
    end,
    on_current_changed = function(i) if places[i] then navigate(places[i].path) end end,
    delegate = function(_, p, s)
      return ui.Item { anchors = { fill = true },
        kit.icon(p.icon, 20, function() return (s.current() and kit.ink("accent") or kit.ink("lo"))() end,
          { x = 10, anchors = { vertical_center = true } }),
        kit.text { x = 40, anchors = { vertical_center = true }, width = SB - 64, elide = "right", text = p.label,
          color = function() return (s.current() and kit.ink("hi") or kit.ink("lo"))() end } }
    end }
  local sidebar = ui.Item { width = SB, height = function() return (get(Hf) or H0) - HEAD end,
    kit.surface { anchors = { fill = true }, color = function() return kit.ink("hi")():alpha(0.04) end },
    place_list }
  -- A drawer over the files stands on the theme's opaque panel.
  local drawer = kit.panel { anchors = { fill = true }, fill = 1, radius = 0 }
  drawer.visible = function() return st.ready and app ~= nil and app.t.collapsed end
  ui.reparent(drawer, sidebar)
  drawer.z = -1

  -- The folders.
  nav_node, nav = widgets.navigation_view { id = sid("folders"), accessible_name = "Folders", width = W0,
    height = H0 - HEAD - FOOT, current = start, pages = pages,
    on_current_changed = function(path)
      st.path = path
      st.selected, st.selected_dir = "", false
      refresh(path)
    end }
  local folders = ui.Item { width = W0, height = H0 - HEAD - FOOT, clip = true, nav_node,
    shortcuts = { BackSpace = up, ["alt+Up"] = up } }

  -- Under the files.
  local function footer_width() return cw() end
  local BUTTONS = 84 + 8 + 92
  local function filtering() return #filters > 0 and mode() ~= "folder" end
  -- The first line: the name (save) or what is chosen; the second the
  -- filter, as wide as the buttons beside it leave room for, and them.
  local function line_width() return footer_width() - 20 end
  local name_input = entry { id = sid("name"), accessible_name = "Name", x = 10, height = 36,
    width = function() return line_width() - 20 end, inset = { 4, 0, 4, 0 },
    placeholder = "File name", text = function() return st.name end,
    on_focus_changed = function(on) typing.name = on end,
    on_edited = function(text) st.name = text end,
    on_accepted = function(text) st.name = text handle.accept() end,
    on_escape = function() if spec.on_cancelled then spec.on_cancelled() end end }
  local filter_names = {}
  for i, f in ipairs(filters) do filter_names[i] = f.name or ("Filter " .. i) end
  local filter_node
  -- (Sized once, as it opens: a combo box lays its field out for the width
  -- it is made with.)
  local filter_w0 = math.max(100, math.min(170,
    W0 - ((W0 < (spec.collapse_below or 520)) and 0 or SB) - 20 - BUTTONS - 8))
  if #filters > 0 then
    filter_node = require("lib.kit.composites").combo_box { id = sid("filter"), accessible_name = "Show",
      x = 10, y = 52, width = filter_w0, height = 36, items = filter_names,
      icon = filter_w0 >= 150 and "filter_list" or nil,
      current = function() return st.filter end, placement = "top-start",
      on_changed = function(i)
        st.filter = i
        refresh(st.path)
      end }
    filter_node.visible = filtering
  end
  local function accept_label()
    if spec.accept_label then return get(spec.accept_label) end
    local m = mode()
    return m == "save" and "Save" or (m == "folder" and "Choose" or "Open")
  end
  local name_node = kit.field { x = 10, y = 8, width = line_width, height = 36,
    focused = function() return typing.name end, name_input }
  -- (Set on the node: a theme's field need not take `visible`.)
  name_node.visible = function() return mode() == "save" end
  local footer = ui.Item { y = H0 - HEAD - FOOT, width = footer_width, height = FOOT, name_node }
  if filter_node then ui.reparent(filter_node, footer) end
  for _, part in ipairs {
    -- What is chosen, or what went wrong: on the first line, or (save,
    -- the name there) beside the buttons.
    kit.label { id = sid("status"), x = 10, elide = "right",
      y = function() return mode() == "save" and 62 or 18 end,
      visible = function() return mode() ~= "save" or not filtering() end,
      width = function() return mode() == "save" and footer_width() - 30 - BUTTONS or line_width() end,
      color = function() return (st.status ~= "" and kit.signal("alert") or kit.ink("lo"))() end,
      text = function()
        if st.status ~= "" then return st.status end
        if mode() == "save" then return "" end
        if mode() == "folder" then return basename(st.path) end
        if st.selected ~= "" then return basename(st.selected) end
        return ""
      end },
    ui.Row { anchors = { right = true, right_margin = 10 }, y = 52, gap = 8,
      widgets.flat { id = sid("cancel"), label = "Cancel", width = 84, height = 36,
        on_clicked = function() if spec.on_cancelled then spec.on_cancelled() end end },
      widgets.suggested { id = sid("accept"), label = accept_label, width = 92, height = 36,
        on_clicked = function() handle.accept() end } } } do ui.reparent(part, footer) end
  local content = ui.Item { width = function() return cw() end, height = function() return (get(Hf) or H0) - HEAD end,
    folders, footer }

  local node
  node, app = shell.make("window_layout", { id = sid("shell"), width = Wf, height = Hf,
    header_bar = header, header_height = HEAD, sidebar = sidebar, content = content, sidebar_width = SB,
    breakpoints = { spec.collapse_below or 520 }, collapse_below = spec.collapse_below or 520,
    title = spec.title or "Files", sidebar_name = "Places", content_name = "Files" })
  local root = ui.Item { id = id, width = Wf, height = Hf,
    shortcuts = { Escape = function() if not spec.on_cancelled then return false end spec.on_cancelled() end },
    node }
  st.ready = true
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  if type(spec.name) == "function" then
    morf.effect("kit.file_chooser.name." .. tostring(root), function() st.name = spec.name() or "" end, { owner = root })
  end

  function handle.accept()
    local m = mode()
    if m == "save" then
      local name = st.name
      if name == "" then st.status = "Enter a file name" return false end
      if spec.on_accepted then
        spec.on_accepted(st.path == "/" and ("/" .. name) or (st.path .. "/" .. name), { folder = st.path, name = name })
      end
      return true
    elseif m == "folder" then
      if spec.on_accepted then spec.on_accepted(st.path, { folder = st.path, is_dir = true }) end
      return true
    end
    if st.selected == "" then st.status = "Choose a file" return false end
    if st.selected_dir then navigate(st.selected) return false end
    return accept_entry({ path = st.selected, name = basename(st.selected), is_dir = false })
  end
  function handle.cancel() if spec.on_cancelled then spec.on_cancelled() end end
  function handle.path() return st.path end
  function handle.selected() return st.selected end
  function handle.name() return st.name end
  function handle.set_name(name) st.name = tostring(name or "") end
  function handle.status() return st.status end
  function handle.set_filter(i) if filters[i] then st.filter = i refresh(st.path) end end
  function handle.entries()
    local _ = st.revision
    local out, model = {}, model_of(st.path)
    for i = 1, model:len() do out[i] = model:get(i).name end
    return out
  end
  function handle.focus()
    local list = lists[st.path]
    if list then morf.focus.set(list, true) end
  end
  function handle.toggle_sidebar() app.toggle_sidebar() end
  handle.shell, handle.node, handle.nav = app, root, nav
  handle.path_input = path_input
  return root, handle
end

return M
