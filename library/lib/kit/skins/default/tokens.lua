-- The default kit's tokens: Adwaita's palette in light, dark and high
-- contrast, sharing every name, and the type, shape and motion the skins
-- read. Nothing here draws.
--
-- The palettes hold libadwaita's named colours (contract.lua, `tokens`)
-- and a few of the default look's own: `dim_fg` secondary text, `info_bg`
-- and `info`, `extra_bg` and `extra` (the two tones a canvas has besides
-- the status ones), `track` an empty trough, `knob` a switch's or
-- slider's knob, `focus` the keyboard's ring, `wash` the alpha of the ink
-- for each state layer (libadwaita derives its hover, active and selected
-- tints from currentColor), `strong` a variant whose controls carry a
-- solid outline, `dark` a dark one.
local M = {}

local WASH = { button = 0.10, hover = 0.07, active = 0.16, selected = 0.10, checked = 0.24, raised_hover = 0.15 }

M.palettes = {
  dark = {
    accent_bg = "#3584e4", accent_fg = "#ffffff", accent = "#78aeed",
    destructive_bg = "#c01c28", destructive_fg = "#ffffff", destructive = "#ff7b63",
    success_bg = "#26a269", success_fg = "#ffffff", success = "#8ff0a4",
    warning_bg = "#cd9309", warning_fg = "#000000cc", warning = "#f8e45c",
    error_bg = "#c01c28", error_fg = "#ffffff", error = "#ff7b63",
    window_bg = "#222226", window_fg = "#ffffff",
    view_bg = "#1d1d20", view_fg = "#ffffff",
    headerbar_bg = "#2e2e32", headerbar_fg = "#ffffff", headerbar_border = "#ffffff", headerbar_backdrop = "#222226",
    headerbar_shade = "#121214",
    sidebar_bg = "#2e2e32", sidebar_fg = "#ffffff", sidebar_backdrop = "#28282c", sidebar_border = "#121214",
    sidebar_shade = "#121214",
    secondary_sidebar_bg = "#28282c", secondary_sidebar_fg = "#ffffff", secondary_sidebar_backdrop = "#252529",
    secondary_sidebar_border = "#121214", secondary_sidebar_shade = "#121214",
    card_bg = "#333337", card_fg = "#ffffff", card_shade = "#121214",
    dialog_bg = "#36363a", dialog_fg = "#ffffff",
    popover_bg = "#36363a", popover_fg = "#ffffff", popover_shade = "#121214",
    thumbnail_bg = "#39393d", thumbnail_fg = "#ffffff",
    shade = "#121214", border = "#47474c", scrollbar_outline = "#000000",
    dim_fg = "#c0c0c4", info_bg = "#1c71d8", info = "#99c1f1", extra_bg = "#9141ac", extra = "#dc8add",
    track = "#4a4a50", knob = "#f6f6f6", focus = "#78aeed",
    wash = WASH, strong = false, dark = true,
  },
  light = {
    accent_bg = "#3584e4", accent_fg = "#ffffff", accent = "#1c71d8",
    destructive_bg = "#e01b24", destructive_fg = "#ffffff", destructive = "#c30000",
    success_bg = "#2ec27e", success_fg = "#ffffff", success = "#1b8553",
    warning_bg = "#e5a50a", warning_fg = "#000000cc", warning = "#9c6e03",
    error_bg = "#e01b24", error_fg = "#ffffff", error = "#c30000",
    window_bg = "#fafafb", window_fg = "#2e2e32",
    view_bg = "#ffffff", view_fg = "#2e2e32",
    headerbar_bg = "#ffffff", headerbar_fg = "#2e2e32", headerbar_border = "#2e2e32", headerbar_backdrop = "#fafafb",
    headerbar_shade = "#cfcfd4",
    sidebar_bg = "#ebebed", sidebar_fg = "#2e2e32", sidebar_backdrop = "#f2f2f4", sidebar_border = "#dcdce0",
    sidebar_shade = "#cfcfd4",
    secondary_sidebar_bg = "#f3f3f5", secondary_sidebar_fg = "#2e2e32", secondary_sidebar_backdrop = "#f6f6fa",
    secondary_sidebar_border = "#dcdce0", secondary_sidebar_shade = "#cfcfd4",
    card_bg = "#ffffff", card_fg = "#2e2e32", card_shade = "#cfcfd4",
    dialog_bg = "#fafafb", dialog_fg = "#2e2e32",
    popover_bg = "#ffffff", popover_fg = "#2e2e32", popover_shade = "#cfcfd4",
    thumbnail_bg = "#ffffff", thumbnail_fg = "#2e2e32",
    shade = "#cfcfd4", border = "#dcdce0", scrollbar_outline = "#ffffff",
    dim_fg = "#6b6b70", info_bg = "#3584e4", info = "#1c71d8", extra_bg = "#9141ac", extra = "#813d9c",
    track = "#dcdce0", knob = "#ffffff", focus = "#3584e4",
    wash = WASH, strong = false, dark = false,
  },
  -- GNOME's HighContrast: white ground, black ink, solid outlines on every
  -- control, darker tones so each meets 4.5:1 as text and 3:1 as a mark.
  high_contrast = {
    accent_bg = "#0b3fa0", accent_fg = "#ffffff", accent = "#0b3fa0",
    destructive_bg = "#a3000b", destructive_fg = "#ffffff", destructive = "#a3000b",
    success_bg = "#0a6b3a", success_fg = "#ffffff", success = "#0a6b3a",
    warning_bg = "#7a5000", warning_fg = "#ffffff", warning = "#7a5000",
    error_bg = "#a3000b", error_fg = "#ffffff", error = "#a3000b",
    window_bg = "#ffffff", window_fg = "#000000",
    view_bg = "#ffffff", view_fg = "#000000",
    headerbar_bg = "#ffffff", headerbar_fg = "#000000", headerbar_border = "#000000", headerbar_backdrop = "#ffffff",
    headerbar_shade = "#000000",
    sidebar_bg = "#f0f0f0", sidebar_fg = "#000000", sidebar_backdrop = "#f0f0f0", sidebar_border = "#000000",
    sidebar_shade = "#000000",
    secondary_sidebar_bg = "#f6f6f6", secondary_sidebar_fg = "#000000", secondary_sidebar_backdrop = "#f6f6f6",
    secondary_sidebar_border = "#000000", secondary_sidebar_shade = "#000000",
    card_bg = "#ffffff", card_fg = "#000000", card_shade = "#000000",
    dialog_bg = "#ffffff", dialog_fg = "#000000",
    popover_bg = "#ffffff", popover_fg = "#000000", popover_shade = "#000000",
    thumbnail_bg = "#ffffff", thumbnail_fg = "#000000",
    shade = "#000000", border = "#000000", scrollbar_outline = "#ffffff",
    dim_fg = "#1f1f1f", info_bg = "#0b3fa0", info = "#0b3fa0", extra_bg = "#6a1f86", extra = "#6a1f86",
    track = "#8a8a8a", knob = "#ffffff", focus = "#000000",
    wash = { button = 0.12, hover = 0.14, active = 0.28, selected = 0.18, checked = 0.30, raised_hover = 0.22 },
    strong = true, dark = false,
  },
}

