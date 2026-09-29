local test = morf.test
local function options(style)
  return { CAELESTIA_STYLE = style, CAELESTIA_DRY_RUN = "1",
    CAELESTIA_APPEARANCE = morf.env("XDG_CACHE_HOME") .. "/appearance-test.json",
    CAELESTIA_SETTINGS = morf.env("XDG_CACHE_HOME") .. "/appearance-shell-test.json" }
end
test.describe("visual themes", function()
  test.it("changes visual components independently of wallpaper colors", function()
    local colors = {}
    for _, style in ipairs { "material", "tsugumori" } do
      test.load("../shell/init.lua", { env = options(style), source = [[
        local theme = require("theme")
        local kit = require("kit")
        kit.card { id = "sample-card", width = 320, height = 180,
          kit.pill { id = "sample-button", width = 150, label = "Sample", on_clicked = function() end } }
        morf.ipc.color = function(source)
          if source then theme.follow(source) end
          return { name = theme.appearance.id, color = theme.color.primary:hex(), font = theme.font }
        end
      ]] })
      test.eq(test.ipc("color").name, style)
      colors[style] = {}
      for _, accent in ipairs { "#22aa88", "#9955ee" } do
        test.ipc("color", accent) test.advance(450)
        colors[style][#colors[style] + 1] = test.ipc("color").color
      end
      test.eq(#test.logs("warn"), 0)
      test.eq(#test.logs("error"), 0)
    end
    test.eq(colors.material, colors.tsugumori, "visual theme changed the wallpaper palette")
    test.truthy(colors.material[1] ~= colors.material[2], "palette no longer follows its source")
  end)
  for _, style in ipairs { "material", "tsugumori" } do
    test.it(style .. " keeps lock password and method handling in the controller", function()
      test.load("../lock/init.lua", { env = options(style), args = { "window", "preview" }, size = { 1920, 1080 } })
      test.advance(50)
      test.eq(test.ipc("stage"), "rest")
      test.type("wrong") test.advance(450)
      test.eq(test.ipc("stage"), "sheet")
      test.key("Return") test.advance(850)
      test.truthy(test.get("lock-message").text:find("Wrong") ~= nil)
      test.click("lock-method") test.advance(450)
      test.key("Escape") test.advance(450)
      test.eq(test.ipc("stage"), "rest")
      test.type("w") test.advance(500)
      if morf.env("CAELESTIA_THEME_SNAPSHOTS") == "1" then test.snapshot(style .. "-lock-sheet.png") end
      test.eq(#test.logs("warn"), 0)
      test.eq(#test.logs("error"), 0)
    end)
    test.it(style .. " opens the greeter and switches authentication methods", function()
      test.load("../greet/init.lua", { env = options(style), args = { "preview" }, size = { 1920, 1080 } })
      test.advance(50)
      test.key("Return") test.advance(600)
      test.eq(test.ipc("stage"), "sheet")
      test.click("greet-method") test.advance(450)
      test.type("example") test.key("Escape") test.key("Escape") test.advance(450)
      test.eq(test.ipc("stage"), "rest")
      test.key("Return") test.advance(600)
      if morf.env("CAELESTIA_THEME_SNAPSHOTS") == "1" then test.snapshot(style .. "-greet-sheet.png") end
      test.eq(#test.logs("warn"), 0)
      test.eq(#test.logs("error"), 0)
    end)
  end
end)
