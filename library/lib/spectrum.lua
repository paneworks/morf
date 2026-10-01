-- An audio visualiser's bars, from the bands `morf.audio.monitor` measures.
--
-- The engine handles band resampling, sensitivity, smoothing, gravity and
-- neighbour spread in Rust. This wrapper owns subscriptions and signals.
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

-- Native numeric operations; this module owns only options and subscriptions.
spectrum.resample=morf.audio.spectrum_resample
function spectrum.filter(opts)
  local o=options(opts)
  local filter=morf.audio.spectrum_filter(o)
  filter.options=o
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
