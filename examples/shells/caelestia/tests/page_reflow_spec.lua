-- Settings pages must use their available space as controls appear and
-- disappear, including while a narrow panel changes its heading.
local test=morf.test
local function load(style,source)
  test.load("../shell/init.lua",{size={520,400},env={CAELESTIA_STYLE=style},source=[[
    local ui=require("morf.ui")
    local P=require("themes.layouts.page")
    morf.surface.height=400
  ]]..source})
  test.advance(700)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." button rows redistribute space when actions are hidden",function()
    load(style,[[
      local shown=morf.signal("page.actions.shown",true)
      local calls=0
      ui.Item {x=24,y=24,width=321,height=60,
        P.buttons {width=321,
          {id="first",label="First"},
          {id="optional",label="Optional",visible=function() return shown:get() end},
          {id="last",label="Last",on_clicked=function() calls=calls+1 end}}}
      morf.ipc.show=function(v) shown:set(v=="yes") end
      morf.ipc.calls=function() return calls end
    ]])
    for _,shown in ipairs {"yes","no","yes"} do
      test.ipc("show",shown) test.advance(300)
      local first,last=test.get("first"),test.get("last")
      test.near(last.x+last.width,first.x+321,.01,"Hidden action left unused space")
      test.truthy(math.abs(first.width-last.width)<=1,"Actions have unequal widths")
      if shown=="no" then test.near(last.x-first.x-first.width,8,.01) end
      test.click("last")
    end
    test.eq(test.ipc("calls"),3)
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
  test.it(style.." frame reclaims empty subtitle space and keeps navigation reachable",function()
    load(style,[[
      local detail=morf.signal("page.detail",false)
      local backs=0
      ui.Item {x=24,y=24,width=320,height=320,
        P.frame {id="frame",width=320,height=320,title="Settings",titled=true,
          build=function(w,h)
            return ui.Rect {id="body",width=w,height=h,color="#334455"},
              {subtitle=function() return detail:get() and "Settings / Network" or "" end,
                can_back=function() return detail:get() end,
                back=function() backs=backs+1 detail:set(false) end}
          end}}
      morf.ipc.detail=function() detail:set(true) end
      morf.ipc.backs=function() return backs end
    ]])
    local compact=test.get("frame-head").height
    local body=test.get("body")
    local compact_y=body.y
    local frame=test.get("frame")
    local bottom=frame.y+frame.height
    test.falsy(test.get("frame-head-subtitle").visible,"Empty subtitle still reserves a line")
    test.near(body.y+body.height,bottom,.01)
    test.ipc("detail") test.advance(700)
    local header=test.get("frame-head")
    test.truthy(header.height>=compact+12,"Subtitle does not change the reserved height")
    test.truthy(test.get("body").y>=compact_y+12)
    test.truthy(test.get("frame-head-back").visible)
    test.truthy(header.height>=test.get("frame-head-back").height)
    test.near(test.get("body").y+test.get("body").height,bottom,.01)
    test.click("frame-head-back") test.advance(700)
    test.eq(test.ipc("backs"),1)
    test.near(test.get("body").y,compact_y,.01)
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
end
