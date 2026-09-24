-- Where an application's icon is: a themed icon, a file, or nowhere.
--
-- Quickshell.iconPath(name, true) in the original. A desktop entry's `Icon`
-- is a theme name or a path; a Hyprland class is often the name in lower
-- case, or a reverse-DNS id whose last part is. Lookups are remembered: the
-- theme does not change under a running shell, and a list redrawn on every
-- keystroke should not walk icon directories on every keystroke.

local ui = require("morf.ui")
local kit = require("components.kit")
local theme = require("theme")

local fs = morf.fs
local M = {}

-- The icon theme GTK is set to, which is the one the desk's applications
-- draw with; hicolor (the engine's default) when none is set.
local function gtk_theme()
  local config = fs.dir("config") or (fs.home() .. "/.config")
  for _, file in ipairs { "gtk-4.0/settings.ini", "gtk-3.0/settings.ini" } do
    local text = fs.read(fs.join(config, file))
    local name = text and text:match("gtk%-icon%-theme%-name%s*=%s*([^\n]+)")
    if name then
      name = name:match("^%s*(.-)%s*$")
      if name ~= "" then return name end
    end
  end
end
M.theme = gtk_theme()

local cache = {}

local function lookup(name)
  if name == "" then return nil end
  if M.theme then
    local ok, found = pcall(morf.has_icon, name, M.theme)
    if ok and found then return { name = name, theme = M.theme } end
  end
  local ok, found = pcall(morf.has_icon, name)
  if ok and found then return { name = name, theme = "hicolor" } end
  for _, ext in ipairs { ".svg", ".png", ".xpm" } do
    local path = "/usr/share/pixmaps/" .. name .. ext
    if fs.exists(path) then return { path = path } end
  end
end

--- `{ name, theme }` for a themed icon, `{ path }` for a file, or nil.
function M.resolve(name)
  name = tostring(name or "")
  if name == "" then return nil end
  local known = cache[name]
  if known ~= nil then return known or nil end
  local found
  if name:sub(1, 1) == "/" then
    found = fs.exists(name) and { path = name } or nil
  else
    found = lookup(name) or lookup(name:lower())
      or lookup((name:match("([^%.]+)$") or ""):lower())
  end
  cache[name] = found or false
  return found
end

--- A square icon node for a binding that names the application: the themed
--- icon, else the file, else `fallback` (a glyph) in a disc.
function M.node(values)
  local size = values.size or 26
  local resolved = function() return M.resolve(values.name()) end
  local themed = function() local r = resolved() return r and r.name or "" end
  local path = function() local r = resolved() return r and r.path or "" end
  return ui.Item {
    width = values.width or size, height = values.height or size,
    anchors = values.anchors, visible = values.visible,
    ui.Icon {
      anchors = { fill = true }, source_width = 2 * size, source_height = 2 * size,
      name = themed,
      theme = function() local r = resolved() return r and r.theme or "hicolor" end,
      visible = function() return themed() ~= "" end,
    },
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_fit",
      source = path,
      visible = function() return path() ~= "" end,
    },
    ui.Rect {
      anchors = { fill = true }, radius = size / 2,
      color = theme.color.islandSurfaceHover,
      visible = function() return resolved() == nil end,
      kit.glyph {
        anchors = { center_in = true }, size = math.floor(size * 0.54),
        glyph = values.fallback or "󰀻",
        color = values.fallback_color or theme.color.accent,
      },
    },
  }
end

return M
