local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  package.loaded.services={}
  package.loaded.notifs={dnd=morf.signal("test.eq.dnd",false)}
  local calculate=morf.audio.equalizer_curve
  morf.audio={equalizer_curve=calculate,available=function() return false end}
  local model=require("utilities")
  local content=model.page_content
  model.page_content=function(key,w,h)
    if key=="sound" or key:match("^sound/") then return content(key,w,h) end
    return ui.Item {width=w,height=h}
  end
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  ui.Item {width=W,height=H,require("themes.layouts.page").frame {id="page-settings",width=W,
    height=function() return H end,title="Settings",title_id="settings-title",build=model.page}}
  require("presentation").set("sidebar.settings",true)
  local eq=require("equalizer_model")
  morf.ipc.select=model.request
  morf.ipc.set=function(key,json) return eq.set(key,morf.json.decode(json)) end
  morf.ipc.edit=function(ear,i,value) eq.edit(ear,tonumber(i),value) end
  morf.ipc.saved=function() return morf.json.decode(morf.fs.read(morf.state_path("caelestia-equalizer.json"))) end
  morf.ipc.state=function() return {page=model.detail:get(),mode=eq.preset(),enabled=eq.get("enabled"),
    strength=eq.current("strength"),bands=eq.current("bands"),curve=eq.curve:get(),
    left=eq.get("profile.left"),draft=eq.draft:get(),error=eq.editor_error:get()} end
]]
local serial=0
local function load(style,w,h)
  serial=serial+1
  morf.fs.remove(morf.state_path("caelestia-equalizer.json"))
  test.load("../shell/init.lua",{source=HOST,size={w or 408,h or 800},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",CAELESTIA_SETTINGS=morf.state_path("eq-theme-"..serial..".json"),
    TEST_WIDTH=tostring(w or 408),TEST_HEIGHT=tostring(h or 800)}})
  test.advance(250)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." EQ cards contain their controls and remain reachable in a short pane",function()
    load(style,340,400) test.ipc("select","sound/equalizer") test.advance(400)
    local card,reset,headroom=test.get("equalizer-bands"),test.get("equalizer-reset"),test.get("equalizer-headroom")
    test.truthy(reset.y+reset.height <= card.y+card.height-8)
    test.truthy(headroom.y >= reset.y+reset.height+12)
    local trim,scroll=test.get("equalizer-trim"),test.get("equalizer-scroll")
    test.wheel(0,trim.y-160,{x=scroll.x+scroll.width-2,y=scroll.y+80}) test.advance(200)
    trim=test.get("equalizer-trim") test.truthy(trim.y>=scroll.y)
    test.truthy(trim.y+trim.height<=scroll.y+scroll.height)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Sound opens nested EQ and audiogram; Back returns one level",function()
    load(style) test.ipc("select","sound") test.advance(350)
    test.click("sound-equalizer") test.advance(400)
    test.eq(test.ipc("state").page,"sound/equalizer")
    test.click("equalizer-mode-headphones") test.advance(100)
    test.eq(test.ipc("state").mode,"headphones")
    test.ipc("set","headphones.strength","72") test.ipc("set","speakers.strength","18")
    test.click("equalizer-mode-speakers") test.advance(100) test.eq(test.ipc("state").strength,18)
    test.click("equalizer-mode-headphones") test.advance(100) test.eq(test.ipc("state").strength,72)
    -- Scroll to the nested editor button in short panes.
    local button=test.get("equalizer-audiogram")
    local scroll=test.get("equalizer-scroll")
    test.wheel(0,button.y-160,{x=scroll.x+scroll.width-2,y=scroll.y+80}) test.advance(200)
    test.click("equalizer-audiogram") test.advance(400)
    test.eq(test.ipc("state").page,"sound/equalizer/audiogram")
    test.ipc("edit","left","1","55")
    test.click("settings-back") test.advance(400)
    test.eq(test.ipc("state").page,"sound/equalizer")
    test.click("settings-back") test.advance(400) test.eq(test.ipc("state").page,"sound")
    test.click("settings-back") test.advance(400) test.eq(test.ipc("state").page,"")
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." audiogram validates input, saves, and discards unfinished edits",function()
    load(style,340,520) test.ipc("select","sound/equalizer/audiogram") test.advance(400)
    test.click("audiogram-left-1") test.key("a","ctrl") test.type("55") test.advance(100)
    test.eq(test.ipc("state").draft.left[1],"55")
    test.ipc("edit","right","2","bad")
    local save=test.get("audiogram-save") local scroll=test.get("audiogram-scroll")
    test.wheel(0,save.y-160,{x=scroll.x+scroll.width-2,y=scroll.y+80}) test.advance(200)
    test.click("audiogram-save") test.advance(100)
    test.truthy(test.ipc("state").error~="") test.eq(test.ipc("state").page,"sound/equalizer/audiogram")
    test.ipc("edit","right","2","20") test.click("audiogram-save") test.advance(400)
    test.eq(test.ipc("state").page,"sound/equalizer") test.eq(test.ipc("state").left[1],55)
    test.eq(test.ipc("saved").profile.left[1],55)
    test.ipc("select","sound/equalizer/audiogram") test.advance(400)
    test.ipc("edit","left","1","90") test.click("settings-back") test.advance(300)
    test.ipc("select","sound/equalizer/audiogram") test.advance(400)
    test.eq(test.ipc("state").draft.left[1],55)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
