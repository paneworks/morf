-- The default kit (library/lib/kit/skins/default) as a theme: every token
-- of the contract in each of its three variants, the variants apart, the
-- desktop's preference followed when none is fixed, motion that stops when
-- it is reduced, and a label's type classes read off the type tokens.
--
--     morf test --no-dbus library/tests/default_kit_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local default = require("lib.kit.skins.default")
  local contract = require("lib.kit.contract")
  local function join(list) return table.concat(list, "\n") end
  -- What a variant's tokens lack, or give that is not a colour.
  morf.ipc.tokens = function(variant)
    local kit = default.make { variant = variant, use = false }
    local wrong = {}
    for _, name in ipairs(contract.tokens.colors) do
      local ok, c = pcall(kit.tokens.colors[name])
      if not ok or type(c) ~= "userdata" and type(c) ~= "table" then wrong[#wrong + 1] = name end
    end
    for _, group in ipairs { "type", "shape", "motion" } do
      for _, name in ipairs(contract.tokens[group]) do
        if kit.tokens[group][name] == nil then wrong[#wrong + 1] = group .. "." .. name end
      end
    end
    return join(wrong)
  end
  morf.ipc.ground = function(variant)
    return default.make { variant = variant, use = false }.tokens.colors.window_bg():hex()
  end
  morf.ipc.motion = function(reduced)
    local m = default.make { variant = "light", use = false, reduced_motion = reduced == "yes" }.tokens.motion
    return m.duration_normal, m.spring.duration
  end
  morf.ipc.preferred = function() return default.preferred() end
  package.loaded.kit = default.make { variant = "light" }
  local w = require("lib.kit.widgets")
  local title = w.label { id = "title", text = "Title", variant = "title_1" }
  local caption = w.label { id = "caption", y = 60, text = "Caption", variant = "caption dim" }
  morf.ipc.fonts = function() return title.font_size, title.font_weight, caption.font_size end
  ui.Item { width = 400, height = 200, title, caption }
]]

local function load(env) test.load { source = HOST, env = env } test.settle(50) end

test.it("gives every contract token in each variant", function()
  load()
  for _, variant in ipairs { "light", "dark", "high_contrast" } do
    test.eq(test.ipc("tokens", variant), "", variant)
  end
end)

test.it("keeps its variants apart", function()
  load()
  test.eq(test.ipc("ground", "light"), "#fafafb")
  test.eq(test.ipc("ground", "dark"), "#222226")
  test.eq(test.ipc("ground", "high_contrast"), "#ffffff")
end)

test.it("follows the desktop's preference when no variant is fixed", function()
  load()
  test.eq(test.ipc("preferred"), "light")
end)

test.it("stops its motion when motion is reduced", function()
  load()
  local duration, spring = test.ipc("motion", "no")
  test.eq(duration, 250)
  test.eq(spring, nil)
  duration, spring = test.ipc("motion", "yes")
  test.eq(duration, 0)
  test.eq(spring, 0)
end)

test.it("sets a label's type class from the type tokens", function()
  load()
  local size, weight, small = test.ipc("fonts")
  test.eq(size, 27)
  test.eq(weight, 800)
  test.eq(small, 12)
  test.eq(#test.logs("error"), 0)
end)
