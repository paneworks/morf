local test=morf.test
local function env(style)
  return {CAELESTIA_DRY_RUN="1",CAELESTIA_STYLE=style,HYPRLAND_INSTANCE_SIGNATURE=false,
    CAELESTIA_APPEARANCE=morf.env("XDG_CACHE_HOME").."/handoff-appearance.json"}
end
local inspect=[[
  morf.ipc.inspect=function()
    local lule=require("lule_studio")
    return {theme=require("themes").current.id,opened=require("dashboard").drawer.open:get(),
      tab=require("dashboard").tab:get(),mode=lule.mode:get(),method=lule.method:get(),
      folder_draft=lule.folder_draft:get(),busy=require("themes.switcher").busy:get(),
      font=require("theme").font,chosen_font=require("themes").font,
      stored_font=require("themes").preferences.get("font"),font_file=require("theme").font_file,
      history=require("lib.sysinfo").snapshot_history().history.cpu,
      stored=require("themes").preferences.get("theme")}
  end
]]
for _,pair in ipairs {{"material","tsugumori"},{"tsugumori","material"},{"tsugumori","tsugumori","Goku"}} do
  test.it(pair[1].." hands off an open Lule page and graph history to "..(pair[3] or pair[2]),function()
    test.stub_run("task",{code=0,stdout="[]"})
    test.stub_run("fc-list",{code=0,stdout="Goku\nIBM Plex Mono\nRubik\nGoku\n"})
    test.load("../shell/init.lua",{size={1920,1080},env=env(pair[1]),source=[[
      require("services").here=function() return true end
      require("init")
      morf.reload=function() end
      morf.broadcast=function() return false end
      morf.ipc.prepare=function(target)
        local dashboard=require("dashboard")
        dashboard.tab:set(6) dashboard.drawer.set(true)
        local lule=require("lule_studio")
        lule.mode:set("light") lule.method:set("tonal")
        lule.folder_draft:set("/unfinished folder")
        require("lib.sysinfo").restore_history {history={cpu={11,22,33}}}
        return true
      end
      morf.ipc.handoff=function() return require("themes.session").snapshot() end
    ]]..inspect})
    test.advance(400)
    test.truthy(test.ipc("prepare",pair[2]))
    test.advance(1000)
    if pair[3] then
      test.click("lule-font") test.advance(80)
      test.click("lule-font-search") test.type("goku") test.advance(80)
      test.truthy(test.get("lule-font-option-1").visible)
      test.falsy(test.get("lule-font-option-2").visible)
      test.click("lule-font-option-1")
    else test.click("lule-theme-"..pair[2]) end
    test.advance(400)
    local handoff=test.ipc("handoff")
    test.truthy(handoff.switching)
    local seed=morf.json.encode(handoff)
    test.load("../shell/init.lua",{size={1920,1080},env=env(pair[1]),source=([[
      local native=morf.reloadable
      local bank=native("caelestia.theme.session",morf.json.decode(%q))
      morf.reloadable=function(name,initial)
        if name=="caelestia.theme.session" then return bank end
        return native(name,initial)
      end
      local completed
      morf.on_reload_completed=function(fn) completed=fn end
      require("services").here=function() return true end
      require("init")
      morf.timer(1,completed,false)
    ]]):format(seed)..inspect})
    test.advance(1000)
    local state=test.ipc("inspect")
    test.eq(state.theme,pair[2]) test.eq(state.stored,pair[2])
    if pair[3] then
      test.eq(state.font,"Goku") test.eq(state.chosen_font,"Goku")
      test.eq(state.stored_font,"Goku") test.eq(state.font_file,"")
      test.truthy(test.find{text="GOKU",visible=true})
    end
    test.truthy(state.opened) test.eq(state.tab,6) test.falsy(state.busy)
    test.eq(state.mode,"light") test.eq(state.method,"tonal")
    test.eq(state.folder_draft,"/unfinished folder")
    test.eq(test.get("lule-folder").text,"/unfinished folder")
    test.eq({state.history[1],state.history[2],state.history[3]},{11,22,33})
    test.truthy(test.get("lule-theme-"..pair[2]).visible)
    test.eq(#test.logs("error"),0)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("handoff-"..(pair[3] or pair[2])..".png") end
  end)
end
