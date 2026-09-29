local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." VPN section titles reveal when scrolled into view",function()
    test.load("../shell/init.lua",{size={430,360},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"},source=[[
      local ui=require("morf.ui")
      morf.surface.height=360
      local links={}
      for i=1,8 do links[i]={uuid="preview-"..i,id="Campus "..i,type="wireguard",active=false} end
      local state=morf.state {available=true,vpn_connections=links}
      package.loaded.services={net={state=state}}
      package.loaded["lib.vpns"]={rows={tunnel=morf.signal("vpn.typography.rows",{
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
    local suffix=style=="tsugumori" and "-text" or ""
    test.eq(test.get("vpn-tunnel-network-title"..suffix).text,style=="tsugumori" and "NETWORKMANAGER" or "NetworkManager")
    local viewport=test.get("vpn-tunnel-scroll")
    test.wheel(0,test.get("vpn-tunnel-apps-title").y-viewport.y-60,
      {x=viewport.x+viewport.width-2,y=viewport.y+200}) test.advance(200)
    if style=="tsugumori" then
      test.truthy(test.get("vpn-tunnel-apps-title-text").text~="APPS","offscreen section missed its reveal")
      test.advance(2200)
      test.eq(test.get("vpn-tunnel-apps-title-text").text,"APPS")
    end
    local title=test.get("vpn-tunnel-apps-title")
    test.truthy(title.y>=viewport.y and title.y+title.height<=viewport.y+viewport.height)
    test.ipc("show","no") test.advance(100)
    test.ipc("show","yes") test.advance(200)
    if style=="tsugumori" then
      test.eq(test.get("vpn-tunnel-apps-title-text").text,"APPS")
      test.truthy(test.get("vpn-tunnel-network-title-text").text~="NETWORKMANAGER","reopened register should begin at the top")
    end
    test.advance(2200)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-vpn-section.png") end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
