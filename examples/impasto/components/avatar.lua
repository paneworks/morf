-- A round picture, or initials when there is none: Avatar.qml.
--
-- The common case is no picture (most machines have no `~/.face`), and a
-- picture that is set but gone, or not a picture the engine decodes, falls
-- back to the initials too (the original shows the picture only once it is
-- Ready). ui.Image reports no status, so the file is checked, and its header
-- read with `morf.image.info`, once per file and modification.
--
--     avatar { size = 86, source = function() return path end, initials = fn, ring = colour }

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color

local function read(v) if type(v) == "function" then return v() end return v end

local seen = {}
local function shows(path)
  if not path or path == "" then return false end
  if not morf.fs.is_file(path) then return false end
  local ok_stat, stat = pcall(morf.fs.stat, path)
  local stamp = path .. "@" .. tostring(ok_stat and stat and stat.modified or "")
  if seen[stamp] == nil then
    local ok, info = pcall(morf.image.info, path)
    seen[stamp] = ok and info ~= nil
  end
  return seen[stamp]
end

return function(values)
  local size = values.size or 86
  local source = function() return read(values.source) or "" end
  local picture = function() return shows(source()) end
  return ui.ClipRect {
    x = values.x, y = values.y, anchors = values.anchors,
    width = size, height = size, radius = size / 2,
    color = C.islandSurface,
    border_width = 2, border_color = values.ring or C.hairline,
    content_under_border = true,
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source_width = size * 2, source_height = size * 2,
      source = function() return picture() and source() or "" end,
      visible = picture,
    },
    kit.text {
      anchors = { center_in = true },
      visible = function() return not picture() end,
      text = function() return read(values.initials) or "?" end,
      size = math.floor(size * 0.36 + 0.5), weight = 300, color = C.text,
    },
  }
end
