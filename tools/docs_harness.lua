-- The configuration a documentation fragment runs in (tools/docs_check.py).
--
-- `morf test` loads this through `test.source` with one fragment appended,
-- as the demo specs load theirs. Everything that reaches outside the run is
-- replaced first, on the real `morf` table, so a fragment, a library module
-- it requires and every alias (`morf.io.run`, `morf.core.*`) see the stubs.
-- The fragment then runs in a fresh environment over the fixtures below and
-- reports through the `docs.result` IPC verb.

local morf = require("morf")
local ui = require("morf.ui")
morf.surface.namespace = "docs-check"
morf.surface.width = 800
morf.surface.height = 600

local harness = { blocked = {} }

-- What a fragment may name but not do: processes, writes, buses, network,
-- and the session around the run.
local BLOCKED = {
  process = { "run", "spawn", "exec_detached", "process", "process_view", "kill",
              "terminal.listen" },
  write = { "fs.write", "fs.append", "fs.copy", "fs.mkdir", "fs.remove", "fs.rename",
            "fs.symlink", "file", "file_view", "json.write_file", "image.encode_png" },
  network = { "connect", "socket", "socket_server", "request_socket", "broadcast",
              "http.get", "http.post", "http.query", "http.request", "dbus", "greetd",
              "pam", "status_notifier", "idle.inhibit" },
  session = { "quit", "reload", "audio", "clipboard.set", "gamma", "output_power",
              "virtual_keyboard", "screencopy", "lock_surface", "retain_lock", "toplevel",
              "workspace", "input_method", "text_input", "shortcuts.inhibit", "drag.start" },
}

