-- Gallery samples for the status display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local morf = require("morf")
local ui = require("morf.ui")

local value = morf.signal("gallery.value", 0.62)
local function v() return value:get() end

return {
  badge = function(kit)
    return ui.Row { gap = 14, align = "center",
      kit.badge { count = 3 }, kit.badge { count = 42, kind = "accent" }, kit.badge { count = 128 },
      kit.badge { text = "New", kind = "ok" } }
  end,
  dot = function(kit)
    return ui.Row { gap = 10, align = "center",
      kit.dot { kind = "ok", pulse = true }, kit.dot { kind = "warn" }, kit.dot { kind = "alert", size = 14, pulse = true },
      kit.dot { kind = "info", size = 8 } }
  end,
  led = function(kit)
    return ui.Column { gap = 12,
      kit.led { kind = "ok", label = "Online" }, kit.led { kind = "warn", label = "Degraded" },
      kit.led { kind = "alert", on = false, label = "Offline" } }
  end,
  tag = function(kit)
    return ui.Column { gap = 10,
      ui.Row { gap = 8, kit.tag { text = "Stable" }, kit.tag { text = "Beta", kind = "warn" } },
      ui.Row { gap = 8, kit.tag { text = "Passing", kind = "ok", icon = "check" }, kit.tag { text = "Failed", kind = "alert" } } }
  end,
  progress = function(kit)
    return ui.Column { gap = 22,
      kit.progress { width = 260, value = v, label = "Download" },
      kit.progress { width = 260, value = function() return 0.25 end },
      kit.progress { width = 260 } }
  end,
  progress_ring = function(kit)
    return ui.Row { gap = 18, align = "center",
      kit.progress_ring { size = 110, value = v }, kit.progress_ring { size = 64, value = function() return 0.3 end, kind = "warn" } }
  end,
  semicircle = function(kit)
    return kit.semicircle { size = 220, value = v, caption = "Load" }
  end,
  segmented_progress = function(kit)
    return ui.Column { gap = 22,
      kit.segmented_progress { width = 260, value = v },
      kit.segmented_progress { width = 260, height = 16, segments = 5, value = function() return 0.4 end, kind = "ok" } }
  end,
  battery = function(kit)
    return ui.Column { gap = 16,
      kit.battery { level = v, percent = true },
      kit.battery { level = function() return 0.22 end, percent = true },
      kit.battery { level = function() return 0.8 end, charging = true, percent = true } }
  end,
  signal_bars = function(kit)
    return ui.Row { gap = 20, align = "end",
      kit.signal_bars { level = v, size = 40 }, kit.signal_bars { active = 1, size = 28, kind = "warn" },
      kit.signal_bars { level = function() return 1 end, bars = 5, size = 28 } }
  end,
  status_card = function(kit)
    return ui.Column { gap = 12,
      kit.status_card { kind = "warn", title = "Disk almost full", detail = "92% of /home is in use", width = 260 },
      kit.status_card { kind = "ok", title = "All systems normal", detail = "Last check a minute ago", width = 260 } }
  end,
  banner = function(kit)
    return ui.Column { gap = 12,
      kit.banner { kind = "info", title = "Update ready", text = "Restart to finish installing", width = 260 },
      kit.banner { kind = "alert", title = "Connection lost", text = "Retrying in 5 seconds", width = 260 } }
  end,
  empty_state = function(kit)
    return kit.empty_state { icon = "inbox", title = "No notifications", text = "You're all caught up.",
      width = 260, height = 190 }
  end,
  result_page = function(kit)
    return kit.result_page { kind = "success", title = "Backup complete", text = "1,284 files copied to the archive.",
      width = 260, height = 190 }
  end,
  skeleton = function(kit)
    return ui.Column { gap = 24,
      kit.skeleton { width = 260, lines = 2, avatar = true },
      kit.skeleton { width = 260, lines = 3 } }
  end,
  toast = function(kit)
    return ui.Column { gap = 14,
      kit.toast { title = "Screenshot saved", text = "~/Pictures/shot.png", icon = "screenshot_monitor", width = 260 },
      kit.toast { title = "Battery low", text = "15% remaining", kind = "warn", width = 260 } }
  end,
}
