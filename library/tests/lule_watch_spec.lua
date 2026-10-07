local test=morf.test
local root=morf.env("XDG_CACHE_HOME").."/lule-watch"
local function palette(image)
  return morf.json.encode {wallpaper=image,theme="dark",colors={"#111111","#ffaa66"}}
end
test.it("all Lule subscribers keep receiving wallpapers after another subscriber opens",function()
  morf.fs.write(root.."/colors.json",palette("first.png"))
  test.load("../lib/lule.lua",{env={LULE_A=root},source=[[
    local lule=require("lib.integrations.lule")
    local wallpaper=lule.watch("wallpaper")
    local studio=lule.watch("studio")
    morf.ipc.current=function() return {wallpaper:get().wallpaper,studio:get().wallpaper} end
    morf.ipc.churn=function()
      -- Collect an unreferenced watch before Lule next writes its palette.
      for i=1,5000 do local scratch=string.rep(tostring(i),100) end
    end
  ]]})
  test.eq(test.ipc("current"),{"first.png","first.png"})
  for i=1,4 do test.ipc("churn") test.advance(20) end
  for _,image in ipairs {"second.png","third.png"} do
    morf.fs.write(root.."/colors.json",palette(image))
    test.wait(function()
      local current=test.ipc("current")
      return current[1]==image and current[2]==image
    end,2000,"A later subscriber stopped the wallpaper's updates")
  end
  test.eq(test.logs("error"),{})
end)
