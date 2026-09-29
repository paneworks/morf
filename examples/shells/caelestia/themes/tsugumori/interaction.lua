-- Shared feedback for every actionable target. Decoration moves; the input
-- rectangle stays fixed so pressing never makes a target slide away.
local ui = require("morf.ui")
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
