local test = morf.test
local function load()
  test.load("../shell/init.lua", {size={600,240},env={CAELESTIA_STYLE="tsugumori"},source=[[
    local ui = require("morf.ui")
    local kit = require("kit")
    morf.surface.height = 240
    local shown = morf.signal("heading.shown", false)
    local title = morf.signal("heading.title", "Signal register")
    ui.Item {width=600,height=240,
      kit.heading {id="heading",x=20,y=20,width=400,text=function() return title:get() end,
        active=function() return shown:get() end,reveal_delay=80},
    }
    morf.ipc.show=function(on) shown:set(on=="yes") end
    morf.ipc.title=function(value) title:set(value) end
  ]]})
end
test.it("headings decode on appearance and settle without moving the label",function()
  load()
  test.eq(test.get("heading-text").text,"SIGNAL REGISTER")
  local x=test.get("heading-text").x
  test.ipc("show","yes") test.advance(80)
  test.truthy(test.get("heading-text").text~="SIGNAL REGISTER")
  test.truthy(test.get("heading-ghost-a").opacity>0)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("mara-heading-decoding.png") end
  test.near(test.get("heading-text").x,x,0.01)
  test.advance(700)
  test.eq(test.get("heading-text").text,"SIGNAL REGISTER")
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("mara-heading-settled.png") end
  test.near(test.get("heading-ghost-a").opacity,0,0.001)
  test.near(test.get("heading-light").opacity,0.3,0.001)
  test.truthy(test.settle(500)<100)
  test.eq(#test.logs("warn"),0)
end)

test.it("scrolling titles decode only when they enter the viewport and stop when they leave",function()
  test.load("../shell/init.lua", {size={600,240},env={CAELESTIA_STYLE="tsugumori"},source=[[
    local ui,kit=require("morf.ui"),require("kit")
    morf.surface.height=240
    local viewport
    local shown=morf.signal("scroll-heading.shown",true)
    local title=kit.heading {id="scroll-heading",y=300,width=380,text="Upcoming plans",
      active=function() return shown:get() end,viewport=function() return viewport end}
    viewport=ui.Flickable {id="heading-scroll",x=20,y=20,width=450,height=180,clip=true,
      ui.Item {width=450,height=550,title}}
    ui.Item {width=600,height=240,viewport}
    morf.ipc.scroll=function(y) viewport.content_y=tonumber(y) end
    morf.ipc.show=function(on) shown:set(on=="yes") end
  ]]})
  test.advance(2400)
  test.eq(test.get("scroll-heading-text").text,"UPCOMING PLANS")
  test.ipc("scroll","220") test.advance(200)
  test.truthy(test.get("scroll-heading-text").text~="UPCOMING PLANS")
  test.ipc("scroll","0") test.advance(100)
  test.eq(test.get("scroll-heading-text").text,"UPCOMING PLANS")
  test.near(test.get("scroll-heading-ghost-a").opacity,0,.001)
  test.ipc("scroll","220") test.advance(200)
  test.truthy(test.get("scroll-heading-text").text~="UPCOMING PLANS")
  test.advance(2200)
  test.eq(test.get("scroll-heading-text").text,"UPCOMING PLANS")
  test.ipc("show","no") test.ipc("scroll","0") test.ipc("scroll","220") test.advance(200)
  test.eq(test.get("scroll-heading-text").text,"UPCOMING PLANS")
  test.ipc("show","yes") test.advance(200)
  test.truthy(test.get("scroll-heading-text").text~="UPCOMING PLANS")
  test.advance(2200)
  test.truthy(test.settle(500)<100)
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
test.it("hiding cancels a decode and reopening or changing text starts fresh",function()
  load()
  test.ipc("show","yes") test.advance(160)
  test.ipc("show","no") test.advance(16)
  test.eq(test.get("heading-text").text,"SIGNAL REGISTER")
  test.near(test.get("heading-ghost-b").opacity,0,0.001)
  test.ipc("title","München / Δ") test.advance(100)
  test.eq(test.get("heading-text").text,"MüNCHEN / Δ")
  test.ipc("show","yes") test.advance(200)
  test.truthy(test.get("heading-text").text~="MüNCHEN / Δ")
  test.ipc("title","Connected") test.advance(1800)
  test.eq(test.get("heading-text").text,"CONNECTED")
  test.eq(#test.logs("error"),0)
end)

test.it("nested viewports defer titles until every surrounding pane reveals them",function()
  test.load("../shell/init.lua",{size={600,240},env={CAELESTIA_STYLE="tsugumori"},source=[[
    local ui,kit=require("morf.ui"),require("kit")
    morf.surface.height=240
    local outer,inner
    local title=kit.with_viewport(function() return outer end,function()
      return kit.heading {id="nested-title",y=20,width=300,text="Nested register",
        viewport=function() return inner end}
    end)
    inner=ui.Flickable {x=0,y=300,width=400,height=120,clip=true,
      ui.Item {width=400,height=400,title}}
    outer=ui.Flickable {x=20,y=20,width=450,height=180,clip=true,
      ui.Item {width=450,height=600,inner}}
    -- A later sibling must not inherit the temporary viewport context.
    local sibling=kit.heading {id="sibling-title",x=480,y=20,width=100,text="Ready"}
    ui.Item {width=600,height=240,outer,sibling}
    morf.ipc.scroll=function(y) outer.content_y=tonumber(y) end
  ]]})
  test.advance(200)
  test.eq(test.get("nested-title-text").text,"NESTED REGISTER")
  test.truthy(test.get("sibling-title-text").text~="READY")
  test.advance(2200)
  test.ipc("scroll","280") test.advance(200)
  test.truthy(test.get("nested-title-text").text~="NESTED REGISTER")
  test.ipc("scroll","0") test.advance(80)
  test.eq(test.get("nested-title-text").text,"NESTED REGISTER")
  test.ipc("scroll","280") test.advance(2400)
  test.eq(test.get("nested-title-text").text,"NESTED REGISTER")
  test.truthy(test.settle(500)<100)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
