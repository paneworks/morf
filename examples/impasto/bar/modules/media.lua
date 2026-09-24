-- Media: the chip announces playback -- artwork in a ring driven by the
-- loudness, the title as the figure. The detail is the player: artwork,
-- title and artist with the spectrum, progress with a seek strip, and the
-- transport.
--
-- Port of MediaModule.qml. The spectrum follows the actual audio
-- (`services.spectrum`) and is white: colour on the bar signals a level.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local media = require("services.media")
local spectrum = require("services.spectrum")
local kit = require("components.kit")
local controls = require("components.controls")
local bars = require("components.spectrum")
local card = require("bar.controls.media_card")

local C = theme.color

modules.define("media", {
  glyph = function() return "󰎇" end,
  value = function()
    if not media.available() then return "Nothing playing" end
    local title = media.title()
    if title ~= "" then return title end
    local identity = media.identity()
    return identity ~= "" and identity or "Playing"
  end,
  has = media.available,
  runs = media.playing,
  limit = 150,
  chip = function()
    local h = theme.capsule_height()
    local art = math.floor(h * 0.62 + 0.5)
    return ui.Item {
      width = h, height = h,
      controls.ring {
        size = h, thickness = 2.5, sweep_ms = 90,
        progress = function() return media.playing() and spectrum.level:get() or 0 end,
        track_color = C.indicatorDim, fill_color = C.indicator,
        ui.Item { anchors = { center_in = true }, width = art, height = art,
          card.art { size = art, radius = art / 2, glyph_size = math.floor(art * 0.55) } },
      },
    }
  end,
  detail = function()
    local w = modules.entry("media").width
    local inner = w - 8 - 28
    local text_w = inner - 52 - 13
    local hovered = controls.signal("media.seek", false)
    local elapsed = kit.text { text = function() return media.clock(media.position()) end,
      mono = true, size = theme.size.label, color = C.textMuted }
    local length = kit.text { text = function() return media.clock(media.length()) end,
      mono = true, size = theme.size.label, color = C.textMuted }
    local track_w = inner - 2 * 40
    return ui.Item {
      anchors = { fill = true },
      ui.Column {
        anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
        gap = 10,
        ui.Row {
          gap = 13, align = "center",
          card.art { size = 52, glyph_size = 22 },
          ui.Column {
            gap = 2,
            ui.Item {
              width = text_w, height = 20,
              kit.text {
                anchors = { left = true, vertical_center = true },
                text = function()
                  local title = media.title()
                  return title ~= "" and title or media.identity()
                end,
                size = theme.size.medium, weight = 600, width = text_w - 44, elide = "right",
              },
              ui.Item { anchors = { right = true, vertical_center = true }, width = 34, height = 18,
                bars { height = 18, bar_width = 3, bars = 5, minimum = 2 } },
            },
            kit.text {
              text = function()
                local artist = media.artist()
                return artist ~= "" and artist or media.identity()
              end,
              size = theme.size.small, color = C.textMuted, width = text_w, elide = "right",
            },
          },
        },
        -- Hidden for streams, which have no length.
        ui.Item {
          width = inner, height = 20,
          visible = media.seekable,
          ui.Item { anchors = { left = true, vertical_center = true }, width = 34, height = 14, elapsed },
          ui.Item {
            x = 40, width = track_w, height = 20,
            controls.usage_bar {
              anchors = { vertical_center = true }, width = track_w,
              height = function() return hovered:get() and 6 or 4 end,
              progress = media.progress, fill_color = C.indicator,
              fill_behavior = { duration = 900, easing = "linear" },
            },
            ui.Rect {
              anchors = { vertical_center = true }, width = 10, height = 10, radius = 5,
              color = C.indicator,
              x = function() return track_w * media.progress() - 5 end,
              visible = function() return media.can_seek() and hovered:get() end,
            },
            controls.drag_area {
              width = track_w, from = 0, to = 1000, value = function() return media.progress() * 1000 end,
              enabled = media.can_seek, hovered = hovered,
              on_moved = function(value) media.seek(value / 1000) end,
            },
          },
          ui.Item { anchors = { right = true, vertical_center = true }, width = 34, height = 14,
            ui.Item { anchors = { right = true, vertical_center = true }, height = 14,
              width = function() return length.layout_width or 0 end, length } },
        },
        ui.Item {
          width = inner, height = 34,
          ui.Item { anchors = { center_in = true }, width = 34 * 3 + 20, height = 34,
            card.transport { diameter = 34, gap = 10, small = 17, large = 22 } },
        },
      },
    }
  end,
})
