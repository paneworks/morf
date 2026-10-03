-- Gallery samples for the Range widgets (lib.kit.samples): each one as a
-- configuration would make it, at a value that shows its look.
local S = {}

S.slider = function(_, w) return w.slider { width = 240, value = 0.42 } end
S.vertical_slider = function(_, w) return w.vertical_slider { width = 48, height = 200, value = 0.6 } end
S.range_slider = function(_, w) return w.range_slider { width = 240, first = 0.25, second = 0.7 } end
S.discrete_slider = function(_, w) return w.discrete_slider { width = 240, value = 6 } end
S.log_slider = function(_, w) return w.log_slider { width = 260, value = 1200 } end
S.angle_slider = function(_, w) return w.angle_slider { value = 135 } end
S.knob = function(_, w) return w.knob { value = 0.68 } end
S.bipolar_knob = function(_, w) return w.bipolar_knob { value = -0.35 } end
S.stepped_knob = function(_, w) return w.stepped_knob { value = 4 } end
S.fader = function(_, w) return w.fader { value = 0.62 } end
S.spin_button = function(_, w) return w.spin_button { value = 42 } end
S.scrubber = function(_, w) return w.scrubber { width = 260, value = 0.38, duration = 214 } end
S.scroll_bar = function(_, w)
  return w.scroll_bar { width = 12, height = 200, orientation = "vertical", value = 0.3, size = 0.35 }
end
S.seek_bar = function(_, w) return w.seek_bar { width = 240, value = 0.35 } end
S.volume = function(_, w) return w.volume { width = 260, value = 0.72 } end
S.brightness = function(_, w) return w.brightness { width = 260, value = 0.45 } end
S.zoom = function(_, w) return w.zoom { width = 270, value = 1.5 } end
S.rating = function(_, w) return w.rating { value = 3 } end
S.level_control = function(_, w) return w.level_control { width = 250, value = 0.58 } end
S.osd_level = function(_, w) return w.osd_level { width = 260, value = 0.64, icon = "volume_up" } end

return S
