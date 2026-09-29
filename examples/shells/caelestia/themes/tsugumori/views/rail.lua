local stroke = require("themes.tsugumori.strokes")
-- Visible workspace pills matching the right-edge level markers.
local morf=require("morf")
local ui=require("morf.ui")
local theme=require("theme")
local kit=require("kit")
local C=theme.color
local V={}
function V.geometry(model)
  local w,h=model.desk_size()
  local item,gap=14,10
  local track=model.count*item+(model.count-1)*gap
  return {w=w,h=h,item=item,gap=gap,top=math.floor((h-track)/2),pill_x=theme.LEFT/2-3,bud_x=theme.LEFT+8}
end
function V.build(model)
  local W,H=144,72
  local function geometry() return V.geometry(model) end
  local function center(id)
    local g=geometry()
    return g.top+((id-1)%model.count)*(g.item+g.gap)+g.item/2
  end
  local shown=morf.signal("tsugumori.rail.shown",false)
  local digits=kit.text {id="rail-value",x=12,y=29,width=W-24,height=36,font_size=28,
    text=function() return ("%02d"):format(model.active()) end,color=function() return C.primary end}
  local card=ui.Item {id="rail-swell",x=-W-12,width=W,height=H,visible=false,opacity=0,
    y=function() return math.max(12,math.min(geometry().h-H-12,center(model.active())-H/2)) end,
    behavior={y={duration=380,easing="out_cubic"}},
    ui.Rect {width=W,height=H,color="transparent",border_width=1,border_color=function() return stroke(C,"focus") end},
    kit.heading {id="rail-title",text="Workspace",x=12,y=9,width=W-24,level="caption",
      active=function() return shown:get() end,reveal_delay=0,decode_lead=160,decode_stagger=20},
    digits,
  }
  local shape=ui.SdfShape {id="rail-swell-background",shape="box",radius=0,
    operation="smooth_union",blend_group=1000,track=card,opacity=0}
  local root=ui.Item {id="rail",anchors={fill=true},visible=model.enabled,card}
  for i=1,model.count do
    local function id() return model.base(model.active())+i-1 end
    ui.reparent(ui.Rect {id="rail-pill-"..i,x=function() return geometry().pill_x end,width=6,
      height=function() return geometry().item end,
      y=function() return center(id())-geometry().item/2 end,
      color=function() return C.primary end,
      opacity=function() return model.occupied(id()) and .65 or .28 end,
      behavior={opacity={duration=180}}},root)
  end
  ui.reparent(ui.Rect {id="rail-selector",x=function() return geometry().pill_x end,width=6,
    height=function() return geometry().item end,
    y=function() return center(model.active())-geometry().item/2 end,
    color=function() return C.primary end,behavior={y={duration=380,easing="out_cubic"}}},root)
  kit.ride("rail",root,model.leftbar.drawer,
    function() return theme.SIDE_W+theme.STRIP/2+theme.LEFT/2 end)
  local motion,hide
  local function cancel()
    if hide then hide:cancel() hide=nil end
    if motion then motion:stop() motion=nil end
  end
  local function close(immediate)
    cancel() shown:set(false)
    if immediate then
      card.visible,card.opacity,shape.opacity=false,0,0
      card.x=-W-12
      return
    end
    motion=morf.animation.play {{parallel={
      {node=card,property="x",to=-W-12,duration=260,easing="in_out_quint"},
      {node=card,property="opacity",to=0,duration=180},
      {node=shape,property="opacity",to=0,duration=180},
    }},on_finished=function(reason) if reason=="completed" then card.visible=false end end}
  end
  local function pop()
    cancel() shown:set(true) card.visible=true
    motion=morf.animation.play {{parallel={
      {node=card,property="x",to=geometry().bud_x,duration=340,easing="out_expo"},
      {node=card,property="opacity",to=1,duration=180},
      {node=shape,property="opacity",to=1,duration=180},
      {node=digits,property="translate_y",from=-8,to=0,duration=280,easing="out_cubic"},
      {node=digits,property="opacity",from=0,to=1,duration=180},
    }}}
    hide=morf.timer(model.hold(),function() hide=nil close(false) end,false)
  end
  local last=model.active()
  morf.effect("tsugumori.rail.follow",function()
    local id,on=model.active(),model.enabled()
    local changed=id~=last
    last=id
    if not on then close(true) elseif changed then pop() end
  end,{owner=root})
  return {node=root,shape=shape}
end
return V
