-- `examples/demos/render/glass-cases.lua`: the four glass cases over one
-- busy backdrop, asserted on the pixels they draw.
--
--     nixVulkan morf test examples/demos/tests/glass_spec.lua --snapshots DIR
--
-- Without a GPU the pixel cases note why and pass; the layout ones run
-- everywhere.

local test = morf.test

-- The backdrop's two stripe colours, and each case's panel.
local SAND, TEAL = "#e9c46a", "#264653"
local PANEL = { y = 110, width = 260, height = 200 }
local X = { clear = 30, frosted = 320, darkened = 610, lightened = 900 }

-- The stripes alone, above every panel.
local BACKDROP = { x = 0, y = 0, width = 250, height = 100 }

--- The middle of a panel: clear of its rim, its lens and its label.
local function inner(name)
  return { x = X[name] + 60, y = PANEL.y + 30, width = 140, height = 100 }
end

local function frame()
  local drawn, why = test.frame()
  if not drawn then test.note("no pixels to read: " .. tostring(why)) end
  return drawn
end

local function linear(channel)
  channel = channel / 255
  return channel <= 0.04045 and channel / 12.92 or ((channel + 0.055) / 1.055) ^ 2.4
end

local function encoded(light)
  light = math.max(0, math.min(1, light))
  local channel = light <= 0.0031308 and light * 12.92 or 1.055 * light ^ (1 / 2.4) - 0.055
  return math.floor(channel * 255 + 0.5)
end

local function channels(hex)
  return { tonumber(hex:sub(2, 3), 16), tonumber(hex:sub(4, 5), 16), tonumber(hex:sub(6, 7), 16),
    #hex >= 9 and tonumber(hex:sub(8, 9), 16) or 255 }
end

--- The colour a glass tinted `tint` (with its alpha) shows over the
--- stripes blurred together, mixed in linear light as the surface mixes.
local function tinted(tint)
  local sand, teal, wash = channels(SAND), channels(TEAL), channels(tint)
  local strength = wash[4] / 255
  local out = {}
  for index = 1, 3 do
    local behind = (linear(sand[index]) + linear(teal[index])) / 2
    out[index] = encoded(behind * (1 - strength) + linear(wash[index]) * strength)
  end
  return out
end

local function near(hex, wanted, slack, what)
  local got = channels(hex)
  for index = 1, 3 do
    if math.abs(got[index] - wanted[index]) > slack then
      error(("%s: %s is not within %d of %s"):format(what, hex, slack, test.show(wanted)), 2)
    end
  end
end

test.describe("glass cases", function()
  test.before_each(function()
    test.load("../render/glass-cases.lua", { size = { 1200, 420 } })
    test.settle()
  end)

  test.it("lays the four cases out in a row", function()
    local xs = {}
    for _, label in ipairs { "clear", "frosted", "darkened", "lightened" } do
      xs[#xs + 1] = test.get { id = label }.x
    end
    test.eq(xs, { X.clear, X.frosted, X.darkened, X.lightened })
  end)

  test.it("says nothing is wrong", function()
    test.eq(test.logs("warn"), {})
  end)

  test.it("clear glass bends the stripes at its rim and leaves its middle sharp", function()
    local drawn = frame()
    if not drawn then return end
    -- Just inside the left edge the teal stripe under the glass is drawn
    -- from the sand one further in.
    for x = 42, 46 do
      near(drawn.pixel(x, 50), channels(TEAL), 3, "the stripe at " .. x)
      near(drawn.pixel(x, 210), channels(SAND), 12, "the lens at " .. x)
    end
    -- Past the rim nothing moves and nothing blurs.
    for _, x in ipairs { 137, 162, 187 } do
      test.eq(drawn.pixel(x, 210), drawn.pixel(x, 50), "the middle at " .. x)
    end
    local behind = drawn.region(BACKDROP).variance
    test.truthy(drawn.region(inner("clear")).variance > behind * 0.8, "clear keeps the stripes")
  end)

  test.it("frosted glass takes the detail out of what is behind", function()
    local drawn = frame()
    if not drawn then return end
    local behind, frosted = drawn.region(BACKDROP), drawn.region(inner("frosted"))
    test.truthy(behind.variance > 0.04, "the stripes are busy: " .. behind.variance)
    test.truthy(frosted.variance < behind.variance * 0.01,
      ("frosted variance %.5f against %.5f behind"):format(frosted.variance, behind.variance))
  end)

  test.it("darkened glass darkens what is behind under light text, by its tint in linear light", function()
    local drawn = frame()
    if not drawn then return end
    local behind, dark = drawn.region(BACKDROP), drawn.region(inner("darkened"))
    test.truthy(dark.luma < behind.luma - 0.12, ("luma %.3f against %.3f"):format(dark.luma, behind.luma))
    test.truthy(dark.variance < behind.variance * 0.01, "and frosts the busy stripes")
    near(dark.mean, tinted("#121214b3"), 4, "the darkened glass")
    test.truthy(morf.color(dark.mean):contrast(morf.color("#ffffff")) >= 4.5, "light text reads on it")
  end)

  test.it("lightened glass lightens what is behind under dark text, by its tint in linear light", function()
    local drawn = frame()
    if not drawn then return end
    local behind, light = drawn.region(BACKDROP), drawn.region(inner("lightened"))
    test.truthy(light.luma > behind.luma + 0.12, ("luma %.3f against %.3f"):format(light.luma, behind.luma))
    test.truthy(light.variance < behind.variance * 0.01, "and frosts the busy stripes")
    near(light.mean, tinted("#fafafbb8"), 4, "the lightened glass")
    test.truthy(morf.color(light.mean):contrast(morf.color("#1d1d20")) >= 4.5, "dark text reads on it")
  end)

  test.it("draws itself, where there is a GPU", function()
    local drawn, where = test.snapshot("glass.png")
    if drawn then test.note("wrote " .. where) end
  end)
end)
