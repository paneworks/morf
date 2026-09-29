-- Rectangular level register beside the shell's right-edge markers.
local morf=require("morf")
local ui=require("morf.ui")
local theme=require("theme")
local kit=require("kit")
local C=theme.color
local V={}
function V.build(model)
  local W,H=222,88
  local function geometry()
    local r=model.rail_geometry()
    local top=math.floor((r.h-2*r.item-r.gap)/2)
    return {w=r.w,h=r.h,item=r.item,pill_h=r.item,gap=r.gap,pill_x=r.w-theme.BORDER/2-3,
      clear=8,tops={volume=top,brightness=top+r.item+r.gap}}
  end
  local function kind() return model.shown:get()~="" and model.shown:get() or "volume" end
  local function value() return model.value[kind()]() end
  local function ink() return kind()=="volume" and model.muted() and C.onSurfaceVariant or C.primary end
  local function tucked() return geometry().w+theme.BORDER+2 end
  local function out() return geometry().w-W-theme.BORDER-8 end
  local body=ui.Item {id="levels-bud",width=W,height=H,
    ui.Rect {width=W,height=H,color="transparent",border_width=1,border_color=function() return ink():alpha(.18) end},
    kit.heading {id="levels-title",x=12,y=9,width=W-24,level="caption",reveal_delay=0,
      active=function() return model.active:get() end,ink=ink,
      text=function() return kind()=="brightness" and "Brightness" or model.muted() and "Muted" or "Volume" end},
    kit.icon(function() return model.icon[kind()]() end,24,ink,{x=12,y=37}),
    kit.text {id="levels-value",x=49,y=29,width=W-63,font_size=28,font_weight=500,
      horizontal_alignment="right",color=ink,text=function() return ("%d%%"):format(math.floor(value()*100+.5)) end},
    kit.bar {id="levels-meter",x=12,y=H-13,width=W-24,stroke=3,value=value,
      color=ink,track=function() return ink():alpha(.12) end},
  }
  local panel=ui.Item {id="levels-swell",x=tucked(),width=0,height=0,opacity=0,visible=false,
    y=function()
      local g=geometry()
      return math.max(12,math.min(g.h-H-12,g.tops[kind()]+g.item/2-H/2))
    end,behavior={y={duration=200,easing="out_cubic"}},body}
  local shape=ui.SdfShape {id="levels-swell-background",shape="box",operation="smooth_union",blend_group=1001,
    track=panel,radius=0,opacity=0}
  local root=ui.Item {id="levels",anchors={fill=true},panel}
  for _,name in ipairs {"volume","brightness"} do
    ui.reparent(ui.Rect {id="levels-"..name,x=function() return geometry().pill_x end,
      y=function() return geometry().tops[name] end,width=6,height=function() return geometry().pill_h end,
      color=function() return model.shown:get()==name and C.primary or C.outlineVariant end,
      opacity=function() return model.shown:get()==name and 1 or .6 end,
      behavior={color={duration=180},opacity={duration=180}},
    },root)
  end
  kit.ride("levels",root,model.sidebar.drawer,
    function() return -(theme.SIDE_W+theme.STRIP/2+theme.BORDER/2) end)
  local motion
  local function show()
    if motion then motion:stop() end
    panel.width,panel.height=W,H
    panel.visible=true
    motion=morf.animation.play {{parallel={
      {node=panel,property="x",to=out(),duration=340,easing="out_expo"},
      {node=panel,property="opacity",to=1,duration=180},
      {node=shape,property="opacity",to=1,duration=180},
    }}}
  end
  local function hide(done)
    if motion then motion:stop() end
    motion=morf.animation.play {{parallel={
      {node=panel,property="x",to=tucked(),duration=260,easing="in_out_quint"},
      {node=panel,property="opacity",to=0,duration=200},
      {node=shape,property="opacity",to=0,duration=200},
    }},on_finished=function(reason)
      if reason~="completed" then return end
      panel.width,panel.height=0,0
      panel.visible=false
      done()
    end}
  end
  return {node=root,shape=shape,show=show,hide=hide,hold=2200,geometry=geometry}
end
return V
