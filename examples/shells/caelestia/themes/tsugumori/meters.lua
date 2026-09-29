-- Quiet instrument rails. Only the grip and ink move; input bounds stay fixed.
local morf = require("morf")
local ui = require("morf.ui")

return function(theme, kit)
  local C = theme.color
  local M = {}
  local motion = { duration = 160, easing = "out_cubic" }
  local function clamp(v) return math.max(0, math.min(1, v)) end

  function M.slider(spec)
    local W, H = spec.width, spec.height or 44
    local compact = H < 36
    local left = compact and spec.icon and 28 or 7
    local right = W - (compact and spec.label ~= false and 56 or 7)
    local span = math.max(1, right - left)
    local cy = compact and (H + 8) / 2 or H - 5
    local area
    local function value() return clamp(spec.value()) end
    local function change(x) spec.set(clamp((x - left) / span)) end
    -- Move fill and grip in the same native animation. While held both follow
    -- the pointer immediately; service/key/wheel changes ease into place.
    local fill, grip
    local function engaged() return area and (area.hovered or area.pressed) end
    area = kit.action {
      id = spec.id, width = W, height = H + 8, cursor = "pointer",
      on_pressed = function(_, _, x) change(x) end,
      on_dragged = function(_, _, _, _, x) if area.pressed then change(x) end end,
      on_wheel = function(_, _, _, _, _, y)
        if y ~= 0 then spec.set(clamp(value() + (y > 0 and -0.05 or 0.05))) end
      end,
      ui.Rect { id = spec.id .. "-track", x = left, y = cy - 2, width = span, height = 4,
        color = function() return C.primary:alpha(engaged() and .19 or .11) end,
        behavior = { color = motion } },
      spec.icon and kit.icon(spec.icon, compact and 16 or 20,
        function() return engaged() and C.primary or C.onSurfaceVariant end,
        { x = compact and 3 or 5, y = compact and cy - 8 or 2 }) or nil,
      spec.label ~= false and kit.text { id = spec.id .. "-value",
        x = W - 45, y = compact and cy - 8 or 3, width = 40, height = 18,
        horizontal_alignment = "right", font_size = 12,
        color = function() return engaged() and C.primary or C.onSurfaceVariant end,
        text = function() return ("%d%%"):format(math.floor(value() * 100 + .5)) end } or nil,
    }
    fill = ui.Rect { id = spec.id .. "-level", x = left, y = cy - 2, height = 4,
        width = span * value(),
        color = function() return C.primary:alpha(engaged() and 1 or .78) end,
        behavior = { color = motion } }
    grip = ui.Item { id = spec.id .. "-handle", x = left + span * value() - 6,
        y = cy - 9, width = 12, height = 18,
        scale = function() return area and area.pressed and .88 or engaged() and 1.12 or 1 end,
        behavior = { scale = { duration = 130, easing = "out_cubic" } },
        ui.Path { width = 12, height = 18, view_box = {0, 0, 12, 18},
          d = "M3 0 H12 V15 L9 18 H0 V3 Z", fill_color = function() return C.primary end },
        ui.Rect { x = 5, y = 5, width = 2, height = 8, color = function() return C.onPrimary end },
      }
    ui.reparent(fill, area)
    ui.reparent(grip, area)
    local running
    morf.effect("tsugumori.slider." .. spec.id, function()
      local target, pressed = left + span * value(), area.pressed
      if running then running:stop() running = nil end
      if pressed then fill.width, grip.x = target - left, target - 6
      else running = morf.animation.play {{parallel={
        {node=fill, property="width", to=target-left, duration=160, easing="out_cubic"},
        {node=grip, property="x", to=target-6, duration=160, easing="out_cubic"},
      }}} end
    end, { owner = area })
    return area
  end

  function M.bar(spec)
    local W, H = spec.width, spec.stroke or 4
    local color = spec.color or function() return C.primary end
    return ui.Item { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H,
      ui.Rect { width = W, height = H, color = spec.track or function() return C.primary:alpha(.12) end },
      ui.Rect { id = spec.id and spec.id .. "-fill", height = H,
        width = function() return W * clamp(spec.value()) end, color = color,
        behavior = { width = motion } },
    }
  end

  function M.media_progress(spec)
    local W = spec.width
    local previous = clamp(spec.value())
    local function tip(v) return math.max(0, math.min(W - 3, W * v - 1.5)) end
    local fill = ui.Rect { id = "media-progress-fill", y = 15, height = 4, width = W * previous,
      color = function() return C.primary:alpha(.8) end }
    local grip = ui.Rect { id = "media-progress-handle", y = 10, width = 3, height = 14,
      x = tip(previous), color = function() return C.primary end }
    local rail = ui.Item { width = W, height = 34,
      ui.Rect { y = 15, width = W, height = 4, color = function() return C.primary:alpha(.12) end },
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
