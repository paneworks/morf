-- The default kit's Press skin, in the Adwaita manner: grey buttons that
-- darken under the pointer, the accent or the destructive red for a
-- suggested or destructive action, flat buttons that show only a wash,
-- raised ones on their shade, pills and circles, osd buttons dark and
-- translucent; switches with a white knob, accent-filled checks and
-- radios, menu rows with their marks. The behaviour is the archetype's.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick, ring = draw.get, draw.quick, draw.ring
  local has = require("lib.kit.archetypes.press").has
  local caption = require("lib.kit.archetypes.press").caption

  local function height(t) return (t.height and t.height > 0) and t.height or theme.control_height end

  --- GtkButton, by its classes; a toggle button is one that stays checked.
  local function button(t, spec)
    local suggested, destructive, opaque = has(spec, "suggested"), has(spec, "destructive"), has(spec, "opaque")
    local flat, raised, osd = has(spec, "flat"), has(spec, "raised"), has(spec, "osd")
    local round = has(spec, "pill") or has(spec, "circular")
    local filled = suggested or destructive or opaque
    local icon_only = spec.icon and not spec.label
    local function radius() return round and height(t) / 2 or R.small end
    local function fill()
      local p = P()
      if destructive then return p.destructive_bg end
      if opaque then return get(spec.color) or p.accent_bg end
      return p.accent_bg
    end
    local function ink()
      local p = P()
      if filled then return get(spec.ink) or (destructive and p.destructive_fg or p.accent_fg) end
      if osd then return morf.color("#ffffff") end
      return get(spec.ink) or p.window_fg
    end
    local function ground()
      local p = P()
      local w = p.wash
      if filled then
        local c = fill()
        if t.down then return c:mix(p.shade, 0.3) end
        if t.hovered then return c:mix(p.accent_fg, 0.1) end
        return c
      end
      if osd then
        if flat then return morf.color("#ffffff"):alpha(t.down and 0.25 or (t.hovered and 0.15 or 0)) end
        local a = t.down and 0.9 or (t.hovered and 0.78 or 0.65)
        return morf.color("#000000"):alpha(a)
      end
      if flat then
        if t.down then return p.window_fg:alpha(w.active) end
        if t.checked then return p.window_fg:alpha(w.checked) end
        if t.hovered then return p.window_fg:alpha(w.hover) end
        return p.window_fg:alpha(0)
      end
      if t.down then return p.window_fg:alpha(w.checked + 0.06) end
      if t.checked then return p.window_fg:alpha(w.checked) end
      if t.hovered then return p.window_fg:alpha(w.raised_hover) end
      return p.window_fg:alpha(w.button)
    end
    local function edge()
      local p = P()
      if raised then return 1 end
      if p.strong and not flat then return 1 end
      if p.strong and flat and t.hovered then return 1 end
      return 0
    end
    local weight = 700
    local content
    if icon_only then
      local glyph = math.max(16, math.min(24, math.floor(height(t) * 0.5)))
      content = draw.icon(spec.icon, glyph, ink, { anchors = { center_in = true } })
    else
      local side = has(spec, "pill") and 32 or 10
      content = caption { width = spec.width, height = function() return height(t) end, gap = 6, left = side,
        right = side,
        before = spec.icon and { draw.icon(spec.icon, 16, ink) } or {},
        measure = draw.text { text = spec.label or "", font_weight = weight, opacity = 0 },
        label_visible = function() local s = get(spec.label) return s ~= nil and s ~= "" end,
        label = function(w)
          return draw.text { text = spec.label or "", width = w, font_weight = weight, color = ink, elide = "right" }
        end }
    end
    if spec.loading then
      content.visible = function() return not get(spec.loading) end
      content = ui.Item { anchors = { fill = true }, content,
        draw.spinner(16, ink, { anchors = { center_in = true }, visible = function() return get(spec.loading) == true end }) }
    end
    return {
      background = ui.Rect { anchors = { fill = true }, radius = radius, color = ground,
        border_width = edge,
        border_color = function()
          local p = P()
          if raised and not p.strong then return p.shade:alpha(p.dark and 0.9 or 0.7) end
          return p.border
        end,
        shadow_color = raised and function() return P().shade:alpha(0.45) end or nil,
        shadow_blur = raised and 2 or nil, shadow_offset_y = raised and function() return t.down and 0 or 1 end or nil,
        behavior = { color = quick() } },
      content = content,
      indicator = ring(t, radius),
    }
  end

  --- GtkSwitch: a pill trough, grey off and the accent on, and a white
  --- knob that slides across.
  local function switch(t)
    local TW, TH, K = 46, 26, 22
    local function oy() return math.floor(((t.height and t.height > 0 and t.height or 32) - TH) / 2) end
    local function ox() return math.floor(((t.width and t.width > 0 and t.width or 52) - TW) / 2) end
    return {
      background = ui.Rect { x = ox, y = oy, width = TW, height = TH, radius = TH / 2,
        color = function()
          local p = P()
          if t.checked then return t.hovered and p.accent_bg:mix(p.accent_fg, 0.1) or p.accent_bg end
          return p.window_fg:alpha(t.hovered and p.wash.raised_hover + 0.05 or p.wash.raised_hover)
        end,
        border_width = function() return P().strong and 1 or 0 end,
        border_color = function() return P().border end,
        behavior = { color = quick() } },
      indicator = ui.Rect { width = K, height = K, radius = K / 2,
        y = function() return oy() + (TH - K) / 2 end,
        x = function() return ox() + ((t.checked ~= (t.mirrored == true)) and (TW - K - 2) or 2) end,
        color = function() return P().knob end,
        border_width = 1,
        border_color = function() local p = P() return p.strong and p.window_fg or p.shade:alpha(p.dark and 0.5 or 0.35) end,
        behavior = { x = draw.spring(420, 40) } },
      badge = ring(t, TH / 2),
    }
  end

  --- GtkCheckButton: a rounded square outlined while off, the accent with a
  --- white tick when checked, a dash when partial; in a `group`, a radio's
  --- ring and dot. Its label after it.
  local function check_button(t, spec)
    local radio = spec.group ~= nil
    local S0 = 18
    local function on() return t.checked or (not radio and t.partial) end
    local box = ui.Rect { x = 3, width = S0, height = S0, radius = radio and S0 / 2 or 5,
      y = function() return math.floor((((t.height and t.height > 0) and t.height or 24) - S0) / 2) end,
      color = function()
        local p = P()
        if on() then return t.hovered and p.accent_bg:mix(p.accent_fg, 0.1) or p.accent_bg end
        return t.hovered and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
      end,
      border_width = function() return on() and 0 or 2 end,
      border_color = function() local p = P() return p.strong and p.window_fg or p.window_fg:alpha(0.3) end,
      behavior = { color = quick() } }
    local mark
    if radio then
      mark = ui.Rect { anchors = { center_in = true }, width = 6, height = 6, radius = 3,
        color = function() return P().accent_fg end, visible = function() return t.checked end }
    else
      mark = draw.icon(function() return t.partial and "remove" or "check" end, 16, function() return P().accent_fg end,
        { anchors = { center_in = true }, visible = function() return on() end, font_weight = 700 })
    end
    ui.reparent(mark, box)
    local slots = { background = box, badge = ring(t, radio and 12 or 6) }
    if spec.label then
      slots.content = ui.Row { gap = 6, align = "center",
        ui.Item { width = 24, height = 24 },
        draw.text { text = spec.label, elide = "right" } }
    end
    return slots
  end

  --- GtkLinkButton: the accent's words, underlined under the pointer.
  local function link_button(t, spec)
    return {
      content = draw.text { anchors = { center_in = true }, text = spec.label or spec.uri or "",
        color = function() return P().accent end,
        decoration = function() return t.hovered and { line = "under" } or {} end },
      indicator = ring(t, R.small),
    }
  end

  --- A GtkPopoverMenu row: its icon and label on a hover wash; `kind`
  --- "check" a tick at the right while on, "radio" a radio mark.
  local function menu_item(t, spec)
    local left = spec.icon and 38 or 12
    local marked = spec.kind == "check" or spec.kind == "radio"
    local right = marked and 36 or 12
    local slots = {
      background = ui.Rect { anchors = { fill = true }, radius = R.small,
        color = function()
          local p = P()
          if t.down then return p.window_fg:alpha(p.wash.active) end
          return (t.hovered or t.visual_focus) and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
        end, behavior = { color = quick() } },
      icon = spec.icon and draw.icon(spec.icon, 16, function() return P().popover_fg end,
        { x = 12, anchors = { vertical_center = true } }) or nil,
      label = draw.text { x = left, anchors = { vertical_center = true }, text = spec.label,
        width = function() return math.max(1e-3, t.width - left - right) end, elide = "right",
        color = function() return P().popover_fg end },
    }
    if marked then
      slots.indicator = draw.icon(function()
        if spec.kind == "radio" then return t.checked and "radio_button_checked" or "radio_button_unchecked" end
        return "check"
      end, 16, function() return P().popover_fg end, { anchors = { right = true, right_margin = 12, vertical_center = true },
        visible = function() return spec.kind == "radio" or t.checked end })
    end
    return slots
  end

  --- AdwTabButton: the count of open tabs in a rounded square.
  local function tab_button(t, spec)
    local slots = button(t, { widget = "button", variant = "flat", width = spec.width })
    slots.content = ui.Item { anchors = { center_in = true }, width = 18, height = 18,
      ui.Rect { anchors = { fill = true }, radius = 4, color = "transparent", border_width = 2,
        border_color = function() return P().window_fg end },
      draw.text { anchors = { center_in = true }, font_size = 10, font_weight = 800,
        text = function() local n = tonumber(get(spec.count)) or 0 return n > 99 and ":)" or tostring(n) end } }
    return slots
  end

  local DRAW = { switch = switch, check_button = check_button, link_button = link_button, menu_item = menu_item,
    tab_button = tab_button }

  function S.Press(t, spec)
    local draw = DRAW[spec.widget]
    if look then return look(t, spec) end
    return button(t, spec)
  end
end
