-- Material's skins for the kit's archetypes (lib.kit.skin): the looks of
-- every press and range, drawn from a control's live state `t` -- the
-- behaviour (pressing, dragging, keys, focus) is the archetype's.
local morf = require("morf")
local ui = require("morf.ui")

return function(theme, M)
  local S = {}
  local function C() return theme.color end
  local function get(v) if type(v) == "function" then return v() end return v end
  local function clamp01(v) return math.max(0, math.min(1, v)) end
  -- The keyboard's ring: a 2 px secondary outline inside the edge, made the
  -- first time the control is reached (lib.kit.skin: a slot given as a
  -- function) -- most never are.
  local function ring(t, radius)
    return function()
      return ui.Rect { anchors = { fill = true, margins = 1 }, z = 50, color = "transparent", radius = radius,
        border_width = 2, border_color = function() return C().secondary end,
        visible = function() return t.visual_focus end }
    end
  end

  -- ----------------------------------------------------------- presses --

  --- A pill-shaped filled button: `icon`, `label`, `color`/`ink`
  --- (primaryContainer and its ink).
  local function pill(t, spec)
    local color = spec.color or function() return C().primaryContainer end
    local ink = spec.ink or function() return C().onPrimaryContainer end
    local h = function() return t.height > 0 and t.height or 32 end
    return {
      background = ui.Rect { anchors = { fill = true },
        radius = function() return t.down and h() * 0.22 or h() / 2 end,
        color = function()
          local c = color()
          if t.down then return c:mix(ink(), 0.12) end
          return t.hovered and c:mix(ink(), 0.08) or c
        end,
        behavior = { color = { duration = theme.duration.small }, radius = ui.spring { stiffness = 520, damping = 22 } } },
      content = ui.Row { anchors = { center_in = true }, gap = 8, align = "center",
        spec.icon and M.icon(spec.icon, 18, ink) or nil,
        M.text { text = spec.label, font_size = theme.size.normal, color = ink } },
      indicator = ring(t, function() return h() / 2 end),
    }
  end

  --- The M3 switch: a track that fills when on, and a thumb that springs
  --- across, grows when on and more when pressed, and morphs from a circle
  --- to a scalloped cookie.
  local function switch(t)
    local motion = { duration = theme.duration.small, easing = theme.ease.standard }
    local function thumb() return t.down and 28 or (t.checked and 24 or 16) end
    local jump = M.spring(520, 22)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = 16,
        color = function() return t.checked and C().primary or C().surfaceContainerHighest end,
        border_width = function() return t.checked and 0 or 2 end,
        border_color = function() return C().outline end,
        behavior = { color = motion } },
      indicator = ui.Item {
        x = function() return (t.checked and 36 or 16) - thumb() / 2 end,
        y = function() return 16 - thumb() / 2 end,
        width = thumb, height = thumb,
        behavior = { x = jump, y = jump, width = jump, height = jump },
        stretch = M.STRETCH,
        M.shape { anchors = { fill = true },
          shape = function() return t.checked and "cookie12" or "circle" end,
          color = function() return t.checked and C().onPrimary or C().outline end },
        M.icon(function() return t.checked and "check" or "close" end, 14, function()
          return t.checked and C().primary or C().surfaceContainerHighest
        end, { anchors = { center_in = true }, visible = function() return thumb() >= 20 end }),
      },
      badge = ring(t, 16),
    }
  end

  --- A small icon toggle: `icon_on`, `icon_off`, `on` (fn; the error tone
  --- while on), `size` (20). Round off, a rounded square on, tighter while
  --- pressed, its icon filling in.
  local function icon(t, spec)
    local W, H = spec.width or 36, spec.height or 30
    local h = math.min(W, H)
    local function on() return spec.on and spec.on() == true end
    local function ink() return on() and C().onErrorContainer or C().onSurfaceVariant end
    return {
      background = ui.Rect { width = W, height = H,
        radius = function()
          if t.down then return h * 0.2 end
          return on() and h * 0.3 or h / 2
        end,
        color = function()
          local base = on() and C().errorContainer or C().surfaceContainerHighest
          if t.down then return base:mix(ink(), 0.12) end
          if t.hovered then return base:mix(ink(), 0.08) end
          return base
        end,
        behavior = { radius = ui.spring { stiffness = 480, damping = 18 }, color = { duration = theme.duration.small } } },
      icon = M.icon(function() return on() and spec.icon_on or spec.icon_off end, spec.size or 20, ink,
        { anchors = { center_in = true }, fill = on }),
      indicator = ring(t, h / 2 - 1),
    }
  end

  --- A checkbox: a rounded square, filled and ticked when checked, a dash
  --- when partial.
  local function checkbox(t)
    return {
      background = ui.Rect { anchors = { fill = true, margins = 3 }, radius = 3,
        color = function() return (t.checked or t.partial) and C().primary or "transparent" end,
        border_width = function() return (t.checked or t.partial) and 0 or 2 end,
        border_color = function() return C().onSurfaceVariant end,
        behavior = { color = { duration = theme.duration.small } } },
      indicator = M.icon(function() return t.partial and "remove" or "check" end, 16,
        function() return C().onPrimary end,
        { anchors = { center_in = true }, visible = function() return t.checked or t.partial end }),
      badge = ring(t, 6),
    }
  end

  --- A radio button: a ring with a dot that grows in when checked.
  local function radio(t)
    return {
      background = ui.Rect { anchors = { fill = true, margins = 2 }, radius = 10, color = "transparent",
        border_width = 2, border_color = function() return t.checked and C().primary or C().onSurfaceVariant end },
      indicator = ui.Rect { anchors = { center_in = true }, color = function() return C().primary end,
        width = function() return t.checked and 10 or 0 end, height = function() return t.checked and 10 or 0 end,
        radius = 5, behavior = { width = M.spring(520, 22), height = M.spring(520, 22) } },
      badge = ring(t, 12),
    }
  end

  function S.Press(t, spec)
    local widget = spec.widget
    -- A layout's own area draws itself: only the keyboard's ring here.
    if widget == "area" then
      return { indicator = ring(t, function() return math.min(16, t.height / 2) end) }
    end
    if widget == "switch" then return switch(t)
    elseif widget == "icon" then return icon(t, spec)
    elseif widget == "checkbox" or widget == "check_menu_item" then return checkbox(t)
    elseif widget == "radio" or widget == "radio_menu_item" then return radio(t)
    end
    return pill(t, spec)
  end

  -- ------------------------------------------------------------ ranges --

  --- The M3 expressive slider: the active part, a gap, a slim upright
  --- handle standing past the track, the rest of the track, an icon inside
  --- the active part once it fits and the reading at the end. `spec`:
  --- `width`, `height` (44, the track's), `icon`, `label` (false hides it).
  local function slider(t, spec)
    local W, H = spec.width, spec.bar_height or 44
    local GAP = 6
    local motion = M.spring(190, 9)
    local function hx() return H / 2 + (W - H) * clamp01(t.visual_position) end
    local function grip() return t.down and 2 or 4 end
    local slots = {
      -- The travel the pointer maps onto: the handle's centre keeps the
      -- track's rounded ends clear.
      track = ui.Item { x = H / 2, y = 4, width = W - H, height = H },
      background = ui.Rect { y = 4, height = H,
        x = function() return hx() + grip() / 2 + GAP end,
        width = function() return math.max(0, W - (hx() + grip() / 2 + GAP)) end,
        top_left_radius = 6, bottom_left_radius = 6, top_right_radius = H / 2, bottom_right_radius = H / 2,
        color = function() return C().surfaceContainerHighest end,
        behavior = { x = motion, width = motion } },
      fill = ui.Rect { id = spec.id and spec.id .. "-level", x = 0, y = 4, height = H,
        width = function() return math.max(0, hx() - grip() / 2 - GAP) end,
        top_left_radius = H / 2, bottom_left_radius = H / 2, top_right_radius = 6, bottom_right_radius = 6,
        color = function() return C().primary end,
        behavior = { width = motion } },
      handle = ui.Rect { id = spec.id and spec.id .. "-handle", y = 0, height = H + 8, radius = 2,
        x = function() return hx() - grip() / 2 end, width = grip,
        color = function() return C().primary end,
        behavior = { x = motion, width = { duration = 150 } } },
      second_handle = ring(t, H / 2),
    }
    local size = math.floor(H / 2)
    if spec.icon then
      local function inside() return hx() - GAP > size + 18 end
      slots.ticks = M.icon(spec.icon, size, function()
        return inside() and C().onPrimary or C().onSurfaceVariant
      end, { y = 4 + (H - size) / 2,
        x = function()
          if inside() then return math.floor(H / 2 - size / 2) end
          return math.floor(hx() + grip() / 2 + GAP + 6)
        end,
        behavior = { x = motion } })
    end
    if spec.label ~= false then
      slots.value_label = M.text { id = spec.id and spec.id .. "-value", width = 40, horizontal_alignment = "right",
        y = 4 + (H - 20) / 2, height = 20,
        x = function()
          if hx() > W - 64 then return hx() - GAP - 10 - 40 end
          return W - 14 - 40
        end,
        text = function() return ("%d"):format(math.floor(clamp01(t.position) * 100 + 0.5)) end,
        font_size = H >= 40 and theme.size.normal or theme.size.small,
        color = function() return hx() > W - 64 and C().onPrimary or C().onSurfaceVariant end,
        behavior = { x = motion } }
    end
    return slots
  end

  --- The media rail: a wave that runs while playing, up to an upright
  --- thumb, then the rest of the track and a stop dot. `spec`: `width`,
  --- `active` and `playing` (fns).
  local function seek_bar(t, spec)
    local W, WAVE = spec.width, 38
    local function wave_path(width)
      local d = { "M0 6" }
      for k = 1, math.ceil(width / 2) do
        local x = k * 2
        d[#d + 1] = ("L%d %.2f"):format(x, 6 - 4 * math.sin(x / WAVE * 2 * math.pi))
      end
      return table.concat(d, " ")
    end
    local function at() return clamp01(t.visual_position) end
    -- A drag follows at once; the player's own ticks flow over a second.
    local motion = function() return t.dragging and { duration = 60 } or { duration = 1000, easing = "linear" } end
    local function playing() return get(spec.active) ~= false and spec.playing and spec.playing() end
    return {
      track = ui.Item { width = W, height = 34 },
      fill = ui.Item { y = 11, width = W, height = 12,
        ui.Item { id = "media-progress-wave", x = 0, y = 0, height = 12, clip = true,
          width = function() return math.max(0, at() * W - 6) end,
          behavior = { width = motion() },
          ui.Path { width = W + WAVE, height = 12, view_box = { 0, 0, W + WAVE, 12 }, d = wave_path(W + WAVE),
            fill_color = "transparent", stroke_width = 5, stroke_cap = "round",
            stroke_color = function() return C().primary end,
            loop = function()
              if not playing() then return nil end
              return { translate_x = { from = 0, to = -WAVE, duration = 1300, easing = "linear" } }
            end } } },
      background = ui.Item { width = W, height = 34,
        M.surface { y = 13, height = 8, radius = 4,
          x = function() return math.min(W, at() * W + 6) end,
          width = function() return math.max(0, W - math.min(W, at() * W + 6)) end,
          behavior = { x = motion(), width = motion() },
          color = function() return C().surfaceVariant end },
        M.surface { x = W - 6, y = 15, width = 4, height = 4, radius = 2, color = function() return C().primary end } },
      handle = M.surface { id = "media-progress-handle", y = 0, height = 34, radius = 2,
        width = function() return t.down and 6 or 4 end,
        x = function() return math.max(0, at() * W - 2) end,
        behavior = { x = motion() },
        color = function() return C().primary end },
    }
  end

  function S.Range(t, spec)
    if spec.widget == "seek_bar" then return seek_bar(t, spec) end
    return slider(t, spec)
  end

  return S
end
