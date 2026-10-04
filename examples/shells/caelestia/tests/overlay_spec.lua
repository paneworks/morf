-- The overlay layer over caelestia's dashboard: a popup opened from a page
-- control sits beside it, closes on Escape and on a press outside, and
-- gives focus back to the control that opened it.
--
--     morf test --no-dbus examples/shells/caelestia/tests/overlay_spec.lua

local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  package.loaded.bar={desk=function() return 0,0,W,H end}
  local sample=morf.signal("test.dashboard.sample",42)
  local reads,actions={},{}
  local function read(name,value)
    reads[name]=(reads[name] or 0)+1
    sample:get()
    return value
  end
  local sources={}
  for _,name in ipairs {"cpu","memory","drives","network","gpu","fans","battery"} do
    sources[name]={pin=function() end,running=function() return true end,samples=0,interval=3000}
  end
  package.loaded["lib.services.sysinfo"]={sources=sources,history_size=60,
    restore_history=function() end,snapshot_history=function() return {} end,
    cpu=function() return read("cpu",{usage=sample:get()}) end,
    memory=function() return read("memory",{percent=58}) end,
    disks=function() return read("disks",{{mount="/",percent=34}}) end,
    battery=function() return read("battery",{batteries={{name="BAT1",capacity=76,status="Discharging"}}}) end,
    history=function(key) return read("history",key:match("voltage$") and {11.4,11.6,11.5,11.7} or {42,58,68,76}) end,
    system=function() return read("system",{hostname="workstation",uptime=4386}) end}
  package.loaded.services={hyprland={available=function() return false end},
    weather=function() return read("weather",{available=true,temperature=21,condition="Clear",code=0,is_day=true,
      daily={{low=10,high=20},{low=11,high=21},{low=12,high=22},{low=13,high=23},
        {low=14,high=24},{low=15,high=25},{low=16,high=26}}}) end,
    weather_symbol=function() return "sunny" end,
    player=function() return read("player",{title="Morning signal",artist="Preview",album="Desktop",art_url="",playing=false}) end,
    media={play_pause=function() actions[#actions+1]="play_pause" end,
      previous=function() actions[#actions+1]="previous" end,next=function() actions[#actions+1]="next" end}}
  package.loaded.lule_studio={active=morf.signal("test.dashboard.lule",false)}
  local kit=require("kit")
  local C=require("theme").color
  local model=require("dashboard_model")
  model.username,model.face="preview",""
  model.clock=function(format) return format=="%H:%M" and "08:24" or format=="%I" and "08" or format=="%M" and "24" or "MONDAY, 28 SEPTEMBER" end
  model.calendar=function() return model.month(model.month_offset:get(),{year=2026,month=9,day=28}) end
  local page_sizes={{840,439},{1000,350},{1400,760},{1000,680},{870,650},{960,489}}
  local page_cache={}
  -- A test popup: two entries, opened beside the page's action.
  local menu
  local closes={}
  function open_menu(index)
    menu=menu or kit.card {id="test-menu",width=180,height=96,
      kit.action {id="test-menu-one",x=8,y=8,width=164,height=36,on_clicked=function() actions[#actions+1]="one" end},
      kit.action {id="test-menu-two",x=8,y=52,width=164,height=36,on_clicked=function() actions[#actions+1]="two" end}}
    morf.overlay.open(menu,{anchor=page_cache[index].page,placement="bottom-end",
      on_close=function(reason) closes[#closes+1]=reason end})
  end
  model.page=function(index)
    if index==5 and morf.env("TEST_REAL_WEATHER")=="1" then return require("dashboard_weather") end
    if index==4 and morf.env("TEST_REAL_BATTERY")=="1" then return require("dashboard_battery") end
    if not page_cache[index] then
      local w,h=table.unpack(page_sizes[index])
      local ctx=require("dashboard_state").context(index)
      local node=kit.card {id="test-page-"..index,width=w,height=h,
        kit.heading {id="test-title-"..index,x=20,y=20,text=model.tabs[index].name,active=ctx.opened},
        kit.subtitle {x=20,y=58,text="The page keeps its original controller and actions."},
        kit.pill {id="test-action-"..index,x=w-170,y=h-52,width=150,height=36,label="Page action",
          on_clicked=function() actions[#actions+1]="page-"..index open_menu(index) end},
      }
      page_cache[index]={WIDTH=w,HEIGHT=h,page=node}
    end
    return page_cache[index]
  end
  local dashboard=require("dashboard")
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return C.surfaceContainerLowest end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surface end,dashboard.drawer.shape},dashboard.drawer.panel},
  }
  morf.ipc.show=function(on) dashboard.drawer.set(on=="yes") end
  morf.ipc.select=function(index) model.select(tonumber(index)) end
  morf.ipc.sample=function(value) sample:set(tonumber(value)) end
  morf.ipc.menu=function() return menu~=nil and morf.overlay.is_open(menu),table.concat(closes,",") end
  morf.ipc.state=function()
    return {tab=model.tab:get(),displayed=model.displayed:get(),opened=model.opened:get(),reads=reads,actions=actions,
      month=model.month_offset:get(),lule=require("lule_studio").active:get()}
  end
]]

local function load(style)
  test.load("../shell/init.lua",{source=HOST,size={1920,1080},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH="1920",TEST_HEIGHT="1080"}})
  test.ipc("show","yes") test.settle(2000)
  test.ipc("select","3") test.settle(2400)
end

local function focused()
  for _,node in ipairs(test.nodes()) do if node.focused then return node.id end end
end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." popup over the dashboard closes on Escape and gives focus back",function()
    load(style)
    -- Reach the page's action by keyboard and press it.
    for _=1,40 do
      if focused()=="test-action-3" then break end
      test.key("Tab") test.advance(20)
    end
    test.eq(focused(),"test-action-3")
    test.key("Return") test.settle(500)
    test.truthy(test.ipc("menu"),"the popup did not open")
    local menu,page=test.get("test-menu"),test.get("test-page-3")
    test.truthy(menu.visible)
    -- Beside the page, inside the surface, focus on its first entry.
    test.truthy(menu.y>=page.y+page.height-1 or menu.y+menu.height<=page.y+1,"not beside its anchor")
    test.truthy(menu.x>=0 and menu.x+menu.width<=1920)
    test.eq(focused(),"test-menu-one")
    test.truthy(test.get("test-menu-one").visual_focus)
    -- Tab walks inside it; Return picks an entry.
    test.key("Tab") test.advance(20)
    test.eq(focused(),"test-menu-two")
    test.key("Escape") test.settle(500)
    local open,closes=test.ipc("menu")
    test.falsy(open) test.eq(closes,"escape")
    test.falsy(test.get("test-menu").visible)
    test.eq(focused(),"test-action-3")
    test.eq(#test.logs("error"),0)
  end)

  test.it(style.." popup closes on a press outside and a click on the opener reopens it",function()
    load(style)
    test.click("test-action-3") test.settle(500)
    test.truthy(test.ipc("menu"))
    -- A press inside it keeps it open.
    test.click("test-menu-two") test.settle(200)
    test.truthy(test.ipc("menu"))
    test.click(20,1060) test.settle(500)
    local open,closes=test.ipc("menu")
    test.falsy(open) test.eq(closes,"outside")
    test.eq(test.ipc("state").actions[2],"two")
  end)
end
