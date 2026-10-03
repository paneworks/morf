-- The default kit's looks for each Overflow widget, in the Adwaita manner.
-- The layout is the glue's (lib.kit.overflow): items at running x that
-- spring to their places as the line overflows, and a "more" button
-- where the hidden ones went. Here: the button's face -- the view-more
-- dots on a round wash, a chevron that turns while a tab strip's menu is
-- open, "…" in a breadcrumb trail, "+N" on a chip -- and a tab strip's
-- accent underline, a field bar riding the chosen tab.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function ink(s) return function() local p = P() return s.open() and p.accent_ink or p.ink end end

  --- The pointer's wash and the open menu's checked wash, on a round
  --- (or `radius`) ground the size of the button.
  local function wash(s, radius)
    return ui.Rect { anchors = { fill = true, margins = 2 }, radius = radius,
      color = function()
        local p = P()
        if s.down() then return p.ink:alpha(p.wash.active) end
        if s.open() then return p.ink:alpha(p.wash.checked) end
        return s.hovered() and p.ink:alpha(p.wash.hover) or p.ink:alpha(0)
      end, behavior = { color = quick() } }
  end

  local function dots(s, glyph)
    return ui.Item { anchors = { fill = true }, wash(s, (s.height - 4) / 2),
      M.icon(glyph or "more_vert", 20, ink(s), { anchors = { center_in = true } }) }
  end

  local function menu() return { placement = "bottom-end", width = 220, item_height = 36 } end

  function S.Overflow(t, spec)
    return { background = ui.Item {}, more = function(s) return dots(s) end, menu = menu }
  end

  function S.overflow_toolbar(t, spec)
    return { background = ui.Item {}, more = function(s) return dots(s, "more_vert") end, menu = menu }
  end

  --- A tab strip: a hairline along its foot, the accent bar under the
  --- chosen tab (a field box riding a track that springs and stretches),
  --- a chevron for the rest that turns while their menu is open.
  function S.overflow_tabs(t, spec)
    local track = ui.Item { y = function() return (t.height or 40) - 3 end, height = 3,
      x = function() return (t.cur_x or 0) + 8 end, width = function() return math.max(0, (t.cur_w or 0) - 16) end,
      behavior = { x = M.spring(420, 40), width = M.spring(420, 40) }, stretch = M.STRETCH }
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
          color = function() local p = P() return p.strong and p.border or p.ink:alpha(0.12) end },
        track,
        ui.Sdf { anchors = { fill = true }, z = 1, visible = function() return (t.current or 0) > 0 end,
          ui.SdfShape { shape = "box", track = track, radius = 1.5, fill_color = function() return P().accent end } } },
      more = function(s)
        return ui.Item { anchors = { fill = true }, wash(s, R.small),
          ui.Item { anchors = { center_in = true }, width = 20, height = 20,
            rotation = function() return s.open() and 180 or 0 end, behavior = { rotation = M.spring(420, 40) },
            M.icon("expand_more", 20, ink(s)) } }
      end,
      menu = menu,
    }
  end

  --- A breadcrumb trail: the chevron that leads to the gap, and "…" on a
  --- small wash.
  function S.overflow_breadcrumbs(t, spec)
    return {
      background = ui.Item {},
      more = function(s)
        return ui.Row { anchors = { fill = true }, align = "center", gap = 0,
          ui.Item { width = 18, height = 18,
            M.icon("chevron_right", 16, function() return P().ink_dim end, { anchors = { center_in = true } }) },
          ui.Item { width = s.width - 18, height = s.height, wash(s, R.small),
            M.text { anchors = { fill = true }, text = "…", font_size = theme.size.normal, font_weight = 700,
              color = ink(s), horizontal_alignment = "center", vertical_alignment = "center" } } }
      end,
      menu = function() return { placement = "bottom-start", width = 220, item_height = 36 } end,
    }
  end

  --- Chips: the rest as one more chip, "+N", outlined.
  function S.chip_overflow(t, spec)
    return {
      background = ui.Item {},
      more = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 1 }, radius = (s.height - 2) / 2,
            color = function()
              local p = P()
              if s.down() then return p.ink:alpha(p.wash.active) end
              if s.open() then return p.accent:alpha(p.dark and 0.3 or 0.18) end
              return s.hovered() and p.ink:alpha(p.wash.hover) or p.ink:alpha(0)
            end, behavior = { color = quick() },
            border_width = 1, border_color = function() local p = P() return p.strong and p.ink or p.border end },
          M.text { anchors = { fill = true }, text = function() return "+" .. tostring(s.count()) end,
            font_size = theme.size.small, font_weight = 700, color = ink(s),
            horizontal_alignment = "center", vertical_alignment = "center" } }
      end,
      menu = function() return { placement = "bottom-start", width = 200, item_height = 36 } end,
    }
  end

  --- A site's navigation: the chosen link underlined as a tab strip's,
  --- the rest behind the view-more dots.
  function S.priority_nav(t, spec)
    local slots = S.overflow_tabs(t, spec)
    slots.more = function(s) return dots(s, "more_horiz") end
    return slots
  end
end
