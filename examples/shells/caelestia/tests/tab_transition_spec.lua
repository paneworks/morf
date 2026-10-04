local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local open=morf.signal("fixture.open",false)
  local tabs=require("tabbed").new {id="fixture",width=600,height=function() return 400 end,tabs={
    {key="one",name="One",build=function() return ui.Rect {width=578,height=200,color="#335544"} end},
    {key="two",name="Two",build=function() return ui.Rect {width=578,height=200,color="#554433"} end},
    {key="three",name="Three",build=function() return ui.Rect {width=578,height=200,color="#443355"} end},
  }}
  ui.Item {width=800,height=600,visible=function() return open:get() end,tabs.content}
  -- Exactly how side_panel and bottom forward the controller's visibility.
  morf.effect("fixture.shown",function() tabs.shown(open:get()) end)
  morf.ipc.open=function(on) open:set(on=="yes") end
  morf.ipc.select=tabs.select
  morf.ipc.state=function() return {selected=tabs.tab:get(),displayed=tabs.displayed:get()} end
]]
local function load()
  test.load("../shell/init.lua",{source=HOST,size={800,600},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1"}})
  test.advance(50)
end
local function switch(key,from,to)
  test.ipc("select",key) test.advance(60)
  test.truthy(test.get("fixture-page-curtain").visible,"switch to "..key.." skipped its cover")
  test.truthy(test.get("fixture-page-curtain").width>0)
  test.eq(test.ipc("state").displayed,from,"page changed before it was covered")
  test.advance(600)
  test.eq(test.ipc("state").displayed,to)
  test.falsy(test.get("fixture-page-curtain").visible)
end
test.it("first and subsequent tab switches animate after every opening",function()
  load()
  for _=1,3 do
    test.ipc("open","yes") test.advance(700)
    switch("two",1,2) switch("three",2,3) switch("one",3,1)
    test.ipc("open","no") test.advance(700)
  end
  test.eq(#test.logs("error"),0)
end)
test.it("closing cancels a tab wipe; reopening and rapid changes reveal the latest tab",function()
  load() test.ipc("open","yes") test.advance(700)
  test.ipc("select","two") test.advance(60)
  test.ipc("open","no") test.advance(50)
  test.ipc("select","three") test.advance(50)
  test.ipc("open","yes") test.advance(700)
  test.eq(test.ipc("state").displayed,3)
  switch("one",3,1)
  test.ipc("select","two") test.advance(60)
  test.ipc("select","three") test.advance(600)
  test.eq(test.ipc("state").displayed,3)
  test.falsy(test.get("fixture-page-curtain").visible)
end)
test.it("the real dashboard and all three tabbed drawers animate their first switch",function()
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{size={1920,1080},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1",
    HYPRLAND_INSTANCE_SIGNATURE=false},source=[[
    require("services").here=function() return true end
    require("init")
    morf.ipc.displayed=function(panel)
      if panel=="dashboard" then return require("dashboard_state").displayed:get() end
      return require(panel).panel.displayed:get()
    end
  ]]})
  test.advance(300)
  for _,case in ipairs {
    {"dashboard","dashboard","media"}, {"bottom","assistant","drop"},
    {"leftbar","tasks","calendar"}, {"sidebar","settings","notifications"},
  } do
    local panel,first,second=table.unpack(case)
    test.ipc(panel,"open") test.advance(600)
    for _,step in ipairs {{second,1,2},{first,2,1}} do
      test.click(panel.."-tab-"..step[1]) test.advance(60)
      test.truthy(test.get(panel.."-page-curtain").visible,panel.." skipped its tab wipe")
      test.truthy(test.get(panel.."-page-curtain").width>0)
      test.eq(test.ipc("displayed",panel),step[2])
      test.advance(340)
      test.eq(test.ipc("displayed",panel),step[3])
      test.falsy(test.get(panel.."-page-curtain").visible)
    end
    test.ipc(panel,"close") test.advance(600)
  end
  test.eq(#test.logs("error"),0)
end)
