-- Gallery samples for the aviation instruments: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")

--- `node` (`w` by `h`) centred in the 260x190 cell.
local function centre(node, w, h)
  node.x, node.y = math.floor((260 - w) / 2), math.floor((190 - h) / 2)
  return ui.Item { width = 260, height = 190, node }
end

local ROUTE = {
  { x = 4, y = 12, name = "ODINA" }, { x = -3, y = 22, name = "KOPRI" }, { x = 6, y = 33, name = "LJU" },
  { x = 18, y = 36, name = "VEBAL" },
}

--- A storm ahead: rows of range bins, nearest first, each across the arc.
local function storm()
  local rows = {}
  for r = 1, 9 do
    local row = {}
    for c = 1, 16 do
      local d = math.sqrt(((c - 11) / 3.2) ^ 2 + ((r - 6) / 2.2) ^ 2)
      local e = math.sqrt(((c - 4) / 2.4) ^ 2 + ((r - 4) / 1.6) ^ 2)
      row[c] = math.max(0, 1 - d * .55, .7 - e * .5)
    end
    rows[r] = row
  end
  return rows
end

return {
  airspeed_tape = function(kit)
    return centre(kit.airspeed_tape { width = 96, height = 186, speed = 142 }, 96, 186)
  end,
  altimeter_tape = function(kit)
    return centre(kit.altimeter_tape { width = 112, height = 186, altitude = 12340 }, 112, 186)
  end,
  attitude_indicator = function(kit)
    return centre(kit.attitude_indicator { size = 184, pitch = 6, roll = -14 }, 184, 184)
  end,
  heading_indicator = function(kit)
    return centre(kit.heading_indicator { size = 184, heading = 274 }, 184, 184)
  end,
  course_deviation = function(kit)
    return centre(kit.course_deviation { width = 250, height = 120, deviation = -.36, to_from = "to", course = 91 }, 250, 120)
  end,
  eicas_strip = function(kit)
    return centre(kit.eicas_strip { width = 250, height = 186, params = {
      { label = "N1", value = 92.4, min = 0, max = 110, warn = 100, alert = 105 },
      { label = "EGT", value = 712, min = 0, max = 1000, warn = 800, alert = 900 },
      { label = "N2", value = 101.2, min = 0, max = 110, warn = 100, alert = 105 },
      { label = "FF", value = 2.8, min = 0, max = 6, warn = 5, alert = 5.6 },
      { label = "Oil", value = 46, min = 0, max = 100, warn = 80, alert = 90 },
    } }, 250, 186)
  end,
  flight_path_marker = function(kit)
    return centre(kit.flight_path_marker { width = 240, height = 176, drift = 3.2, path = -2.5 }, 240, 176)
  end,
  heading_tape = function(kit)
    return centre(kit.heading_tape { width = 250, height = 66, heading = 274 }, 250, 66)
  end,
  hsi = function(kit)
    return centre(kit.hsi { width = 260, height = 186, heading = 274, course = 300, deviation = .4, to_from = "to" }, 260, 186)
  end,
  nav_display = function(kit)
    return centre(kit.nav_display { width = 260, height = 188, heading = 20, track = 24, range = 40, waypoints = ROUTE }, 260, 188)
  end,
  pitch_ladder = function(kit)
    return centre(kit.pitch_ladder { width = 220, height = 186, pitch = 8, roll = 12 }, 220, 186)
  end,
  radar_altimeter = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.radar_altimeter { x = 0, y = 0, width = 260, height = 88, altitude = 1240, threshold = 200 },
      kit.radar_altimeter { x = 0, y = 100, width = 260, height = 88, altitude = 184, threshold = 200 },
    }
  end,
  range_rings = function(kit)
    return centre(kit.range_rings { size = 186, range = 40, rings = 4, heading = 20 }, 186, 186)
  end,
  bank_scale = function(kit)
    return centre(kit.bank_scale { width = 240, height = 150, roll = 22 }, 240, 150)
  end,
  rolling_digits = function(kit)
    return ui.Column { gap = 12,
      kit.rolling_digits { value = 12345.4, digits = 5, size = 34, label = "Odometer" },
      kit.rolling_digits { value = 4220, digits = 5, roll = 2, step = 20, size = 26 },
    }
  end,
  turn_coordinator = function(kit)
    return centre(kit.turn_coordinator { size = 184, rate = 3, slip = .35 }, 184, 184)
  end,
  vertical_speed = function(kit)
    return centre(kit.vertical_speed { size = 184, fpm = 850, max = 2000 }, 184, 184)
  end,
  weather_radar = function(kit)
    return centre(kit.weather_radar { width = 260, height = 188, cells = storm(), range = 40 }, 260, 188)
  end,
}
