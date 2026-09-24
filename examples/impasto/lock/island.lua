-- The island on the lock: the padlock, and the face scan round it.
--
-- Port of LockIsland.qml. The bar's island, where the bar has it and at its
-- resting size, holding a padlock. While the camera looks it grows with the
-- ring round the padlock; a match closes the ring and opens the padlock, a
-- miss turns both red and shakes the island. As the lock lets go the
-- padlock gives way to the bar's own time, so the island left on screen is
-- the bar's.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local lock = require("services.lock")
local clock = require("bar.modules.clock")
local kit = require("components.kit")
local padlock = require("lock.padlock")
local face_ring = require("lock.face_ring")

local C = theme.color
local st = lock.state

--- `screen_width()` is the width to centre in; `held()` fades the padlock.
return function(values)
  local screen_width = values.screen_width
  local held = values.held

  -- Red for as long as the island shakes, then back to rest.
  local missed = morf.signal("impasto.lock.island.missed", false)

  local attached = function() return settings.islandAttached end
  local notch_pad = function() return attached() and theme.bar_top_margin() or 0 end
  local open = function()
    return not st.leaving and (st.face_scanning or st.face_matched or missed:get())
  end
  local width = function()
    if open() then return 176 end
    return settings.clockShowsDate and 240 or 150
  end
  local height = function()
    return (open() and 124 or theme.capsule_height()) + notch_pad()
  end

  local ring = face_ring {
    diameter = 72,
    anchors = { center_in = true },
    scanning = function() return st.face_scanning end,
    closed = function() return st.face_matched end,
    hue = function()
      if st.face_matched then return C.indicatorGood end
      if missed:get() then return C.indicatorBad end
      return C.text()
    end,
    shown = open,
  }

  local lock_glyph = padlock {
    anchors = { center_in = true },
    scale = function() return open() and 1.5 or 0.8 end,
    opacity = held,
    opened = function() return st.face_matched or st.leaving end,
    tint = function() return missed:get() and C.indicatorBad or C.text() end,
    behavior = { scale = { duration = math.max(1, theme.duration_morph()), easing = "out_back" } },
  }

  -- The bar's time, drawn as the bar's clock draws it, arriving as the
  -- padlock goes.
  local time = kit.text {
    anchors = { center_in = true },
    text = function()
      local text = clock.text()
      if settings.clockShowsDate then text = text .. "   " .. morf.time.format("%a %-d %b") end
      return text
    end,
    size = theme.size.regular, weight = 600,
    opacity = function() return 1 - held() end,
  }

  local body = ui.Rect {
    x = function() return (screen_width() - width()) / 2 end,
    y = function() return attached() and 0 or theme.bar_top_margin() end,
    width = width,
    height = height,
    color = C.island,
    radius = function()
      if open() then return 2 * theme.radius_large + 8 end
      return math.min(height() / 2, theme.radius_large + 4)
    end,
    top_left_radius = function() return attached() and 0 or -1 end,
    top_right_radius = function() return attached() and 0 or -1 end,
    behavior = {
      x = theme.behave("morph"),
      width = theme.behave("morph"),
      height = theme.behave("morph"),
      radius = theme.behave("morph"),
    },
    -- Centred as the bar centres its own: half the notch's pad above, half
    -- below.
    ui.Item {
      anchors = function()
        local pad = notch_pad() / 2
        return { fill = true, top_margin = pad, bottom_margin = pad }
      end,
      ring, lock_glyph, time,
    },
  }

  -- A miss: red, and two shakes.
  local seen = st.face_missed
  morf.effect("impasto.lock.island.miss", function()
    local count = st.face_missed
    if count == seen then return end
    seen = count
    missed:set(true)
    morf.timer(math.max(1, theme.duration_morph() * 2), function() missed:set(false) end, false)
    morf.animation.play {
      loops = 2,
      { node = body, property = "translate_x", to = -10, duration = 55, easing = "out_cubic" },
      { node = body, property = "translate_x", to = 10, duration = 55, easing = "out_cubic" },
      { node = body, property = "translate_x", to = 0, duration = 55, easing = "out_cubic" },
    }
  end)

  return body
end
