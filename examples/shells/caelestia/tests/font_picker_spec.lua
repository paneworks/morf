local test=morf.test
local function load(style, fail)
  test.stub_run("fc-list",{code=fail and 1 or 0,stdout="Rubik\nGoku\nGoku\nIBM Plex Mono\nNoto Sans\nNoto Serif\nJetBrains Mono\nAdwaita Sans\n"})
  test.load("../shell/init.lua",{size={960,545},env={CAELESTIA_STYLE=style,
    CAELESTIA_APPEARANCE=morf.env("XDG_CACHE_HOME").."/font-picker-appearance.json"},source=[[
    local ui,kit,theme=require("morf.ui"),require("kit"),require("theme")
    morf.surface.height=545
    local appearance=require("themes.switcher")
    local chosen=nil
    appearance.request_font=function(family) chosen=family return true end
    local active=morf.signal("font-test.active",true)
    local fonts=require("themes.fonts")
    ui.Rect {width=960,height=545,color=function() return theme.color.surface end,
      kit.pill {id="open-fonts",x=582,y=484,width=362,height=34,label="Choose font",on_clicked=fonts.open},
      require("themes.layouts.font_picker") {active=active,busy=morf.signal("font-test.busy",false),appearance=appearance},
    }
    morf.ipc.state=function() return {chosen=chosen,opened=fonts.opened:get(),rows=fonts.rows:get(),page=fonts.page:get(),status=fonts.status:get()} end
    morf.ipc.hide=function() active:set(false) end
    morf.ipc.current=function(family) require("themes").font=family end
  ]]})
  test.advance(200)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." font picker searches, previews and chooses installed families",function()
    load(style)
    test.click("open-fonts") test.advance(100)
    test.eq(#test.ipc("state").rows,7)
    test.click("lule-font-next") test.advance(30)
    test.eq(test.ipc("state").page,2)
    test.click("lule-font-search") test.type("goku") test.advance(100)
    test.eq(test.ipc("state").page,1)
    test.eq(test.ipc("state").rows,{"Goku"})
    test.truthy(test.get("lule-font-preview-1").visible)
    test.falsy(test.get("lule-font-option-2").visible)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-font-picker.png") end
    test.click("lule-font-option-1") test.advance(100)
    test.eq(test.ipc("state").chosen,"Goku") test.falsy(test.ipc("state").opened)
    test.click("open-fonts") test.advance(100)
    test.click(100,100) test.advance(100)
    test.falsy(test.ipc("state").opened)
    test.ipc("current","Goku") test.click("open-fonts") test.advance(50)
    test.click("lule-font-default") test.advance(100)
    test.eq(test.ipc("state").chosen,"")
    test.click("open-fonts") test.advance(50) test.ipc("hide") test.advance(100)
    test.falsy(test.ipc("state").opened)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("font scan failure can be retried and an empty search stays usable",function()
  load("tsugumori",true)
  test.click("open-fonts") test.advance(100)
  test.truthy(test.ipc("state").status:find("Could not read",1,true))
  test.click(100,100)
  test.stub_run("fc-list",{code=0,stdout="Goku\n"})
  test.click("open-fonts") test.advance(100)
  test.eq(test.ipc("state").rows,{"Goku"})
  test.click("lule-font-search") test.type("missing family") test.advance(100)
  test.eq(#test.ipc("state").rows,0)
  test.truthy(test.find{text="No matching fonts",visible=true})
  test.click("lule-font-default") test.advance(100)
  test.falsy(test.ipc("state").opened)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
test.it("saved font survives a fresh load and default restores theme typography",function()
  local path=morf.env("XDG_CACHE_HOME").."/font-restart.json"
  for _,family in ipairs {"Goku",""} do
    test.load("../shell/init.lua",{env={CAELESTIA_STYLE="tsugumori",CAELESTIA_APPEARANCE=path},source=([[
      local settings=require("lib.settings").open {path=%q,defaults={theme="tsugumori",font=""}}
      settings.set("font",%q) settings.flush()
      require("morf.ui").Item {width=100,height=100}
    ]]):format(path,family)})
    test.advance(100)
    test.load("../shell/init.lua",{env={CAELESTIA_STYLE="tsugumori",CAELESTIA_APPEARANCE=path},source=[[
      local theme=require("theme")
      morf.ipc.font=function() return {family=theme.font,icons=theme.icon_font,selected=require("themes").font} end
      require("morf.ui").Item {width=100,height=100}
    ]]})
    local font=test.ipc("font")
    test.eq(font.selected,family)
    test.truthy(font.icons~=family)
    test.eq(font.family,family=="" and "IBM Plex Mono, JetBrains Mono, monospace" or family)
  end
end)
