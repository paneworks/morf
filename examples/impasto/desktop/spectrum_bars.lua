-- Sound bars rising from one edge of a box, for the spectrum on a square and
-- along a screen edge.
--
-- Port of SpectrumBars.qml and spectrum.frag. As in the original, one
-- shader draws every bar -- rounded or square columns, segments, dots or a
-- wave, fading, solid or blended, lows at the corners or along the edge,
-- with falling peak caps -- rather than a node per bar redrawn on every
-- audio frame. The shader is written in morf's shader Lua and compiled to
-- WGSL once, when this file loads. Its parameters are fixed at
-- construction in morf, so everything that can change -- the look, the
-- colours, the box -- travels in a data block (`cfg`) beside the bands and
-- the peaks, and is pushed with `morf.shader_data`.
--
-- The bands come from `morf.audio.monitor` on the default output (the
-- original ran cava). It runs only while something listens: while any
-- spectrum is on the desk and not away (see `wanted` below), or while a
-- caller holds `M.listen(true)`. The bars step with the monitor's frames,
-- thirty a second, rather than gliding between them.
--
-- `build { x, y, width, height, looks, listening, edge, sample }`: `looks()`
-- is `services.desktop.spectrum_of(row)`, `listening()` whether the bars
-- draw at all, `edge` the side they rise from (bottom, left, right).
-- `sample = true` draws a fixed spectrum instead of the sound, for a tile
-- that has to show a look in silence.
--
-- IMPASTO_SPECTRUM_DEMO=1 feeds a moving synthetic spectrum instead of the
-- monitor, to see the bars where there is no sound server (a nested test
-- compositor).

local ui = require("morf.ui")
local theme = require("theme")

local M = {}

M.BANDS = 64
M.RATE = 30

-- --------------------------------------------------------------- shader --

local CFG = 24

