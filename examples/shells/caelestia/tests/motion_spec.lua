-- Exercise animation trajectories, including reversals, rather than only
-- asserting the final open state. This fixture uses the real drawer driver.
local test = morf.test
local HOST = [[
  local ui = require("morf.ui")
  local drawer = require("drawer")
  local edge = morf.env("TEST_EDGE")
  local cards = { ui.Rect { width = 100, height = 60 }, ui.Rect { x = 120, width = 100, height = 60 } }
  local content = ui.Item { width = 320, height = 240, table.unpack(cards) }
  local d = drawer.new { name = "sample", edge = edge, width = 320, height = 240, content = content }
  ui.Item { width = 1280, height = 720,
    ui.Sdf { width = 1280, height = 720, d.shape }, d.panel }
  morf.ipc.open = d.set
  morf.ipc.pose = function()
    return { x = d.panel.translate_x, y = d.panel.translate_y, scale = d.panel.scale,
      opacity = content.opacity, background = d.shape.opacity, visible = d.panel.visible }
  end
  local running = {}
  morf.ipc.cards = function(coming)
    for _, handle in ipairs(running) do handle:stop() end
    local entries = {} for _, node in ipairs(cards) do entries[#entries + 1] = { node = node } end
    running = require("theme").motion.entries(entries, coming, { delay = 0, stagger = 160 })
  end
  morf.ipc.card_pose = function()
    local out = {}
    for _, node in ipairs(cards) do out[#out + 1] = { opacity = node.opacity, scale = node.scale, y = node.translate_y } end
    return out
  end
]]
test.describe("theme motion", function()
  for _, style in ipairs { "material", "tsugumori" } do
    for _, edge in ipairs { "top", "bottom", "left", "right", "center" } do
      test.it(style .. " reverses " .. edge .. " without jumping or sticking", function()
        test.load("../shell/init.lua", { source = HOST, size = { 1280, 720 }, env = {
          CAELESTIA_STYLE = style, TEST_EDGE = edge,
          CAELESTIA_APPEARANCE = morf.env("XDG_CACHE_HOME") .. "/motion-appearance.json",
          CAELESTIA_SETTINGS = morf.env("XDG_CACHE_HOME") .. "/motion-settings.json",
        } })
        test.falsy(test.ipc("pose").visible)
        test.ipc("open", true) test.advance(60)
        local entering = test.get("drawer-sample")
        test.truthy(entering.opacity < 1 or math.abs(entering.width - 320) > 0.01 or edge ~= "center" or style == "tsugumori",
          "floating panel skipped its entrance animation")
        test.advance(120)
        local middle = test.ipc("pose")
        test.truthy(middle.visible)
        if edge ~= "center" then
          test.truthy(math.abs(middle.x) > 0.01 or math.abs(middle.y) > 0.01,
            "panel jumped directly to its final position")
        end
        test.ipc("open", false)
        local reversed = test.ipc("pose")
        test.near(reversed.x, middle.x, 0.01)
        test.near(reversed.y, middle.y, 0.01)
        test.near(reversed.scale, middle.scale, 0.01)
        test.advance(45)
        local closing = test.ipc("pose")
        test.ipc("open", true)
        local reopening = test.ipc("pose")
        test.near(reopening.x, closing.x, 0.01)
        test.near(reopening.y, closing.y, 0.01)
        test.near(reopening.scale, closing.scale, 0.01)
        test.near(reopening.opacity, closing.opacity, 0.01, "content flashed on reversal")
        test.near(reopening.background, closing.background, 0.01, "background flashed on reversal")
        test.advance(900)
        local opened = test.ipc("pose")
        test.truthy(opened.visible)
        test.near(opened.x, 0, 0.01) test.near(opened.y, 0, 0.01)
        test.near(opened.scale, 1, 0.01)
        test.near(opened.opacity, 1, 0.01) test.near(opened.background, 1, 0.01)
        test.ipc("open", false) test.advance(900)
        test.falsy(test.ipc("pose").visible, "closed panel remained visible")
        test.eq(#test.logs("warn"), 0)
        test.eq(#test.logs("error"), 0)
      end)
    end
    test.it(style .. " staggers cards and reverses them from the current pose", function()
      test.load("../shell/init.lua", { source = HOST, size = { 1280, 720 }, env = {
        CAELESTIA_STYLE = style, TEST_EDGE = "top",
        CAELESTIA_APPEARANCE = morf.env("XDG_CACHE_HOME") .. "/motion-appearance.json",
        CAELESTIA_SETTINGS = morf.env("XDG_CACHE_HOME") .. "/motion-settings.json",
      } })
      test.ipc("open", true) test.advance(900)
      test.ipc("cards", true) test.advance(60)
      local first = test.ipc("card_pose")
      test.truthy(first[1].opacity > 0 and first[1].opacity < 1)
      test.near(first[2].opacity, 0, 0.01, "second card ignored its reveal delay")
      test.ipc("cards", false) test.advance(20)
      local closing = test.ipc("card_pose")
      test.ipc("cards", true)
      local reversed = test.ipc("card_pose")
      test.near(reversed[1].opacity, closing[1].opacity, 0.01)
      test.near(reversed[1].scale, closing[1].scale, 0.01)
      test.near(reversed[1].y, closing[1].y, 0.01)
      test.advance(800)
      for _, card in ipairs(test.ipc("card_pose")) do
        test.near(card.opacity, 1, 0.01)
        test.near(card.scale, 1, 0.01)
        test.near(card.y, 0, 0.01)
      end
      test.eq(#test.logs("warn"), 0)
    end)
  end
  test.it("Tsugumori covers the panel before travelling and uncovers only after arrival", function()
    test.load("../shell/init.lua", { source = HOST, size = { 1280, 720 }, env = {
      CAELESTIA_STYLE = "tsugumori", TEST_EDGE = "top",
      CAELESTIA_APPEARANCE = morf.env("XDG_CACHE_HOME") .. "/motion-appearance.json",
      CAELESTIA_SETTINGS = morf.env("XDG_CACHE_HOME") .. "/motion-settings.json",
    } })
    test.ipc("open", true)
    test.advance(200)
    test.near(test.get("drawer-sample-curtain").width, 320, 0.01)
    test.advance(80)
    test.near(test.ipc("pose").y, 0, 0.01)
    test.advance(70)
    local cover = test.get("drawer-sample-curtain")
    test.truthy(cover.width > 0 and cover.width < 320, "cover never retracted")
    test.truthy(cover.visible)
    test.truthy(test.get("drawer-sample-reveal-flash").opacity>0)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-drawer-flash.png") end
    test.advance(300)
    test.falsy(test.get("drawer-sample-curtain").visible)
    test.falsy(test.get("drawer-sample-reveal-flash").visible)
    test.truthy(test.get("drawer-sample-lock-corner-a").opacity>0)
    test.truthy(test.get("drawer-sample-lock-pip").visible)
    test.advance(240)
    test.falsy(test.get("drawer-sample-lock-pip").visible)
    test.ipc("open", false) test.advance(120)
    test.near(test.ipc("pose").y, 0, 0.01, "panel left before the cover arrived")
    test.truthy(test.get("drawer-sample-curtain").width > 0)
    test.ipc("open", true) test.advance(900)
    test.truthy(test.ipc("pose").visible)
    test.falsy(test.get("drawer-sample-curtain").visible, "interrupted cover blocked the controls")
    test.falsy(test.get("drawer-sample-lock-corner-a").visible)
  end)
end)
