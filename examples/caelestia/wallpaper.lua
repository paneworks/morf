-- The wallpaper: which picture, and a background layer that paints it.
--
-- The picture is, first found: CAELESTIA_WALLPAPER, the setting
-- `wallpaper.path`, or the path the caelestia tools keep in
-- ~/.local/state/caelestia/wallpaper/path.txt (so a desk set up for the
-- original keeps its picture). Without one the layer paints the scheme's
-- background.

local morf = require("morf")
local ui = require("morf.ui")
local config = require("config")
local theme = require("theme")

local M = {}

local function expand(path)
  if path:sub(1, 2) == "~/" then return morf.fs.home() .. path:sub(2) end
  return path
end

local function state_file()
  local base = (morf.env and morf.env("XDG_STATE_HOME")) or (morf.fs.home() .. "/.local/state")
  return base .. "/caelestia/wallpaper/path.txt"
end

M.current = morf.signal("caelestia.wallpaper", "")

local function resolve()
  local path = (morf.env and morf.env("CAELESTIA_WALLPAPER")) or ""
  if path == "" then path = config.get("wallpaper.path") end
  if path == "" then path = (morf.fs.read(state_file()) or ""):match("^%s*(.-)%s*$") end
  path = expand(path or "")
  if path ~= "" and not morf.fs.exists(path) then
    morf.log("warn", "caelestia: no wallpaper at " .. path)
    path = ""
  end
  return path
end

M.current:set(resolve())

--- Sets the picture (and the setting), and the scheme follows it.
function M.set(path)
  config.set("wallpaper.path", path)
  M.current:set(resolve())
end

--- A background layer with the picture on it.
function M.open_layer()
  return morf.window.layer {
    blend = "srgb",
    namespace = "caelestia-wallpaper",
    layer = "background",
    anchors = { top = true, bottom = true, left = true, right = true },
    exclusive_zone = -1,
    keyboard_focus = "none",
    visible = true,
    root = ui.Item {
      anchors = { fill = true },
      ui.Rect { anchors = { fill = true }, color = function() return theme.color.surface end },
      ui.Image {
        id = "wallpaper",
        anchors = { fill = true },
        fill_mode = "preserve_aspect_crop",
        source = function() return M.current:get() end,
        visible = function() return M.current:get() ~= "" end,
      },
    },
  }
end

return M