local SOURCE = [[
    function level_at(position)
      local place = clamp(position, 0.0, 1.0) * 63.0
      local below = i32(floor(place))
      local above = min(i32(63), below + i32(1))
      return mix(bands[below], bands[above], place - floor(place))
    end

    function peak_at(position)
      local place = clamp(position, 0.0, 1.0) * 63.0
      local below = i32(floor(place))
      local above = min(i32(63), below + i32(1))
      return mix(peaks[below], peaks[above], place - floor(place))
    end

    function round_box(p, half, radius)
      local outside = abs(p) - half + vec2(radius, radius)
      return length(max(outside, vec2(0.0, 0.0))) + min(max(outside.x, outside.y), 0.0) - radius
    end

    function cover(outline)
      return 1.0 - smoothstep(-0.5, 0.5, outline)
    end

    function fragment(uv, time, resolution)
      local area = vec2(cfg[0], cfg[1])
      local side = cfg[2]
      local look = cfg[3]
      local fill = cfg[4]
      local lows = cfg[5]
      local peaks_on = cfg[6]
      local pitch = max(cfg[7], 1.0)
      local bar_width = cfg[8]
      local floor_length = cfg[9]
      local curve = cfg[10]
      local base = cfg[11]
      local tip = cfg[12]
      local color = vec4(cfg[13], cfg[14], cfg[15], cfg[16])
      local color2 = vec4(cfg[17], cfg[18], cfg[19], cfg[20])
      local opacity = cfg[21]

      local across = side < 0.5
      local pixel = uv * area
      local run = area.y
      local depth = area.x
      local along = pixel.y
      local from_edge = pixel.x
      if across then
        run = area.x
        depth = area.y
        along = pixel.x
        from_edge = area.y - pixel.y
      elseif side > 1.5 then
        from_edge = area.x - pixel.x
      end

      -- Which band a bar reads: mirrored with the lows at both corners
      -- along the bottom, lows at the bottom of a side, or straight along.
      local count = floor(run / pitch)
      local lead = (run - count * pitch) * 0.5
      local slot = floor((along - lead) / pitch)
      local middle = clamp(along / run, 0.0, 1.0)
      if look < 3.5 then
        middle = (slot + 0.5) / max(count, 1.0)
      end
      local position = middle
      if lows < 0.5 then
        if across then
          position = 1.0 - abs(2.0 * middle - 1.0)
        else
          position = 1.0 - middle
        end
      end
      local reach = max(floor_length, depth * pow(max(level_at(position), 0.0), curve))

      local coverage = 0.0
      local full = 0.0
      if look > 3.5 then
        -- The wave: one filled area along the edge, lit along its top.
        local body = cover(from_edge - reach)
        local rim = cover(abs(from_edge - reach) - 1.0)
        coverage = max(body, rim)
        full = rim
      else
        if count >= 1.0 and slot >= 0.0 and slot < count then
          local offset = along - (lead + (slot + 0.5) * pitch)
          local half_width = bar_width * 0.5
          if look < 0.5 then
            coverage = cover(round_box(vec2(offset, from_edge - reach * 0.5),
              vec2(half_width, reach * 0.5), min(half_width, reach * 0.5)))
          elseif look < 1.5 then
            coverage = cover(round_box(vec2(offset, from_edge - reach * 0.5),
              vec2(half_width, reach * 0.5), 0.0))
          elseif look < 2.5 then
            -- Cells as long as the bar is wide, as many as the level reaches.
            local cell = max(bar_width, 3.0)
            local cell_gap = max(2.0, pitch - bar_width)
            local cell_step = cell + cell_gap
            local lit = max(1.0, floor((reach + cell_gap) / cell_step))
            local index = floor(from_edge / cell_step)
            local within = from_edge - index * cell_step
            if index < lit then
              coverage = cover(round_box(vec2(offset, within - cell * 0.5),
                vec2(half_width, cell * 0.5), min(1.5, half_width)))
            end
          else
            -- A disc at the level, on a faint stem down to the edge.
            local centre = max(half_width, reach - half_width)
            local disc = cover(length(vec2(offset, from_edge - centre)) - half_width)
            local stem = cover(abs(offset) - max(1.0, bar_width * 0.12)) * (1.0 - step(centre, from_edge)) * 0.35
            coverage = max(disc, stem)
          end
          -- A cap just above the highest the band has been lately.
          if peaks_on > 0.5 then
            local cap_height = max(2.0, bar_width * 0.3)
            local cap_centre = max(max(floor_length, depth * pow(max(peak_at(position), 0.0), curve)), reach)
              + 2.0 + cap_height * 0.5
            local cap_radius = 0.0
            if look < 0.5 then
              cap_radius = cap_height * 0.5
            end
            local cap = cover(round_box(vec2(offset, from_edge - cap_centre),
              vec2(half_width, cap_height * 0.5), cap_radius))
            if cap > coverage then
              coverage = cap
              full = cap
            end
          end
        end
      end

      local ink = color
      local strength = base
      if fill > 1.5 then
        ink = mix(color, color2, clamp(from_edge / depth, 0.0, 1.0))
      elseif fill < 0.5 then
        strength = mix(mix(base, tip, clamp(from_edge / max(reach, 1.0), 0.0, 1.0)), base, full)
      end
      return vec4(ink.x, ink.y, ink.z, ink.w * coverage * strength * opacity)
    end
  ]]

-- One program for every spectrum: each node that wears it has its own
-- data blocks, so a square and an edge draw with their own numbers.
-- Registered while the configuration loads: a shader registered later is
-- never handed to the renderer, and its nodes draw nothing.
local SHADER = "impasto_spectrum"
morf.shader(SHADER, {
  kind = "surface",
  data = { cfg = CFG, bands = M.BANDS, peaks = M.BANDS },
  fragment = SOURCE,
})

-- ---------------------------------------------------------------- bands --

local levels = {}
local peaks = {}
local peak_held = {}
for i = 1, M.BANDS do levels[i], peaks[i], peak_held[i] = 0, 0, 0 end

