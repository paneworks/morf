local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." VPN section titles stay reachable and reopen at the top",function()
    test.load("../shell/init.lua",{size={430,360},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"},source=[[
      local ui=require("morf.ui")
      morf.surface.height=360
      local links={}
      for i=1,8 do links[i]={uuid="preview-"..i,id="Campus "..i,type="wireguard",active=false} end
      local state=morf.state {available=true,vpn_connections=links}
      package.loaded.services={net={state=state}}
      package.loaded["lib.services.vpns"]={rows={tunnel=morf.signal("vpn.typography.rows",{
        {id="mullvad",detail="Disconnected",address="",up=false,can_toggle=false},
        {id="protonvpn",detail="Disconnected",address="",up=false,can_toggle=false},
      })},is_mesh_link=function() return false end,watch=function() end,release=function() end,
        set=function() error("Typography preview must not change VPN state") end}
      local shown=morf.signal("vpn.typography.shown",true)
      local presentation=require("presentation")
      ui.Item {x=10,y=10,width=408,height=340,visible=function() return shown:get() end,
        require("net_pages").vpn_page("tunnel",408,function() return 340 end)}
      presentation.set("settings.tunnel",true)
      morf.ipc.show=function(on)
        shown:set(on=="yes") presentation.set("settings.tunnel",on=="yes")
      end
    ]]})
    test.advance(2400)
    -- (Section captions are the style kit's static caption rows.)
    local function word(text) return style=="tsugumori" and text:upper() or text end
    test.truthy(test.find {text=word("NetworkManager"),visible=true})
    local viewport=test.get("vpn-tunnel-scroll")
    local top=test.get("vpn-tunnel-network-title").y
    test.wheel(0,test.get("vpn-tunnel-apps-title").y-viewport.y-60,
      {x=viewport.x+viewport.width-2,y=viewport.y+200}) test.advance(200)
    local title=test.get("vpn-tunnel-apps-title")
    test.truthy(title.y>=viewport.y and title.y+title.height<=viewport.y+viewport.height)
    test.truthy(test.find {text=word("Apps"),visible=true})
    test.ipc("show","no") test.advance(100)
    test.ipc("show","yes") test.advance(200)
    test.near(test.get("vpn-tunnel-network-title").y,top,1,"reopened register should begin at the top")
    test.advance(2200)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-vpn-section.png") end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