M.VARIANTS = { "dark", "light", "high_contrast" }

-- -------------------------------------------------------------------- type --

-- Adwaita Sans is GNOME's face since 48; Cantarell before it; Inter and the
-- system sans where neither is installed.
M.font = "Adwaita Sans, Cantarell, Inter, sans-serif"
M.mono = "Adwaita Mono, Source Code Pro, DejaVu Sans Mono, monospace"
-- Symbolic icons by name, from the icon face the shared widgets name them in.
M.icon_font = "Material Symbols Rounded"

-- In pixels: libadwaita's 11 pt body lands near 15 px.
M.size = { small = 13, smaller = 14, normal = 15, larger = 17, large = 20, extra = 28 }

--- libadwaita's type classes: `size` in px, `weight`, `family`.
M.type = {
  title_1 = { size = 27, weight = 800, family = M.font },
  title_2 = { size = 20, weight = 800, family = M.font },
  title_3 = { size = 20, weight = 700, family = M.font },
  title_4 = { size = 17, weight = 700, family = M.font },
  heading = { size = 15, weight = 700, family = M.font },
  body = { size = 15, weight = 400, family = M.font },
  caption_heading = { size = 12, weight = 700, family = M.font },
  caption = { size = 12, weight = 400, family = M.font },
  monospace = { size = 15, weight = 400, family = M.mono },
  numeric = { size = 15, weight = 400, family = M.font, features = { "tnum" } },
}

-- ------------------------------------------------------------------- sizes --

M.radius = { small = 6, medium = 9, large = 12, window = 15 }
M.PAD = 12
M.GAP = 12
M.control_height = 34

M.shape = {
  window_radius = M.radius.window, dialog_radius = M.radius.window, card_radius = M.radius.large,
  button_radius = M.radius.small, popover_radius = M.radius.large,
  border_opacity = 0.15, dim_opacity = 0.55, focus_ring_width = 2,
}

-- ------------------------------------------------------------------ motion --

-- libadwaita moves on ease-out cubic over short times, never overshooting.
M.ease = {
  standard = "out_cubic",
  decelerate = { x1 = 0, y1 = 0, x2 = 0.2, y2 = 1 },
  accelerate = { x1 = 0.4, y1 = 0, x2 = 1, y2 = 1 },
}

local DURATION = { small = 150, normal = 250, large = 400, page = 300 }

--- The durations, every one 0 when motion is reduced.
function M.duration(reduced)
  local out = {}
  for k, v in pairs(DURATION) do out[k] = reduced and 0 or v end
  return out
end

--- The motion tokens: libadwaita's springs (critically damped: nothing
--- overshoots), its durations and its easing, all instant when `reduced`.
function M.motion(reduced)
  local function spring(stiffness)
    if reduced then return { duration = 0 } end
    return { kind = "spring", mass = 1, stiffness = stiffness, damping = 2 * math.sqrt(stiffness) }
  end
  local d = M.duration(reduced)
  return {
    spring = spring(1000), spring_snappy = spring(1500), spring_gentle = spring(320),
    duration_short = d.small, duration_normal = d.normal, duration_long = d.large,
    easing = M.ease.standard,
  }
end

return M
