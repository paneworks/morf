local test = morf.test
local HOST = [[
  local ui = require("morf.ui")
  local W,H = tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  package.loaded.bar={desk=function() return 0,0,W,H end}
  package.loaded.providers={PREFIXES={}}
  package.loaded.menus={source=morf.signal("test.menu",""),back=function() return false end,
    open=function() end,search=function() return {},"apps" end}
  package.loaded.wallpaper={current=morf.signal("test.wallpaper","not-current")}
  local activated={}
  package.loaded.apps={refresh=function() end,icon=function() return nil end,
    activate=function(row)
      if not row then return "keep" end
      activated[#activated+1]=row.name
      if row.run then return row.run() end
      return row.next_step or "close"
    end,
    search=function(q)
      if q=="none" then return {},"apps" end
      if q=="wall" then
        local out={}
        for i=1,90 do out[i]={kind="wallpaper",id=tostring(i),path="",name="Wallpaper "..i} end
        return out,"wallpapers"
      end
      if q=="calc" then return {{id="answer",kind="calc",question="2 + 2",name="4",description="Result",next_step="keep"}},"calculator" end
      if q=="nested " then return {{id="next",kind="action",name="Nested result",next_step="close"}},"apps" end
      return {
        {id="one",kind="app",name="First application",description="A deterministic search result",actions={
          {name="Inspect application",run=function() return "nested " end}}},
        {id="two",kind="action",name="Open submenu",description="Continue searching",next_step="nested "},
        {id="three",kind="app",name="Third application",description="Different section"},
      },"apps"
    end}
  local launcher=require("launcher")
  ui.Item {width=W,height=H,ui.Sdf {anchors={fill=true},fill_color=function() return require("theme").color.surface end,
    launcher.drawer.shape},launcher.drawer.panel}
  morf.ipc.open=function(on) launcher.drawer.set(on=="yes") end
  morf.ipc.query=launcher.set_query
  morf.ipc.move=function(delta) launcher.move(tonumber(delta)) end
  morf.ipc.state=function() return {selected=launcher.selected:get(),query=launcher.query:get(),acting=launcher.acting:get(),
    activated=activated,open=launcher.drawer.open:get(),count=launcher.count:get()} end
]]
local function load(style,w,h)
  w,h=w or 1280,h or 800
  test.load("../shell/init.lua",{source=HOST,size={w,h},env={CAELESTIA_STYLE=style,TEST_WIDTH=tostring(w),TEST_HEIGHT=tostring(h)}})
  test.ipc("open","yes") test.advance(2200)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." launcher skips section headings and opens nested actions",function()
    load(style)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-launcher-register.png") end
    test.eq(test.ipc("state").selected,2)
    test.key("Down") test.advance(300)
    test.eq(test.ipc("state").selected,4)
    test.key("Return") test.advance(500)
    test.eq(test.get("launcher-search").text,"nested ")
    test.eq(test.ipc("state").query,"nested ")
    test.key("Escape") test.advance(900)
    test.falsy(test.get("drawer-launcher").visible)
    test.ipc("open","yes") test.advance(900)
    test.eq(test.get("launcher-search").text,"")
    test.key("Tab") test.advance(400)
    test.truthy(test.ipc("state").acting~="")
    test.truthy(test.find {text="Inspect application",visible=true})
    test.key("Return") test.advance(400)
    test.eq(test.ipc("state").query,"nested ")
    test.key("Return") test.advance(900)
    test.falsy(test.ipc("state").open)
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." launcher handles empty results, answers, wallpaper navigation and reset",function()
    load(style)
    test.ipc("query","none") test.advance(500)
    test.truthy(test.get("launcher-empty").visible)
    test.key("Return") test.truthy(test.ipc("state").open)
    test.ipc("query","calc") test.advance(500)
    test.eq(test.get("launcher-answer").text,"4")
    test.key("Return") test.truthy(test.ipc("state").open)
    test.ipc("query","wall") test.advance(700)
    test.truthy(test.get("launcher-wallpapers").visible)
    test.key("Right") test.advance(400)
    test.eq(test.ipc("state").selected,2)
    test.key("Left") test.eq(test.ipc("state").selected,1)
    test.key("Return") test.advance(900)
    test.falsy(test.ipc("state").open)
    test.ipc("open","yes") test.advance(900)
    test.eq(test.ipc("state").query,"")
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori launcher fits a compact output and reaches the end of large wallpaper lists",function()
  load("tsugumori",800,600)
  local panel=test.get("drawer-launcher")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=600)
  test.key("Down") test.key("Down") test.advance(400)
  local row=test.get("launcher-row-app:three")
  test.truthy(row.y+row.height<=panel.y+panel.height)
  test.ipc("query","wall") test.advance(700)
  test.ipc("move","89") test.advance(400)
  test.truthy(test.find {text="Wallpaper 90",visible=true})
  panel=test.get("drawer-launcher")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-launcher-compact.png") end
  test.key("Return") test.advance(900)
  test.eq(test.ipc("state").activated[1],"Wallpaper 90")
  test.eq(#test.logs("warn"),0)
end)
test.it("Tsugumori launcher section titles decode as navigation reveals them",function()
  load("tsugumori",800,360)
  local title="launcher-section-header:Suggestions:4-text"
  test.eq(test.get(title).text,"SUGGESTIONS")
  test.key("Down") test.key("Down") test.advance(400)
  local viewport=test.get("launcher-results")
  local label=test.get(title)
  test.truthy(label.y>=viewport.y and label.y+label.height<=viewport.y+viewport.height)
  test.truthy(label.text~="SUGGESTIONS","section decoded before navigation revealed it")
  test.advance(1800)
  test.eq(test.get(title).text,"SUGGESTIONS")
  test.key("Up") test.key("Up") test.advance(400)
  test.eq(test.get(title).text,"SUGGESTIONS")
  test.key("Down") test.key("Down") test.advance(400)
  test.truthy(test.get(title).text~="SUGGESTIONS")
  test.advance(1800)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("launcher-scrolled-heading.png") end
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
