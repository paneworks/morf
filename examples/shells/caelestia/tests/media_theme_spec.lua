local test = morf.test
local function load(style)
  test.load("../shell/init.lua", {size={1040,400},env={CAELESTIA_STYLE=style},source=[[
    local ui = require("morf.ui")
    morf.surface.height = 400
    local shown = morf.signal("media.test.shown",true)
    local player = morf.signal("media.test.player",{name="test",identity="Test player",title="Glass signals",
      artist={"First artist","Second artist"},album="Night register",art_url="",playing=false,
      length=240,position=60,volume=0.5,shuffle=false,loop="none"})
    local calls, starts, stops = {}, 0, 0
    local function change(key,value)
      local next = {} for k,v in pairs(player:get()) do next[k]=v end
      next[key]=value player:set(next)
    end
    morf.audio.monitor = function()
      starts=starts+1
      return {stop=function() stops=stops+1 end}
    end
    local services = require("services")
    services.player=function() return player:get() end
    services.playing_something=function() return player:get().title~="" end
    local rows={{name="test",identity="Test player"},{name="second",identity="Second player"}}
    services.media={state={available=true,players={len=function() return #rows end,get=function(_,i) return rows[i] end}}}
    for _,action in ipairs {"play_pause","previous","next","set_shuffle","set_loop","set_volume","set_position","set_active"} do
      services.media[action]=function(value)
        calls[#calls+1]={action=action,value=value}
        if action=="play_pause" then change("playing",not player:get().playing)
        elseif action=="set_shuffle" then change("shuffle",value)
        elseif action=="set_loop" then change("loop",value)
        elseif action=="set_volume" then change("volume",value)
        elseif action=="set_position" then change("position",value) end
      end
    end
    package.loaded["lib.lyrics"]={follow=function() return {
      status=morf.signal("media.test.lyrics.status","synced"),
      lines=morf.signal("media.test.lyrics.lines",{{text="Signals in the dark"},{text="Find their way home"}}),
      index=morf.signal("media.test.lyrics.index",1),
    } end}
    local ctx={opened=function() return shown:get() end,current=function() return true end,area=require("kit").action}
    package.loaded["dashboard_state"]={context=function() return ctx end}
    local view=require("dashboard_media")
    ui.Item {width=1040,height=400,view.page}
    morf.ipc.shown=function(on) shown:set(on=="yes") end
    morf.ipc.state=function() return {calls=calls,starts=starts,stops=stops,player=player:get()} end
    morf.ipc.empty=function() change("title","") change("playing",false) end
    morf.ipc.track=function(value) change("title",value) end
  ]]})
  test.advance(2200)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." retains transport, seek, volume, lyrics and player selection",function()
    load(style)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-media-register.png") end
    test.eq(test.get(style=="tsugumori" and "media-tab-title-text" or "media-tab-title").text,
      style=="tsugumori" and "GLASS SIGNALS" or "Glass signals")
    test.eq(test.get("media-tab-artist").text,"First artist, Second artist")
    test.truthy(test.find {text="Signals in the dark",visible=true})
    test.click("media-tab-play") test.advance(20)
    test.eq(test.ipc("state").starts,1)
    test.click("media-tab-next")
    test.click("media-tab-previous")
    test.click("media-shuffle") test.click("media-repeat")
    local seek=test.get("media-seek")
    test.click(seek.x+seek.width*0.25,seek.y+seek.height/2)
    local volume=test.get("media-volume-slider")
    test.click(volume.x+volume.width-1,volume.y+volume.height/2)
    local state=test.ipc("state")
    test.eq(state.player.shuffle,true)
    test.eq(state.player.loop,"playlist")
    test.near(state.player.position,60,1)
    test.near(state.player.volume,1,0.01)
    test.click("media-player") test.advance(40)
    test.click("media-player-2")
    state=test.ipc("state")
    test.eq(state.calls[#state.calls].action,"set_active")
    test.eq(state.calls[#state.calls].value,"second")
    test.ipc("shown","no") test.advance(150)
    test.eq(test.ipc("state").stops,1)
    test.ipc("shown","yes") test.advance(20)
    test.eq(test.ipc("state").starts,2)
    test.ipc("empty") test.advance(100)
    test.truthy(test.get("media-nothing").visible)
    test.falsy(test.get("media-track").visible)
    test.eq(test.ipc("state").stops,2)
    test.eq(#test.logs("error"),0)
    if style=="tsugumori" then test.eq(#test.logs("warn"),0) end
  end)
end
test.it("Tsugumori progress rail retains seeking and compact player volume",function()
  load("tsugumori")
  local seek=test.get("media-seek")
  test.click(seek.x+seek.width*.75,seek.y+seek.height/2)
  test.advance(250)
  test.near(test.ipc("state").player.position,180,1)
  test.near(test.get("media-progress-fill").width,seek.width*.75,1)
  test.truthy(test.get("media-progress-handle").width<6)
  local volume=test.get("media-volume-slider")
  test.click(volume.x+volume.width-1,volume.y+volume.height/2)
  test.advance(250)
  test.near(test.ipc("state").player.volume,1,.01)
  test.eq(test.get("media-volume-slider-value").text,"100")
  local grip,label=test.get("media-volume-slider-handle"),test.get("media-volume-slider-value")
  test.truthy(grip.x+grip.width<label.x)
  test.eq(#test.logs("error"),0)
end)
test.it("media titles decode on entry, track change and empty-state entry",function()
  load("tsugumori")
  test.ipc("shown","no")
  test.ipc("track","Next transmission") test.advance(100)
  test.eq(test.get("media-tab-title-text").text,"NEXT TRANSMISSION")
  test.ipc("shown","yes") test.advance(900)
  test.truthy(test.get("media-tab-title-text").text~="NEXT TRANSMISSION")
  test.advance(1500)
  test.eq(test.get("media-tab-title-text").text,"NEXT TRANSMISSION")
  test.ipc("track","New signal") test.advance(900)
  test.truthy(test.get("media-tab-title-text").text~="NEW SIGNAL")
  test.ipc("empty") test.advance(900)
  test.truthy(test.get("media-empty-title-text").visible)
  test.truthy(test.get("media-empty-title-text").text~="NOTHING PLAYING")
  test.ipc("shown","no") test.advance(100)
  test.eq(test.get("media-empty-title-text").text,"NOTHING PLAYING")
  test.near(test.get("media-empty-title-ghost-a").opacity,0,0.001)
  test.eq(#test.logs("error"),0)
end)
