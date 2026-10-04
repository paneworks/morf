local test=morf.test
local function load(style,page)
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{size={450,440},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",TEST_PAGE=page},source=[[
    local ui=require("morf.ui")
    morf.surface.height=440
    local page=morf.env("TEST_PAGE")
    local presentation=require("presentation")
    local shown=morf.signal("planner.fixture.shown",true)
    local content=require(page.."_page").build(430,function() return 420 end)
    ui.Item {x=10,y=10,width=430,height=420,visible=function() return shown:get() end,content}
    presentation.set("leftbar."..page,true)
    morf.ipc.show=function(on)
      shown:set(on=="yes") presentation.set("leftbar."..page,on=="yes")
    end
  ]]})
  test.advance(2400)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." planner descriptions and titles fit a compact pane",function()
    load(style,"calendar")
    test.eq(test.get("planner-subtitle").text,"Your plans, with space for what comes next.")
    local title=test.get(style=="material" and "planner-title" or "planner-title-text")
    test.eq(title.text,style=="material" and "A day at a time." or "A DAY AT A TIME.")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-planner-title.png") end
    test.wheel(0,3000,{x=400,y=300}) test.advance(200)
    if style=="tsugumori" then
      test.truthy(test.get("planner-work-title-text").text~="WORK CALENDAR","offscreen title missed its reveal")
      test.advance(2400)
      test.eq(test.get("planner-work-title-text").text,"WORK CALENDAR")
    end
    local work=test.get("planner-work-title")
    test.truthy(work.y>=10 and work.y+work.height<430,"final section is not reachable")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-planner-work.png") end
    test.ipc("show","no") test.advance(100)
    test.ipc("show","yes") test.advance(200)
    if style=="tsugumori" then
      test.eq(test.get("planner-work-title-text").text,"WORK CALENDAR")
      test.truthy(test.get("planner-title-text").text~="A DAY AT A TIME.","reopened calendar did not reset to the top")
    end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." task descriptions share the panel subtitle role",function()
    load(style,"tasks")
    test.eq(test.get("tasks-subtitle").text,"0 open tasks · Taskwarrior")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-tasks-title.png") end
    test.click("tasks-add") test.advance(2400)
    test.truthy(test.get("task-editor-title").visible)
    test.click("task-cancel") test.advance(2400)
    test.truthy(test.get("tasks-subtitle").visible)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
