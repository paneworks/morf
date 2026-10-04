-- The default kit's tokens: an Adwaita-like (GNOME HIG) palette in three
-- variants that share every name, and the sizes, radii, type and motion
-- the components read. Nothing here draws.
--
-- Colours are opaque; washes are the ink at an alpha (`wash` below), as
-- libadwaita derives its hover, active and selected tints from
-- currentColor.
local M = {}

-- ----------------------------------------------------------------- palette --
--
-- `window`   the window's ground          `view`     a list's or a text view's
-- `card`     a card or boxed list         `raised`   popovers, dialogs, menus
-- `sidebar`  a sidebar's ground           `header`   a header bar
-- `ink`      text and symbolic icons      `ink_dim`  secondary text
-- `border`   hairlines (cards, entries)   `shade`    the line under a raised thing
-- `accent`   the accent's fill            `accent_ink` accent-coloured text
-- `on_accent` text on the accent fill     `track`    an empty trough
-- `knob`     a switch's or slider's knob  `focus`    the keyboard's ring
-- the four status pairs (`success`, `warning`, `error`, `info`, `extra`,
-- each with `*_ink` for text in that tone) and `destructive`.
-- `wash` gives the alpha of the ink for each state layer; `strong` marks
-- a variant whose controls also carry a solid outline (high contrast).

M.palettes = {
  dark = {
    window = "#222226", view = "#1d1d20", card = "#333337", raised = "#36363a", sidebar = "#2e2e32",
    header = "#2e2e32", ink = "#ffffff", ink_dim = "#c0c0c4", border = "#47474c", shade = "#121214",
    accent = "#3584e4", accent_ink = "#78aeed", on_accent = "#ffffff", track = "#4a4a50", knob = "#f6f6f6",
    focus = "#78aeed",
    success = "#26a269", success_ink = "#8ff0a4", warning = "#cd9309", warning_ink = "#f8e45c",
    error = "#c01c28", error_ink = "#ff7b63", info = "#1c71d8", info_ink = "#99c1f1",
    extra = "#9141ac", extra_ink = "#dc8add", destructive = "#c01c28",
    wash = { button = 0.10, hover = 0.07, active = 0.16, selected = 0.10, checked = 0.24, raised_hover = 0.15 },
    strong = false, dark = true,
  },
  light = {
    window = "#fafafb", view = "#ffffff", card = "#ffffff", raised = "#ffffff", sidebar = "#ebebed",
    header = "#ffffff", ink = "#2e2e32", ink_dim = "#6b6b70", border = "#dcdce0", shade = "#cfcfd4",
    accent = "#3584e4", accent_ink = "#1c71d8", on_accent = "#ffffff", track = "#dcdce0", knob = "#ffffff",
    focus = "#3584e4",
    success = "#2ec27e", success_ink = "#1b8553", warning = "#e5a50a", warning_ink = "#9c6e03",
    error = "#e01b24", error_ink = "#c30000", info = "#3584e4", info_ink = "#1c71d8",
    extra = "#9141ac", extra_ink = "#813d9c", destructive = "#e01b24",
    wash = { button = 0.10, hover = 0.07, active = 0.16, selected = 0.10, checked = 0.24, raised_hover = 0.15 },
    strong = false, dark = false,
  },
  -- GNOME's HighContrast: white ground, black ink, solid outlines on every
  -- control, darker tones so each meets 4.5:1 as text and 3:1 as a mark.
  high_contrast = {
    window = "#ffffff", view = "#ffffff", card = "#ffffff", raised = "#ffffff", sidebar = "#f0f0f0",
    header = "#ffffff", ink = "#000000", ink_dim = "#1f1f1f", border = "#000000", shade = "#000000",
    accent = "#0b3fa0", accent_ink = "#0b3fa0", on_accent = "#ffffff", track = "#8a8a8a", knob = "#ffffff",
    focus = "#000000",
    success = "#0a6b3a", success_ink = "#0a6b3a", warning = "#7a5000", warning_ink = "#7a5000",
    error = "#a3000b", error_ink = "#a3000b", info = "#0b3fa0", info_ink = "#0b3fa0",
    extra = "#6a1f86", extra_ink = "#6a1f86", destructive = "#a3000b",
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

-- ------------------------------------------------------------------- sizes --

M.radius = { small = 6, medium = 9, large = 12, window = 15 }
M.ROUNDING = M.radius.large
M.PAD = 12
M.GAP = 12
M.control_height = 34

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

return M
