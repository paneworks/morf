-- The default kit's looks for each Roving widget, in the Adwaita manner. The
-- layout is the glue's (lib.kit.roving): members in a row, one Tab stop.
-- What moves is one thing: a distance-field plate (or a ring, over
-- members with grounds of their own) riding a track whose leading edge
-- leaves first, so it stretches towards the next member and settles in
-- there. It shows while focus is inside the group. A toolbar sits on a
-- faint trough, a menu bar on the header's tone with the open menu's
-- title on the checked wash, separators are hairlines.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function glide(t, inset)
    return require("lib.kit.roving").glide(t, { reduced = theme.reduced, stretch = M.STRETCH,
      fit = function(x, y, w, h) return x + inset, y + inset, math.max(0, w - 2 * inset), math.max(0, h - 2 * inset) end })
  end
  local function get_radius(r) if type(r) == "function" then return r() end return r end
  local function shown(t) return function() return (t.within or (t.open or 0) > 0) and 1 or 0 end end

  --- A plate under the current member: the selected wash (the checked
  --- one under an open menu's title), deeper when a keyboard put focus
  --- there.
  local function plate(t, radius)
    local track = glide(t, 0)
    return ui.Item { anchors = { fill = true }, z = -1, track,
      ui.Sdf { anchors = { fill = true },
        ui.SdfShape { shape = "box", track = track, radius = radius, opacity = shown(t), behavior = { opacity = quick() },
          fill_color = function()
            local p = P()
            if (t.open or 0) > 0 then return p.ink:alpha(p.wash.checked) end
            return t.keyboard and p.accent:alpha(p.dark and 0.26 or 0.16) or p.ink:alpha(p.wash.hover)
          end } } }
  end

  --- A ring over the current member, for members with grounds of their
  --- own (segments, chips): the box less a box 2 px in.
  local function ring(t, radius)
    local track = glide(t, -2)
    local inner = ui.Item { anchors = { fill = true, margins = 2 } }
    ui.reparent(inner, track)
    return ui.Item { anchors = { fill = true }, z = 2, track,
      ui.Sdf { anchors = { fill = true }, opacity = function() return t.keyboard and 1 or 0 end,
        behavior = { opacity = quick() },
        fill_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        ui.SdfShape { shape = "box", track = track, radius = radius },
        ui.SdfShape { shape = "box", track = inner, radius = function() return math.max(0, get_radius(radius) - 2) end,
          operation = "subtract" } } }
  end

  local function hairline(vertical)
    return ui.Item { width = vertical and 9 or 24, height = vertical and 24 or 9,
      ui.Rect { anchors = { center_in = true }, width = vertical and 1 or 16, height = vertical and 16 or 1,
        color = function() local p = P() return p.strong and p.border or p.ink:alpha(0.15) end } }
  end
  local function trough(radius)
    return ui.Rect { anchors = { fill = true }, radius = radius, z = -2,
      color = function() local p = P() return p.ink:alpha(p.dark and 0.06 or 0.04) end,
      border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end }
  end

  function S.Roving(t, spec)
    return {
      background = trough(R.medium + 2),
      indicator = plate(t, R.small),
      separator = hairline,
    }
  end

  --- A toolbar: the trough, the plate rounded as a flat button is.
  function S.toolbar_group(t, spec)
    return { background = trough(R.medium + 2), indicator = plate(t, R.small), separator = hairline }
  end

  --- A menu bar: the header's tone with a hairline under it; the open
  --- menu's title on the checked wash.
  function S.menubar(t, spec)
    return {
      background = ui.Item { anchors = { fill = true }, z = -2,
        ui.Rect { anchors = { fill = true }, radius = R.small, color = function() return P().header end },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
          color = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 0.5 or 0.35) end } },
      indicator = plate(t, R.small),
      separator = hairline,
    }
  end

  --- A linked button group: the segments draw the row; the focus ring
  --- slides over them.
  function S.button_group(t, spec)
    return { background = ui.Item {}, indicator = ring(t, R.small), separator = function() return ui.Item { width = 1, height = 1 } end }
  end

  --- A row of chips: no ground; a round ring slides from chip to chip.
  function S.chip_row(t, spec)
    return { background = ui.Item {}, indicator = ring(t, function() return (t.cur_h or 32) / 2 + 2 end),
      separator = hairline }
  end

  --- A bar of icons: a pill trough, a round plate.
  function S.icon_bar(t, spec)
    return { background = trough(function() return (t.height or 40) / 2 end),
      indicator = plate(t, function() return (t.cur_h or 32) / 2 end), separator = hairline }
  end
end
