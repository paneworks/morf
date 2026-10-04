local test=morf.test
local A="11111111-1111-1111-1111-111111111111"
local B="22222222-2222-2222-2222-222222222222"
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local today=morf.time.format("%Y-%m-%d")
  local stamp=today:gsub("-","")
  local A,B="11111111-1111-1111-1111-111111111111","22222222-2222-2222-2222-222222222222"
  local data={
    {uuid=A,description="Prepare the seminar",project="university",priority="H",tags={"work","talk"},status="pending",
      scheduled=stamp.."T090000Z",due=stamp.."T170000Z",depends={B}},
    {uuid=B,description="Read the paper",project="research",tags={"reading"},status="pending",start=stamp.."T080000Z"},
    {uuid="33333333-3333-3333-3333-333333333333",description="Overdue notes",project="",tags={},due="20200101T090000Z",status="pending"},
  }
  local tasks=morf.signal("planner.fixture.tasks",data)
  local reads,calls,pending=0,{},{}
  local deferred=false
  local client={tasks={get=function() reads=reads+1 return tasks:get() end},
    error=morf.signal("planner.fixture.error",""),busy=morf.signal("planner.fixture.busy",false),
    loaded=morf.signal("planner.fixture.loaded",true),revision=morf.signal("planner.fixture.revision",0)}
  local function update() tasks:set(data) client.revision:set(client.revision:get()+1) end
  local function respond(done)
    if deferred then client.busy:set(true) pending[#pending+1]=done
    elseif done then done(true) end
    return true
  end
  client.save=function(id,fields,done)
    if fields.description=="" then client.error:set("Give the task a description.") return false end
    calls[#calls+1]={kind="save",id=id or "",fields=fields}
    return respond(done)
  end
  client.action=function(id,action,done)
    calls[#calls+1]={kind=action,id=id} return respond(done)
  end
  client.refresh=function() calls[#calls+1]={kind="refresh"} return true end
  client.watch=function() end
  local dates=require("lib.integrations.taskwarrior")
  dates.new=function() return client end
  local presentation=require("presentation")
  local shown=morf.signal("planner.fixture.page","")
  local function page(key)
    shown:set(key)
    presentation.set("leftbar.tasks",key=="tasks") presentation.set("leftbar.calendar",key=="calendar")
  end
  package.loaded.leftbar={panel={select=page},drawer={set=function(on) if not on then page("") end end}}
  local model
  local ok,models=pcall(require,"tasks_model")
  if ok then local new=models.new models.new=function(...) model=new(...) return model end end
  local task_page,calendar=require("tasks_page"),require("calendar_page")
  ui.Item {width=W,height=H,
    ui.Item {width=W,height=H,visible=function() return shown:get()=="tasks" end,task_page.build(W,function() return H end)},
    ui.Item {width=W,height=H,visible=function() return shown:get()=="calendar" end,calendar.build(W,function() return H end)}}
  morf.ipc.page=page
  morf.ipc.edit=function(id)
    if id=="new" then task_page.edit() return end
    for _,row in ipairs(data) do if row.uuid==id then task_page.edit(row) return end end
  end
  morf.ipc.state=function() return {page=shown:get(),calls=calls,reads=reads,error=client.error:get(),
    editing=model and model.editing:get() or false,selected=model and model.selected:get() or "",
    day=require("planner").selected_day:get(),month=require("planner").month_offset:get(),today=today,
    scheduled=dates.date(data[1] and data[1].scheduled)} end
  morf.ipc.defer=function() deferred=true end
  morf.ipc.reply=function(index,ok)
    client.busy:set(false)
    if ok~="yes" then client.error:set("Database locked") end
    pending[tonumber(index)](ok=="yes")
  end
  morf.ipc.change=function(kind)
    if kind=="remove" then table.remove(data,1)
    elseif kind=="start" then data[1].start=stamp.."T100000Z"
    elseif kind=="many" then for i=1,24 do
      data[#data+1]={uuid=("%08d-aaaa-aaaa-aaaa-aaaaaaaaaaaa"):format(i),description="Task "..i,project="",tags={},scheduled=stamp.."T090000Z"}
    end end
    update()
  end
  morf.ipc.save=function() return model.save() end
]]
local function load(style,w,h,dry)
  w,h=w or 408,h or 1000
  test.load("../shell/init.lua",{source=HOST,size={w,h},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w),TEST_HEIGHT=tostring(h),CAELESTIA_DRY_RUN=dry and "1" or "0"}})
end
local function page(name) test.ipc("page",name) test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
local function scroll_to(id,viewport_id)
  local node,viewport=test.get(id),test.get(viewport_id)
  if node.y<viewport.y or node.y+node.height>viewport.y+viewport.height then
    test.wheel(0,node.y-viewport.y-40,{x=viewport.x+viewport.width-2,y=viewport.y+30}) test.advance(300)
  end
end
local function type_field(id,value)
  scroll_to(id,"task-editor-scroll") test.click(id) test.key("a","Ctrl") test.type(value)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Tasks filters and edits without rewriting untouched dates",function()
    load(style) page("tasks") shot(style.."-tasks")
    test.click("tasks-filter-active") test.advance(100)
    test.truthy(test.find{id="task-row-"..B}) test.falsy(test.find{id="task-row-"..A})
    test.click("tasks-filter-today") test.advance(100)
    test.truthy(test.find{id="task-row-"..A}) test.falsy(test.find{id="task-row-"..B})
    test.click("tasks-filter-open") test.click("tasks-search") test.type("research") test.advance(100)
    test.truthy(test.find{id="task-row-"..B}) test.falsy(test.find{id="task-row-"..A})
    test.click("tasks-search") test.key("a","Ctrl") test.key("BackSpace") test.advance(100)
    test.click("task-edit-"..A) test.advance(2500)
    test.eq(test.get("task-scheduled").text,test.ipc("state").scheduled)
    shot(style.."-task-editor")
    type_field("task-description","Prepare the lecture")
    type_field("task-project","faculty") test.type(".work")
    test.eq(test.get("task-project").text,"faculty.work","editing a second field stole focus")
    test.eq(test.get("task-description").text,"Prepare the lecture")
    test.click("task-save") test.advance(100)
    local saved=test.ipc("state").calls[1]
    test.eq(saved,{kind="save",id=A,fields={description="Prepare the lecture",project="faculty.work"}})
    test.falsy(test.ipc("state").editing)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Task actions use current state and require delete confirmation",function()
    load(style) page("tasks")
    test.click("task-done-"..B)
    test.eq(test.ipc("state").calls[1],{kind="done",id=B})
    test.click("task-edit-"..A) test.advance(500)
    test.ipc("change","start") test.advance(100)
    scroll_to("task-start","task-editor-scroll") test.click("task-start")
    test.eq(test.ipc("state").calls[2],{kind="stop",id=A})
    test.ipc("edit",A) test.advance(300)
    scroll_to("task-delete","task-editor-scroll") test.click("task-delete") test.advance(100)
    test.eq(#test.ipc("state").calls,2)
    test.click("task-delete") test.eq(test.ipc("state").calls[3],{kind="delete",id=A})
    test.ipc("edit",A) test.advance(300) test.ipc("change","remove") test.ipc("save")
    test.eq(#test.ipc("state").calls,3) test.truthy(test.ipc("state").editing)
    test.eq(test.ipc("state").error,"This task is no longer available.")
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." Calendar navigates months and hands the selected day to the task editor",function()
    load(style) page("calendar") shot(style.."-calendar")
    local today=test.ipc("state").today
    test.click("planner-next") test.advance(100) test.eq(test.ipc("state").month,1)
    test.click("planner-previous") test.advance(100) test.eq(test.ipc("state").month,0)
    test.click("planner-day-"..today) test.advance(100)
    scroll_to("planner-task-"..A,"planner-scroll") test.click("planner-task-"..A) test.advance(2500)
    test.eq(test.ipc("state").page,"tasks") test.eq(test.get("task-description").text,"Prepare the seminar")
    test.click("task-cancel") page("calendar")
    scroll_to("planner-add","planner-scroll") test.click("planner-add") test.advance(2500)
    test.eq(test.ipc("state").page,"tasks") test.eq(test.get("task-scheduled").text,today.."T09:00")
    test.eq(test.get("task-description").text,"")
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("Tasks preserve newer drafts after delayed saves and suppress dry-run writes",function()
  load("tsugumori") test.advance(100) test.eq(test.ipc("state").reads,0)
  page("tasks") test.click("task-edit-"..A) test.advance(300)
  test.ipc("defer") test.click("task-save")
  test.ipc("edit",B) test.advance(100) test.ipc("reply","1","yes") test.advance(100)
  test.truthy(test.ipc("state").editing) test.eq(test.get("task-description").text,"Read the paper")
  page("") local reads=test.ipc("state").reads
  test.ipc("change","start") test.advance(100) test.eq(test.ipc("state").reads,reads)
  load("tsugumori",nil,nil,true) page("tasks") test.click("task-edit-"..A) test.advance(100)
  test.click("task-save") test.eq(test.ipc("state").calls,{})
  test.truthy(test.ipc("state").editing)
  test.eq(#test.logs("error"),0)
end)
test.it("Compact Tasks reaches every editor field and keeps save available",function()
  load("tsugumori",360,420) page("tasks") test.click("tasks-add") test.advance(2500)
  type_field("task-description","A compact task")
  type_field("task-depends",B)
  local save=test.get("task-save") test.truthy(save.y>=0 and save.y+save.height<=420)
  shot("tasks-compact-editor-end")
  test.click("task-save")
  test.eq(test.ipc("state").calls[1].fields.depends,B)
  test.ipc("change","many") test.advance(100)
  local last="task-edit-00000024-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
  scroll_to(last,"tasks-list") test.click(last) test.advance(500)
  test.eq(test.get("task-description").text,"Task 24")
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
test.it("Compact Calendar reaches the final agenda and work calendar sections",function()
  load("tsugumori",360,420) test.ipc("change","many") page("calendar")
  scroll_to("planner-work-title","planner-scroll") test.advance(2200)
  local work=test.get("planner-work-title") test.truthy(work.y>=0 and work.y+work.height<=420)
  shot("calendar-compact-work")
  local last="planner-task-00000024-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
  scroll_to(last,"planner-scroll") test.click(last) test.advance(2500)
  test.eq(test.get("task-description").text,"Task 24")
  page("calendar") test.near(test.get("planner-title").y,16,1)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
