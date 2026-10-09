local test=morf.test
local SOURCE=[[
  local scroll=require("lib.kit.scroll")
  local make=scroll.make
  local views={}
  scroll.make=function(widget,spec)
    local root,flick,state,ctl=make(widget,spec)
    if spec.id then views[spec.id]={flick=flick,state=state} end
    return root,flick,state,ctl
  end
  require("init")
  morf.surface.width=420 morf.surface.height=800
  morf.ipc.scroll_state=function(id)
    local view=assert(views[id],id)
    return {offset=view.flick.content_y or 0,content=view.state.content_height,
      viewport=view.state.viewport_height,tab=require("dashboard").tab:get(),
      dashboard=require("dashboard").drawer.open:get(),sidebar=require("sidebar").drawer.open:get()}
  end
  morf.ipc.top_state=function()
    local sidebar=require("sidebar")
    local keys={} for i,tab in ipairs(sidebar.TABS) do keys[i]=tab.key end
    return {open=sidebar.drawer.open:get(),notifications=sidebar.showing("notifications"),
      settings=sidebar.showing("settings"),keys=keys,detail=require("utilities").detail:get()}
  end
]]
local function load(style)
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{source=SOURCE,size={420,800},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",CAELESTIA_SCALE_MODE="compositor",CAELESTIA_WALLPAPER=""}})
  test.advance(800)
end
local function drag(x,y,dy)
  test.touch("down",0,x,y)
  for i=1,8 do test.advance(25) test.touch("move",0,x,y+dy*i/8) end
  test.touch("up",0,x,y+dy) test.advance(350)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." top pulls select by starting side and settings is the rightmost tab",function()
    load(style)
    local tabs=test.ipc("top_state").keys
    test.eq(tabs[1],"notifications") test.eq(tabs[#tabs],"settings")
    drag(50,4,300)
    local left=test.ipc("top_state")
    test.truthy(left.open) test.truthy(left.notifications)
    drag(50,4,300)
    test.truthy(test.ipc("top_state").notifications)
    test.ipc("sidebar","close") test.advance(600)
    test.touch("down",0,380,4)
    test.touch("move",0,160,300)
    test.touch("up",0,160,300) test.advance(700)
    test.truthy(test.ipc("top_state").settings)
    test.ipc("settings","theme/lule") test.advance(700)
    drag(380,4,300)
    test.truthy(test.ipc("top_state").settings)
    test.eq(test.ipc("top_state").detail,"")
    drag(50,4,300)
    test.truthy(test.ipc("top_state").notifications)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." dashboard scrolls by touch from the page background",function()
    load(style)
    test.ipc("dashboard","open") test.advance(1000)
    local r=test.get("dashboard-scroll-1")
    local before=test.ipc("scroll_state","dashboard-scroll-1")
    test.truthy(before.content>before.viewport)
    drag(r.x+4,r.y+r.height-30,-140)
    local after=test.ipc("scroll_state","dashboard-scroll-1")
    test.truthy(after.offset>100,"page did not scroll: "..after.offset)
    test.truthy(after.dashboard) test.eq(after.tab,1)
    drag(r.x+4,r.y+40,80)
    local back=test.ipc("scroll_state","dashboard-scroll-1")
    test.truthy(back.offset<after.offset-50)
    test.truthy(back.dashboard)
    local cards=test.get("dashboard-cards-1")
    local viewport=test.get("dashboard-viewport-1")
    test.near(cards.y,viewport.y,1)
    test.near(cards.height,viewport.height,1)
    test.truthy(viewport.clip)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." settings scrolls vertically without dismissing its panel",function()
    load(style)
    test.ipc("settings","theme/lule") test.advance(1000)
    local r=test.get("lule-scroll")
    local before=test.ipc("scroll_state","lule-scroll")
    test.truthy(before.content>before.viewport)
    drag(r.x+4,r.y+r.height-30,-140)
    local after=test.ipc("scroll_state","lule-scroll")
    test.truthy(after.offset>100,"settings did not scroll: "..after.offset)
    test.truthy(after.sidebar,"scroll dismissed settings")
    test.eq(test.logs("error"),{})
  end)
end
