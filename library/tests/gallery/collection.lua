-- Gallery samples for the Collection widgets: lists, grids, a column view
-- and a tree, each over rows as an application has them.
local M = { span = { column_view = { 2, 1 } } }

local W, H = 280, 220

local function rows(labels, extra)
  local out = {}
  for i, label in ipairs(labels) do
    local row = { key = tostring(i), label = label }
    for k, v in pairs(extra and extra(i) or {}) do row[k] = v end
    out[i] = row
  end
  return out
end

function M.list_view(_, w)
  return (w.list_view { width = W, height = H, row_height = 40, current = 2,
    rows = rows { "Inbox", "Starred", "Sent", "Drafts", "Archive", "Spam", "Trash" } })
end
function M.list_box(_, w)
  return (w.list_box { width = W, height = H, row_height = 54, current = 1, variant = "boxed_list",
    rows = {
      { key = "wifi", label = "Wi-Fi", subtitle = "Connected to Home", icon = "wifi" },
      { key = "bt", label = "Bluetooth", subtitle = "2 devices", icon = "bluetooth" },
      { key = "vpn", label = "VPN", subtitle = "Off", icon = "vpn_key" },
    } })
end
function M.grid_view(_, w)
  local icons = { "image", "music_note", "movie", "description", "download", "folder" }
  return (w.grid_view { width = W, height = H, cell_width = 92, cell_height = 104, current = 2,
    rows = rows({ "Photos", "Music", "Videos", "Documents", "Downloads", "Projects" },
      function(i) return { icon = icons[i] } end) })
end
function M.flow_box(_, w)
  return (w.flow_box { width = W, height = H, cell_width = 130, cell_height = 44, mode = "multi", selected = { 1, 4 },
    rows = rows { "Rust", "Lua", "Wayland", "Vulkan", "Shaders", "Layout" } })
end
function M.column_view(_, w)
  local data = {
    { key = "a", name = "morf-scene", lang = "Rust", lines = 6210 },
    { key = "b", name = "morf-layout", lang = "Rust", lines = 4120 },
    { key = "c", name = "lib.kit", lang = "Lua", lines = 9800 },
  }
  local node, view = w.column_view { width = 600, height = H, row_height = 34, current = 2, rows = data,
    columns = { { key = "name", title = "Crate", width = 260, sortable = true },
      { key = "lang", title = "Language", width = 160, sortable = true },
      { key = "lines", title = "Lines", width = 140, sortable = true, align = "end" } } }
  view.sort("lines")
  return node
end
function M.tree_expander(_, w)
  return (w.tree_expander { width = W, height = H, row_height = 32, current = 3, expanded = { "src" },
    tree = {
      { key = "src", label = "src", children = { { key = "main", label = "main.rs" }, { key = "lib", label = "lib.rs" } } },
      { key = "readme", label = "README.md" },
    } })
end

return M
