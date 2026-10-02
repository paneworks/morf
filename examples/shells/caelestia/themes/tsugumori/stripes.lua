-- Tsugumori's hatched fills: diagonal stripes cut to their box, so a filled
-- run is a clipped box of these whose width moves while the stripes stay
-- put -- a changing level redraws nothing but its clip.
local ui = require("morf.ui")
local S = {}

local cache = {}
--- Path data for `/` stripes across `w` x `h`, one every `gap` pixels
--- along the bottom edge, each cut where it leaves the box.
function S.hatch_d(w, h, gap)
  local key = ("d%g:%g:%g"):format(w, h, gap)
  if cache[key] then return cache[key] end
  local out = {}
  for c = -h, w, gap do
    local x1, x2 = math.max(c, 0), math.min(c + h, w)
    if x2 > x1 then
      out[#out + 1] = ("M%.1f %.1f L%.1f %.1f "):format(x1, h - (x1 - c), x2, h - (x2 - c))
    end
  end
  cache[key] = #out > 0 and table.concat(out) or "M0 0"
  return cache[key]
end

--- A hatched box: `width`, `height` (numbers), `gap` (6), `weight`
--- (stroke, 2), `color`; any other node properties pass through.
function S.box(spec)
  local w, h = spec.width, spec.height
  local gap, weight = spec.gap or 6, spec.weight or 2
  return ui.Path { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = w, height = h,
    opacity = spec.opacity, view_box = { 0, 0, w, h }, d = S.hatch_d(w, h, gap), fill_color = "transparent",
    stroke_color = spec.color, stroke_width = weight, stroke_cap = "butt" }
end

--- Path data for the `/` stripes inside the area under a stepped series:
--- step `i` spans [x0 + (i-1)*dx, x0 + i*dx] at screen height `ys[i]`, the
--- floor is `h`, the box `w` wide. Each stripe walks only the steps it
--- crosses and keeps its inside runs merged, so a stripe is one or a few
--- segments however many samples there are.
function S.under_steps_d(x0, dx, ys, w, h, gap)
  local n = #ys
  if n == 0 or dx <= 0 then return "M0 0" end
  local out = {}
  local floor = math.floor
  for c = floor((x0 - h) / gap) * gap, w, gap do
    -- The stripe y = h - (x - c), from x = max(c, x0) to min(c + h, w).
    local xa, xb = math.max(c, x0), math.min(c + h, w)
    local i = floor((xa - x0) / dx) + 1
    local run_a
    local x = xa
    while x < xb and i <= n do
      local step_end = math.min(x0 + i * dx, xb)
      -- Inside over this step while the stripe is below the level y: x <= c + h - y.
      local limit = c + h - ys[i]
      local b = math.min(step_end, limit)
      if b > x then
        if not run_a then run_a = x end
        if b < step_end then
          out[#out + 1] = ("M%.1f %.1f L%.1f %.1f "):format(run_a, h - (run_a - c), b, h - (b - c))
          run_a = nil
        end
      elseif run_a then
        out[#out + 1] = ("M%.1f %.1f L%.1f %.1f "):format(run_a, h - (run_a - c), x, h - (x - c))
        run_a = nil
      end
      x = step_end
      i = i + 1
    end
    if run_a and x > run_a then
      out[#out + 1] = ("M%.1f %.1f L%.1f %.1f "):format(run_a, h - (run_a - c), x, h - (x - c))
    end
  end
  return #out > 0 and table.concat(out) or "M0 0"
end

return S
