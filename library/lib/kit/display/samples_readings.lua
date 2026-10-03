-- Gallery samples for the readings display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")

local function trend()
  local t = {}
  for i = 1, 48 do t[i] = 40 + 22 * math.sin(i / 5) + 9 * math.sin(i / 1.7) end
  return t
end

return {
  thermometer = function(kit)
    return kit.thermometer { width = 200, height = 186, value = .58, label = "CPU temp", unit = "°C", from = 20, to = 100 }
  end,
  tank = function(kit)
    return ui.Row { gap = 20,
      kit.tank { width = 120, height = 186, value = .64, label = "Water" },
      kit.tank { width = 120, height = 186, value = .22, label = "Fuel", color = "warn" },
    }
  end,
  led_bar = function(kit)
    return ui.Column { gap = 18,
      kit.led_bar { width = 260, height = 22, value = .82, count = 12, label = "Output" },
      kit.led_bar { width = 260, height = 14, value = .45, count = 16 },
      kit.led_bar { width = 260, height = 14, value = .95, count = 16 },
    }
  end,
  seven_segment = function(kit)
    return ui.Column { gap = 10,
      kit.seven_segment { text = function() return "12:45" end, digits = 4, size = 52, label = "Clock" },
      kit.seven_segment { text = function() return "-3.14" end, digits = 4, size = 24 },
    }
  end,
  vu_meter = function(kit)
    return kit.vu_meter { width = 260, height = 160, value = .68, label = "VU" }
  end,
  peak_meter = function(kit)
    return ui.Column { gap = 14,
      kit.peak_meter { width = 260, height = 14, value = .55, peak = .78, label = "Left" },
      kit.peak_meter { width = 260, height = 14, value = .72, peak = .93, label = "Right" },
      kit.peak_meter { width = 260, height = 10, value = .3 },
    }
  end,
  compass = function(kit)
    -- (The card turns, and the gallery measures a turned path by its
    -- turned box: 160 square keeps that inside the cell.)
    return ui.Item { width = 260, height = 190,
      kit.compass { x = 50, y = 15, size = 160, heading = 247, label = "Hdg" } }
  end,
  sparkline = function(kit)
    return ui.Column { gap = 16,
      kit.sparkline { width = 260, height = 84, values = trend, label = "Network" },
      kit.sparkline { width = 260, height = 56, values = trend(), area = false, color = "info" },
    }
  end,
  segmented_meter = function(kit)
    return ui.Column { gap = 16,
      kit.segmented_meter { width = 260, height = 20, value = .62, segments = 12, label = "Battery" },
      kit.segmented_meter { width = 260, height = 12, value = .3, segments = 20, color = "ok" },
      kit.segmented_meter { width = 260, height = 12, value = .9, segments = 20, color = "alert" },
    }
  end,
}
