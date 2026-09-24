-- The island at rest: the clock in the middle, and a recording, countdown
-- or track beside it while one runs -- one activity split across both
-- sides (mark leading, figure trailing), or one per side.
--
-- Port of IslandRest.qml. It replaces the clock-only "modules" layer
-- init.lua registers (the auto-loader runs after it). The width is
-- `modules.rest_width()`: the catalogue's clock alone, narrower clock and
-- two slots with activities.
--
-- The recording and the countdown belong to other parts of the port: they
-- plug in through `modules.define` with `runs`, `mark(hovered)`, `figure()`
-- and, for the recording, `stop()` -- a recording stops with one click
-- here, since nothing else on screen can, and its dot squares off into a
-- stop button under the pointer. The countdown's ring and figure and the
-- track's artwork and spectrum are built here when their modules bring
-- none. A side under the pointer holds the glance off (`island.rest_busy`),
-- so a click on the dot never lands on a glance that opened under it.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local modules = require("services.modules")
local clock = require("bar.modules.clock")
local kit = require("components.kit")
local bars = require("components.spectrum")
local ring = require("components.ring_indicator")
local card = require("bar.controls.media_card")

local C = theme.color

local ACTIVITIES = { "recorder", "timer", "media" }

local ok_timer, timer = pcall(require, "services.timer")
if not ok_timer then timer = nil end

-- The parts built here when a module brings none.
local built_in = {
  media = {
    mark = function() return card.art { size = 20, glyph_size = 11 } end,
    figure = function() return bars { height = 14, bar_width = 2, bars = 6 } end,
  },
}
if timer then
  built_in.timer = {
    mark = function()
      return ring { size = 16, thickness = 2, progress = timer.progress,
        track_color = C.indicatorDim, fill_color = timer.tint }
    end,
    figure = function()
      return kit.text {
        text = timer.display, mono = true, size = theme.size.small, weight = 600,
        color = function() return timer.paused() and C.textMuted() or C.text() end,
      }
    end,
  }
end

local function part(id, which, hovered)
  local provider = modules.providers[id] or {}
  local build = provider[which] or (built_in[id] and built_in[id][which])
  if build then return build(hovered) end
  return ui.Item {}
end

--- One side. `activity()` is the id it shows, `what()` "mark", "figure"
--- or "both". Returns the node and its pointer area.
local function segment(activity, what, anchor)
  local area
  local hovered = function() return area ~= nil and area.hovered or false end
  -- One loader per activity and per way of showing it; one at most is
  -- active, and it builds only the parts it shows.
  local slots = { anchors = { fill = true } }
  for _, id in ipairs(ACTIVITIES) do
    for _, mode in ipairs { "mark", "figure", "both" } do
      local shows = function() return activity() == id and what() == mode end
      slots[#slots + 1] = ui.Loader {
        anchors = { center_in = true },
        active = shows,
        source = function()
          local row = { gap = 7, align = "center" }
          if mode ~= "figure" then row[#row + 1] = part(id, "mark", hovered) end
          if mode ~= "mark" then row[#row + 1] = part(id, "figure", hovered) end
          return ui.Row(row)
        end,
      }
    end
  end
  area = ui.MouseArea {
    anchors = { fill = true }, cursor = "pointer",
    on_clicked = function()
      local id = activity()
      local provider = modules.providers[id] or {}
      if id == "recorder" and provider.stop then provider.stop()
      else modules.activate(id) end
    end,
  }
  return ui.Item {
    anchors = anchor,
    width = function() return modules.activity_side() end,
    height = function() return theme.capsule_height() end,
    visible = function() return activity() ~= "" end,
    ui.Item(slots),
    area,
  }, area
end

island.register_layer("modules", {
  size = function() return modules.rest_width(), theme.capsule_height(), 0 end,
  build = function()
    local list = function() return modules.activities() end
    local split = function() return #list() == 1 end
    -- The time, and the date beside it smaller and muted, as the clock
    -- module draws it.
    local face = clock.face()
    local time = ui.Item {
      anchors = { center_in = true },
      width = function() return face.layout_width or 0 end,
      height = function() return face.layout_height or 16 end,
      face,
    }
    local leading, leading_area = segment(function() return list()[1] or "" end,
      function() return split() and "mark" or "both" end,
      { left = true, vertical_center = true })
    local trailing, trailing_area = segment(function()
        local l = list()
        if #l == 1 then return l[1] end
        return l[2] or ""
      end,
      function() return split() and "figure" or "both" end,
      { right = true, vertical_center = true })
    local node = ui.Item {
      anchors = { fill = true },
      on_destroyed = function() island.rest_busy:set(false) end,
      time, leading, trailing,
    }
    morf.effect("impasto.rest.busy", function()
      local busy = (leading.visible and leading_area.hovered) or (trailing.visible and trailing_area.hovered)
      island.rest_busy:set(busy and true or false)
    end, { owner = node })
    return node
  end,
})
