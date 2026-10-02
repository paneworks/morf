-- Quiet instrument rails. Only the grip and ink move; input bounds stay fixed.
local morf = require("morf")
local ui = require("morf.ui")
local stripes = require("themes.tsugumori.stripes")

return function(theme, kit)
  local C = theme.color
  local M = {}
  local motion = { duration = 160, easing = "out_cubic" }
  local function clamp(v) return math.max(0, math.min(1, v)) end

  -- A tick ruler along `len`: a short tick every 8 px, a long one every 5th.
  local function ruler(x, y, len, size, color)
    local n = math.max(4, math.floor(len / 8))
    local d = {}
    for k = 0, n do
      local at = len * k / n
      d[#d + 1] = ("M%.1f 0 V%g "):format(at, k % 5 == 0 and size or size / 2)
    end
    return ui.Path { x = x, y = y, width = len, height = size, view_box = { 0, 0, len, size },
      d = table.concat(d), fill_color = "transparent", stroke_color = color, stroke_width = 1 }
  end

  --- The slider: a faint band, the hatched run up to the value, a tick
  --- ruler under it, a square block handle and the reading at the end.
  function M.slider(spec)
    local W, H = spec.width, spec.height or 44
    local compact = H < 36
    local isz = compact and 16 or 20
    local left = spec.icon and (isz + 12) or 0
    local right = W - (spec.label ~= false and 50 or 0)
    local span = math.max(1, right - left)
    local band = compact and math.max(6, math.floor(H * .42)) or math.floor(H * .42)
    local by = compact and 4 + math.floor((H - band) / 2) or 4 + math.floor(H * .18)
    local function value() return clamp(spec.value()) end
    local function at(x) return clamp((x - left) / span) end
    local area
    local function engaged() return area and (area.hovered or area.pressed) end
    local follow = { duration = 90, easing = "out_cubic" }
    local tick = compact and 4 or 6
    local ty = by + band + 3
    area = kit.action {
      id = spec.id, width = W, height = H + 8, cursor = "pointer",
      on_pressed = function(_, _, x) spec.set(at(x)) end,
      on_dragged = function(_, _, _, _, x) if area.pressed then spec.set(at(x)) end end,
      on_wheel = function(_, _, _, _, _, y)
        if y ~= 0 then spec.set(clamp(value() + (y > 0 and -0.05 or 0.05))) end
      end,
      ui.Rect { id = spec.id .. "-track", x = left, y = by, width = span, height = band,
        color = function() return C.primary:alpha(engaged() and .1 or .06) end,
        border_width = 1, border_color = function() return C.primary:alpha(.18) end, behavior = { color = motion } },
      ui.Item { id = spec.id .. "-level", x = left, y = by, height = band, clip = true,
        width = function() return span * value() end, behavior = { width = follow },
        visible = function() return value() > .002 end,
        ui.Rect { width = span, height = band, color = function() return C.primary:alpha(.2) end },
        stripes.box { width = span, height = band, gap = 6, weight = 2, color = function() return C.primary end } },
      ty + tick <= H + 8 and ruler(left, ty, span, tick, function() return C.primary:alpha(.32) end) or nil,
      ui.Rect { id = spec.id .. "-handle", y = by - 4, width = 6, height = band + 8,
        x = function() return left + span * value() - 3 end, behavior = { x = follow },
        color = function() return C.primary end },
      spec.icon and kit.icon(spec.icon, isz, function() return engaged() and C.primary or C.onSurfaceVariant end,
        { x = 2, y = by + math.floor((band - isz) / 2) }) or nil,
      spec.label ~= false and kit.text { id = spec.id .. "-value", x = W - 46, y = by + math.floor((band - 18) / 2),
        width = 46, height = 18, horizontal_alignment = "right", font_size = 12,
        color = function() return engaged() and C.primary or C.onSurfaceVariant end,
        text = function() return ("%03d"):format(math.floor(value() * 100 + .5)) end } or nil,
    }
    return area
  end

  --- A thin level: square segments, lit up to the value.
  function M.bar(spec)
    local W, H = spec.width, spec.stroke or 4
    local color = spec.color or function() return C.primary end
    local n = math.max(6, math.floor(W / 7))
    local gap = 2
    local seg = (W - gap * (n - 1)) / n
    local d = ("M0 %g H%g"):format(H / 2, W)
    local function lit() return math.floor(clamp(spec.value()) * n + .5) / n end
    return ui.Item { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H,
      ui.Path { width = W, height = H, view_box = { 0, 0, W, H }, d = d, fill_color = "transparent",
        stroke_width = H, dash = { seg, gap }, stroke_cap = "butt",
        stroke_color = spec.track or function() return C.primary:alpha(.16) end },
      ui.Path { id = spec.id and spec.id .. "-fill", width = W, height = H, view_box = { 0, 0, W, H }, d = d,
        fill_color = "transparent", stroke_width = H, dash = { seg, gap }, stroke_cap = "butt", stroke_color = color,
        opacity = function() return lit() > 0 and 1 or 0 end,
        trim_end = function() return math.max(.0001, lit()) end, behavior = { trim_end = motion } },
    }
  end

  --- The media position: a hatched run with a block head over a faint band
  --- and a tick ruler; it flows between the player's one-second updates.
  function M.media_progress(spec)
    local W = spec.width
    local previous = clamp(spec.value())
    local function tip(v) return math.max(0, math.min(W - 4, W * v - 2)) end
    local fill = ui.Item { id = "media-progress-fill", y = 11, height = 10, width = W * previous, clip = true,
      ui.Rect { width = W, height = 10, color = function() return C.primary:alpha(.2) end },
      stripes.box { width = W, height = 10, gap = 6, weight = 2, color = function() return C.primary end } }
    local grip = ui.Rect { id = "media-progress-handle", y = 7, width = 4, height = 18,
      x = tip(previous), color = function() return C.primary end }
    local rail = ui.Item { width = W, height = 34,
      ui.Rect { y = 11, width = W, height = 10, color = function() return C.primary:alpha(.06) end,
        border_width = 1, border_color = function() return C.primary:alpha(.18) end },
      ruler(0, 24, W, 5, function() return C.primary:alpha(.3) end),
      fill, grip,
    }
    local running
    morf.effect("tsugumori.media.progress", function()
      local v = clamp(spec.value())
      local active = not spec.active or spec.active()
      local playing = spec.playing and spec.playing()
      -- Interpolate the player's one-second ticks; seeks settle promptly.
      local flowing = playing and v > previous and v - previous < .03
      previous = v
      if running then running:stop() running = nil end
      if not active then fill.width, grip.x = W * v, tip(v) return end
      local duration, easing = flowing and 1000 or 180, flowing and "linear" or "out_cubic"
      running = morf.animation.play {{parallel={
        {node=fill, property="width", to=W*v, duration=duration, easing=easing},
        {node=grip, property="x", to=tip(v), duration=duration, easing=easing},
      }}}
    end, {owner=rail})
    return rail
  end
  return M
end
