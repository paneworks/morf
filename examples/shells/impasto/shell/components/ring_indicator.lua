-- A circular gauge that fills clockwise from the top, with whatever sits
-- inside it.
--
-- Port of RingIndicator.qml. The track and the sweep are one small SVG
-- document (an arc with round caps, which reads as a gauge rather than a cut
-- pie slice); children are drawn over it. `size`, `progress`, `thickness`,
-- `track_color`, `fill_color`; all but the thickness may be functions.

local ui = require("morf.ui")
local draw = require("pets.draw")

local function read(v)
  if type(v) == "function" then return v() end
  return v
end

local n = draw.n

local function document(size, thickness, progress, track, fill)
  local r = (size - thickness) / 2
  local c = size / 2
  local parts = {
    string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="%s"/>',
      n(c), n(c), n(r), track, n(thickness)),
  }
  -- Rounded ends would still paint a dot at zero, hence the guard.
  if progress > 0 then
    local sweep = math.max(2, 360 * math.min(1, progress))
    if sweep >= 359.99 then
      parts[#parts + 1] = string.format(
        '<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="%s"/>',
        n(c), n(c), n(r), fill, n(thickness))
    else
      local a = math.rad(sweep - 90)
      parts[#parts + 1] = string.format(
        '<path d="M %s %s A %s %s 0 %d 1 %s %s" fill="none" stroke="%s" stroke-width="%s" stroke-linecap="round"/>',
        n(c), n(c - r), n(r), n(r), sweep > 180 and 1 or 0, n(c + r * math.cos(a)), n(c + r * math.sin(a)),
        fill, n(thickness))
    end
  end
  return string.format('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %s %s">%s</svg>',
    n(size), n(size), table.concat(parts))
end

return function(values)
  local size = function() return read(values.size) end
  local out = {
    width = size, height = size,
    x = values.x, y = values.y, anchors = values.anchors, scale = values.scale, visible = values.visible,
    ui.Image {
      anchors = { fill = true },
      source = function()
        -- Snapped to a hundredth, so a trickle of experience does not mint
        -- a new picture for every point.
        local p = math.floor((read(values.progress) or 0) * 100 + 0.5) / 100
        return document(size(), values.thickness or 3, p,
          draw.hex(read(values.track_color) or "#262626"), draw.hex(read(values.fill_color) or "#0a84ff"))
      end,
    },
  }
  for _, child in ipairs(values) do out[#out + 1] = child end
  return ui.Item(out)
end
