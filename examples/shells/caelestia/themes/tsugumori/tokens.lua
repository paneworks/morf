-- Independent visual design, using the same semantic palette as Material.
local base = require("themes.material.tokens")
local M = {}
for key, value in pairs(base) do M[key] = value end
M.font = "IBM Plex Mono, JetBrains Mono, monospace"
M.auth_font = M.font
M.key_radius = 0
M.mono = M.font
M.typography = { title = 16, section = 14, caption = 11, hero = 24, subtitle = 12, label = 11, menu = 13 }
M.BORDER, M.LEFT, M.ROUNDING, M.SEAM = 10, 10, 2, 0
M.duration = { small = 140, normal = 240, large = 400, drawer_open = 280, drawer_close = 180 }
M.ease = {
  standard = "out_cubic", standard_decel = "out_cubic", standard_accel = "in_cubic",
  emphasized_decel = "out_cubic", emphasized_accel = "in_cubic",
  spatial = "out_cubic", emphasized = "in_out_cubic",
}
return M
