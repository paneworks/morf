-- The default kit's looks for each Sheet widget, in the Adwaita manner:
-- a spreadsheet of hairline grid lines under header strips in the
-- header's tone, its range an accent plate with a fill handle square at
-- the corner and a thick accent ring on the current cell; a data grid of
-- striped rows under a bold header; a step sequencer of rounded pads lit
-- in the accent, beats grouped by four, the playing column glowing; a seat
-- map of seats free, taken and chosen; a plain cell grid. The ring springs
-- from cell to cell and the plate stretches over the range. Every layout
-- here is shared with the caelestia themes' (material, tsugumori): only
-- the style differs.
local ui = require("morf.ui")

return function(S, theme, M)
  local P, R = theme.P, theme.radius
  local SZ = theme.size
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  -- (Numbers sit to the right, as a spreadsheet's do.)
  local function numeric(v) return type(v) == "number" or (type(v) == "string" and v:match("^%-?[%d%.,]+$") ~= nil) end

  -- -------------------------------------------------------------- parts --

  --- A text cell: grid lines (right and bottom, or the bottom only),
  --- a stripe on even rows, numbers to the right.
  local function text_cell(o)
    return function(t, spec)
      return function(s)
        return ui.Item { anchors = { fill = true },
          o.zebra and ui.Rect { anchors = { fill = true },
            color = function() local p = P() return p.ink:alpha(s.row() % 2 == 0 and (p.dark and 0.035 or 0.03) or 0) end } or nil,
          o.right ~= false and ui.Rect { anchors = { right = true, top = true, bottom = true }, width = 1,
            color = function() return P().border end } or nil,
          ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
            color = function() local p = P() return o.zebra and p.border:alpha(0.6) or p.border end },
          M.text { anchors = { fill = true, left_margin = o.pad or 8, right_margin = o.pad or 8 },
            vertical_alignment = "center", elide = "right", font_size = SZ.normal,
            horizontal_alignment = o.center and "center" or function() return numeric(s.value()) and "right" or "left" end,
            text = function() return s.text() end,
            font_family = o.mono and theme.mono or nil,
            color = function() local p = P() return s.disabled() and p.ink_dim or p.ink end } }
      end
    end
  end

  --- A header: the strip in the header's tone, hairlines between, the
  --- title dim -- in the accent over the current column or row, on a wash
  --- over the range.
  local function header(o)
    o = o or {}
    return function(t, spec)
      return function(h)
        local row = h.kind == "row"
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().header end },
          ui.Rect { anchors = { fill = true },
            color = function() local p = P() return p.accent:alpha(h.selected() and (p.dark and 0.22 or 0.12) or 0) end,
            behavior = { color = quick() } },
          ui.Rect { anchors = { right = true, top = true, bottom = true }, width = 1, color = function() return P().border end },
          ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end },
          -- The current column's or row's edge in the accent.
          h.kind ~= "corner" and ui.Rect {
            anchors = row and { right = true, top = true, bottom = true } or { left = true, right = true, bottom = true },
            width = row and 2 or nil, height = row and nil or 2,
            color = function() return P().accent end, opacity = function() return h.current() and 1 or 0 end,
            behavior = { opacity = quick() } } or nil,
          M.text { anchors = { fill = true, left_margin = o.left and 10 or 4, right_margin = 4 },
            vertical_alignment = "center", horizontal_alignment = (o.left and not row) and "left" or "center",
            elide = "right", text = function() return h.title() end,
            font_size = SZ.small, font_weight = o.bold and 700 or 600,
            color = function() local p = P() return h.current() and p.accent_ink or p.ink_dim end } }
      end
    end
  end

  --- The ring on the current cell, springing between cells.
  local function ring(o)
    return function(t, spec)
      local function box(i) return function() return select(i, spec.cursor_box(t)) end end
      local inset = o.inset or 0
      local spring = M.spring(o.stiffness or 420, 30)
      return ui.Rect { x = function() return box(1)() + inset end, y = function() return box(2)() + inset end,
        width = function() return box(3)() - 2 * inset end, height = function() return box(4)() - 2 * inset end,
        radius = o.radius or 2, color = "transparent", border_width = o.width or 2,
        border_color = function() local p = P() return p.focus end,
        opacity = function() return (t.focused or t.visual_focus) and 1 or 0.55 end,
        visible = function() return not t.editing end,
        behavior = { x = spring, y = spring, width = spring, height = spring, opacity = quick() } }
    end
  end

  --- The range: an accent plate that stretches over the cells, shown once
  --- it is more than one; with `handle`, a fill handle square at its
  --- corner.
  local function plate(o)
    return function(t, spec)
      local function box(i) return function() return select(i, spec.range_box(t)) end end
      local function many() local r0, c0, r1, c1 = tostring(t.range or ""):match("(%d+),(%d+),(%d+),(%d+)")
        return r0 ~= nil and (r0 ~= r1 or c0 ~= c1) end
      local spring = M.spring(360, 30)
      return ui.Item { x = box(1), y = box(2), width = box(3), height = box(4),
        behavior = { x = spring, y = spring, width = spring, height = spring },
        ui.Rect { anchors = { fill = true }, radius = o.radius or 0,
          color = function() local p = P() return p.accent:alpha(p.dark and 0.2 or 0.12) end,
          border_width = o.border or 1, border_color = function() return P().accent end,
          opacity = function() return many() and 1 or 0 end, behavior = { opacity = quick() } },
        o.handle and ui.Rect { anchors = { right = true, bottom = true, right_margin = -3, bottom_margin = -3 },
          width = 7, height = 7, color = function() return P().accent end,
          border_width = 1, border_color = function() return P().view end,
          visible = function() return not t.editing end } or nil }
    end
  end

  --- The editor over the cell: the view's ground, the accent edge.
  local function editor(o)
    return function(t, spec)
      return function(props)
        props.font_size = SZ.normal
        props.color = function() return P().ink end
        props.selection_color = function() return P().accent:alpha(0.35) end
        props.caret_color = function() return P().accent_ink end
        props.vertical_alignment = "center"
        props.anchors = { fill = true, left_margin = 8, right_margin = 6 }
        local input = ui.TextInput(props)
        local node = ui.Rect { anchors = { fill = true, margins = -1 }, radius = o.radius or 2,
          color = function() return P().view end, border_width = 2, border_color = function() return P().accent end,
          shadow_color = function() return P().shade:alpha(0.5) end, shadow_blur = 6, shadow_offset_y = 2, input }
        return node, input
      end
    end
  end

  local function ground(color)
    return function(t, spec)
      return ui.Rect { anchors = { fill = true }, color = color, border_width = 1,
        border_color = function() return P().border end }
    end
  end

  -- -------------------------------------------------------- the widgets --

  S.Sheet = {
    background = ground(function() return P().view end),
    cell = text_cell {},
    header = header {},
    range = plate {},
    cursor = ring {},
    editor = editor {},
  }

  S.spreadsheet = {
    cell = text_cell { mono = false },
    header = header {},
    range = plate { handle = true },
    cursor = ring { width = 3, radius = 1, inset = -1 },
    editor = editor { radius = 1 },
  }

  S.data_grid = {
    cell = text_cell { zebra = true, right = false, pad = 12 },
    header = header { left = true, bold = true },
    range = plate { radius = R.small, border = 0 },
    cursor = ring { width = 2, radius = R.small, inset = 1 },
    editor = editor { radius = R.small },
  }

  S.cell_grid = {
    background = ground(function() return P().card end),
    cell = text_cell { center = true },
    range = plate {},
    cursor = ring { width = 2, radius = R.small, inset = 1 },
    editor = editor {},
  }

  -- Pads: lit in the accent, beats grouped by four, the playing column
  -- glowing behind them and a lit pad on it flashing.
  S.step_sequencer = {
    background = function() return ui.Rect { anchors = { fill = true }, radius = R.large,
      color = function() return P().card end, border_width = 1, border_color = function() return P().border end } end,
    cell = function(t, spec)
      return function(s)
        local down_beat = (s.column - 1) % 8 < 4
        local function on() return s.value() and s.value() ~= 0 and s.value() ~= "" end
        return ui.Rect { anchors = { fill = true, margins = 3 }, radius = R.small,
          color = function()
            local p = P()
            if on() then return p.accent end
            return p.ink:alpha(down_beat and (p.dark and 0.12 or 0.09) or (p.dark and 0.07 or 0.05))
          end,
          behavior = { color = quick() },
          ui.Rect { anchors = { fill = true }, radius = R.small,
            color = function() return P().on_accent:alpha(0.35) end,
            opacity = function() return (on() and s.playing()) and 1 or 0 end,
            behavior = { opacity = { duration = 90 } } } }
      end
    end,
    header = function(t, spec)
      return function(h)
        if h.kind == "corner" then return nil end
        if h.kind == "row" then
          return M.text { anchors = { fill = true, left_margin = 12, right_margin = 6 }, vertical_alignment = "center",
            elide = "right", text = function() return h.title() end, font_size = SZ.small, font_weight = 600,
            color = function() local p = P() return h.current() and p.accent_ink or p.ink_dim end }
        end
        local beat = (h.index() - 1) % 4 == 0
        return ui.Item { anchors = { fill = true },
          beat and M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
            text = function() return h.title() end, font_size = SZ.small, font_weight = 600,
            color = function() local p = P() return (h.playing() or h.current()) and p.accent_ink or p.ink_dim end } or nil,
          not beat and ui.Rect { anchors = { center_in = true }, width = 4, height = 4, radius = 2,
            color = function() local p = P() return h.playing() and p.accent or p.ink_dim:alpha(0.6) end } or nil }
      end
    end,
    cursor = function(t, spec)
      local spring = M.spring(420, 30)
      local function box(i) return function() return select(i, spec.cursor_box(t)) end end
      local function col(i) return function() return select(i, spec.column_box(spec.playhead_column())) end end
      return ui.Item {
        -- The playhead: a column of light behind the pads.
        ui.Rect { x = col(1), y = 0, width = col(3), height = col(4), radius = R.small, z = -1,
          color = function() local p = P() return p.accent:alpha(p.dark and 0.2 or 0.14) end,
          visible = function() return spec.playhead_column() > 0 end,
          behavior = { x = { duration = 90, easing = "out_cubic" } } },
        ui.Rect { x = function() return box(1)() + 1 end, y = function() return box(2)() + 1 end,
          width = function() return box(3)() - 2 end, height = function() return box(4)() - 2 end,
          radius = R.small + 2, color = "transparent", border_width = 2,
          border_color = function() return P().focus end,
          opacity = function() return t.focused and 1 or 0 end,
          behavior = { x = spring, y = spring, opacity = quick() } } }
    end,
  }

  -- Seats: free ones outlined, taken ones faint, chosen ones filled.
  S.seat_map = {
    background = function() return ui.Rect { anchors = { fill = true }, radius = R.large,
      color = function() return P().card end, border_width = 1, border_color = function() return P().border end } end,
    cell = function(t, spec)
      return function(s)
        local function chosen() local v = s.value() return v and v ~= 0 and v ~= "" end
        return ui.Rect { anchors = { fill = true, margins = 3 }, radius = R.small,
          color = function()
            local p = P()
            if s.disabled() then return p.ink:alpha(0.06) end
            if chosen() then return p.accent end
            return p.ink:alpha(0)
          end,
          border_width = function() return (s.disabled() or chosen()) and 0 or 1 end,
          border_color = function() return P().border end,
          behavior = { color = quick() },
          M.icon("event_seat", math.floor(math.min(s.width, s.height) * 0.62), function()
            local p = P()
            if s.disabled() then return p.ink_dim:alpha(0.4) end
            if chosen() then return p.on_accent end
            return p.ink_dim
          end, { anchors = { center_in = true } }) }
      end
    end,
    header = function(t, spec)
      return function(h)
        if h.kind == "corner" then return nil end
        return M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          text = function() return h.title() end, font_size = SZ.small, font_weight = 600,
          color = function() local p = P() return h.current() and p.accent_ink or p.ink_dim end }
      end
    end,
    cursor = ring { width = 2, radius = R.small + 2, inset = 0 },
  }
end
