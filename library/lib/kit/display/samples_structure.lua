-- Gallery samples for the structure display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")

return {
  separator = function(kit)
    return ui.Column { gap = 12,
      kit.text { text = "Wi-Fi", width = 260 },
      kit.separator { width = 260 },
      kit.text { text = "Bluetooth", width = 260 },
      kit.separator { width = 260 },
      ui.Row { gap = 14, align = "center", kit.text { text = "Left" }, kit.separator { vertical = true, height = 40 },
        kit.text { text = "Right" } } }
  end,
  spacer = function(kit)
    return ui.Column { gap = 0,
      ui.Row { kit.tag { text = "Start" }, kit.spacer { width = 96 }, kit.tag { text = "End", kind = "ok" } },
      kit.spacer { height = 40 },
      kit.tag { text = "After 40 px", kind = "info" } }
  end,
  group_box = function(kit)
    return kit.group_box { title = "Network", width = 260, height = 170,
      ui.Column { gap = 14,
        kit.led { kind = "ok", label = "Ethernet" },
        kit.led { kind = "warn", label = "Wi-Fi" },
        kit.led { kind = "alert", on = false, label = "VPN" } } }
  end,
  labelled_divider = function(kit)
    return ui.Column { gap = 26,
      kit.labelled_divider { text = "Today", width = 260 },
      kit.labelled_divider { text = "Yesterday", width = 260 },
      kit.labelled_divider { text = "Older", width = 260, align = "start" } }
  end,
  frame = function(kit)
    return kit.frame { title = "Frame", width = 260, height = 170,
      kit.text { text = "Content sits inside the frame, below its title.", width = 230, wrap = true } }
  end,
  inset = function(kit)
    return kit.frame { width = 260, height = 150,
      kit.inset { margins = 18,
        kit.text { text = "Inset by 18 px", width = 200 },
        kit.tag { text = "Padded" } } }
  end,
}
