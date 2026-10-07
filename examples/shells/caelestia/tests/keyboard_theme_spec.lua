local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  morf.surface.keyboard_focus="none"
  package.loaded.bar={desk=function() return 10,10,W-20,H-20 end}
  local attached=false
  package.loaded["lib.services.keyboards"]={attached=function() return attached end}
  local events,ime_callback={},nil
  morf.input_method={subscribe=function(fn) ime_callback=fn end,
    commit=function(value) events[#events+1]={text=value} end}
  morf.virtual_keyboard={key=function(code,on) events[#events+1]={code=code,on=on} end,
    modifiers=function(mask) events[#events+1]={mask=mask} end}
  require("config").set("keyboard.auto",true)
  local keyboard=require("keyboard")
  local C=require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return C.surface end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainer end,keyboard.drawer.shape},keyboard.drawer.panel}}
  morf.ipc.show=keyboard.show
  morf.ipc.close=function() keyboard.drawer.set(false) end
  morf.ipc.manual=function() if keyboard.set then keyboard.set(true) else keyboard.drawer.set(true) end end
  morf.ipc.ime=function(on) ime_callback(on=="yes") end
  morf.ipc.attached=function(on) attached=on=="yes" end
  morf.ipc.auto=function(on) require("config").set("keyboard.auto",on=="yes") end
  morf.ipc.numbers=function(on) keyboard.keys.numbers:set(on=="yes") end
  morf.ipc.clear=function() events={} end
  morf.ipc.state=function() return {open=keyboard.drawer.open:get(),mode=keyboard.keys.mode:get(),
    shift=keyboard.keys.shift:get(),page=keyboard.keys.page:get(),events=events,focus=morf.surface.keyboard_focus} end
]]
local function load(style,w,h,dry)
  test.load("../shell/init.lua",{source=HOST,size={w or 1400,h or 800},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN=dry and "1" or "0",TEST_WIDTH=tostring(w or 1400),TEST_HEIGHT=tostring(h or 800)}})
