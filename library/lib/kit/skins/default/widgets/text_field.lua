-- The default kit's look for each TextField widget, in the Adwaita manner:
-- soft grey wells with 6 px corners, a 2 px accent ring that settles in
-- on focus (the error tone while what is typed would not be accepted), and
-- a title that floats up and shrinks once there is something to show -- as
-- an entry row's does. Each widget adds what it is: a lock, a magnifier, a
-- unit, chips, digit cells, a gutter. The input itself is the
-- configuration's; what it accepts, the archetype's.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function settle() return M.spring(520) end

  local function sides(v)
    if type(v) == "table" then return v[1] or 0, v[2] or 0, v[3] or 0, v[4] or 0 end
    v = v or 0
    return v, v, v, v
  end
  -- The line under a field that says what it wants (`supporting`).
  local FOOT = 22
  local function foot(spec) return spec.supporting and FOOT or 0 end
  local function bad(t) return not t.acceptable and not t.empty end

  -- --------------------------------------------------------- pieces --

  --- The well: `radius` (a number or a binding), `color`, `edge` (a border
  --- binding, none by default).
  local function well(t, spec, o)
    o = o or {}
    return ui.Rect { anchors = { left = true, right = true, top = true }, radius = o.radius or R.small,
      height = function() return math.max(0, (t.height or 0) - foot(spec)) end,
      color = o.color or function()
        local p = P()
        if bad(t) then return p.error:alpha(0.1) end
        return p.ink:alpha((t.hovered and not t.focused) and (p.dark and 0.11 or 0.1) or (p.dark and 0.08 or 0.07))
      end,
      border_width = o.edge and function() return P().strong and 2 or 1 end or function() return P().strong and 1 or 0 end,
      border_color = o.edge or function() return P().border end,
      behavior = { color = quick() } }
  end

  --- The focus ring: 2 px of the accent (the error tone while it would not
  --- be accepted) that settles in from a little outside as focus arrives.
  local function ring(t, spec, radius)
    return ui.Rect { anchors = { left = true, right = true, top = true }, z = 6, color = "transparent",
      height = function() return math.max(0, (t.height or 0) - foot(spec)) end,
      radius = radius or R.small, border_width = 2,
      border_color = function()
        local p = P()
        if bad(t) then return p.error end
        return p.strong and p.accent or p.focus:alpha(0.6)
      end,
      opacity = function() return (t.focused or bad(t)) and 1 or 0 end,
      scale = function() return (t.focused or bad(t)) and 1 or 1.04 end,
      behavior = { opacity = quick(), scale = settle(), border_color = quick() } }
  end

  --- The title (`spec.label`): at the middle of the well while the field
  --- is empty and idle, floated to its top and shrunk once it is focused or
  --- holds text -- a move and a scale of one drawing.
  local function title(t, spec, always_up)
    if not spec.label then return nil end
    local l = sides(spec.inset)
    local size = theme.size.normal
    local lh = math.ceil(size * 1.3)
    local function box() return math.max(0, (t.height or 0) - foot(spec)) end
    local function rest() return math.floor((box() - lh) / 2) end
    local function up() return always_up or t.focused or not t.empty or (spec.tags ~= nil) end
    return M.text { text = spec.label, x = l, height = lh, font_size = size, vertical_alignment = "center",
      transform_origin_x = 0, transform_origin_y = 0, z = 4,
      y = rest,
      translate_y = function() return up() and (6 - rest()) or 0 end,
      scale = function() return up() and 0.8 or 1 end,
      color = function()
        local p = P()
        if bad(t) then return p.error_ink end
        if t.focused then return p.accent_ink end
        return p.ink_dim
      end,
      behavior = { translate_y = settle(), scale = settle(), color = quick() } }
  end

  --- A glyph at the start, centred on the well (or on its text line).
  local function leading(t, spec, color, y)
    if not spec.icon then return nil end
    return M.icon(spec.icon, 18, color or function()
        local p = P()
        if bad(t) then return p.error_ink end
        return t.focused and p.accent_ink or p.ink_dim
      end,
      { x = 15, y = y or function() return math.floor(((t.height or 0) - foot(spec) - 18) / 2) end,
        behavior = { color = quick() } })
  end

  --- The supporting line under the well, the error tone while it would not
  --- be accepted; the count against `max_length` at its end.
  local function supporting(t, spec)
    if not spec.supporting and not spec.max_length then return nil end
    local l = sides(spec.inset)
    local function tone() local p = P() return bad(t) and p.error_ink or p.ink_dim end
    local node = ui.Item { anchors = { left = true, right = true, bottom = true }, height = FOOT }
    if spec.supporting then
      ui.reparent(M.text { text = spec.supporting, x = l, anchors = { bottom = true }, height = 18,
        font_size = theme.size.small, color = tone, behavior = { color = quick() } }, node)
    end
    if spec.max_length then
      -- (Clear of a corner grip when there is no line of its own.)
      ui.reparent(M.text { anchors = { right = true, bottom = true, right_margin = spec.supporting and 6 or 22,
        bottom_margin = spec.supporting and 0 or 2 }, height = 18,
        font_size = theme.size.small, color = tone,
        text = function() return ("%d / %d"):format(t.length or 0, spec.max_length) end }, node)
    end
    return node
  end

  -- -------------------------------------------------------------- looks --

  local LOOK = {}

  --- An entry: the well, its title, the ring.
  function LOOK.entry(t, spec)
    return { background = well(t, spec), placeholder = title(t, spec), error = ring(t, spec),
      counter = supporting(t, spec) }
  end

  --- A password: a lock before it (the reveal is the archetype skin's).
  function LOOK.password(t, spec)
    local s = LOOK.entry(t, spec)
    s.leading = leading(t, spec)
    return s
  end

  --- Search: a round-ended well and a magnifier.
  function LOOK.search(t, spec)
    local function r() return math.max(0, (t.height or 0) - foot(spec)) / 2 end
    return { background = well(t, spec, { radius = r }), placeholder = title(t, spec),
      error = ring(t, spec, r), leading = leading(t, spec) }
  end

  --- Text over several lines: the view's white page in a hairline, the
  --- title fixed above the text, a count and a grip in the corner.
  function LOOK.text_area(t, spec)
    local s = { error = ring(t, spec, R.medium), placeholder = title(t, spec, true), counter = supporting(t, spec) }
    s.background = well(t, spec, { radius = R.medium, color = function() return P().view end,
      edge = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 1 or 0.9) end })
    s.content = ui.Path { anchors = { right = true, bottom = true, right_margin = 4, bottom_margin = FOOT + 4 },
      width = 10, height = 10, view_box = { 0, 0, 10, 10 }, d = "M9 2 L2 9 M9 6 L6 9", fill_color = "transparent",
      stroke_width = 1.2, stroke_cap = "round", stroke_color = function() return P().ink_dim end }
    if not spec.supporting then
      -- (A count needs the line under it.)
      s.content.anchors = { right = true, bottom = true, right_margin = 4, bottom_margin = 4 }
    end
    return s
  end

  --- An address: its glyph, and a mark at the end once it can be judged --
  --- a check when it would be accepted, an alert when not.
  local function judged(t, spec)
    local s = LOOK.entry(t, spec)
    s.leading = leading(t, spec)
    s.trailing = M.icon(function() return bad(t) and "error" or "check_circle" end, 18,
      function() local p = P() return bad(t) and p.error_ink or p.success_ink end,
      { anchors = { right = true, right_margin = 14 },
        y = function() return math.floor(((t.height or 0) - foot(spec) - 18) / 2) end,
        opacity = function() return t.empty and 0 or 1 end,
        scale = function() return t.empty and 0.6 or 1 end,
        behavior = { opacity = quick(), scale = settle() } })
    return s
  end
  LOOK.url = judged
  LOOK.email = judged

  --- A number: its unit after it.
  function LOOK.numeric_entry(t, spec)
    local s = LOOK.entry(t, spec)
    if spec.unit then
      s.trailing = M.text { text = spec.unit, anchors = { right = true, right_margin = 14 }, height = 20,
        y = function() local _, top, _, bottom = sides(spec.inset) return top + math.floor(((t.height or 0) - foot(spec) - top - bottom - 20) / 2) end,
        font_size = theme.size.normal, vertical_alignment = "center", color = function() return P().ink_dim end }
    end
    return s
  end

  --- A one-time code: a cell per digit, the one being typed into ringed;
  --- the digits are drawn in the cells (the input under them is hidden).
  function LOOK.otp(t, spec)
    local n = tonumber(spec.max_length) or 6
    local gap = 8
    if spec.input then spec.input.color, spec.input.caret_color = "transparent", "transparent" end
    local function cell_w() return math.max(0, ((t.width or 0) - gap * (n - 1)) / n) end
    local cells = ui.Item { anchors = { fill = true } }
    for i = 1, n do
      local function x() return (i - 1) * (cell_w() + gap) end
      local function current() return t.focused and ((t.length or 0) + 1 == i or ((t.length or 0) == n and i == n)) end
      ui.reparent(ui.Rect { x = x, width = cell_w, height = function() return t.height or 0 end, radius = R.medium,
        color = function()
          local p = P()
          return p.ink:alpha((i <= (t.length or 0)) and (p.dark and 0.12 or 0.1) or (p.dark and 0.06 or 0.05))
        end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        behavior = { color = quick() } }, cells)
      ui.reparent(ui.Rect { x = x, width = cell_w, height = function() return t.height or 0 end, radius = R.medium,
        color = "transparent", border_width = 2, z = 2,
        border_color = function() local p = P() return p.strong and p.accent or p.focus:alpha(0.7) end,
        opacity = function() return current() and 1 or 0 end,
        scale = function() return current() and 1 or 1.08 end,
        behavior = { opacity = quick(), scale = settle() } }, cells)
      ui.reparent(M.text { x = x, width = cell_w, height = function() return t.height or 0 end, z = 3,
        horizontal_alignment = "center", vertical_alignment = "center", font_size = theme.size.large,
        font_weight = 700, text = function() return (t.text or ""):sub(i, i) end,
        scale = function() return i <= (t.length or 0) and 1 or 0.4 end,
        behavior = { scale = settle() } }, cells)
    end
    return { background = cells, placeholder = ui.Item {}, error = ui.Item {}, counter = ui.Item {} }
  end

  --- Tags: the entered ones as accent chips along the top, the input on the
  --- line under them.
  function LOOK.tag_input(t, spec)
    local s = LOOK.entry(t, spec)
    local l = sides(spec.inset)
    local row = { x = l, y = 24, gap = 6 }
    for _, tag in ipairs(spec.tags or {}) do
      local label = M.text { text = tostring(tag), x = 10, height = 24, vertical_alignment = "center",
        font_size = theme.size.small, color = function() return P().accent_ink end }
      row[#row + 1] = ui.Rect { height = 24, radius = 12, accessible_name = tostring(tag),
        width = function() return (label.layout_width or 0) + 34 end,
        color = function() local p = P() return p.accent:alpha(p.dark and 0.3 or 0.15) end,
        label,
        M.icon("close", 14, function() return P().accent_ink end,
          { y = 5, x = function() return (label.layout_width or 0) + 14 end }) }
    end
    s.leading = ui.Row(row)
    return s
  end

  --- Mentions: a composer -- the page-white capsule, the @ before it, a
  --- send disc after.
  function LOOK.mentions(t, spec)
    local function r() return math.max(0, (t.height or 0) - foot(spec)) / 2 end
    local function box() return math.max(0, (t.height or 0) - foot(spec)) end
    return {
      background = well(t, spec, { radius = r, color = function() return P().view end,
        edge = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 1 or 0.9) end }),
      error = ring(t, spec, r), placeholder = title(t, spec),
      leading = leading(t, spec, function() return P().accent_ink end),
      trailing = ui.Rect { anchors = { right = true, right_margin = 6 }, width = function() return box() - 12 end,
        height = function() return box() - 12 end, y = 6, radius = function() return (box() - 12) / 2 end,
        color = function() local p = P() return t.empty and p.ink:alpha(0.1) or p.accent end,
        behavior = { color = quick() },
        M.icon("arrow_upward", 18, function() local p = P() return t.empty and p.ink_dim or p.on_accent end,
          { anchors = { center_in = true } }) },
    }
  end

  --- A rename in place: only the words until it is hovered or focused, then
  --- the page-white well and the ring; a pencil after it while idle.
  function LOOK.inline_rename(t, spec)
    return {
      background = well(t, spec, { color = function()
        local p = P()
        if t.focused then return p.view end
        return p.ink:alpha(t.hovered and p.wash.hover or 0)
      end }),
      error = ring(t, spec),
      trailing = spec.icon and M.icon(spec.icon, 16, function() return P().ink_dim end,
        { anchors = { right = true, right_margin = 10, vertical_center = true },
          opacity = function() return t.focused and 0 or (t.hovered and 1 or 0.6) end,
          behavior = { opacity = quick() } }) or nil,
      placeholder = ui.Item {},
    }
  end

  --- An entry row: a boxed list row -- the card, its title floating, the
  --- apply mark after it that lights once focused.
  function LOOK.entry_row(t, spec)
    return {
      background = well(t, spec, { radius = R.large, color = function()
          local p = P()
          return t.hovered and not t.focused and p.card:mix(p.ink, p.wash.hover) or p.card
        end,
        edge = function() local p = P() return (p.dark and not p.strong) and p.card or p.border end }),
      placeholder = title(t, spec), error = ring(t, spec, R.large),
      trailing = spec.icon and ui.Rect { anchors = { right = true, right_margin = 12, vertical_center = true },
        width = 30, height = 30, radius = 15,
        color = function() local p = P() return t.focused and p.accent or p.ink:alpha(0) end,
        behavior = { color = quick() },
        M.icon(spec.icon, 18, function() local p = P() return t.focused and p.on_accent or p.ink_dim end,
          { anchors = { center_in = true } }) } or nil,
      counter = supporting(t, spec),
    }
  end

  --- A filter: a compact well that takes the accent's tint while it is
  --- filtering, its glyph with it.
  function LOOK.filter_field(t, spec)
    return {
      background = well(t, spec, { color = function()
        local p = P()
        if not t.empty then return p.accent:alpha(p.dark and 0.24 or 0.12) end
        return p.ink:alpha(t.hovered and 0.1 or 0.07)
      end }),
      error = ring(t, spec),
      leading = leading(t, spec, function() local p = P() return t.empty and p.ink_dim or p.accent_ink end),
      placeholder = ui.Item {},
    }
  end

  --- Code: a page with a gutter of line numbers in the mono face.
  function LOOK.code_input(t, spec)
    local l, top = sides(spec.inset)
    local size = get(spec.font_size) or 14
    local numbers = M.text { x = 4, y = top, width = math.max(0, l - 14), horizontal_alignment = "right",
      font_family = theme.mono, font_size = size, line_height = 1.2, z = 3,
      color = function() return P().ink_dim end,
      text = function()
        local lines = select(2, (t.text or ""):gsub("\n", "\n")) + 1
        local out = {}
        for i = 1, lines do out[i] = tostring(i) end
        return table.concat(out, "\n")
      end }
    return {
      background = ui.Item { anchors = { fill = true },
        well(t, spec, { radius = R.small, color = function() return P().view end,
          edge = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 1 or 0.9) end }),
        ui.Rect { anchors = { left = true, top = true, bottom = true, margins = 1 }, width = math.max(0, l - 8),
          top_left_radius = R.small, bottom_left_radius = R.small,
          color = function() local p = P() return p.ink:alpha(p.dark and 0.06 or 0.04) end } },
      leading = numbers, error = ring(t, spec), placeholder = ui.Item {},
    }
  end

  for widget, look in pairs(LOOK) do S[widget] = look end
end
