-- An audio visualiser's bars, from the bands `morf.audio.monitor` measures.
--
-- The engine hands over one level per frequency band, as loud as the band
-- is at that moment. Bars that look alive need more: a noise floor, a
-- sensitivity that follows the music's loudness so quiet songs still move
-- and loud ones don't pin, a little memory so bars don't flicker, a fall
-- that accelerates like something dropping, and neighbours that lean on one
-- another so a peak reads as a hill. All of that is here, in Lua, as a
-- `filter` that takes bands and gives bars, and `new`, which runs one on a
-- monitor and keeps the bars in a signal.
--
--   local spectrum = require("lib.spectrum")
--   local vis = spectrum.new { bars = 24 }
--   for i = 1, 24 do
--     ui.Rect { width = 6, height = function() return 4 + 60 * (vis.bars:get()[i] or 0) end }
--   end
--   vis:stop()
--
-- `filter` is pure and deterministic, for tests and for other sources.

local morf = require("morf")

local spectrum = {}

spectrum.DEFAULTS = {
  bars = 24,
  rate_hz = 60,
  -- Levels below this are silence.
  noise = 0.002,
  -- How much of the last frame stays in a rising bar (0 none, 1 all).
  smoothing = 0.55,
  -- A falling bar's acceleration, in heights per second squared.
  gravity = 3.2,
  -- How far a peak leans on its neighbours (monstercat); 0 is none.
  spread = 1.5,
  -- Auto sensitivity: how quickly it comes down after a loud moment, and
  -- up again when it is quiet, per second.
  attack = 4.0,
  release = 0.35,
  -- The fixed sensitivity instead, when `auto` is false.
  auto = true,
  sensitivity = 1.0,
}

local function options(opts)
  local o = {}
  for k, v in pairs(spectrum.DEFAULTS) do o[k] = v end
  for k, v in pairs(opts or {}) do o[k] = v end
  return o
end

--- A band list resampled to `count` bars, each the loudest band it covers,
--- or interpolated between bands when there are more bars than bands.
function spectrum.resample(bands, count)
  local n = #bands
  local out = {}
  if n == 0 then
    for i = 1, count do out[i] = 0 end
    return out
  end
  for i = 1, count do
    local from = (i - 1) * n / count
    local to = i * n / count
    if to - from >= 1 then
      local peak = 0
      for b = math.floor(from) + 1, math.max(math.floor(from) + 1, math.ceil(to)) do
        peak = math.max(peak, bands[math.min(b, n)] or 0)
      end
      out[i] = peak
    else
      local x = (from + to) / 2 - 0.5
      local lo = math.max(1, math.min(n, math.floor(x) + 1))
      local hi = math.min(n, lo + 1)
      local t = x - math.floor(x)
      out[i] = (bands[lo] or 0) * (1 - t) + (bands[hi] or 0) * t
    end
  end
  return out
end

--- A filter: `step(bands, dt)` gives bars from 0 to 1 for one reading of
--- the bands, `dt` seconds after the last. It remembers what it showed.
function spectrum.filter(opts)
  local o = options(opts)
  local shown, fall_speed, peak_hold = {}, {}, {}
  for i = 1, o.bars do shown[i], fall_speed[i] = 0, 0 end
  local gain = o.auto and 8 or o.sensitivity
  local filter = { options = o }

  function filter.step(bands, dt)
    dt = math.max(dt or 1 / o.rate_hz, 1e-3)
    local raw = spectrum.resample(bands, o.bars)
    -- Sensitivity, then the noise floor.
    local target = {}
    local overshoot = false
    for i = 1, o.bars do
      local v = math.max(0, (raw[i] or 0) - o.noise) * gain
      if v > 1 then overshoot = true end
      target[i] = v
    end
    if o.auto then
      -- Down fast when bars would clip, up slowly while nothing does.
      if overshoot then
        gain = gain / (1 + o.attack * dt)
      else
        gain = gain * (1 + o.release * dt)
      end
      gain = math.max(0.05, math.min(gain, 400))
    end
    -- Neighbours lean on a peak: each bar is at least the others' height
    -- divided by the spread to the power of the distance.
    if o.spread > 1 then
      for i = 1, o.bars do
        local v = target[i]
        for j = 1, o.bars do
          if j ~= i then
            local lean = target[j] / o.spread ^ math.abs(i - j)
            if lean > v then v = lean end
          end
        end
        peak_hold[i] = v
      end
      for i = 1, o.bars do target[i] = peak_hold[i] end
    end
    -- Rise with a little memory; fall under gravity.
    for i = 1, o.bars do
      local t = math.min(target[i], 1)
      local s = shown[i]
      if t >= s then
        local keep = o.smoothing ^ (dt * 60)
        s = s * keep + t * (1 - keep)
        fall_speed[i] = 0
      else
        fall_speed[i] = fall_speed[i] + o.gravity * dt
        s = math.max(t, s - fall_speed[i] * dt)
      end
      shown[i] = s
    end
    local out = {}
    for i = 1, o.bars do out[i] = shown[i] end
    return out
  end

  --- The sensitivity it has settled on.
  function filter.gain() return gain end

  return filter
end

--- Bars from the machine's sound: `morf.audio.monitor` read `rate_hz`
--- times a second through a filter. Returns `{ bars = signal of a list,
--- level = signal (the louder channel), filter, stop() }`. Options: those
--- of `filter`, `device` (a source or sink monitor id; the default output
--- otherwise), `bands` (what the monitor measures, twice the bars up to
--- its most), `name` (the signals' name prefix).
function spectrum.new(opts)
  local o = options(opts)
  local name = o.name or "spectrum"
  local vis = {
    bars = morf.signal(name .. ".bars", {}),
    level = morf.signal(name .. ".level", 0),
  }
  vis.filter = spectrum.filter(o)
  local clock = morf.elapsed_timer()
  local last = clock:elapsed_ms()
  local ok, meter = pcall(morf.audio.monitor, {
    device = o.device,
    rate_hz = o.rate_hz,
    bands = math.min(o.bands or o.bars * 2, 64),
    on_level = function(left, right, bands)
      local now = clock:elapsed_ms()
      local dt = (now - last) / 1000
      last = now
      vis.bars:set(vis.filter.step(bands or {}, dt))
      vis.level:set(math.max(left or 0, right or 0))
    end,
  })
  vis.available = ok and meter ~= nil
  function vis:stop()
    if ok and meter then meter:stop() end
    ok = false
  end
  return vis
end

return spectrum
