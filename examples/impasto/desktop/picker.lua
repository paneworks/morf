-- A photo widget's picker, on the desk in the inspector's place.
--
-- Port of Picker.qml: the places down the side (Pictures, the wallpapers,
-- Downloads, Desktop, Home), the folder as thumbnails. A click on a folder
-- goes into it, a click on a picture is the choice, the arrow goes up a
-- level and the wheel scrolls. Pointer only, as the original: the desk has
-- no typed path and no search. The folder is listed with `morf.fs.list`.
--
-- The card sits beside the widget where there is room, else over the
-- middle of the board. It opens on the current picture's folder, else the
-- pictures folder.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local wallpaper = require("services.wallpaper")
local controls = require("desktop.arrange.controls")

local C = theme.color
local M = {}

M.PAD = 16
M.GAP = 14
M.THUMB = 108
M.LIMIT = 120

local model = controls.keyed("picker", function() return desk.picking:get() end)

local function home() return morf.fs.home() end

local function user_dir(name, fallback)
  local ok, path = pcall(morf.fs.dir, name)
  if ok and type(path) == "string" and path ~= "" then return path end
  return home() .. "/" .. fallback
end

local function places()
  local out = {
    { glyph = "󰋩", label = "Pictures", path = user_dir("pictures", "Pictures") },
    { glyph = "󰸉", label = "Wallpapers", path = wallpaper.dir() },
    { glyph = "󰇚", label = "Downloads", path = user_dir("download", "Downloads") },
    { glyph = "󰍹", label = "Desktop", path = user_dir("desktop", "Desktop") },
    { glyph = "󰋜", label = "Home", path = home() },
  }
  local kept = {}
  for _, p in ipairs(out) do
    if p.path == home() and p.label ~= "Home" then p = nil end
    if p and (p.label == "Home" or morf.fs.is_dir(p.path)) then kept[#kept + 1] = p end
  end
  return kept
end

-- Thumbnails: a folder of full-size photographs decoded on the drawing
-- thread would stall the desk for seconds, so each picture is made small
-- once, by `morf.image.process` on a worker, and kept in the cache; a tile
-- shows its thumbnail when it is there.
local THUMB_DIR = morf.cache_path and morf.cache_path("impasto-thumbs")
  or (morf.fs.home() .. "/.cache/impasto-thumbs")
local made = morf.signal("impasto.desk.picker.thumbs", 0)
local pending = {}

local function thumb_of(entry)
  local stamp = tostring(math.floor(tonumber(entry.modified) or 0))
  return morf.fs.join(THUMB_DIR, (entry.path:gsub("[^%w]", "_")) .. "_" .. stamp .. ".jpg")
end

local function ensure_thumb(entry)
  local out = thumb_of(entry)
  if morf.fs.is_file(out) or pending[out] then return out end
  pending[out] = true
  morf.fs.mkdir(THUMB_DIR)
  local ok = morf.image.process {
    source = entry.path, output = out, quality = 80,
    ops = { { "resize", M.THUMB * 2, M.THUMB * 2, "fill" } },
    on_done = function()
      pending[out] = nil
      made:set(made:get() + 1)
    end,
  }
  if not ok then pending[out] = nil end
  return out
end

local function parent_of(path)
  local cut = path:match("^(.*)/[^/]*$")
  if not cut or cut == "" then return "/" end
  return cut
end

-- Folders first, then pictures, each by name; hidden ones left out.
local function listing(folder)
  local entries = morf.fs.list(folder) or {}
  local dirs, pictures = {}, {}
  for _, e in ipairs(entries) do
    if not e.hidden and e.name:sub(1, 1) ~= "." then
      if e.is_dir then
        dirs[#dirs + 1] = { path = e.path, name = e.name, dir = true }
      elseif e.is_file and desk.picture_types[(e.extension or ""):lower()] then
        pictures[#pictures + 1] = { path = e.path, name = e.name, dir = false, modified = e.modified }
      end
    end
  end
  local by_name = function(a, b) return a.name:lower() < b.name:lower() end
  table.sort(dirs, by_name)
  table.sort(pictures, by_name)
  local out = {}
  for _, d in ipairs(dirs) do if #out < M.LIMIT then out[#out + 1] = d end end
  for _, p in ipairs(pictures) do if #out < M.LIMIT then out[#out + 1] = p end end
  return out
end

local function card_for(key)
  local row = desk.entry_of(key)
  if not row then return ui.Item {} end
  local current = desk.picture_of(row)
  local list_of = places()
  local start = current ~= "" and parent_of(current) or (list_of[1] and list_of[1].path or home())
  local folder = controls.signal("picker.folder", start)
  local scrolled = controls.signal("picker.scrolled", 0)
  local count = controls.signal("picker.count", 0)
  local items = morf.list_model({})

  local function open(path)
    local rows = listing(path)
    folder:set(path)
    scrolled:set(0)
    items:replace(rows, "path")
    count:set(#rows)
  end
  open(start)

  local board = desk.board()
  local gutter = theme.desktop_gutter
  local width = math.min(700, board.width - 2 * gutter)
  local height = math.min(560, board.height - 2 * gutter)
  local side_w = 150
  local grid_w = width - side_w - 12 - 1 - 12 - 8 - 12
  local head_h = 30
  local top = M.PAD + head_h + 12 + 1
  local grid_h = height - top - 12 - 8
  local per_row = math.max(1, math.floor(grid_w / (M.THUMB + 16)))
  local cell_w = grid_w / per_row
  local cell_h = M.THUMB + 28

  local function content_h() return math.ceil(count:get() / per_row) * cell_h end
  local function overflow() return math.max(0, content_h() - grid_h) end

  local function choose(path)
    desk.set_picture(key, path)
    desk.picking:set("")
  end

  local function trail()
    local f = folder:get()
    local h = home()
    local inside = f == h or f:sub(1, #h + 1) == h .. "/"
    local shown = inside and ("~" .. f:sub(#h + 1)) or f
    local parts = {}
    for part in shown:gmatch("[^/]+") do parts[#parts + 1] = part end
    if not inside then table.insert(parts, 1, "/") end
    return table.concat(parts, " › ")
  end

  -- The place the folder is in: the longest path it starts with.
  local function place_index()
    local f, best, length = folder:get(), 0, -1
    for i, p in ipairs(list_of) do
      if (f == p.path or f:sub(1, #p.path + 1) == p.path .. "/") and #p.path > length then
        best, length = i, #p.path
      end
    end
    return best
  end

  local side = {}
  for i, p in ipairs(list_of) do
    local hovered = controls.signal("picker.place", false)
    local here = function() return place_index() == i end
    side[#side + 1] = ui.Rect {
      width = side_w, height = 32, radius = theme.radius_small,
      color = function() return (here() or hovered:get()) and C.islandSurfaceHover or morf.color("transparent") end,
      kit.glyph { x = 10, y = 0, width = 16, height = 32, vertical_alignment = "center", glyph = p.glyph, size = 14,
        color = function() return here() and C.accent() or C.textMuted() end },
      kit.text { x = 36, y = 0, width = side_w - 46, height = 32, vertical_alignment = "center", elide = "right",
        text = p.label, size = theme.size.small,
        color = function() return here() and C.text() or C.textMuted() end },
      ui.MouseArea { anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() open(p.path) end },
    }
  end

  local function tile(r)
    if not r.dir then ensure_thumb(r) end
    local hovered = controls.signal("picker.tile", false)
    local lit = function() return hovered:get() or (not r.dir and r.path == current) end
    return ui.Item {
      width = cell_w, height = cell_h,
      ui.ClipRect {
        x = (cell_w - M.THUMB) / 2, y = 0, width = M.THUMB, height = M.THUMB,
        radius = M.THUMB * theme.picture_corner, color = C.islandSurface,
        border_width = 2, content_under_border = true,
        border_color = function() return lit() and C.accent() or morf.color("transparent") end,
        r.dir and kit.glyph { anchors = { fill = true }, vertical_alignment = "center", glyph = "󰉋", size = 30,
          color = function() return lit() and C.accent() or C.textMuted() end }
          or ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
            source = function()
              made:get()
              local thumb = thumb_of(r)
              return morf.fs.is_file(thumb) and thumb or ""
            end },
      },
      kit.text { x = (cell_w - M.THUMB - 4) / 2, y = M.THUMB + 5, width = M.THUMB + 4,
        horizontal_alignment = "center", elide = "middle", text = r.name, size = theme.size.label,
        color = function() return lit() and C.text() or C.textMuted() end },
      ui.MouseArea { anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() if r.dir then open(r.path) else choose(r.path) end end,
        on_wheel = function(_, _, _, py, _, steps)
          scrolled:set(math.max(0, math.min(overflow(), scrolled:get() + (steps ~= 0 and steps * 60 or -py))))
        end },
    }
  end

  -- Beside the widget where there is room, else over the middle.
  local function box() return desk.geometry(key) or { x = 0, y = 0, width = 0, height = 0 } end
  local function right_fits() local b = box() return b.x + b.width + M.GAP + width <= board.width - gutter end
  local function left_fits() return box().x - M.GAP - width >= gutter end

  return ui.Rect {
    x = function()
      local b = box()
      if right_fits() then return b.x + b.width + M.GAP end
      if left_fits() then return b.x - M.GAP - width end
      return (board.width - width) / 2
    end,
    y = function() return math.max(gutter, math.min(board.height - gutter - height, box().y)) end,
    width = width, height = height,
    radius = theme.radius_large, color = C.island, border_color = C.islandBorder, border_width = 1,
    ui.MouseArea {
      anchors = { fill = true }, accepted_buttons = { "left", "right" },
      on_wheel = function(_, _, _, py, _, steps)
        scrolled:set(math.max(0, math.min(overflow(), scrolled:get() + (steps ~= 0 and steps * 60 or -py))))
      end,
    },
    -- The head: up a level, the title, the folder, cancel.
    ui.Item {
      x = M.PAD, y = M.PAD, width = width - 2 * M.PAD, height = head_h,
      ui.Item { x = 0, y = 0, width = 30, height = 30,
        opacity = function() return folder:get() ~= "/" and 1 or 0.3 end,
        kit.icon_button { glyph = "󰁍", glyph_size = 14, diameter = 30, color = "#00000000",
          hover_color = C.islandSurfaceHover,
          on_click = function() if folder:get() ~= "/" then open(parent_of(folder:get())) end end } },
      kit.text { x = 40, y = 0, height = 30, vertical_alignment = "center", text = "Choose a picture",
        size = theme.size.medium, weight = 600, color = C.text },
      kit.text { x = 190, y = 0, width = width - 2 * M.PAD - 190 - 90, height = 30,
        vertical_alignment = "center", elide = "left", text = trail, size = theme.size.small, color = C.textMuted },
      ui.Item { x = width - 2 * M.PAD - 80, y = 2, width = 80, height = 26,
        require("components.pill_button") { text = "Cancel", height = 26, width = 80,
          on_click = function() desk.picking:set("") end } },
    },
    ui.Rect { x = 0, y = M.PAD + head_h + 12, width = width, height = 1, color = C.hairline },
    ui.Column { x = 12, y = top + 12, width = side_w, gap = 2, table.unpack(side) },
    ui.Rect { x = 12 + side_w + 12, y = top, width = 1, height = height - top, color = C.hairline },
    ui.ClipRect {
      x = 12 + side_w + 12 + 1 + 12, y = top + 12, width = grid_w, height = grid_h,
      color = morf.color("transparent"),
      ui.Repeater {
        as = "grid", columns = per_row, gap = 0,
        y = function() return -math.min(scrolled:get(), overflow()) end,
        model = items, delegate = tile,
      },
      kit.text { anchors = { center_in = true }, text = "No pictures here", size = theme.size.small,
        color = C.textMuted, visible = function() return count:get() == 0 end },
    },
  }
end

--- The picker, filling the board.
function M.build()
  return ui.Item {
    anchors = { fill = true },
    ui.Repeater {
      model = model,
      delegate = function(r) return card_for(r.id) end,
    },
  }
end

return M