end
local function key(name,mode,page) return "caelestia.osk.key."..(mode or "full").."."..(page or "letters").."."..name end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." dev keyboard offers two symbols per letter with digits as the hold default",function()
    load(style,360,800) test.ipc("show","dev") test.ipc("ime","yes") test.advance(2400)
    test.falsy(test.find {id=key("page:symbols","dev"),visible=true})
    local offered={}
    for char in ("qwertyuiopasdfghjklzxcvbnm"):gmatch(".") do
      local id=key(char,"dev")
      local k=test.get(id)
      local first=test.get(id..".hint.primary")
      local second=test.get(id..".hint.secondary")
      test.truthy(first.visible and second.visible)
      test.truthy(first.x>=k.x and second.x+second.width<=k.x+k.width,
        "symbol hints extend outside "..char)
      test.near(first.y,second.y,1)
      test.truthy(first.text~=second.text)
      offered[first.text],offered[second.text]=true,true
      test.ipc("clear") test.click(id)
      test.eq(test.ipc("state").events,{{text=char}})
      local x,y=k.x+k.width/2,k.y+k.height/2
      test.ipc("clear") test.press(x,y) test.advance(450)
      local strip=test.get("caelestia.osk.alternates")
      test.truthy(strip.visible)
      test.release(x,y)
      test.eq(test.ipc("state").events,{{text=first.text}},char.." hold must default to its first symbol")
      test.ipc("clear") test.press(x,y) test.advance(450)
      strip=test.get("caelestia.osk.alternates")
      local pick_x=strip.x+4+(strip.width-8)*.75
      test.move(pick_x,strip.y+strip.height/2)
      test.release(pick_x,strip.y+strip.height/2)
      test.eq(test.ipc("state").events,{{text=second.text}},char.." must offer its second symbol")
    end
    for i,char in ipairs {"q","w","e","r","t","y","u","i","o","p"} do
      test.eq(test.get(key(char,"dev")..".hint.primary").text,tostring(i%10))
    end
    for code=33,126 do
      local char=string.char(code)
      if not char:match("%a") then test.truthy(offered[char],"dev cannot type "..char) end
    end
    shot(style.."-keyboard-dev-pairs-360")
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." dev keyboard sends Super shortcuts and releases its modifier",function()
    for _,size in ipairs {{360,800},{744,1656},{1400,800}} do
      load(style,size[1],size[2]) test.ipc("show","dev") test.advance(2400)
      local super=key("mod:super","dev")
      test.truthy(test.get(super).visible)
      test.click(super) test.click(key("enter","dev"))
      test.eq(test.ipc("state").events,{{mask=64},{code=28,on=true},{code=28,on=false},{mask=0}})
      test.ipc("clear") test.click(key("q","dev"))
      test.eq(test.ipc("state").events,{{code=16,on=true},{code=16,on=false}})
      test.ipc("ime","yes") test.ipc("clear")
      test.click(super) test.click(key("mod:alt","dev")) test.click(key("space","dev"))
      test.eq(test.ipc("state").events,{{mask=72},{code=57,on=true},{code=57,on=false},{mask=0}},
        "input method swallowed the Super+Alt shortcut")
      test.ipc("clear") test.click(super) test.click(super)
      test.click(key("enter","dev")) test.click(key("enter","dev"))
      test.eq(test.ipc("state").events,{{mask=64},{code=28,on=true},{code=28,on=false},{mask=0},
        {mask=64},{code=28,on=true},{code=28,on=false},{mask=0}})
      test.click(super) test.ipc("clear") test.click(key("enter","dev"))
      test.eq(test.ipc("state").events,{{code=28,on=true},{code=28,on=false}})
      shot(style.."-keyboard-dev-super-"..size[1])
      test.eq(test.logs("error"),{})
    end
  end)
  test.it(style.." keyboard layouts retain key delivery, shift and symbols",function()
    load(style) test.ipc("show","full") test.advance(2400)
    shot(style.."-keyboard-full")
    test.click(key("q"))
    test.eq(test.ipc("state").events,{{code=16,on=true},{code=16,on=false}})
    test.click(key("shift")) test.eq(test.ipc("state").shift,"once")
    test.ipc("clear") test.click(key("q"))
    test.eq(test.ipc("state").events,{{mask=1},{code=16,on=true},{code=16,on=false},{mask=0}})
    test.eq(test.ipc("state").shift,"off")
    test.click(key("shift")) test.click(key("shift")) test.eq(test.ipc("state").shift,"lock")
    test.click(key("q")) test.eq(test.ipc("state").shift,"lock")
    test.click(key("page:symbols")) test.advance(300)
    test.eq(test.ipc("state").shift,"off") test.eq(test.ipc("state").page,"symbols")
    shot(style.."-keyboard-symbols")
    test.ipc("clear") test.click(key("1","full","symbols"))
    test.eq(test.ipc("state").events,{{code=2,on=true},{code=2,on=false}})
    for _,mode in ipairs {"dev","letters","numbers","phone","pattern"} do
      test.ipc("show",mode) test.advance(2400)
      shot(style.."-keyboard-"..mode)
      test.eq(test.ipc("state").mode,mode)
    end
    test.eq(test.ipc("state").focus,"none")
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." keyboard cancels held repeats and alternates on dismissal",function()
    load(style) test.ipc("show","full") test.advance(2400)
    local back=test.get(key("backspace"))
    test.press(back.x+back.width/2,back.y+back.height/2) test.advance(600)
    local count=#test.ipc("state").events
    test.truthy(count>2,"held backspace did not repeat")
    test.ipc("close") test.advance(700)
    test.eq(#test.ipc("state").events,count,"hidden keyboard kept repeating")
    test.release(back.x+back.width/2,back.y+back.height/2)
    test.ipc("show","full") test.advance(2400)
    local e=test.get(key("e"))
    test.press(e.x+e.width/2,e.y+e.height/2) test.advance(100)
    test.truthy(test.get("caelestia.osk.preview-bubble").visible)
    test.advance(350) test.truthy(test.get("caelestia.osk.alternates").visible)
    shot(style.."-keyboard-alternates")
    test.ipc("close") test.advance(700)
    test.release(e.x+e.width/2,e.y+e.height/2)
    test.eq(#test.ipc("state").events,count,"old long press committed after dismissal")
    test.ipc("show","dev") test.advance(2400)
    test.falsy(test.get("caelestia.osk.alternates").visible)
    test.ipc("clear") test.click(key("mod:ctrl","dev")) test.click(key("c","dev"))
    test.eq(test.ipc("state").events,{{mask=4},{code=46,on=true},{code=46,on=false},{mask=0}})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." keyboard auto-show honors hardware, manual ownership and dry preview",function()
    load(style)
    test.ipc("attached","yes") test.ipc("ime","yes") test.advance(100)
    test.falsy(test.ipc("state").open)
    test.ipc("attached","no") test.ipc("ime","yes") test.advance(2400)
    test.truthy(test.ipc("state").open)
    test.click(key("e")) test.eq(test.ipc("state").events,{{text="e"}})
    test.ipc("clear")
    local e=test.get(key("e"))
    test.press(e.x+e.width/2,e.y+e.height/2) test.advance(450)
    local strip=test.get("caelestia.osk.alternates")
    local x=strip.x+4+(strip.width-8)/7*2.5
    test.move(x,strip.y+strip.height/2)
    test.release(x,strip.y+strip.height/2)
    test.eq(test.ipc("state").events,{{text="é"}},"alternate did not reach the input method")
    test.ipc("ime","no") test.advance(600) test.falsy(test.ipc("state").open)
    test.ipc("ime","yes") test.advance(600)
    test.ipc("manual") test.ipc("ime","no") test.advance(600)
    test.truthy(test.ipc("state").open,"manual opening lost ownership to input method")
    test.ipc("close") test.ipc("auto","no") test.ipc("ime","yes") test.advance(600)
    test.falsy(test.ipc("state").open)
    load(style,nil,nil,true) test.ipc("show","full") test.advance(2400)
    test.click(key("q")) test.eq(test.ipc("state").events,{})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("Tsugumori keyboard fits compact outputs in every layout",function()
  load("tsugumori",500,720) test.ipc("show","full") test.advance(900)
  test.advance(1600)
  local panel=test.get("drawer-keyboard")
  test.truthy(panel.x>=0 and panel.x+panel.width<=500)
  test.truthy(panel.y>=0 and panel.y+panel.height<=720)
  test.ipc("numbers","yes") test.advance(600)
  shot("tsugumori-keyboard-compact")
  test.ipc("show","pattern") test.advance(2400)
  local pattern=test.get("caelestia.osk.pattern")
  test.truthy(pattern.y>=0 and pattern.y+pattern.height<=720)
  for _,mode in ipairs {"full","dev","letters","numbers","phone"} do
    test.ipc("show",mode) test.advance(2400)
    local drawer=test.get("drawer-keyboard")
    test.truthy(drawer.x>=0 and drawer.x+drawer.width<=500, mode.." exceeds output width")
    test.truthy(drawer.y>=0 and drawer.y+drawer.height<=720, mode.." exceeds output height")
  end
  shot("tsugumori-keyboard-compact-pattern")
  test.ipc("close") test.advance(600) test.falsy(test.ipc("state").open)
  test.ipc("show","numbers") test.advance(900)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
