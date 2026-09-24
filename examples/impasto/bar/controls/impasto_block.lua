-- The control centre's impasto block: ImpastoBlock.qml.
--
-- The terminal greeting as a block: the palette board where fastfetch puts
-- its logo, and the same lines beside it (user@host, distribution, kernel,
-- compositor, load, uptime, colour daubs). The machine's names are read
-- once from /etc and /proc; the load is `services.stats`. Not clickable.
--
-- Small square: board, title and uptime. Wide: board beside the machine
-- column. Large square: the full greeting, with the wordmark under the
-- board.

local ui = require("morf.ui")
local theme = require("theme")
local stats = require("services.stats")
local account = require("services.account")
local kit = require("components.kit")
local controls = require("components.controls")
local palette_board = require("components.palette_board")

local C = theme.color
local M = {}

-- ---------------------------------------------------------------- machine --

local fs = morf.fs
local machine

local function first_line(path)
  local text = fs.read(path, 64 * 1024)
  return text and (text:match("^%s*([^\n]-)%s*\n") or text:match("^%s*(.-)%s*$")) or ""
end

--- The machine's names, as MachineService reads them: `{ host, os, kernel,
--- wm, shell, packages }`; `packages` is nil where no package database is
--- found.
function M.machine()
  if machine then return machine end
  local release = {}
  local text = fs.read("/etc/os-release", 64 * 1024) or fs.read("/usr/lib/os-release", 64 * 1024) or ""
  for key, value in text:gmatch("([%w_]+)=([^\n]*)") do
    release[key] = value:match('^"(.*)"$') or value:match("^'(.*)'$") or value
  end
  local shell = (morf.env("SHELL") or ""):match("([^/]+)$") or ""
  local packages
  -- pacman keeps one directory per installed package.
  local entries = fs.list("/var/lib/pacman/local")
  if entries then
    packages = 0
    for _, entry in ipairs(entries) do if entry.is_dir then packages = packages + 1 end end
  end
  machine = {
    host = first_line("/proc/sys/kernel/hostname"),
    os = release.PRETTY_NAME or release.NAME or "Linux",
    kernel = first_line("/proc/sys/kernel/osrelease"),
    wm = morf.env("XDG_CURRENT_DESKTOP") or morf.env("DESKTOP_SESSION") or "",
    shell = shell,
    packages = packages,
  }
  return machine
end

-- ------------------------------------------------------------------ lines --

local PAINTS = { "accent", "green", "yellow", "red", "blue" }

local function pad(key) return key .. string.rep(" ", math.max(0, 6 - #key)) end

--- The five colour daubs from the greeting's last line.
local function daubs(dot)
  local row = { gap = 5, align = "center" }
  for _, name in ipairs(PAINTS) do
    row[#row + 1] = ui.Rect { width = dot, height = dot, radius = dot / 2, color = C[name],
      behavior = { color = theme.behave("medium") } }
  end
  return ui.Row(row)
end

--- One greeting line: glyph and key in the accent, then the value, and a
--- `note` that must stay visible (the live percentage after a long CPU
--- model) at the end. Hidden with no value.
local function line(size, width, icon, key, value, note)
  local line_h = math.ceil(size * 1.3)
  local head = kit.text { text = icon .. " " .. pad(key) .. "  ", mono = true, size = size, color = C.accent }
  local tail = note and kit.text { text = function() return "  " .. note() end,
    mono = true, size = size, color = C.textMuted }
  local tail_w = function()
    if not tail or note() == "" then return 0 end
    return tail.layout_width or 0
  end
  local node = {
    width = width, height = line_h,
    visible = function() return value() ~= "" end,
    ui.Row {
      anchors = { left = true, vertical_center = true }, gap = 0, align = "center",
      head,
      kit.text {
        text = value, mono = true, size = size, elide = "right",
        width = function() return width - (head.layout_width or 0) - tail_w() end,
      },
    },
  }
  if tail then
    node[#node + 1] = ui.Item { anchors = { right = true, vertical_center = true }, height = line_h,
      visible = function() return note() ~= "" end,
      width = function() return math.max(1, tail.layout_width or 0) end, tail }
  end
  return ui.Item(node)
end

--- `user@host`, the `@` muted.
local function title(size, width, centred)
  local row = ui.Row {
    gap = 0, align = "center",
    kit.text { text = "󰣇 ", mono = true, size = size, weight = 600, color = C.accent },
    kit.text { text = account.user or "", mono = true, size = size, weight = 600 },
    kit.text { text = "@", mono = true, size = size, weight = 600, color = C.textMuted },
    kit.text { text = M.machine().host, mono = true, size = size, weight = 600 },
  }
  return ui.ClipRect {
    width = width, height = math.ceil(size * 1.3), color = "#00000000",
    ui.Item {
      x = function() return centred and math.max(0, (width - (row.layout_width or 0)) / 2) or 0 end,
      width = function() return row.layout_width or 0 end, height = math.ceil(size * 1.3),
      ui.Item { anchors = { vertical_center = true }, height = math.ceil(size * 1.3), row },
    },
  }
end

local function uptime() return stats.duration(stats.uptime()) end
local function os_name() return M.machine().os end
local function kernel() return M.machine().kernel end
local function packages() local p = M.machine().packages return p and tostring(p) or "" end

local function colours(size, width, dot)
  return ui.Row {
    gap = 0, align = "center",
    kit.text { text = "󰏘 " .. pad("colors") .. "  ", mono = true, size = size, color = C.accent },
    daubs(dot),
  }
end

local function cpu_model()
  local model = tostring(stats.model() or "")
  return (model:gsub("%((R)%)", ""):gsub("%((TM)%)", ""):gsub("%s+", " "):match("^%s*(.-)%s*$"))
end

local function disk()
  local list = stats.disks() or {}
  return list[1]
end

-- ------------------------------------------------------------------ faces --

local function square(w, h, size)
  return ui.Column {
    x = 0, y = function() return 0 end,
    gap = 4, align = "center", width = w,
    palette_board { size = 46 },
    title(size, w, true),
    kit.text { text = function() return "󰅐 " .. uptime() end, mono = true, size = size,
      width = w, elide = "right", horizontal_alignment = "center" },
    daubs(7),
  }
end

local function wide(w, h, size)
  local column_w = w - 64 - 14
  return ui.Row {
    gap = 14, align = "center", height = h,
    palette_board { size = 64 },
    ui.Column {
      gap = 1, width = column_w,
      title(size, column_w),
      line(size, column_w, "󰣇", "os", os_name),
      line(size, column_w, "󰒓", "kernel", kernel),
      line(size, column_w, "󰅐", "uptime", uptime),
      line(size, column_w, "󰏗", "pkgs", packages),
      colours(size, column_w, 7),
    },
  }
end

local function tall(w, h, size)
  local column_w = w - 72 - 14
  local hairline = function() return ui.Item { width = w, height = 9,
    controls.hairline { anchors = { vertical_center = true }, width = w } } end
  local short_hairline = ui.Item { width = column_w, height = 5,
    controls.hairline { anchors = { vertical_center = true }, width = column_w } }
  return ui.Column {
    gap = 2, width = w,
    ui.Row {
      gap = 14, align = "center",
      ui.Column {
        gap = 0, align = "center",
        palette_board { size = 72 },
        ui.Text { text = "impasto", font_family = theme.font_signature, font_size = 20, color = C.text },
      },
      ui.Column {
        gap = 2, width = column_w,
        title(size, column_w),
        short_hairline,
        line(size, column_w, "󰣇", "os", os_name),
        line(size, column_w, "󰒓", "kernel", kernel),
        line(size, column_w, "󰖳", "wm", function() return M.machine().wm end),
        line(size, column_w, "󰆍", "shell", function() return M.machine().shell end),
      },
    },
    hairline(),
    line(size, w, "󰘚", "cpu", cpu_model,
      function() return stats.ready() and (math.floor((stats.cpu() or 0) + 0.5) .. "%") or "" end),
    line(size, w, "󰋊", "disk",
      function()
        local d = disk()
        if not d then return "" end
        return stats.bytes(d.used, 0) .. " / " .. stats.bytes(d.total, 0)
      end,
      function()
        local d = disk()
        if not d or (d.total or 0) <= 0 then return "" end
        return math.floor(d.used / d.total * 100 + 0.5) .. "%"
      end),
    line(size, w, "󰍛", "mem",
      function()
        local total = stats.memory_total() or 0
        if total <= 0 then return "" end
        return stats.bytes(stats.memory_used()) .. " / " .. stats.bytes(total)
      end,
      function()
        if (stats.memory_total() or 0) <= 0 then return "" end
        return math.floor(stats.memory_fraction() * 100 + 0.5) .. "%"
      end),
    line(size, w, "󰏗", "pkgs", packages),
    hairline(),
    line(size, w, "󰅐", "uptime", uptime),
    colours(size, w, 8),
  }
end

function M.build(o)
  local w, h = o.width - 28, o.height - 28
  local is_tall = o.rows >= 4
  local is_wide = o.cols >= 2 and not is_tall
  -- The greeting's own size on the two smaller faces; one step up on the
  -- large square.
  local size = is_tall and theme.size.small or theme.size.label
  local face
  if is_tall then face = tall(w, h, size)
  elseif is_wide then face = wide(w, h, size)
  else face = square(w, h, size) end
  return controls.card {
    width = o.width, height = o.height,
    ui.Item {
      width = w, height = h,
      ui.Item {
        anchors = { vertical_center = true },
        width = w, height = function() return face.layout_height or h end,
        face,
      },
    },
  }
end

return M
