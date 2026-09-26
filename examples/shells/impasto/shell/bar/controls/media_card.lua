-- The player: MediaCard.qml (the control centre's media block) and the
-- artwork every media piece shows.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local media = require("services.media")

local C = theme.color
local M = {}

--- The artwork, or a note on the island's grey when there is none.
--- `size`, `radius` (the picture corner by default), `glyph_size`,
--- `glyph_color`.
function M.art(values)
  local size = values.size
  return ui.ClipRect {
    width = size, height = size,
    radius = values.radius or size * theme.picture_corner,
    color = C.islandSurfaceHover,
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source = media.art,
      source_width = size * 2, source_height = size * 2,
      visible = function() return media.art() ~= "" end,
    },
    kit.glyph {
      anchors = { center_in = true }, glyph = "󰎇", size = values.glyph_size or math.floor(size * 0.4),
      color = values.glyph_color or C.indicator,
      visible = function() return media.art() == "" end,
    },
  }
end

--- Transport: previous, play/pause, next.
function M.transport(sizes)
  sizes = sizes or {}
  local d = sizes.diameter
  local function button(icon, size, on_click, enabled)
    return controls.icon_button {
      icon = icon, icon_size = size, on_click = on_click, enabled = enabled,
      width = d or 32, height = d or 28, radius = d and d / 2 or nil,
    }
  end
  return ui.Row {
    gap = sizes.gap or 6, align = "center",
    button("󰒮", sizes.small or 15, media.previous, media.can_previous),
    button(function() return media.playing() and "󰏤" or "󰐊" end, sizes.large or 18, media.toggle),
    button("󰒭", sizes.small or 15, media.next, media.can_next),
  }
end

--- The control centre block, `width` x `height`.
function M.build(options)
  local width, height = options.width, options.height
  local inner_w, inner_h = width - 28, height - 28
  local text_w = inner_w - 58 - 12
  return controls.card {
    width = width, height = height,
    kit.text {
      anchors = { center_in = true }, text = "Nothing playing",
      size = theme.size.small, color = C.textMuted,
      visible = function() return not media.available() end,
    },
    ui.Item {
      anchors = { fill = true },
      visible = media.available,
      ui.Row {
        x = 0, y = 0, gap = 12, align = "center",
        M.art { size = 58, glyph_size = 22, glyph_color = C.accent },
        ui.Column {
          gap = 2,
          kit.text { text = media.title, size = theme.size.regular, weight = 600,
            width = text_w, wrap = true, max_lines = 2 },
          kit.text { text = media.artist, size = theme.size.small, color = C.textMuted,
            width = text_w, elide = "right" },
        },
      },
      -- Progress, only when the track has a length.
      ui.Rect {
        x = 0, y = inner_h - 34 - 11 - 3, width = inner_w, height = 3, radius = 1.5,
        color = C.islandSurfaceHover,
        visible = media.seekable,
        ui.Rect {
          height = 3, radius = 1.5, color = C.accent,
          width = function() return inner_w * media.progress() end,
          behavior = { width = { duration = 900, easing = "linear" } },
        },
      },
      ui.Item {
        x = 0, y = inner_h - 34, width = inner_w, height = 34,
        kit.text { anchors = { left = true, vertical_center = true },
          text = function() return media.clock(media.position()) end,
          mono = true, size = theme.size.label, color = C.textMuted, visible = media.seekable },
        ui.Item { anchors = { center_in = true }, width = 32 * 3 + 12, height = 28, M.transport() },
        kit.text { anchors = { right = true, vertical_center = true },
          text = function() return media.clock(media.length()) end,
          mono = true, size = theme.size.label, color = C.textMuted, visible = media.seekable },
      },
    },
  }
end

return M
