-- Volume: the ring is the level and the glyph the output; the detail is a
-- slider and the two mutes. The ring stays white: volume is a choice, not
-- a warning.
--
-- Port of VolumeModule.qml.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local audio = require("services.audio")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color

local function ring(size, thickness, glyph_size)
  return controls.ring_glyph {
    size = size, thickness = thickness,
    progress = function() return audio.muted() and 0 or audio.volume() / 100 end,
    track_color = C.indicatorDim, fill_color = C.indicator,
    glyph = audio.icon, glyph_size = glyph_size,
    glyph_color = function() return audio.muted() and C.textMuted() or C.indicator end,
  }
end

modules.define("volume", {
  glyph = audio.icon,
  value = function() return audio.muted() and "Muted" or (audio.volume() .. "%") end,
  has = audio.ready,
  chip = function()
    return ring(theme.capsule_height(), 2.5, math.floor(theme.capsule_height() * 0.38 + 0.5))
  end,
  detail = function()
    local w = modules.entry("volume").width
    local column_w = w - 8 - 28 - 56 - 14
    local hovered = controls.signal("volume.slider", false)
    return ui.Item {
      anchors = { fill = true },
      ui.Row {
        anchors = { left = true, left_margin = 14, vertical_center = true },
        gap = 14, align = "center",
        ring(56, 3.5, 20),
        ui.Column {
          gap = 8, width = column_w,
          ui.Item {
            width = column_w, height = 16,
            kit.text { anchors = { left = true, vertical_center = true },
              text = "Volume", size = theme.size.small, weight = 600 },
            kit.text { anchors = { right = true, vertical_center = true },
              text = function() return audio.muted() and "Muted" or (audio.volume() .. "%") end,
              mono = true, size = theme.size.small,
              color = function() return audio.muted() and C.textMuted() or C.text() end },
          },
          -- The whole strip is the hit area.
          ui.Item {
            width = column_w, height = 16,
            controls.usage_bar {
              anchors = { vertical_center = true },
              width = column_w,
              height = function() return hovered:get() and 6 or 4 end,
              progress = function() return audio.volume() / 100 end,
              fill_color = function() return audio.muted() and C.indicatorDim or C.accent() end,
            },
            controls.drag_area {
              width = column_w, from = 0, to = 100, value = audio.volume,
              hovered = hovered, on_moved = audio.set_volume,
            },
          },
          ui.Row {
            gap = 9,
            controls.pill { text = "Mute", width = (column_w - 9) / 2, active = audio.muted,
              on_click = audio.toggle_mute },
            controls.pill { text = "Mic", width = (column_w - 9) / 2, active = audio.source_muted,
              on_click = audio.toggle_source_mute },
          },
        },
      },
    }
  end,
})
