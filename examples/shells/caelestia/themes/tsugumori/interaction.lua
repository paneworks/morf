-- Shared feedback for every actionable target. Decoration moves; the input
-- rectangle stays fixed so pressing never makes a target slide away.
local ui = require("morf.ui")
local morf = require("morf")
return function(theme)
  local C = theme.color
  local attached = setmetatable({}, {__mode="k"})
  local serial=0
  return function(area, name)
    if attached[area] then return area end
    attached[area]=true
    serial=serial+1
    local id=name or "tsugumori-action-"..serial
    ui.reparent(ui.Rect { id=id.."-wash",anchors={fill=true},z=40,
      color=function() return C.primary end,
      opacity=function() return area.pressed and 0.18 or area.hovered and 0.055 or 0 end,
      behavior={opacity={duration=140,easing="out_cubic"}} },area)
    -- A single clipped glint crosses the control on entry or press. It has no
    -- idle animation and never changes the target's input geometry.
    local glint = ui.Path { id=id.."-glint",x=-36,width=28,
      height=function() return math.max(1,area.height) end,
      view_box={0,0,28,100},d="M20 0 H28 L8 100 H0 Z",
      fill_color=function() return C.primary end,opacity=0 }
    ui.reparent(ui.Item { id=id.."-glint-clip",anchors={fill=true},clip=true,z=41,glint },area)
    local hovered,pressed,running=false,false,nil
    morf.effect(id..".glint",function()
      local over,down=area.hovered,area.pressed
      local fire=(over and not hovered) or (down and not pressed)
      hovered,pressed=over,down
      if not over and not down then
        if running then running:stop() running=nil end
        glint.opacity=0
      elseif fire then
        if running then running:stop() end
        running=morf.animation.play {{parallel={
          {node=glint,property="x",from=-36,to=area.width+36,duration=330,easing="out_cubic"},
          {node=glint,property="opacity",duration=330,keyframes={
            {at=0,value=0},{at=.13,value=.33},{at=.65,value=.27},{at=1,value=0},
          }},
        }}}
      end
    end,{owner=area})
    for i,corner in ipairs {{left=true,top=true},{right=true,top=true},{left=true,bottom=true},{right=true,bottom=true}} do
      local sx,sy=corner.left and -1 or 1,corner.top and -1 or 1
      -- A filled L is one scene node, instead of a parent and two bars.
      -- Hundreds of hidden controls share this decoration.
      ui.reparent(ui.Path {anchors=corner,width=6,height=6,z=42,
        view_box={0,0,6,6},d="M0 0 H6 V1 H1 V6 H0 Z",
        rotation=({0,90,270,180})[i],fill_color=function() return C.primary end,
        translate_x=function() return area.pressed and 0 or area.hovered and sx*3 or 0 end,
        translate_y=function() return area.pressed and 0 or area.hovered and sy*3 or 0 end,
        opacity=function() return area.hovered and 1 or 0 end,
        behavior={translate_x={duration=340,easing="out_cubic"},translate_y={duration=340,easing="out_cubic"},opacity={duration=180}},
      },area)
    end
    return area
  end
end
