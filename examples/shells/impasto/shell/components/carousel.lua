-- A horizontal strip of tiles with the current one centred and enlarged
-- (Carousel and CarouselTile).
--
-- The original animated one shared `position` and derived every tile from
-- its distance to it. Here each tile is placed from its distance to the
-- current index and eases there on the morph curve, which is the same
-- picture: a step moves every tile at once, and a step taken while they are
-- moving retargets them instead of queueing. Clicking the centred tile (or
-- Enter, from the caller) activates it; any other tile scrolls to it.

local ui = require("morf.ui")
local theme = require("theme")
local controls = require("components.controls")

local M = {}

--- `width`, `height`, `tile_width` (160), `tile_height` (100),
--- `centre_scale` (1.25), `gap` (12), `reach` (3), `model` (a list model
--- whose rows carry `index`), `current` (a signal), `tile(row, index(),
--- centred(), distance(), hovered)` building a tile's contents,
--- `on_activated(index)`.
function M.new(values)
  local tile_w = values.tile_width or 160
  local tile_h = values.tile_height or 100
  local centre = values.centre_scale or 1.25
  local gap = values.gap or 12
  local reach = values.reach or 3
  local current = values.current
  local width, height = values.width, values.height
  local first_step = tile_w * centre / 2 + gap + tile_w / 2
  local stride = tile_w + gap

  local function offset_of(distance)
    local away = math.abs(distance)
    local offset = away <= 1 and away * first_step or first_step + (away - 1) * stride
    return distance < 0 and -offset or offset
  end
  local function scale_of(distance)
    return 1 + (centre - 1) * math.max(0, 1 - math.abs(distance))
  end

  local strip = {}
  function strip.count() return values.model:len() end
  function strip.go_to(index)
    local count = strip.count()
    if count == 0 then return end
    current:set(math.max(0, math.min(count - 1, index)))
  end
  function strip.step(delta) strip.go_to(current:get() + delta) end

  local turned = 0
  strip.node = ui.ClipRect {
    width = width, height = height, color = "#00000000",
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_wheel = function(_, _, px, py, sx, sy)
        local steps = (sx ~= 0 and sx) or sy or 0
        if steps ~= 0 then strip.step(steps > 0 and 1 or -1) return end
        turned = turned + ((px ~= 0 and px) or py or 0)
        while math.abs(turned) >= 60 do
          strip.step(turned > 0 and 1 or -1)
          turned = turned - 60 * (turned > 0 and 1 or -1)
        end
      end,
    },
    ui.Repeater {
      model = values.model,
      delegate = function(row)
        local index = controls.signal("carousel.index", row.index or 0)
        local hovered = controls.signal("carousel.hover", false)
        local distance = function() return index:get() - current:get() end
        local centred = function() return index:get() == current:get() end
        local motion = theme.behave("morph")
        local node = ui.Item {
          width = tile_w, height = tile_h,
          x = function() return (width - tile_w) / 2 + offset_of(distance()) end,
          y = (height - tile_h) / 2,
          scale = function() return scale_of(distance()) end,
          z = function() return -math.abs(distance()) end,
          visible = function() return math.abs(distance()) < reach end,
          behavior = { x = motion, scale = motion },
          values.tile(row, function() return index:get() end, centred, distance, hovered),
          ui.MouseArea {
            anchors = { fill = true }, z = 1, cursor = "pointer",
            on_entered = function() hovered:set(true) end,
            on_exited = function() hovered:set(false) end,
            on_clicked = function()
              if centred() then
                if values.on_activated then values.on_activated(index:get()) end
              else
                strip.go_to(index:get())
              end
            end,
          },
        }
        return node, function(next) index:set(next.index or 0) end
      end,
    },
  }
  return strip
end

return M
