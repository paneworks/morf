local stroke = require("themes.tsugumori.strokes")
-- Overview content, supplied only with dashboard data and action callbacks.
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
local function percent(value) return ("%d%%"):format(math.floor((value or 0) + .5)) end

function V.card(model, index, width, active, tall)
  local tab = model.tabs[index]
  local node = kit.action { id="dashboard-card-"..tab.key, width=width, height=tall and 312 or 144, cursor="pointer",
    on_clicked=function() model.select(model.tab:get()==index and 1 or index) end,
    ui.Rect { anchors={fill=true}, color=function() return C.surfaceContainer end,
      border_width=1, border_color=function() return stroke(C,"idle") end },
    kit.section_label { x=16,y=16,text=("%02d"):format(index-1),color=function() return C.secondary end },
    kit.heading { id="dashboard-card-"..tab.key.."-title",x=48,y=14,width=math.min(180,width-64),
      level="section",text=tab.name,active=active },
    ui.Rect { x=16,y=43,width=32,height=1,color=function() return C.primary end },
  }
  local function add(child) ui.reparent(child,node) end
  if index==2 then
    add(kit.subtitle { id="media-title",x=16,y=55,width=196,elide="right",
      text=function() return model.player().title or "No media" end })
    for i,entry in ipairs {{"skip_previous","previous"},{"play_arrow","play_pause"},{"skip_next","next"}} do
      add(kit.hover(kit.action { id=({"media-previous","media-play","media-next"})[i],
        x=16+(i-1)*60,y=94,width=52,height=32,cursor="pointer",
        on_clicked=function() model.media_control(entry[2]) end,
        kit.icon(function() return i==2 and model.player().playing and "pause" or entry[1] end,19,
          function() return C.primary end,{anchors={center_in=true}}),
      },function(hovered) return hovered and C.surfaceContainerHighest or C.surfaceContainerHigh end))
    end
  elseif index==3 then
    for i,entry in ipairs {{"CPU",model.cpu},{"MEMORY",model.memory},{"STORAGE",model.storage}} do
      add(kit.section_label { x=16,y=55+(i-1)*24,text=entry[1] })
      add(kit.text { id=({"dashboard-cpu-value","dashboard-memory-value","dashboard-storage-value"})[i],
        x=128,y=54+(i-1)*24,width=76,horizontal_alignment="right",font_size=14,
        text=function() return percent(entry[2]()) end,color=function() return C.primary end })
    end
  elseif index==4 then
    add(kit.text { id="dashboard-battery-value",x=16,y=52,font_size=30,
      text=function() local b=model.battery() return b.capacity and percent(b.capacity) or "—" end,
      color=function() return C.primary end })
    add(kit.subtitle { x=16,y=104,text=function() return model.battery().status or "No battery" end })
  elseif index==5 then
    add(kit.text { id="dashboard-temperature",x=16,y=52,font_size=30,
      text=function()
        local w=model.weather()
        return w.available and ("%d%s"):format(math.floor((w.temperature or 0)+.5),w.units and w.units.temperature or "°") or "—"
      end,color=function() return C.primary end })
    add(kit.subtitle { x=16,y=104,width=196,elide="right",
      text=function() return model.weather().condition or "No weather" end })
  else
    add(kit.subtitle { x=16,y=57,text="Wallpaper and palette" })
    add(kit.section_label { x=16,y=106,text="OPEN STUDIO  /  →",color=function() return C.primary end })
    local icon=tab.icon_build(function() return model.tab:get()==index end,"dashboard-lule-card-icon")
    icon.x,icon.y=16,78
    add(icon)
    if tall then
      local swatches={x=16,y=206,gap=8}
      for _,role in ipairs {"primary","secondary","tertiary","surfaceContainerHighest","onSurface"} do
        swatches[#swatches+1]=ui.Rect {width=(width-64)/5,height=64,color=function() return C[role] end}
      end
      add(ui.Item {width=width,height=312,visible=function() return model.tab:get()==1 end,
        kit.heading {id="dashboard-palette-title",scope="dashboard.overview",level="caption",x=16,y=174,
          text="Current palette",width=width-32},ui.Row(swatches)})
    end
  end
  return node
end

function V.identity(model,width)
  return ui.Item {id="dashboard-user",width=width,height=144,
    ui.Rect {anchors={fill=true},color=function() return C.surfaceContainerLow end,
      border_width=1,border_color=function() return stroke(C,"quiet") end},
    kit.heading {id="dashboard-account-title",scope="dashboard.overview",x=16,y=13,width=width-32,
      text=model.username,level="section"},
    kit.subtitle {x=16,y=42,width=width-32,elide="right",text=function() return model.system().hostname or model.desktop() end},
    kit.text {id="dashboard-clock",x=16,y=65,font_size=34,text=function() return model.clock("%H:%M") end,
      color=function() return C.primary end},
    kit.section_label {id="dashboard-uptime",x=16,y=116,
      text=function()
        local minutes=math.floor((model.system().uptime or 0)/60)
        return ("UP %02d:%02d  /  %s"):format(minutes//60,minutes%60,model.desktop():upper())
      end},
  }
end

function V.calendar(model,width)
  local CW=(width-24)/7
  local cells={}
  for i=1,42 do
    local function day() return model.calendar().days[i] end
    cells[#cells+1]=ui.Item {width=CW,height=32,
      ui.Rect {anchors={fill=true,margins=3},color=function() return day().today and C.primary or "transparent" end,
        border_width=1,border_color=function() return day().today and stroke(C,"focus") or stroke(C,"quiet") end},
      kit.text {anchors={center_in=true},text=function() return tostring(day().day) end,font_size=12,
        color=function()
          local d=day()
          return d.today and C.onPrimary or not d.current and C.outline or d.weekend and C.secondary or C.onSurface
        end},
    }
  end
  local weekdays={}
  for _,name in ipairs {"S","M","T","W","T","F","S"} do
    weekdays[#weekdays+1]=ui.Item {width=CW,height=24,
      kit.section_label {text=name,anchors={center_in=true}}}
  end
  local function arrow(id,icon,x,delta)
    return kit.action {id=id,x=x,y=12,width=28,height=28,cursor="pointer",
      on_clicked=function() model.shift_month(delta) end,
      kit.icon(icon,18,function() return C.primary end,{anchors={center_in=true}})}
  end
  return ui.Item {id="dashboard-calendar",width=width,height=312,
    ui.Rect {anchors={fill=true},color=function() return C.surfaceContainerLow end,
      border_width=1,border_color=function() return stroke(C,"idle") end},
    arrow("calendar-previous","chevron_left",8,-1),arrow("calendar-next","chevron_right",width-36,1),
    kit.heading {id="calendar-title",scope="dashboard.overview",level="caption",x=42,y=18,width=width-84,
      text=function() return model.calendar().title end},
    ui.Row {x=12,y=58,table.unpack(weekdays)},
    ui.Grid {x=12,y=86,columns=7,table.unpack(cells)},
    kit.section_label {x=16,y=286,text=function() return model.clock("%A, %d %B"):upper() end},
  }
end
return V
