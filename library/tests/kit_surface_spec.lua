-- The default kit's named surface styles: `kit.surface { style = ... }`
-- wears the glass the style names, what the props give wins, and an
-- unknown name is refused with the names there are.
--
--     morf test library/tests/kit_surface_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local kit = require("lib.kit.skins.default").make { variant = "dark" }
  local plain = kit.surface { id = "plain", width = 40, height = 40, color = "#ff0000" }
  local dark = kit.surface { id = "dark", style = "dark_glass", width = 40, height = 40 }
  local mine = kit.surface { id = "mine", style = "clear_glass", width = 40, height = 40,
    color = "#ffffff22", glass = { refraction = 4, frost = 0 } }
  ui.Item { width = 200, height = 60, plain, dark, mine }
  local function material(node)
    return { refraction = node.glass_refraction, frost = node.glass_frost, rim = node.glass_rim,
      blur = node.backdrop_blur, tint = tostring(node.backdrop_tint), color = tostring(node.color) }
  end
  morf.ipc.material = function(name)
    return material(({ plain = plain, dark = dark, mine = mine })[name])
  end
  morf.ipc.styles = function() return kit.surface_styles end
  morf.ipc.ink = function(name) return tostring(kit.surface_ink(name)) end
  morf.ipc.unknown = function()
    local ok, err = pcall(kit.surface, { style = "velvet" })
    return ok and "made one" or tostring(err)
  end
]]

test.describe("kit surface styles", function()
  test.before_each(function()
    test.load { source = HOST, size = { 200, 60 } }
    test.settle()
  end)

  test.it("names the glass surfaces", function()
    test.eq(test.ipc("styles"), { "clear_glass", "dark_glass", "frosted_glass", "light_glass" })
    test.eq(test.ipc("ink", "dark_glass"), "#ffffff")
    test.eq(test.ipc("ink", "light_glass"), "#1d1d20")
  end)

  test.it("leaves a surface without a style a plain rect", function()
    local plain = test.ipc("material", "plain")
    test.eq({ plain.refraction, plain.rim, plain.blur, plain.color }, { 0, 0, false, "#ff0000" })
  end)

  test.it("wears the glass its style names, on a clear ground", function()
    local dark = test.ipc("material", "dark")
    test.eq({ dark.blur, dark.frost, dark.rim, dark.tint, dark.color },
      { 24, "auto", 0.2, "#121214b3", "#00000000" })
  end)

  test.it("takes the props' own glass and colour over the style's", function()
    local mine = test.ipc("material", "mine")
    test.eq({ mine.refraction, mine.frost, mine.rim, mine.color }, { 4, 0, 0, "#ffffff22" })
  end)

  test.it("refuses a style it does not have", function()
    local err = test.ipc("unknown")
    test.truthy(err:find('no surface style "velvet"', 1, true), err)
    test.truthy(err:find("clear_glass, dark_glass, frosted_glass, light_glass", 1, true), err)
  end)

  test.it("says nothing is wrong", function()
    test.eq(test.logs("warn"), {})
  end)
end)
