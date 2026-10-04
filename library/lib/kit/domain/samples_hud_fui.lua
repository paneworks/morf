-- Gallery samples for the hud_fui instruments: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")
local morf = require("morf")

return {
  tick_ruler = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.tick_ruler { width = 260, value = .62, label = "Heading", from = 0, to = 360, majors = 4 },
      kit.tick_ruler { y = 76, vertical = true, height = 114, value = .4, from = -40, to = 40, majors = 2 },
      kit.tick_ruler { x = 80, y = 104, width = 180, from = 0, to = 10, majors = 5, minor = 4 },
    }
  end,
  radar_sweep = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.radar_sweep { x = 40, y = 2, size = 186, blips = { { 40, .6 }, { 130, .35, "alert" }, { 220, .8 }, { 300, .5, "ok" } } } }
  end,
  segmented_arc_ring = function(kit)
    return ui.Row { gap = 10,
      kit.segmented_arc_ring { size = 150, value = .68, label = "Power" },
      ui.Column { gap = 10, y = 20,
        kit.segmented_arc_ring { size = 80, value = .3, segments = 12, sweep = 360, text = function() return "30" end, color = "warn" },
        kit.segmented_arc_ring { size = 80, value = .9, segments = 12, sweep = 270, text = function() return "90" end, color = "alert" } } }
  end,
  target_lock = function(kit)
    return ui.Row { gap = 16,
      kit.target_lock { width = 122, height = 186, label = "T-04" },
      kit.target_lock { width = 122, height = 186, locked = true, label = "T-07" } }
  end,
  scan_sweep = function(kit)
    return kit.scan_sweep { width = 260, height = 186 }
  end,
  waveform_rings = function(kit)
    return ui.Row { gap = 14,
      kit.waveform_rings { size = 160, level = .7 },
      kit.waveform_rings { size = 86, y = 37, level = .2, rings = 3 } }
  end,
  concentric_rings = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.concentric_rings { x = 35, y = 0, size = 188, text = function() return "07" end } }
  end,
  decode_text = function(kit)
    return ui.Column { gap = 12,
      kit.decode_text { text = "Access granted", every = 4000 },
      kit.decode_text { text = "Uplink established", font_size = 16, every = 5200 },
      kit.decode_text { text = "Node 0x3FA9", font_size = 14, every = 3600, color = "warn" } }
  end,
  glitch_text = function(kit)
    return ui.Column { gap = 14,
      kit.glitch_text { text = "System breach", font_size = 26 },
      kit.glitch_text { text = "Signal lost", font_size = 18, every = 1800, color = "alert" },
      kit.glitch_text { text = "Rerouting", font_size = 14, every = 3100 } }
  end,
  hex_grid = function(kit)
    return kit.hex_grid { width = 260, height = 186 }
  end,
  countdown_ring = function(kit)
    return ui.Row { gap = 12,
      kit.countdown_ring { size = 170, total = 60, start = 42, loop = true, label = "Launch" },
      kit.countdown_ring { size = 78, y = 46, total = 30, seconds = 2, ticks = 30, label = "Seconds" } }
  end,
  dot_matrix_progress = function(kit)
    return ui.Column { gap = 16,
      kit.dot_matrix_progress { width = 260, value = .58, label = "Download" },
      kit.dot_matrix_progress { width = 260, columns = 40, rows = 3, value = .86, label = "Sync", color = "ok" } }
  end,
  biometric_scan = function(kit)
    return ui.Row { gap = 10,
      kit.biometric_scan { width = 125, height = 186, value = .64 },
      kit.biometric_scan { width = 125, height = 186, value = 1 } }
  end,
  data_stream = function(kit)
    return kit.data_stream { width = 260, height = 186 }
  end,
  striped_loading = function(kit)
    return ui.Column { gap = 18,
      kit.striped_loading { width = 260, label = "Loading assets" },
      kit.striped_loading { width = 260, value = .64, label = "Decrypting", color = "warn" },
      kit.striped_loading { width = 260, height = 8, value = .3, label = "Handshake", color = "info" } }
  end,
  signal_noise = function(kit)
    return kit.signal_noise { width = 260, height = 186 }
  end,
  crt_scanlines = function(kit)
    return kit.crt_scanlines { width = 260, height = 186,
      ui.Column { x = 18, y = 18, gap = 4,
        kit.decode_text { text = "Sys online", font_size = 22 },
        kit.text { text = "Core temp nominal", font_size = 14 },
        kit.text { text = "Reactor 98%  Hull 100%", font_size = 14 } },
      kit.striped_loading { x = 18, y = 140, width = 224, value = .7 } }
  end,
  crosshair_grid = function(kit)
    return kit.crosshair_grid { width = 260, height = 186, at = { .42, .38 } }
  end,
  callout = function(kit)
    return kit.callout { width = 260, height = 186, title = "Contact", value = "Hostile 2.4 km" }
  end,
  wireframe = function(kit)
    return ui.Row { gap = 10,
      kit.wireframe { size = 170 },
      kit.wireframe { size = 80, y = 50, shape = "cube", period = 6000 } }
  end,
  telemetry_block = function(kit)
    local t = morf.signal("gallery.hud_fui.telemetry", 0)
    return ui.Item { width = 260, height = 160,
      kit.telemetry_block { width = 260, title = "Vessel",
        rows = {
          { "Velocity", function() return ("%.1f km/s"):format(7.6 + (t:get() % 5) * .1) end },
          { "Altitude", "412 km" },
          { "Fuel", function() return ("%d%%"):format(64 - t:get() % 3) end },
          { "Hull", "Nominal" },
          { "Crew", "6" },
        } },
      ui.Timer { interval = 1500, ["repeat"] = true, running = true, on_triggered = function() t:set(t:get() + 1) end } }
  end,
  motion_tracker = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.motion_tracker { x = 10, y = 30, width = 240, range = 30, label = "Tracker",
        pings = { { -40, .55 }, { 10, .8 }, { 55, .35 } } } }
  end,
  proximity_ring = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.proximity_ring { x = 36, y = 0, size = 188, values = { .1, 0, 0, .3, .55, .9, .4, 0, 0, 0, .2, .05 } } }
  end,
  bracket_tag = function(kit)
    return ui.Column { gap = 10,
      kit.bracket_tag { text = "Target acquired" },
      kit.bracket_tag { text = "Shields up", color = "ok" },
      kit.bracket_tag { text = "Low fuel", color = "warn", blink = false },
      kit.bracket_tag { text = "Hostile", color = "alert" },
      kit.bracket_tag { text = "Sector 7G", color = "info", height = 22, blink = false } }
  end,
  orbit_diagram = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.orbit_diagram { x = 35, y = 0, size = 188 } }
  end,
  starfield = function(kit)
    return kit.starfield { width = 260, height = 186 }
  end,
  assistant_orb = function(kit)
    return ui.Row { gap = 14,
      kit.assistant_orb { size = 160, level = .6 },
      kit.assistant_orb { size = 80, y = 60, level = .2, label = false } }
  end,
}
