local test = morf.test
local HOST = [[
  local ui = require("morf.ui")
  local W,H = tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height = H
  local desk = morf.signal("bottom.test.desk", {w=W,h=H})
  package.loaded.bar = {desk=function() local d=desk:get() return 0,0,d.w,d.h end}
  local bottom = require("bottom")
  local C = require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return C.surface end},
    ui.MouseArea {anchors={fill=true},on_clicked=function() bottom.drawer.set(false) end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainerLowest end,bottom.drawer.shape},
      bottom.drawer.panel},
  }
  morf.ipc.open=function(on) bottom.drawer.set(on=="yes") end
  morf.ipc.select=bottom.panel.select
  morf.ipc.desk=function(w,h) desk:set{w=tonumber(w),h=tonumber(h)} end
  morf.ipc.color=function(accent) require("theme").follow(accent) end
  morf.ipc.state=function()
    return {open=bottom.drawer.open:get(),tab=bottom.panel.tab:get(),
      displayed=(bottom.panel.displayed or bottom.panel.tab):get(),
      assistant=require("presentation").active("bottom.assistant")(),
      drop=require("presentation").active("bottom.drop")()}
  end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1920,h or 1080},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w or 1920),TEST_HEIGHT=tostring(h or 1080),CAELESTIA_DRY_RUN="1"}})
  test.advance(100)
end
local function open() test.ipc("open","yes") test.advance(2400) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." bottom preserves navigation and geometry without starting integrations",function()
    load(style) open()
    test.eq(test.ipc("state").tab,1)
    local panel=test.get("drawer-bottom")
    test.eq(panel.width,1504)
    test.eq(panel.height,612)
    test.truthy(test.get("assistant-page").width>900)
    shot(style.."-bottom-assistant")
    test.click("bottom-tab-drop") test.advance(2500)
    test.eq(test.ipc("state").tab,2)
    test.truthy(test.get("drop-placeholder").visible)
    shot(style.."-bottom-drop")
    test.falsy(test.ipc("select","missing"))
    test.eq(test.ipc("state").tab,2)
    test.click(50,50) test.advance(1000)
    test.falsy(test.get("drawer-bottom").visible)
    test.ipc("select","assistant") open()
    test.truthy(test.get("assistant-page").visible)
    test.eq(#test.runs(),0)
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
end
-- (On a desk the tabs name the pages and the frame draws no title; a
-- compact screen names each page with its decoding heading.)
test.it("Tsugumori bottom pages follow the displayed page under its cover",function()
  load("tsugumori") open()
  test.falsy(test.get("assistant-title-text").visible)
  test.click("bottom-tab-drop") test.advance(100)
  test.eq(test.ipc("state").tab,2)
  test.eq(test.ipc("state").displayed,1)
  test.truthy(test.ipc("state").assistant)
  test.falsy(test.ipc("state").drop)
  test.truthy(test.get("bottom-page-curtain").width>0)
  shot("bottom-switch-cover")
  test.advance(250)
  test.eq(test.ipc("state").displayed,2)
  test.falsy(test.ipc("state").assistant)
  test.truthy(test.ipc("state").drop)
  test.advance(2300)
  test.falsy(test.get("bottom-page-curtain").visible)
  test.truthy(test.get("drop-placeholder").visible)
  test.falsy(test.get("drop-title-text").visible)
  load("tsugumori",500,720) open()
  test.click("bottom-tab-drop") test.advance(100)
  test.truthy(test.get("drop-title-text").visible)
  test.truthy(test.get("drop-title-text").text~="DROP","the page's title did not decode")
  shot("bottom-drop-decoding")
  test.advance(2400)
  test.eq(test.get("drop-title-text").text,"DROP")
  test.eq(#test.logs("error"),0)
end)
test.it("Tsugumori bottom survives rapid switches and closing",function()
  load("tsugumori") open()
  test.click("bottom-tab-drop") test.advance(90)
  test.ipc("open","no") test.advance(70)
  test.ipc("select","assistant") test.ipc("open","yes") test.advance(120)
  test.ipc("select","drop") test.advance(120)
  test.ipc("select","assistant") test.advance(2400)
  test.eq(test.ipc("state").displayed,1)
  test.truthy(test.get("drawer-bottom").visible)
  test.falsy(test.get("bottom-page-curtain").visible)
  test.truthy(test.get("assistant-page").visible)
  test.falsy(test.get("drop-page").visible)
  test.eq(test.get("drawer-bottom").width,1504)
  test.ipc("open","no") test.advance(1200)
  test.falsy(test.ipc("state").assistant)
  test.falsy(test.ipc("state").drop)
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
test.it("Tsugumori compact bottom keeps all content reachable and uses the supplied palette",function()
  load("tsugumori",500,720) open()
  local panel=test.get("drawer-bottom")
  test.truthy(panel.x>=0 and panel.x+panel.width<=500,"the drawer overflows the screen")
  for _,id in ipairs {"bottom-tab-assistant","bottom-tab-drop","assistant-welcome","assistant-connection"} do
    local n=test.get(id)
    test.truthy(n.x>=panel.x and n.x+n.width<=panel.x+panel.width,id.." overflows")
  end
  shot("bottom-compact-assistant")
  local viewport=test.get("assistant-page")
  test.wheel(0,2500,{x=viewport.x+100,y=viewport.y+100}) test.advance(500)
  local last=test.get("assistant-context")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height,"the last row is out of reach")
  test.click("bottom-tab-drop") test.advance(2400)
  viewport=test.get("drop-page")
  local card=test.get("drop-placeholder")
  test.truthy(card.y>=viewport.y and card.y+card.height<=viewport.y+viewport.height)
  shot("bottom-compact-files")
  test.ipc("color","#00c5ae") test.advance(500)
  shot("bottom-compact-palette")
  test.click("bottom-tab-assistant") test.advance(2400)
  test.near(test.get("page-assistant-head").y,test.get("bottom-page-assistant").y,.01)
  test.eq(#test.runs(),0)
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
