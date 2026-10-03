-- Gallery samples for the Collection widgets (lib.kit.samples): each a
-- working collection over a small model -- a table that sorts itself and
-- resizes its columns, a tree whose last folder loads its children, a chat
-- that keeps talking, two transfer lists that move rows between them.
local ui = require("morf.ui")

local M = {}

M.span = { data_table = { 2, 1 }, tree_table = { 2, 1 }, file_list = { 2, 1 }, chat_log = { 2, 1 },
  transfer_list = { 2, 1 }, kanban_column = { 1, 2 } }

local W, H = 280, 220
local WIDE = 600

local function rows(labels, extra)
  local out = {}
  for i, label in ipairs(labels) do
    local row = { key = "k" .. i, label = label }
    for k, v in pairs(extra and extra(i) or {}) do row[k] = v end
    out[i] = row
  end
  return out
end

function M.list(kit, w)
  return (w.list { width = W, height = H, row_height = 40, current = 2,
    rows = rows { "Inbox", "Starred", "Sent", "Drafts", "Archive", "Spam", "Trash" } })
end

function M.boxed_list(kit, w)
  return (w.boxed_list { width = W, height = H, row_height = 54, current = 1,
    rows = {
      { key = "wifi", label = "Wi-Fi", subtitle = "Connected to Home", icon = "wifi" },
      { key = "bt", label = "Bluetooth", subtitle = "2 devices", icon = "bluetooth" },
      { key = "net", label = "Network", subtitle = "Wired, 1 Gb/s", icon = "lan" },
      { key = "vpn", label = "VPN", subtitle = "Off", icon = "vpn_key" },
    } })
end

function M.list_box(kit, w)
  return (w.list_box { width = W, height = H, row_height = 52,
    rows = {
      { key = "a", label = "Automatic Date & Time", subtitle = "Requires internet access", icon = "schedule" },
      { key = "b", label = "Time Zone", subtitle = "Europe/Amsterdam", icon = "public", trailing = "CET" },
      { key = "c", label = "Time Format", subtitle = "24-hour", icon = "timer" },
      { key = "d", label = "Week Starts On", icon = "calendar_month", trailing = "Monday" },
      { key = "e", label = "Clock Seconds", icon = "more_time" },
    } })
end

