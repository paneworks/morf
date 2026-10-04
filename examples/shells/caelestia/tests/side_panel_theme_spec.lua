local test = morf.test
local HOST = [[
  local ui, kit = require("morf.ui"), require("kit")
  local C = require("theme").color
  local W,H = tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height = H
  local desk = morf.signal("side.test.desk",H)
  package.loaded.bar = {desk=function() return 0,0,W,desk:get() end}
  local watch, actions = {}, {}
  package.loaded.planner = {client={watch=function(on) watch[#watch+1]=on end}}
  local covered = morf.signal("side.test.covered",false)
  package.loaded.notifs = {covered=covered}
  local function page(panel,key,title)
    return function(w,h)
      return ui.Item {width=w,height=h,
        kit.card {width=w,height=160,
          kit.heading {id="side-title-"..key,x=16,y=16,text=title,scope=panel.."."..key},
          kit.subtitle {x=16,y=56,text="Shared state, themed presentation."},
          ui.Item {x=16,y=98,width=w-32,height=36,
            kit.pill {id="side-action-"..key,label="Use "..key,width=w-32,height=36,
              on_clicked=function() actions[#actions+1]=key end}},
        },
      }
    end
  end
  package.loaded.tasks_page = {build=page("leftbar","tasks","Tasks")}
  package.loaded.calendar_page = {build=page("leftbar","calendar","Calendar")}
  package.loaded.utilities = {page=page("sidebar","settings","Settings")}
  package.loaded.notification_history = {groups=morf.list_model({}),clear=function() end,
    build=page("sidebar","notifications","Notifications")}
  local left,right = require("leftbar"),require("sidebar")
  local opening = ui.Item {x=10,y=10,width=W-20,height=function() return desk:get()-20 end,
    ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainerLowest end,left.drawer.shape,right.drawer.shape},
    -- A press on the desk shuts either panel: their close policy.
   left.drawer.panel,right.drawer.panel}
  ui.Item {width=W,height=H,ui.Rect {anchors={fill=true},color=function() return C.surface end},opening}
  local panels={left=left,right=right}
  morf.ipc.open=function(side,on) panels[side].drawer.set(on=="yes") end
  morf.ipc.select=function(side,key) return panels[side].panel.select(tonumber(key) or key) end
  morf.ipc.present=function(side,index)
    local model=panels[side].panel
    if model.present then model.present(tonumber(index)) end
  end
  morf.ipc.resize=function(h) desk:set(tonumber(h)) end
  morf.ipc.state=function()
    local function state(p)
      return {open=p.drawer.open:get(),tab=p.panel.tab:get(),displayed=(p.panel.displayed or p.panel.tab):get()}
    end
    local active={}
    for _,scope in ipairs {"leftbar.tasks","leftbar.calendar","sidebar.settings","sidebar.notifications"} do
      active[scope]=require("presentation").active(scope)()
    end
    return {left=state(left),right=state(right),watch=watch,covered=covered:get(),active=active,actions=actions}
  end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1280,h or 800},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1280),TEST_HEIGHT=tostring(h or 800)}})
  test.advance(100)
end
local function state() return test.ipc("state") end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." side panels preserve geometry, navigation and service lifetimes",function()
    load(style)
    test.eq(state().watch,{false})
    test.ipc("open","left","yes") test.advance(2400)
    test.eq(state().watch,{false,true})
    test.truthy(state().active["leftbar.tasks"])
    test.eq(test.get("drawer-leftbar").width,450)
    test.eq(test.get("drawer-leftbar").height,780)
    test.click("side-action-tasks") test.eq(state().actions,{"tasks"})
    shot(style.."-side-tasks")
    test.click("leftbar-tab-calendar") test.advance(2400)
    test.eq(state().left.tab,2)
    test.truthy(state().active["leftbar.calendar"])
    test.falsy(state().active["leftbar.tasks"])
    test.eq(state().watch,{false,true})
    shot(style.."-side-calendar")
    test.ipc("open","left","no") test.advance(1100)
    test.eq(state().watch,{false,true,false})
    test.falsy(state().active["leftbar.calendar"])
    test.ipc("open","right","yes") test.advance(2400)
    test.falsy(state().covered)
    test.eq(test.get("drawer-sidebar").x,820)
    -- One geometry in every theme (THEMING.md, theme rule 4).
    test.eq(test.get("sidebar-pages").width,408)
    shot(style.."-side-settings")
    test.click("sidebar-tab-notifications") test.advance(2400)
    test.truthy(state().covered)
    test.truthy(state().active["sidebar.notifications"])
    shot(style.."-side-history")
    test.ipc("open","right","no") test.advance(1100)
    test.falsy(state().covered)
    test.falsy(state().active["sidebar.notifications"])
    test.eq(#test.runs(),0) test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." side panels keep the latest selection through close, resize and reopen",function()
    load(style,500,720)
    test.ipc("open","left","yes") test.advance(2400)
    test.ipc("select","left","calendar") test.advance(90)
    test.ipc("open","left","no") test.advance(70)
    test.ipc("select","left","tasks") test.ipc("open","left","yes") test.advance(90)
    test.ipc("select","left","calendar") test.advance(2400)
    test.eq(state().left.tab,2) test.eq(state().left.displayed,2)
    test.truthy(state().active["leftbar.calendar"])
    test.falsy(state().active["leftbar.tasks"])
    test.ipc("select","left","missing") test.ipc("select","left","99")
    test.ipc("present","left","1") test.advance(100)
    test.eq(state().left.tab,2) test.eq(state().left.displayed,2)
    test.ipc("resize","550") test.advance(700)
    test.eq(test.get("drawer-leftbar").height,530)
    test.eq(test.get("leftbar-pages").height,444)
    shot(style.."-side-compact")
    test.ipc("open","left","no") test.advance(1200)
    test.falsy(test.get("drawer-leftbar").visible)
    test.falsy(state().active["leftbar.calendar"])
    test.eq(state().watch[#state().watch],false)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori side panel headings follow the displayed page beneath the cover",function()
  load("tsugumori") test.ipc("open","left","yes") test.advance(2400)
  test.eq(test.get("side-title-tasks-text").text,"TASKS")
  test.click("leftbar-tab-calendar") test.advance(100)
  test.eq(state().left.tab,2) test.eq(state().left.displayed,1)
  test.truthy(state().active["leftbar.tasks"])
  test.falsy(state().active["leftbar.calendar"])
  test.truthy(test.get("leftbar-page-curtain").width>0)
  test.click("side-action-tasks") test.eq(#state().actions,0)
  test.advance(250)
  test.eq(state().left.displayed,2)
  test.advance(500)
  test.truthy(test.get("side-title-calendar-text").text~="CALENDAR")
  shot("side-calendar-decoding")
  test.advance(1800)
  test.eq(test.get("side-title-calendar-text").text,"CALENDAR")
  test.falsy(test.get("leftbar-page-curtain").visible)
  test.click("side-action-calendar") test.eq(state().actions,{"calendar"})
  test.eq(#test.logs("error"),0)
end)
