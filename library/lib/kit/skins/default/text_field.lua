-- The default kit's TextField skin, in the Adwaita manner: soft grey wells
-- with 6 px corners, a 2 px accent ring that settles in on focus (the error
-- tone while what is typed would not be accepted), and a title that floats
-- up and shrinks once there is something to show -- as an entry row's
-- does. A password entry adds its lock, a search entry a round well and a
-- magnifier, a text view the page-white well, an editable label only its
-- words until it is reached. The input itself is the configuration's;
-- what it accepts, the archetype's.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick = draw.get, draw.quick
  local function settle() return draw.spring(520) end

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
        if bad(t) then return p.error_bg:alpha(0.1) end
        return p.window_fg:alpha((t.hovered and not t.focused) and (p.dark and 0.11 or 0.1) or (p.dark and 0.08 or 0.07))
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
        if bad(t) then return p.error_bg end
        return p.strong and p.accent_bg or p.focus:alpha(0.6)
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
    local l,_,r = sides(spec.inset)
    local size = theme.size.normal
    local lh = math.ceil(size * 1.3)
    local function box() return math.max(0, (t.height or 0) - foot(spec)) end
    local function rest() return math.floor((box() - lh) / 2) end
    local function up() return always_up or t.focused or not t.empty or (spec.tags ~= nil) end
    return draw.text { text = spec.label, x = l, height = lh, font_size = size, vertical_alignment = "center",
      width=function() return math.max(1e-3,t.width-l-r) end,elide="right",
      transform_origin_x = 0, transform_origin_y = 0, z = 4,
      y = rest,
      translate_y = function() return up() and (6 - rest()) or 0 end,
      scale = function() return up() and 0.8 or 1 end,
      color = function()
        local p = P()
        if bad(t) then return p.error end
        if t.focused then return p.accent end
        return p.dim_fg
      end,
      behavior = { translate_y = settle(), scale = settle(), color = quick() } }
  end

  --- A glyph at the start, centred on the well (or on its text line).
  local function leading(t, spec, color, y)
    if not spec.icon then return nil end
    return draw.icon(spec.icon, 18, color or function()
        local p = P()
        if bad(t) then return p.error end
        return t.focused and p.accent or p.dim_fg
      end,
      { x = 15, y = y or function() return math.floor(((t.height or 0) - foot(spec) - 18) / 2) end,
        behavior = { color = quick() } })
  end

  --- The supporting line under the well, the error tone while it would not
  --- be accepted; the count against `max_length` at its end.
  local function supporting(t, spec)
    if not spec.supporting and not spec.max_length then return nil end
    local l = sides(spec.inset)
    local function tone() local p = P() return bad(t) and p.error or p.dim_fg end
    local node = ui.Item { anchors = { left = true, right = true, bottom = true }, height = FOOT }
    local counter
    if spec.max_length then
      -- (Clear of a corner grip when there is no line of its own.)
      counter=draw.text { anchors = { right = true, bottom = true, right_margin = spec.supporting and 6 or 22,
        bottom_margin = spec.supporting and 0 or 2 }, height = 18,
        font_size = theme.size.small, color = tone,
        text = function() return ("%d / %d"):format(t.length or 0, spec.max_length) end }
      ui.reparent(counter,node)
    end
    if spec.supporting then
      ui.reparent(draw.text {text=spec.supporting,x=l,anchors={bottom=true},height=18,
        width=function() return math.max(1e-3,t.width-l-6-(counter and (counter.layout_width or 0)+8 or 0)) end,
        elide="right",font_size=theme.size.small,color=tone,behavior={color=quick()}},node)
    end
    return node
  end

  -- -------------------------------------------------------------- looks --


  local LOOK = {}

  --- GtkEntry: the well, its title, the ring.
  function LOOK.entry(t, spec)
    return { background = well(t, spec), placeholder = title(t, spec), error = ring(t, spec),
      counter = supporting(t, spec) }
  end

  --- GtkPasswordEntry: a lock before it (the reveal is the archetype skin's).
  function LOOK.password_entry(t, spec)
    local s = LOOK.entry(t, spec)
    s.leading = leading(t, spec)
    return s
  end

  --- GtkSearchEntry: a round-ended well and a magnifier.
  function LOOK.search_entry(t, spec)
    local function r() return math.max(0, (t.height or 0) - foot(spec)) / 2 end
    return { background = well(t, spec, { radius = r }), placeholder = title(t, spec),
      error = ring(t, spec, r), leading = leading(t, spec) }
  end

  --- GtkTextView: the view's white page in a hairline, the
  --- title fixed above the text, a count and a grip in the corner.
  function LOOK.text_view(t, spec)
    local s = { error = ring(t, spec, R.medium), placeholder = title(t, spec, true), counter = supporting(t, spec) }
    s.background = well(t, spec, { radius = R.medium, color = function() return P().view_bg end,
      edge = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 1 or 0.9) end })
    s.content = ui.Path { anchors = { right = true, bottom = true, right_margin = 4, bottom_margin = FOOT + 4 },
      width = 10, height = 10, view_box = { 0, 0, 10, 10 }, d = "M9 2 L2 9 M9 6 L6 9", fill_color = "transparent",
      stroke_width = 1.2, stroke_cap = "round", stroke_color = function() return P().dim_fg end }
    if not spec.supporting then
      -- (A count needs the line under it.)
      s.content.anchors = { right = true, bottom = true, right_margin = 4, bottom_margin = 4 }
    end
    return s
  end

  --- GtkEditableLabel: only the words until it is hovered or focused, then
  --- the page-white well and the ring; a pencil after it while idle.
  function LOOK.editable_label(t, spec)
    return {
      background = well(t, spec, { color = function()
        local p = P()
        if t.focused then return p.view_bg end
        return p.window_fg:alpha(t.hovered and p.wash.hover or 0)
      end }),
      error = ring(t, spec),
      trailing = spec.icon and draw.icon(spec.icon, 16, function() return P().dim_fg end,
        { anchors = { right = true, right_margin = 10, vertical_center = true },
          opacity = function() return t.focused and 0 or (t.hovered and 1 or 0.6) end,
          behavior = { opacity = quick() } }) or nil,
      placeholder = ui.Item {},
    }
  end

  --- AdwEntryRow's field: a boxed list row -- the card, its title floating, the
  --- apply mark after it that lights once focused.
  function LOOK.entry_row(t, spec)
    return {
      background = well(t, spec, { radius = R.large, color = function()
          local p = P()
          return t.hovered and not t.focused and p.card_bg:mix(p.window_fg, p.wash.hover) or p.card_bg
        end,
        edge = function() local p = P() return (p.dark and not p.strong) and p.card_bg or p.border end }),
      placeholder = title(t, spec), error = ring(t, spec, R.large),
      trailing = spec.icon and ui.Rect { anchors = { right = true, right_margin = 12, vertical_center = true },
        width = 30, height = 30, radius = 15,
        color = function() local p = P() return t.focused and p.accent_bg or p.window_fg:alpha(0) end,
        behavior = { color = quick() },
        draw.icon(spec.icon, 18, function() local p = P() return t.focused and p.accent_fg or p.dim_fg end,
          { anchors = { center_in = true } }) } or nil,
      counter = supporting(t, spec),
    }
  end


  for widget, look in pairs(LOOK) do S[widget] = look end

  --- Any text field: the grey well when `well` is asked, a 2 px accent ring
  --- on focus (the error tone while what is typed would not be accepted), a
  --- clear or reveal button, and a counter.
  function S.TextField(t, spec, _, send)
    local slots = {
      error = ui.Rect { anchors = { fill = true }, radius = R.small, color = "transparent", border_width = 2,
        border_color = function()
          local p = P()
          if bad(t) then return p.error_bg end
          return p.strong and p.accent_bg or p.focus:alpha(0.6)
        end,
        opacity = function() return (t.focused or bad(t)) and 1 or 0 end,
        behavior = { opacity = quick() } },
    }
    if spec.well then
      slots.background = ui.Rect { anchors = { fill = true }, radius = R.small,
        color = function()
          local p = P()
          return bad(t) and p.error_bg:alpha(0.12) or p.window_fg:alpha(p.dark and 0.08 or 0.07)
        end,
        border_width = function() return P().strong and 1 or 0 end,
        border_color = function() return P().border end }
    end
    if spec.clear or spec.reveal then
      local reveal = spec.reveal and spec.echo == "password"
      slots.trailing = require("lib.kit.archetypes.press").make("button", { width = 26, height = 26,
        variant = "flat circular", anchors = { right = true, right_margin = 5, vertical_center = true },
        icon = function()
          if not reveal then return "close" end
          return t.revealed and "visibility_off" or "visibility"
        end,
        accessible_name = reveal and "Show the password" or "Clear",
        visible = function() return reveal or not t.empty end,
        on_clicked = function() send(reveal and "reveal" or "clear") end })
    end
    if spec.max_length then
      slots.counter = draw.text { anchors = { right = true, bottom = true, right_margin = 6, bottom_margin = 3 },
        font_size = theme.size.small - 2,
        color = function() local p = P() return bad(t) and p.error or p.dim_fg end,
        text = function() return ("%d/%d"):format(t.length, spec.max_length) end }
    end
    return slots
  end
end