-- A signal per band for anything that wants to bind to one (`M.level`).
-- Only written while something reads them, so the shader path costs no
-- binding work.
local band_signals = {}
local signals_read = false

--- The level of band `i` (1..BANDS), 0..1; a binding follows it.
function M.level(i)
  i = math.max(1, math.min(M.BANDS, math.floor(i)))
  if not band_signals[i] then
    band_signals[i] = morf.signal("impasto.desk.band." .. i, levels[i])
  end
  signals_read = true
  return band_signals[i]:get()
end

-- The shader nodes drawing live bands: node -> true. A node that is gone
-- (its widget removed) makes `shader_data` fail and is dropped then.
local live = {}

-- A shader data write does not ask for a frame by itself (the engine marks
-- nothing dirty for it), so each push also nudges the node's opacity by a
-- hair, which does.
local nudge = false
local function push_bands()
  nudge = not nudge
  for node in pairs(live) do
    local ok = pcall(morf.shader_data, node, "bands", levels)
    if ok then ok = pcall(morf.shader_data, node, "peaks", peaks) end
    if ok then ok = pcall(function() node.opacity = nudge and 1 or 0.999 end) end
    if not ok then live[node] = nil end
  end
  if signals_read then
    for i, s in pairs(band_signals) do s:set(levels[i]) end
  end
end

-- Peaks hold for half a second, then fall slowly.
local function take(values)
  local now = morf.time.now_ms()
  for i = 1, M.BANDS do
    local v = math.max(0, math.min(1, values[i] or 0))
    levels[i] = math.max(v, levels[i] * 0.75)
    if levels[i] >= peaks[i] then
      peaks[i] = levels[i]
      peak_held[i] = now
    elseif now - peak_held[i] > 500 then
      peaks[i] = math.max(levels[i], peaks[i] - 0.02)
    end
  end
  push_bands()
end

-- ------------------------------------------------------------- listening --

local held = 0          -- explicit listeners (`M.listen`)
local wanted_fn = nil   -- the desk's own answer, set by `M.want`
local monitor, demo = nil, nil
local demo_on = (morf.env and morf.env("IMPASTO_SPECTRUM_DEMO") or "") ~= ""

local function start()
  if monitor or demo then return end
  if demo_on then
    local phase = 0
    demo = morf.timer(math.floor(1000 / M.RATE), function()
      phase = phase + 1
      local out = {}
      for i = 1, M.BANDS do
        local x = (i - 1) / (M.BANDS - 1)
        local shape = 0.85 * math.exp(-((x - 0.06) / 0.14) ^ 2) + 0.5 * math.exp(-((x - 0.4) / 0.16) ^ 2) + 0.12
        out[i] = math.min(1, shape * (0.55 + 0.45 * math.sin(phase * 0.21 + i * 0.9) * math.sin(phase * 0.07 + i * 0.3)))
      end
      take(out)
    end, true)
    return
  end
  if not morf.audio or not morf.audio.monitor then return end
  local ok, handle = pcall(morf.audio.monitor, {
    rate_hz = M.RATE, bands = M.BANDS,
    on_level = function(_, _, values) take(values or {}) end,
  })
  monitor = ok and handle or nil
end

local function stop()
  if monitor and monitor.stop then pcall(monitor.stop, monitor) end
  if demo then pcall(demo.cancel, demo) end
  monitor, demo = nil, nil
  for i = 1, M.BANDS do levels[i], peaks[i] = 0, 0 end
  push_bands()
end

local function reconcile()
  local want = held > 0 or (wanted_fn ~= nil and wanted_fn())
  if want then start() else stop() end
end

--- Counts a listener in or out; the monitor runs while there is one.
function M.listen(on)
  held = math.max(0, held + (on and 1 or -1))
  reconcile()
end

