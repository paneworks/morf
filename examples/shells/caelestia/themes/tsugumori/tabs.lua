local stroke = require("themes.tsugumori.strokes")
-- Numbered instrument tabs. Selection fills the header; the lower rail is
-- segmented rather than a rounded Material underline. Labels roll once on
-- hover using native per-character channels (no polling or infinite loop).
local morf = require("morf")
local ui = require("morf.ui")
return function(theme, kit, spec)
  local C, tabs, tab = theme.color, spec.tabs, spec.tab
  local function width() return type(spec.width)=="function" and spec.width() or spec.width end
  local pad, height, gap = spec.pad or 11, spec.height or 64, 8
  local function slot() return (width()-2*pad-gap*(#tabs-1))/#tabs end
  local buttons = { width = width, height = height }
  for i, t in ipairs(tabs) do
    local selected = function() return tab:get()==i end
    local function ink() return selected() and C.onPrimary or C.onSurface end
    local button
    local caption = t.name:upper()
    local advance = theme.typography.menu * .62
    local letters, rolls = {}, {}
    for _, code in utf8.codes(caption) do
      local k, char = #letters+1, utf8.char(code)
      local glyph=kit.menu_label {text=char,height=18,color=ink}
      local strip=ui.Column { gap=0, y=-36,
        kit.menu_label { text="/",height=18,color=ink },
        kit.menu_label { text=k%2==0 and "+" or "|",height=18,color=ink },
        glyph,
      }
      letters[#letters+1]=strip
      rolls[#rolls+1]=ui.Item { width=function() return math.max(1,glyph.layout_width or advance) end,height=18,clip=true,strip }
    end
    local row=ui.Row {gap=0,height=18,table.unpack(rolls)}
    local label=ui.Item { x=34,y=12,height=18,clip=true,
      width=function() return math.max(0,math.min(row.layout_width or utf8.len(caption)*advance,slot()-((t.icon_build or t.icon) and 68 or 44))) end,
      row }
    local function hover() return button and button.hovered end
    button=ui.MouseArea {
      id=spec.id.."-tab-"..(t.key or t.name:lower()),
      x=function() return pad+(i-1)*(slot()+gap) end,y=8,width=slot,height=40,cursor="pointer",
      on_clicked=function() tab:set(i) end,
      behavior=spec.geometry and {x=spec.geometry,width=spec.geometry} or nil,
      ui.Rect { anchors={fill=true},color=function() return C.surfaceContainer end,
        border_width=1,border_color=function() return selected() and stroke(C,"focus") or hover() and stroke(C,"hover") or stroke(C,"quiet") end,
        behavior={border_color={duration=180}} },
      ui.Item {anchors={fill=true,margins=1},clip=true,
      ui.Rect { x=0,y=0,height=38,
        width=function() return selected() and slot()-2 or 0 end,
        color=function() return C.primary end,
        behavior={width={duration=220,easing={x1=0.76,y1=0,x2=0.24,y2=1}}} } },
      kit.section_label { text=("%02d"):format(i),x=9,y=14,color=ink },
      ui.Rect { x=27,y=10,width=1,height=20,color=function() return ink():alpha(0.35) end },
      label,
    }
    if t.icon_build or t.icon then
      local icon_id=spec.id.."-tab-"..(t.key or t.name:lower()).."-icon"
      local icon=t.icon_build and t.icon_build(selected,icon_id,ink)
        or kit.icon(t.icon,18,ink,{id=icon_id,fill=selected})
      icon.anchors={right=true,right_margin=9,vertical_center=true}
      ui.reparent(icon,button)
    end
    for _, corner in ipairs { {left=true,top=true},{right=true,top=true},{left=true,bottom=true},{right=true,bottom=true} } do
      local sx,sy=corner.left and -1 or 1,corner.top and -1 or 1
      ui.reparent(ui.Item { anchors=corner,width=6,height=6,
        translate_x=function() return (hover() or selected()) and sx*3 or 0 end,
        translate_y=function() return (hover() or selected()) and sy*3 or 0 end,
        opacity=function() return (hover() or selected()) and 1 or 0 end,
        behavior={translate_x={duration=340,easing="out_cubic"},translate_y={duration=340,easing="out_cubic"},opacity={duration=220}},
        ui.Rect { anchors=corner,width=6,height=1,color=function() return C.primary end },
        ui.Rect { anchors=corner,width=1,height=6,color=function() return C.primary end },
      },button)
    end
    local running,was={},false
    morf.effect(spec.id..".tab-roll."..i,function()
      local now=button.hovered
      if now==was then return end
      was=now
      for _,handle in ipairs(running) do handle:stop() end
      running={}
      for k,letter in ipairs(letters) do
        if now then
          running[#running+1]=morf.animation.play { {node=letter,property="y",from=0,to=-36,
            delay=42.5*(k-1)/math.max(1,#letters-1),duration=127.5,easing="out_cubic"} }
        else letter.y=-36 end
      end
    end,{owner=button})
    buttons[#buttons+1]=button
    buttons[#buttons+1]=ui.Rect { x=function() return pad+(i-1)*(slot()+gap) end,y=height-7,
      width=slot,height=1,color=function() return selected() and stroke(C,"focus") or stroke(C,"quiet") end,
      behavior={color={duration=220},x=spec.geometry,width=spec.geometry} }
  end
  return ui.Item(buttons)
end
