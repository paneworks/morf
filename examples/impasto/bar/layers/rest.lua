-- The island at rest: the clock in the middle, and a recording, countdown
-- or track beside it while one runs -- one activity split across both
-- sides (mark leading, figure trailing), or one per side.
--
-- Port of IslandRest.qml. It replaces the clock-only "modules" layer
-- init.lua registers (the auto-loader runs after it). The width is
-- `modules.rest_width()`: the catalogue's clock alone, narrower clock and
-- two slots with activities.
--
-- The track is built here. The recording and the countdown belong to other
-- parts of the port: they plug in through `modules.define` with `runs`,
-- `mark()`, `figure()` and, for the recording, `stop()` -- a recording
-- stops with one click here, since nothing else on screen can stop it.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
local modules = require("services.modules")
local clock = require("bar.modules.clock")
local kit = require("components.kit")
local controls = require("components.controls")
local bars = require("components.spectrum")
local card = require("bar.controls.media_card")

local ACTIVITIES = { "recorder", "timer", "media" }

-- The track's two halves: the artwork, and the real spectrum.
local built_in = {
  media = {
    mark = function() return card.art { size = 20, glyph_size = 11 } end,
    figure = function() return bars { height = 14, bar_width = 2, bars = 6 } end,
  },
}

local function part(id, which)
  local provider = modules.providers[id] or {}
  local build = provider[which] or (built_in[id] and built_in[id][which])
  if build then return build() end
  return ui.Item {}
end

--- One side. `activity()` is the id it shows, `part` "mark", "figure" or
--- "both".
local function segment(activity, what, anchor)
  local hovered = controls.signal("rest.segment", false)
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
          if mode ~= "figure" then row[#row + 1] = part(id, "mark") end
          if mode ~= "mark" then row[#row + 1] = part(id, "figure") end
          return ui.Row(row)
        end,
      }
    end
  end
  return ui.Item {
    anchors = anchor,
    width = function() return modules.activity_side() end,
    height = function() return theme.capsule_height() end,
    visible = function() return activity() ~= "" end,
    ui.Item(slots),
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      -- A side under the pointer holds the glance off.
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        local id = activity()
        local provider = modules.providers[id] or {}
        if id == "recorder" and provider.stop then provider.stop()
        else modules.activate(id) end
      end,
    },
  }
end

island.register_layer("modules", {
  size = function() return modules.rest_width(), theme.capsule_height(), 0 end,
  build = function()
    local list = function() return modules.activities() end
    local split = function() return #list() == 1 end
    local time = kit.text {
      anchors = { center_in = true },
      text = function()
        local text = clock.text()
        if settings.clockShowsDate then text = text .. "   " .. morf.time.format("%a %-d %b") end
        return text
      end,
      size = theme.size.regular, weight = 600,
    }
    return ui.Item {
      anchors = { fill = true },
      time,
      segment(function() return list()[1] or "" end,
        function() return split() and "mark" or "both" end,
        { left = true, vertical_center = true }),
      segment(function()
          local l = list()
          if #l == 1 then return l[1] end
          return l[2] or ""
        end,
        function() return split() and "figure" or "both" end,
        { right = true, vertical_center = true }),
    }
  end,
})
