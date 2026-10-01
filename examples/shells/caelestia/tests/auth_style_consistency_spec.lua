local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." lock and greet share clock sizing on short, portrait and desktop monitors",function()
    for _,size in ipairs {{800,480},{720,1280},{1920,1080}} do
      local fonts={}
      for _,part in ipairs {"lock","greet"} do
        test.load("../"..part.."/init.lua",{size=size,args=part=="lock" and {"window","preview"} or {"preview"},
          env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",PART=part},source=[[
            package.loaded["lib.accounts"]={me=function() return {name="fixture",label="Fixture",initial="F"} end,
              list=function() return {{name="fixture",label="Fixture",initial="F"}} end}
            package.loaded["lib.sessions"]={list=function() return {{name="Fixture",command={"false"}}} end,default_index=function() return 1 end}
            package.loaded["lib.keyboards"]={attached=function() return true end}
            package.loaded["lib.lule"]={watch=function() return {get=function() return nil end} end}
            package.loaded["lib.weather"]={new=function() return {get=function() return {} end} end,material_symbol=function() return "cloud" end}
            package.loaded["lib.mpris"]={connect=function() return {state={active={}}} end}
            require("init")
          ]]})
        test.advance(1000)
        local id=part.."-clock"..(style=="tsugumori" and "-1-digit" or "")
        fonts[part]=test.get(id).font_size
        if style=="tsugumori" then test.truthy(test.get(part.."-auth-backdrop")) end
        test.eq(#test.logs("error"),0)
      end
      test.eq(fonts.lock,fonts.greet,"clock styles diverged at "..size[1].."x"..size[2])
    end
  end)
end
