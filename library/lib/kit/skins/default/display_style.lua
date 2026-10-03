-- The default kit's style for the shared display widgets (lib.kit.display):
-- flat fills with Adwaita's corners, hairlines in the ink, the GNOME
-- palette's blue, green, yellow, red, purple and orange for series, the
-- kit's own type. No hatching, no marks.
return function(theme, M)
  local P = theme.P
  -- Series tones in the order GNOME's charts reach for them.
  local SERIES = { "accent", "success", "warning", "error", "extra", "info" }
  return {
    name = "default",
    accent = M.signal("accent"), ok = M.signal("ok"), warn = M.signal("warn"), alert = M.signal("alert"),
    info = M.signal("info"), extra = M.signal("extra"),
    series = function(i)
      return function()
        local p = P()
        local tone = p[SERIES[(i - 1) % #SERIES + 1]] or p.accent
        return i > #SERIES and tone:mix(p.window, 0.4) or tone
      end
    end,
    surface = function() return P().card end,
    raised = function() local p = P() return p.dark and p.raised or p.window:mix(p.ink, 0.04) end,
    track = function() return P().track end,
    line = function() local p = P() return p.strong and p.border or p.ink:alpha(0.15) end,
    ink = function() return P().ink end,
    ink_lo = function() return P().ink_dim end,
    on_accent = function() return P().on_accent end,
    -- Corners: half a small thing's height, Adwaita's 12 at most.
    radius = function(h) return math.min((h or 0) / 2, theme.radius.large) end,
    pill = true,
    hatched = false,
    stroke = 1,
    size = theme.size,
    font = theme.font, mono_font = theme.mono,
    text = M.text, label = M.label, icon = M.icon,
    surface_node = M.surface,
    stroke_of = M.stroke,
    marks = function() return nil end,
    motion = { duration = theme.duration.normal, easing = theme.ease.standard },
    spring = M.spring,
  }
end
