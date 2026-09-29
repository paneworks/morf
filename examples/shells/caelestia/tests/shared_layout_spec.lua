-- Themes may change faces/motion, not the application's content composition.
local test=morf.test
local function load(style)
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{size={1920,1080},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",
    CAELESTIA_FONT_FILE="",LULE_A="/nonexistent/lule",HYPRLAND_INSTANCE_SIGNATURE=false},source=[[
    require("services").here=function() return true end
    require("services").weather=function() return {available=false} end
    require("init")
    morf.ipc.test_page=function(index)
      local dashboard=require("dashboard")
      dashboard.tab:set(tonumber(index)) dashboard.drawer.set(true)
    end
  ]]})
  test.advance(1800)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end

test.it("both themes select the same content builders; approved edge skins stay independent",function()
  test.load("../shell/init.lua",{source=[[
    local themes=require("themes")
    local a,b=themes.load("material"),themes.load("tsugumori")
    morf.ipc.audit=function() return {material=a.views,tsugumori=b.views} end
    require("morf.ui").Item {width=100,height=100}
  ]]})
  local maps=test.ipc("audit")
  for name,path in pairs(maps.material) do
    if name~="frame" and name~="rail" and name~="levels" then
      test.eq(maps.tsugumori[name],path,"different content builder: "..name)
      test.truthy(path:find("themes.layouts.views.",1,true)==1)
    end
  end
end)

test.it("dashboard, settings and workspaces keep the same sections and geometry in both themes",function()
  local cases={
    {"test_page","1",nil,{"dashboard-weather","dashboard-calendar","dashboard-media"}},
    {"test_page","2",nil,{"dashboard-media-tab","media-lyrics"}},
    {"test_page","3",nil,{"performance-devices","performance-main"}},
    {"test_page","4",nil,{"battery-charge-card","battery-main"}},
    {"test_page","5",nil,{"dashboard-weather-tab","weather-now"}},
    {"lule","open",nil,{"lule-wallpaper-card","lule-colors-card","lule-controls"}},
    {"utilities","open",nil,{"utilities-sliders","utilities-toggles","utilities-capture"}},
    {"settings","network",nil,{"network-page","network-rescan"}},
    {"settings","bluetooth",nil,{"bluetooth-page","bluetooth-settings"}},
    {"bottom","open","assistant",{"assistant-page"}},
    {"bottom","open","drop",{"drop-page"}},
    {"tasks","open",nil,{"tasks-page","tasks-list","tasks-add"}},
    {"calendar","open",nil,{"planner-calendar","planner-scroll"}},
    {"capture","open",nil,{"capture-page","capture-target-region","capture-target-screen"}},
  }
  local baseline={}
  for _,style in ipairs {"material","tsugumori"} do
    load(style)
    for index,case in ipairs(cases) do
      test.ipc("close") test.advance(1100)
      test.ipc(case[1],case[2],case[3]) test.advance(2800)
      for _,id in ipairs(case[4]) do
        local node=test.get(id)
        local dimensions={x=node.x,y=node.y,width=node.width,height=node.height}
        if style=="material" then baseline[id.."-visible"]=node.visible
        else test.eq(node.visible,baseline[id.."-visible"],"visibility changed: "..id) end
        if style=="material" then baseline[id]=dimensions
        else
          for key,value in pairs(dimensions) do
            -- The search label uses each skin's font metrics; containers and
            -- control dimensions remain exact, with the same row ordering.
            local tolerance=id=="tasks-list" and key=="y" and 4 or 1
            test.near(value,baseline[id][key],tolerance,id.." "..key)
          end
        end
      end
      if style=="tsugumori" and index==1 then
        for _,key in ipairs {"overview","media","performance","battery","weather","lule"} do
          local button=test.get("dashboard-tab-"..key)
          local icon=test.get("dashboard-tab-"..key.."-icon")
          test.truthy(icon.visible,"missing tab icon: "..key)
          test.near(icon.x+icon.width,button.x+button.width-9,1)
        end
      end
      shot(style.."-shared-"..index)
    end
    test.eq(#test.logs("error"),0)
  end
end)

for _,part in ipairs {"lock","greet"} do
  test.it(part.." keeps the same account, field and authentication sheet geometry",function()
    local baseline={}
    for _,style in ipairs {"material","tsugumori"} do
      test.load("../"..part.."/init.lua",{size={1280,900},args=part=="lock" and {"window","preview"} or {"preview"},
        env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",GREETD_SOCK=false,LULE_A="/nonexistent/lule"}})
      test.ipc("stage","sheet") test.advance(2800)
      for _,suffix in ipairs {"sheet","field","submit","method"} do
        local node=test.get(part.."-"..suffix)
        local dimensions={x=node.x,y=node.y,width=node.width,height=node.height}
        if style=="material" then baseline[suffix]=dimensions
        else for key,value in pairs(dimensions) do test.near(value,baseline[suffix][key],1,suffix.." "..key) end end
      end
      shot(style.."-shared-"..part)
      test.eq(#test.logs("error"),0)
    end
  end)
end