function M.virtual_list(kit, w)
  local out = {}
  local hosts = { "eu-west", "us-east", "ap-south", "eu-north", "us-west", "sa-east" }
  for i = 1, 10000 do
    out[i] = { key = "v" .. i, label = ("GET /api %s"):format(hosts[i % #hosts + 1]), trailing = ("%d ms"):format((i * 37) % 900) }
  end
  local node, list = w.virtual_list { width = W, height = H, row_height = 32, rows = out, current = 3 }
  list.scroll_to(32 * 4000)
  return node
end

function M.grid_view(kit, w)
  local icons = { { "Photos", "photo_library", "extra" }, { "Music", "library_music", "warning" },
    { "Videos", "video_library", "error" }, { "Documents", "description", "info" },
    { "Downloads", "download", "success" }, { "Projects", "deployed_code", "accent" } }
  return (w.grid_view { width = W, height = H, cell_width = 92, cell_height = 104, current = 2,
    rows = rows({ "Photos", "Music", "Videos", "Documents", "Downloads", "Projects" },
      function(i) return { icon = icons[i][2], tone = icons[i][3] } end) })
end

function M.flow_box(kit, w)
  return (w.flow_box { width = W, height = H, cell_width = 140, cell_height = 44, mode = "multi", selected = { 1, 4 },
    rows = rows { "Rust", "Lua", "Wayland", "Vulkan", "Shaders", "Layout", "Text", "Motion" } })
end

local FILES = {
  { key = "f1", name = "Pictures", is_dir = true, items = 214, size = 0, modified = "Today 09:12" },
  { key = "f2", name = "notes.md", size = 4210, modified = "Today 08:40" },
  { key = "f3", name = "holiday.jpg", size = 3481920, modified = "Yesterday" },
  { key = "f4", name = "build.rs", size = 1820, modified = "2 Oct" },
  { key = "f5", name = "talk.mp4", size = 912000000, modified = "28 Sep" },
  { key = "f6", name = "theme.lua", size = 12900, modified = "27 Sep" },
  { key = "f7", name = "archive.tar.gz", size = 52000000, modified = "12 Sep" },
  { key = "f8", name = "report.pdf", size = 840000, modified = "3 Sep" },
}

function M.data_table(kit, w)
  local data = {
    { key = "a", name = "morf-render", lang = "Rust", lines = 18420, tests = 412 },
    { key = "b", name = "morf-kit", lang = "Rust", lines = 9310, tests = 268 },
    { key = "c", name = "library/kit", lang = "Lua", lines = 21880, tests = 190 },
    { key = "d", name = "morf-layout", lang = "Rust", lines = 6120, tests = 144 },
    { key = "e", name = "caelestia", lang = "Lua", lines = 30400, tests = 96 },
    { key = "f", name = "morf-text", lang = "Rust", lines = 7750, tests = 120 },
    { key = "g", name = "morf-wayland", lang = "Rust", lines = 11030, tests = 58 },
  }
  local node, table_ = w.data_table { width = WIDE, height = H, row_height = 34, current = 2, rows = data,
    columns = { { key = "name", title = "Crate", width = 240, sortable = true },
      { key = "lang", title = "Language", width = 140, sortable = true },
      { key = "lines", title = "Lines", width = 110, sortable = true, align = "end" },
      { key = "tests", title = "Tests", width = 110, sortable = true, align = "end" } } }
  -- Sorted by its lines, the most first: the arrow turns down.
  table_.sort("lines")
  table_.sort("lines")
  return node
end

function M.tree_view(kit, w)
  return (w.tree_view { width = W, height = H, row_height = 32, current = 3, expanded = { "src", "ui" },
    tree = {
      { key = "src", label = "src", children = {
        { key = "ui", label = "ui", children = { { key = "button", label = "button.lua" }, { key = "list", label = "list.lua" } } },
        { key = "main", label = "main.rs" },
        { key = "lazy", label = "assets", lazy = true } } },
      { key = "readme", label = "README.md" },
    },
    load_children = function(node, give)
      morf.timer(900, function() give { { key = "logo", label = "logo.svg" }, { key = "font", label = "font.ttf" } } end, false)
    end,
    on_expanded_changed = function() end,
  })
end

function M.tree_table(kit, w)
  return (w.tree_table { width = WIDE, height = H, row_height = 32, current = 3, expanded = { "home", "docs" },
    columns = { { key = "label", title = "Name", width = 300 }, { key = "size", title = "Size", width = 140, align = "end" },
      { key = "kind", title = "Kind", width = 160 } },
    tree = {
      { key = "home", label = "home", kind = "Folder", children = {
        { key = "docs", label = "Documents", kind = "Folder", children = {
          { key = "cv", label = "cv.pdf", size = 182000, kind = "PDF document" },
          { key = "tax", label = "taxes.ods", size = 48200, kind = "Spreadsheet" } } },
        { key = "pics", label = "Pictures", kind = "Folder", lazy = true },
        { key = "bash", label = ".bashrc", size = 3800, kind = "Shell script" } } },
    } })
end

function M.file_list(kit, w)
  local node = w.file_list { width = WIDE, height = H, current = 3, rows = FILES }
  return node
end

function M.timeline(kit, w)
  return (w.timeline { width = W, height = H, row_height = 54, current = 3,
    rows = {
      { key = "a", label = "Order placed", time = "09:02", detail = "Paid by card", done = true },
      { key = "b", label = "Packed", time = "11:40", detail = "Warehouse Utrecht", done = true },
      { key = "c", label = "Out for delivery", time = "14:15", detail = "Courier is 3 stops away" },
      { key = "d", label = "Delivered", time = "", detail = "Expected by 17:00" },
    } })
end

function M.feed(kit, w)
  return (w.feed { width = W, height = H, row_height = 96,
    rows = {
      { key = "a", author = "Ada", time = "2 min", text = "Pushed the new layout engine: grids now take tracks, and flex wraps." },
      { key = "b", author = "Linus", time = "1 h", text = "The compositor is GPU-bound here, so every animation is a transform." },
      { key = "c", author = "Grace", time = "3 h", text = "Reminder: the release candidate freezes on Friday." },
    } })
end

function M.chat_log(kit, w)
  local messages = {
    { key = "1", from = "them", text = "Are we still on for the demo?", time = "10:02" },
    { key = "2", from = "me", text = "Yes, at three. I'll bring the build.", time = "10:03" },
    { key = "3", from = "them", text = "Great. Does the list recycle its rows now?", time = "10:05" },
    { key = "4", from = "me", text = "Ten thousand rows, one screen of delegates.", time = "10:06" },
  }
  local node, chat = w.chat_log { width = WIDE, height = H, rows = messages }
  chat.scroll_to(1e6)
  -- A reply arrives a moment later, rising in from below.
  morf.timer(500, function()
    local row = { key = "5", from = "them", text = "Show me!", time = "10:07" }
    row.height = 60
    chat.insert(chat.model:len() + 1, row)
    morf.timer(30, function() chat.scroll_to(1e6) end, false)
  end, false)
  return node
end

function M.kanban_column(kit, w)
  return (w.kanban_column { width = W, height = 480 - 40, row_height = 70, current = 2,
    rows = {
      { key = "a", label = "Draft the widget guide", tag = "docs" },
      { key = "b", label = "Recycle tree rows", tag = "engine" },
      { key = "c", label = "Squash the sheet handle", tag = "drag" },
      { key = "d", label = "Material chips", tag = "theme" },
      { key = "e", label = "Profile the GPU", tag = "perf" },
    } })
end

function M.transfer_list(kit, w)
  local left = morf.list_model(rows { "Calendar", "Contacts", "Maps", "Weather", "Clocks" })
  local right = morf.list_model({ { key = "r1", label = "Files" }, { key = "r2", label = "Terminal" } })
  local LW = 250
  local left_node, left_list = w.transfer_list { width = LW, height = H, rows = left, mode = "multi", selected = { 2, 4 } }
  local right_node, right_list = w.transfer_list { x = WIDE - LW, width = LW, height = H, rows = right, mode = "multi" }
  -- Moves the picked rows across.
  local function move(from, to, list)
    local picked = {}
    for i = from:len(), 1, -1 do
      if require("lib.kit.control").has(list.t.selected, i) then table.insert(picked, 1, from:get(i)) from:remove(i) end
    end
    for _, row in ipairs(picked) do to:insert(to:len() + 1, row) end
  end
  local function button(icon, y, on)
    return w.icon { icon = icon, icon_off = icon, x = (WIDE - 40) / 2, y = y, width = 40, height = 36, size = 20,
      accessible_name = icon == "chevron_right" and "Move right" or "Move left", on_clicked = on }
  end
  return ui.Item { width = WIDE, height = H, left_node, right_node,
    button("chevron_right", H / 2 - 42, function() move(left, right, left_list) end),
    button("chevron_left", H / 2 + 6, function() move(right, left, right_list) end) }
end

return M
