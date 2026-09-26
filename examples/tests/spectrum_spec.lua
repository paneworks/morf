-- `library/lib/spectrum.lua`: audio bands shaped into visualiser bars.
--
--     morf test examples/tests/spectrum_spec.lua

local test = morf.test
local spectrum = require("lib.spectrum")

local function run(filter, bands, frames)
  local bars
  for _ = 1, frames do bars = filter.step(bands, 1 / 60) end
  return bars
end

test.describe("spectrum", function()
  test.it("resamples bands to bars, the loudest of what each covers", function()
    test.eq(spectrum.resample({ 0.1, 0.5, 0.2, 0.9 }, 2), { 0.5, 0.9 })
    test.eq(#spectrum.resample({ 0.1, 0.5 }, 8), 8)
    test.eq(spectrum.resample({}, 3), { 0, 0, 0 })
  end)

  test.it("shows silence as nothing", function()
    local f = spectrum.filter { bars = 8 }
    for _, v in ipairs(run(f, { 0.001, 0.001, 0.0, 0.0 }, 60)) do test.eq(v, 0) end
  end)

  test.it("finds a sensitivity for quiet and for loud music alike", function()
    local quiet = spectrum.filter { bars = 4, spread = 0 }
    local loud = spectrum.filter { bars = 4, spread = 0 }
    local q = run(quiet, { 0.01, 0.02, 0.015, 0.01 }, 600)
    local l = run(loud, { 0.5, 0.9, 0.7, 0.5 }, 600)
    -- Both settle with their loudest bar near the top, not clipped flat.
    test.truthy(q[2] > 0.6 and q[2] <= 1, "quiet peak " .. q[2])
    test.truthy(l[2] > 0.6 and l[2] <= 1, "loud peak " .. l[2])
    test.truthy(l[1] < l[2], "the shape is kept")
    test.truthy(quiet.gain() > loud.gain() * 10, "quiet music is amplified more")
  end)

  test.it("falls under gravity, slowly at first", function()
    local f = spectrum.filter { bars = 1, auto = false, sensitivity = 1, spread = 0 }
    run(f, { 0.8 }, 120)
    local first = f.step({ 0 }, 1 / 60)[1]
    local second = f.step({ 0 }, 1 / 60)[1]
    local third = f.step({ 0 }, 1 / 60)[1]
    test.truthy(first < 0.8 and first > 0.75, "the first frame drops a little: " .. first)
    test.truthy((second - third) > (first - second), "and faster each frame")
    test.eq(run(f, { 0 }, 120)[1], 0)
  end)

  test.it("lets a peak lean on its neighbours", function()
    local f = spectrum.filter { bars = 5, auto = false, sensitivity = 1, spread = 2 }
    local bars = run(f, { 0, 0, 0.8, 0, 0 }, 120)
    test.near(bars[3], 0.8, 0.01)
    test.near(bars[2], 0.4, 0.01)
    test.near(bars[1], 0.2, 0.01)
  end)
end)