--- Whether the desk wants sound, as a function read inside an effect: the
--- monitor follows it. Set once, by this file, from the desk's rows.
function M.want(fn)
  wanted_fn = fn
  morf.effect("impasto.desk.spectrum.listen", function()
    local want = fn()
    -- Starting a monitor is not node building, but keep it off the
    -- effect's own turn all the same.
    morf.timer(1, function()
      if want then start() else stop() end
      if held > 0 then start() end
    end, false)
  end)
end

-- The desk listens while any spectrum -- square or edge -- is on this
-- screen and not away, or while arranging with one out.
do
  local ok, desk = pcall(require, "services.desktop")
  if ok and desk then
    M.want(function()
      local any = false
      for _, row in ipairs(desk.rows()) do
        if row.id == "spectrum" then any = true break end
      end
      if not any then return false end
      return desk.editing:get() or not desk.spectrum_away()
    end)
  end
end

-- ------------------------------------------------------------------ build --

local LOOKS = { rounded = 0, square = 1, segments = 2, dots = 3, wave = 4 }
local FILLS = { fade = 0, solid = 1, blend = 2 }
local SIDES = { bottom = 0, left = 1, right = 2 }

-- Strong lows, a bump in the middle, falling highs, a little unevenness so
-- neighbours differ: what a tile shows in silence.
local still, still_peaks = {}, {}
for i = 1, M.BANDS do
  local x = (i - 1) / (M.BANDS - 1)
  local shape = 0.85 * math.exp(-((x - 0.06) / 0.14) ^ 2) + 0.5 * math.exp(-((x - 0.4) / 0.16) ^ 2) + 0.12
  still[i] = math.min(1, shape * (0.75 + 0.25 * math.sin((i - 1) * 2.3)))
  still_peaks[i] = math.min(1, still[i] + 0.12)
end

local function cfg_of(values, looks)
  local w, h = values.width, values.height
  local c1 = morf.color(looks.color):rgb()
  local c2 = morf.color(looks.color2):rgb()
  local s = theme.spectrum
  return {
    w, h, SIDES[values.edge or "bottom"] or 0,
    LOOKS[looks.look] or 0, FILLS[looks.fill] or 0,
    looks.lows == "along" and 1 or 0, looks.peaks and 1 or 0,
    (looks.bar or s.bar) + (looks.gap or s.gap), looks.bar or s.bar,
    s.floor, s.curve, s.base, s.tip,
    c1.r, c1.g, c1.b, c1.a or 1,
    c2.r, c2.g, c2.b, c2.a or 1,
    1, 0, 0,
  }
end

function M.build(values)
  local looks = values.looks
  local listening = values.listening or function() return true end
  local sample = values.sample
  local node = ui.Rect {
    x = 0, y = 0, width = values.width, height = values.height,
    color = morf.color("transparent"),
    shader = SHADER,
  }
  if not sample then live[node] = true end
  -- The look travels as data: a binding on a child that pushes it whenever
  -- the row or the palette changes, and dies with the node.
  local pusher = ui.Item {
    width = 0, height = 0, visible = false,
    implicit_width = function()
      pcall(morf.shader_data, node, "cfg", cfg_of(values, looks()))
      if sample then
        pcall(morf.shader_data, node, "bands", still)
        pcall(morf.shader_data, node, "peaks", still_peaks)
      else
        pcall(morf.shader_data, node, "bands", levels)
        pcall(morf.shader_data, node, "peaks", peaks)
      end
      return 0
    end,
  }
  -- Pushed once more on the next tick, when the node is sure to be drawn,
  -- with the nudge that asks for the frame.
  morf.timer(1, function()
    pcall(morf.shader_data, node, "cfg", cfg_of(values, looks()))
    pcall(morf.shader_data, node, "bands", sample and still or levels)
    pcall(morf.shader_data, node, "peaks", sample and still_peaks or peaks)
    pcall(function() node.opacity = 0.999 end)
  end, false)
  return ui.Item {
    x = values.x, y = values.y, width = values.width, height = values.height,
    visible = listening,
    opacity = function() return looks().opacity / 100 end,
    node, pusher,
  }
end

return M
