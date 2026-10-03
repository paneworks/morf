-- Gallery samples for the Plane widgets (lib.kit.samples): each one as a
-- configuration would make it, at a value that shows its look.
local S = {}

S.colour_plane = function(_, w) return w.colour_plane { width = 180, height = 180, x = 0.7, y = 0.25, hue = 210 } end
S.hue_wheel = function(_, w) return w.hue_wheel { width = 180, height = 180, x = 0.58, y = 1 } end
S.xy_pad = function(_, w) return w.xy_pad { width = 180, height = 180, x = 0.66, y = 0.7 } end
S.pan_pad = function(_, w) return w.pan_pad { width = 180, height = 180, x = -0.42, y = 0.4 } end
S.envelope_point = function(_, w) return w.envelope_point { width = 230, height = 160, x = 0.3, y = 0.82 } end
S.joystick = function(_, w) return w.joystick { width = 160, height = 160, x = 0.45, y = 0.3 } end
S.minimap_viewport = function(_, w) return w.minimap_viewport { width = 150, height = 200, x = 0.4, y = 0.35 } end
S.crop_handle = function(_, w) return w.crop_handle { width = 230, height = 170, x = 0.8, y = 0.82 } end

return S
