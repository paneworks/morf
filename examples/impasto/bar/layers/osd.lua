-- The OSD layer: a glyph, a level and a figure, for a moment.
--
-- Port of OsdLayer.qml. What changed (volume, brightness, a toggle) is sent
-- through `island_state.flash(icon, label, progress)`; the level bar shows
-- only when there is a level.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")

local C = theme.color
local s = island.state.signals

island.register_layer("osd", {
  size = function() return 260, theme.capsule_height(), 10 end,
  build = function()
    local has_level = function() return s.osd_progress:get() >= 0 end
    return ui.Item { anchors = { fill = true },
      ui.Row {
        anchors = { center_in = true },
        gap = 12, align = "center",
        kit.glyph { glyph = function() return s.osd_icon:get() end, size = 15, color = C.indicator, width = 18 },
        ui.Item {
          width = function() return has_level() and 130 or 0 end,
          height = 4,
          ui.Rect {
            width = 130, height = 4, radius = 2, color = C.islandSurfaceHover,
            visible = has_level,
            ui.Rect {
              height = 4, radius = 2, color = C.accent,
              width = function()
                local p = math.max(0, math.min(1, s.osd_progress:get()))
                return math.max(4, 130 * p)
              end,
              behavior = { width = theme.behave("fast") },
            },
          },
        },
        kit.text {
          text = function() return s.osd_label:get() end,
          width = 34, horizontal_alignment = "right",
          size = theme.size.small, weight = 600,
        },
      },
    }
  end,
})
