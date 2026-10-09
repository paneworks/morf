-- Stage 11's archetypes in each caelestia theme: a split pane's divider,
-- drawn by the skin's grip and dragged, a navigation view whose pages move
-- by the skin's transition, and an expander drawn by the skin's header.
--
--     morf test --no-dbus examples/shells/caelestia/tests/kit_drag_nav_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  require("kit")
  local drag = require("lib.kit.drag")
  local navigation = require("lib.kit.navigation")
  local disclosure = require("lib.kit.disclosure")
  local split, ratio = drag.split { id = "split", x = 10, y = 10, width = 400, height = 120,
    first = ui.Rect { width = 400, height = 120 }, second = ui.Rect { width = 400, height = 120 } }
  local nav_node, nav = navigation.make("navigation_view", { id = "nav", x = 10, y = 150, width = 300, height = 160,
    current = "home",
    pages = { home = function() return ui.Rect { id = "page-home", width = 300, height = 160 } end,
      sound = function() return ui.Rect { id = "page-sound", width = 300, height = 160 } end } })
  local more = disclosure.make("expander", { id = "more", x = 420, y = 10, width = 200, title = "More",
    content = ui.Rect { id = "more-body", width = 200, height = 60 } })
  ui.Item { width = 640, height = 360, split, nav_node, more }
  morf.ipc.ratio = function() return ratio:get() end
  morf.ipc.push = function(p) nav.push(p) end
  morf.ipc.pop = function() nav.pop() end
]]

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " draws and runs a split, a navigation view and an expander", function()
    test.load("../shell/init.lua", { size = { 640, 360 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
    test.settle(300)
    test.drag({ 10 + 200, 70 }, { 10 + 280, 70 }, { steps = 5 }) test.settle(300)
    test.truthy(test.ipc("ratio") > 0.65, tostring(test.ipc("ratio")))
    test.ipc("push", "sound") test.settle(600)
    test.truthy(test.get("page-sound").visible)
    test.falsy(test.get("page-home").visible, "the old page stayed")
    test.ipc("pop") test.settle(600)
    test.truthy(test.get("page-home").visible)
    test.near(test.get("more").height, 40, 1)
    test.click(470, 30) test.settle(600)
    test.near(test.get("more").height, 100, 1)
    test.eq(#test.logs("error"), 0)
  end)
end

for _,style in ipairs {"material","tsugumori","default"} do
  test.it(style.." navigation sends clicks only to the incoming page during rapid changes",function()
    test.load("../shell/init.lua",{size={400,240},env={CAELESTIA_STYLE=style=="default" and "material" or style},
      source=([[
        local ui=require("morf.ui")
        local kit=require("kit")
        %s
        local calls={}
        local allowed=morf.signal("nav.fixture.allowed",true)
        local function page(name)
          return function()
            return ui.Item {id="page-"..name,width=300,height=180,
              enabled=function() return name~="a" or allowed:get() end,
              kit.pill {id="action-"..name,width=300,height=180,label=name,
                on_clicked=function() calls[#calls+1]=name end}}
          end
        end
        local node,nav=require("lib.kit.navigation").make("view_stack",{
          width=300,height=180,current="a",mode="switcher",order={"a","b","c"},
          pages={a=page("a"),b=page("b"),c=page("c")}})
        ui.Item {width=400,height=240,node}
        morf.ipc.go=function(name) nav.go(name) end
        morf.ipc.enable=function(on) allowed:set(on=="yes") end
        morf.ipc.calls=function() local out=calls calls={} return out end
      ]]):format(style=="default" and 'kit=require("lib.kit.skins.default").make {variant="dark"}' or "")})
    test.advance(500)
    for _,name in ipairs {"b","a","c","b","a"} do
      test.ipc("go",name) test.advance(100)
      test.click(150,90)
      test.eq(test.ipc("calls"),{name},"A leaving page intercepted the new page's action")
    end
    test.advance(500)
    test.truthy(test.get("page-a").visible)
    test.falsy(test.get("page-b").visible) test.falsy(test.get("page-c").visible)
    test.ipc("enable","no")
    test.ipc("go","b") test.advance(100)
    test.ipc("go","a") test.advance(300)
    test.click(150,90) test.eq(test.ipc("calls"),{},"Returning to a page erased its disabled state")
    test.ipc("enable","yes") test.advance(100)
    test.click(150,90) test.eq(test.ipc("calls"),{"a"})
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
end
