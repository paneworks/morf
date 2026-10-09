local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local shown=morf.signal("mobile.fixture.shown",false)
  local mobile=morf.state {available=true,data=true,locked=false,sim_present=true,
    registered=true,connected=true,roaming=false,operator="Vodafone NL",technology="LTE",signal=72}
  local ns=morf.state {available=true,known_connections={},active_connections={}}
  local profiles={{path="/profile/1",uuid="one",id="Mobile data",type="gsm",auto_apn=true},
    {path="/profile/2",uuid="two",id="Work SIM",type="gsm",apn="internet.example"},
    {path="/wifi/1",uuid="wifi",id="Home",type="802-11-wireless"}}
  ns.known_connections:replace(profiles,"path")
  ns.active_connections:replace({{path="/active/1",connection="/profile/1",type="gsm",state="activated"}},"path")
  local calls,pending={},{}
  local delay=false
  local function reply(name,value,done)
    calls[#calls+1]={name=name,value=value}
    if delay then pending[#pending+1]=done else done(true) end
    return true
  end
  package.loaded.services={modem={state=mobile},net={state=ns,
    set_wwan=function(on,done) mobile.data=on return reply("data",on,done) end,
    activate=function(path,done) return reply("connect",path,done) end,
    deactivate=function(path,done) return reply("disconnect",path,done) end,
    connect_mobile=function(done) return reply("setup",true,done) end}}
  local models=require("mobile_model")
  local new=models.new
  local model
  models.new=function(...) model=new(...) return model end
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  ui.Item {width=W,height=H,visible=function() return shown:get() end,
    require("connectivity").mobile_page(W,function() return H end)}
  morf.ipc.show=function(on)
    shown:set(on) require("presentation").set("settings.mobile",on)
  end
  morf.ipc.change=function(key,value)
    if key=="empty" then ns.known_connections:replace({},"path") ns.active_connections:replace({},"path")
    elseif key=="manager" then ns.available=value
    elseif key=="delay" then delay=value
    else mobile[key]=value end
  end
  morf.ipc.reply=function() pending[#pending](nil,"Permission denied") end
  morf.ipc.choose=function(i) return model.choose(profiles[tonumber(i)]) end
  morf.ipc.state=function()
    return {calls=calls,rows=model.list(),message=model.message:get(),failed=model.failed:get(),
      pending=model.pending:get(),status=model.status()}
  end
]]
local function load(style,dry,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 408,h or 900},env={
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN=dry and "1" or "0",
    TEST_WIDTH=tostring(w or 408),TEST_HEIGHT=tostring(h or 900)}})
  test.ipc("show",true) test.advance(300)
end
local function state() return test.ipc("state") end
local function clean() test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0) end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." mobile page shows live carrier, APN and saved connection actions",function()
    load(style)
    test.truthy(test.find{text="Vodafone NL",visible=true})
    test.truthy(test.find{text="LTE · 72% signal",visible=true})
    test.truthy(test.find{text="Connected · Automatic APN",visible=true})
    test.eq(#state().rows,2)
    test.click("mobile-connect-1") test.advance(50)
    test.eq(state().calls[1].name,"disconnect") test.eq(state().calls[1].value,"/active/1")
    test.click("mobile-connect-2") test.advance(50)
    test.eq(state().calls[2].name,"connect") test.eq(state().calls[2].value,"/profile/2")
    test.click("mobile-data") test.advance(50)
    test.eq(state().calls[3].value,false) test.eq(state().status,"Mobile data is off")
    test.ipc("change","data",true) test.ipc("change","roaming",true) test.advance(50)
    test.eq(state().status,"Connected · Roaming")
    test.ipc("change","locked",true) test.advance(50) test.eq(state().status,"SIM locked")
    test.ipc("change","locked",false) test.ipc("change","sim_present",false) test.advance(50)
    test.eq(state().status,"No SIM detected")
    test.ipc("change","available",false) test.advance(50)
    test.truthy(test.find{text="No modem",visible=true}) test.eq(#state().rows,0)
    test.ipc("change","available",true) test.advance(50)
    test.truthy(test.find{text="Vodafone NL",visible=true})
    clean()
  end)
  test.it(style.." mobile setup, failed actions and dry-run behavior",function()
    load(style)
    test.ipc("change","empty",true) test.advance(50)
    test.click("mobile-setup") test.advance(50) test.eq(state().calls[1].name,"setup")
    test.falsy(test.ipc("choose",1)) test.eq(#state().calls,1)
    test.eq(state().message,"Connection no longer exists")
    test.ipc("change","delay",true)
    test.click("mobile-setup") test.advance(50) test.truthy(state().pending)
    test.ipc("reply") test.advance(50)
    test.truthy(state().failed) test.eq(state().message,"Permission denied")
    test.click("mobile-setup") test.advance(50)
    test.ipc("show",false) test.advance(50) test.ipc("reply") test.advance(50)
    test.eq(state().message,"") test.falsy(state().failed)
    clean()
    load(style,true)
    test.click("mobile-data") test.click("mobile-connect-1") test.advance(50)
    test.eq(#state().calls,0) clean()
  end)
end
test.it("mobile page fits a narrow phone and scrolls to connections",function()
  load("material",false,340,440)
  test.truthy(test.find{text="Vodafone NL",visible=true})
  local scroll=test.get("mobile-scroll")
  test.wheel(0,3000,{x=scroll.x+scroll.width-2,y=220}) test.advance(100)
  test.click("mobile-connect-2") test.advance(50)
  test.eq(state().calls[1].value,"/profile/2")
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("mobile-compact.png") end
  clean()
end)