local function record(path, kind)
  harness.blocked[#harness.blocked + 1] = kind .. ": morf." .. path
end

-- A handle whose every method does nothing, for code that keeps what it got;
-- read as a list it is empty.
local inert = setmetatable({}, {
  __index = function(_, key)
    if type(key) == "number" then return nil end
    return function() return nil end
  end,
  __call = function() return nil end,
})

local function resolve(path)
  local value = morf
  for part in path:gmatch("[^.]+") do
    if type(value) ~= "table" then return nil end
    value = value[part]
  end
  return value
end

local stubs = {}
local function collect(value, path, kind, seen)
  if type(value) == "function" then
    stubs[value] = stubs[value] or function()
      record(path, kind)
      return inert
    end
  elseif type(value) == "table" and not seen[value] then
    seen[value] = true
    for key, field in pairs(value) do
      collect(field, path .. "." .. tostring(key), kind, seen)
    end
  end
end
for kind, paths in pairs(BLOCKED) do
  for _, path in ipairs(paths) do collect(resolve(path), path, kind, {}) end
end

local function replace(table_, seen)
  if seen[table_] then return end
  seen[table_] = true
  for key, value in pairs(table_) do
    if stubs[value] then
      table_[key] = stubs[value]
    elseif type(value) == "table" then
      replace(value, seen)
    end
  end
end
replace(morf, {})

-- A terminal starts a program; its stand-in keeps the node and the fields
-- a fragment reads back.
local TERMINAL_READS = { title = "", columns = 0, rows = 0, running = false }
local GENERIC = { "id", "x", "y", "width", "height", "anchors", "visible", "opacity", "layout" }
ui.Terminal = function(properties)
  record("ui.Terminal", "process")
  local kept = {}
  for _, key in ipairs(GENERIC) do kept[key] = properties[key] end
  local node = ui.Item(kept)
  return setmetatable({}, {
    __index = function(_, key)
      if TERMINAL_READS[key] ~= nil then return TERMINAL_READS[key] end
      return node[key]
    end,
    __newindex = function(_, key, value) node[key] = value end,
  })
end

-- The primary root a fragment that builds no node still needs.
ui.Item { id = "docs-check-root" }

-- The default look as `require("kit")`, as a configuration's own kit is,
-- made on the first ask.
local load_module = require
function require(name)
  if name == "kit" and package.loaded.kit == nil then
    package.loaded.kit = load_module("lib.kit.skins.default").make({})
  end
  return load_module(name)
end

-- Library modules the guide names without requiring them.
local KIT = {
  app = "lib.kit.app", canvas = "lib.kit.archetypes.canvas", collection = "lib.kit.archetypes.collection",
  composites = "lib.kit.composites", control = "lib.kit.control", contract = "lib.kit.contract",
  disclosure = "lib.kit.archetypes.disclosure", dock = "lib.kit.archetypes.dock", drag = "lib.kit.archetypes.drag",
  navigation = "lib.kit.archetypes.navigation", plane = "lib.kit.archetypes.plane", popup = "lib.kit.archetypes.popup",
  press = "lib.kit.archetypes.press", range = "lib.kit.archetypes.range", scroll = "lib.kit.archetypes.scroll",
  selection = "lib.kit.archetypes.selection", shell = "lib.kit.archetypes.shell", skin = "lib.kit.skin",
  text_field = "lib.kit.archetypes.text_field", tokens = "lib.kit.tokens", widgets = "lib.kit.widgets",
}

local function namespace(prefix)
  return setmetatable({}, { __index = function(self, key)
    local name = prefix .. "." .. key
    local ok, module = pcall(require, name)
    if not ok and not tostring(module):find("`" .. name .. "` is not available", 1, true) then
      error(module, 2)
    end
    local value = ok and module or namespace(name)
    rawset(self, key, value)
    return value
  end })
end

-- The surrounding configuration a fragment assumes: its handlers, signals,
-- nodes and data. Each is built when a fragment first names it, so a node
-- nobody asked for never competes with the fragment's tree for the root.
local FIXTURES = {
  morf = function() return morf end,
  ui = function() return ui end,
  lib = function() return namespace("lib") end,
  editor = function() return ui.TextInput { width = 200, height = 30 } end,
  target = function() return ui.MouseArea { width = 40, height = 40 } end,
  cell = function() return function() return ui.Rect { height = 40 } end end,
  theme = function()
    return morf.theme({ accent = "#3366cc", paper = "#f6f5f4", ink = "#1b1b1f",
                        alert = "#e5484d", keyword = "#c678dd" })
  end,
  accent = function() return morf.color "#3366cc" end,
  other = function() return morf.color "#cc6633" end,
  over = function() return morf.color "#00000080" end,
  paper = function() return morf.color "#f6f5f4" end,
  rows = function() return { { identifier = 1, title = "one" }, { identifier = 2, title = "two" } } end,
  apps = function() return { { name = "Firefox", exec = "firefox" }, { name = "Files", exec = "files" } } end,
  files = function() return { { name = "a.txt", size = 1 }, { name = "b.txt", size = 2 } } end,
  model = function() return morf.list_model({ { title = "one" }, { title = "two" } }) end,
  notifications = function() return morf.list_model({ { title = "Mail" } }) end,
  KEYWORDS = function() return { ["local"] = true, ["function"] = true } end,
  notification = function() return { body = "<b>Build</b> finished" } end,
  battery = function() return { level = 80 } end,
  state = function() return { playing = false, position = 0, length = 60, volume = 0.5 } end,
  mood = function() return function() return "happy" end end,
  selected = function() return function() return false end end,
  pets = function() return { mood = function() return "awake" end } end,
  count = function() return function() return 0 end end,
  name = function() return "Ada" end,
  id = function() return 1 end,
  graph = function() return { nodes = {}, ports = {}, edges = {} } end,
  data = function()
    return setmetatable({}, { __index = function(t, row) t[row] = {} return t[row] end })
  end,
  win = function() return { width = 800, height = 600 } end,
  hints = function() return { ["image-data"] = { 1, 1, 4, true, 8, 4, "\255\0\0\255" } } end,
  path = function() return "docs-check-missing.png" end,
  avatar = function() return "docs-check-missing.png" end,
  start_notification_server = function() return function() return { close = function() end } end end,
  read_load = function() return function() return 0 end end,
  view = function()
    local _, view = require("lib.kit.widgets").tab_view { width = 200, height = 100, tabs = { { title = "One" } } }
    return view
  end,
}
for name in ([[about apply close copy cut discard dismiss go go_back go_to_line insert
    italic launch load_more new open_file open_mail pick prev run save save_all
    set_bold set_dark set_scale set_size set_wrap share show sign_in toggle toggle_bold
    zoom log rebuild_for set_level move_by retry save_as new_tab undo activate]]):gmatch("%S+") do
  FIXTURES[name] = function() return function() end end
end
for name, initial in pairs({
  hover = false, down = false, open = false, opened = false, busy = true, refused = false, query = "",
  dirty = false, confirm = false, arranging = false, wide = false, tab = 1,
  wrap = false, width = 300, room = 400, day = "2026-10-16", tint = "#3366cc",
  due = "", font = "Inter", alarm = "07:30", page_title = "General", branch = "main",
  done = 0.5, on = false, dark = false, level = 0.5, muted = false, offline = true,
}) do
  FIXTURES[name] = function() return morf.signal("docs." .. name, initial) end
end
for name in ([[panel face menu list button root card detail header details notice
    stack inspector file_tree surface_root search_field sidebar my_node some_control
    some_kit_control toggle_button menu_button left middle right world_drawing
    zoom_buttons content node tabs page account_form]]):gmatch("%S+") do
  FIXTURES[name] = function() return ui.Item { width = 40, height = 40 } end
end
for name in ([[build build_dialog build_menu build_notes build_settings build_sound
    log_view focus_ring]]):gmatch("%S+") do
  FIXTURES[name] = function() return function() return ui.Item { width = 40, height = 40 } end end
end
for name, value in pairs({ x = 10, y = 10, w = 200, h = 100, stack_id = 1 }) do
  FIXTURES[name] = function() return value end
end

local function located(message)
  return (tostring(message):gsub("@([%w_./-]+%.md:%d+)", "%1"))
end

-- Runs one fragment whose fence opens at `line` of `file`.
function harness.run(code, file, line)
  local built = {}
  local env = setmetatable({}, { __index = function(_, name)
    local value = built[name]
    if value == nil and FIXTURES[name] then
      value = FIXTURES[name]()
      built[name] = value
    end
    if value == nil and KIT[name] then value = require(KIT[name]) end
    if value == nil then value = _G[name] end
    if value == nil then
      error(("`%s` is neither a global nor a docs harness fixture"):format(name), 2)
    end
    return value
  end })
  local result = { ok = true, message = "", blocked = harness.blocked }
  local chunk, err = load(string.rep("\n", line) .. code, "@" .. file, "t", env)
  if chunk then
    local ok, raised = pcall(chunk)
    if not ok then result.ok, result.message = false, located(raised) end
  else
    result.ok, result.message = false, located(err)
  end
  morf.ipc["docs.result"] = function() return result end
  return result
end

return harness
