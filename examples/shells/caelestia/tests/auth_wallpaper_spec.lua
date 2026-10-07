local test=morf.test
local root=morf.env("XDG_CACHE_HOME").."/auth-wallpaper"
local image=root.."/shared.svg"
morf.fs.write(image,'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#ff8844" d="M0 0h16v16H0z"/></svg>')
local palette={wallpaper=image,theme="dark",colors={"#111111","#ff8844"}}
morf.fs.write(root.."/shared/colors.json",morf.json.encode(palette))
morf.fs.write(root.."/user/colors.json",morf.json.encode(palette))
morf.fs.write(root.."/config.json",morf.json.encode {directory=root.."/shared",user="akari",command="morf-wallpaper-test"})
for _,role in ipairs {"greet","lock"} do
  for _,style in ipairs {"material","tsugumori"} do
    test.it(style.." "..role.." displays the shared blurred image",function()
      test.load("../"..role.."/init.lua",{size={406,903},source=[[
        local ui=require("morf.ui") local make=ui.Image local picture
        local keyboard=require("themes.auth_keyboard") local board=keyboard.new local context
        keyboard.new=function(ctx) context=ctx return board(ctx) end
        ui.Image=function(props)
          local node=make(props)
          if props.id=="lock-wallpaper" or props.id=="greet-wallpaper" then picture=node end
          return node
        end
        require("init")
        ui.Image=make
        morf.ipc.wallpaper_test=function() return picture.source end
        morf.ipc.password_test=function() context.method:set("password") end
      ]],
        args=role=="lock" and {"window","preview"} or {"preview"},
        env={CAELESTIA_DRY_RUN="1",CAELESTIA_STYLE=style,CAELESTIA_SCALE_MODE="compositor",
          MORF_WALLPAPER_CONFIG=root.."/config.json",LULE_A=root..(role=="greet" and "/unreadable-user-cache" or "/user"),GREETD_SOCK=false}})
      test.advance(700)
      local background=test.get(role.."-wallpaper")
      test.eq(test.ipc("wallpaper_test"),image)
      test.truthy(background.visible)
      test.ipc("stage","sheet") test.ipc("password_test") test.advance(700)
      test.truthy(test.get(role.."-wallpaper").height<903,"backdrop no longer follows resized content")
      test.eq(#test.logs("error"),0)
    end)
  end
end
test.it("wallpaper handoff runs only for the configured user and outside previews",function()
  local function load(dry)
    test.stub_run("morf-wallpaper-test",{code=0})
    test.load("../shell/init.lua",{source=[[
      require("morf.ui").Item {width=10,height=10}
      local handoff=require("themes.wallpaper_handoff")
      morf.ipc.publish=function() handoff.publish("/an image with spaces.png","/cache/colors.json") end
    ]],env={USER="akari",CAELESTIA_DRY_RUN=dry and "1" or "0",MORF_WALLPAPER_CONFIG=root.."/config.json"}})
  end
  load(false) test.ipc("publish") test.advance(40)
  test.eq(test.runs()[1],{"morf-wallpaper-test","publish","--image=/an image with spaces.png","--palette=/cache/colors.json"})
  load(true) test.ipc("publish") test.advance(40) test.eq(#test.runs(),0)
end)
