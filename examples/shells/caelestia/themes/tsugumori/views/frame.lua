-- Straight-edged shell frame with corner registration marks; no M3 seams.
local ui=require("morf.ui")
local theme=require("theme")
local C=theme.color
local V={}
function V.insets(bar)
  bar=bar or {left=0,top=0,right=0,bottom=0}
  return {left=theme.LEFT+bar.left,top=theme.BORDER+bar.top,
    right=theme.BORDER+bar.right,bottom=theme.BORDER+bar.bottom}
end
function V.build(model)
  local opening=ui.SdfShape {id="frame-opening",shape="box",operation="subtract",radius=0,
      x=function() local x=model.desk() return x+theme.LEFT end,
      y=function() local _,y=model.desk() return y+theme.BORDER end,
      width=function() local _,_,w=model.desk() return w-theme.LEFT-theme.BORDER end,
      height=function() local _,_,_,h=model.desk() return h-2*theme.BORDER end}
  local transition=require("themes.session").transition
  require("themes.switcher").morph(opening,"radius",0,transition and transition.frame_rounding)
  local field={id="frame",anchors={fill=true},fill_color=function() return C.surface end,blend=0,
    ui.SdfShape {shape="box",anchors={fill=true}},
    opening,
  }
  for _,drawer in ipairs(model.drawers) do field[#field+1]=drawer.shape end
  field[#field+1]=model.rail.shape
  field[#field+1]=model.levels.shape
  local corners={anchors={fill=true,left_margin=theme.LEFT-1,top_margin=theme.BORDER-1,
    right_margin=theme.BORDER-1,bottom_margin=theme.BORDER-1}}
  for i,corner in ipairs {{left=true,top=true},{right=true,top=true},{left=true,bottom=true},{right=true,bottom=true}} do
    corners[#corners+1]=ui.Path {id="frame-corner-"..i,anchors=corner,width=18,height=18,
      rotation=({0,90,270,180})[i],view_box={0,0,18,18},d="M0 0 H18 V1 H1 V18 H0 Z",
      fill_color=function() return C.primary:alpha(.45) end}
  end
  return require("themes.frame_host")(model,ui.Sdf(field),V.insets(),ui.Item(corners))
end
return V
