-- Device-specific plots; the model supplies readings and history only.
local ui=require("morf.ui")
local kit=require("kit")
local graphs=require("graphs")
local theme=require("theme")
local C=theme.color
local V={}
function V.build(model,kind,width,color,active)
  local function history(key) return active() and model.history(key) or {} end
  local function minutes(section)
    return ("%d min"):format(math.floor(model.samples*model.intervals[section]/60000+.5))
  end
  if kind=="cpu" then
    local count=math.max(1,model.info.logical or 1)
    local function cols() return math.min(count,4,math.max(1,math.floor(width()/170))) end
    local function cell_w() return (width()-12*(cols()-1))/cols() end
    local function height() return 32+math.ceil(count/cols())*100 end
    local nodes={width=width,height=height,
      kit.heading {id="performance-utilization-title",text="Utilization · "..minutes("cpu"),
        level="caption",active=active,width=width},
    }
    for i=0,count-1 do
      local graph=graphs.graph {id="performance-core-"..i,width=cell_w,height=70,color=color,top=100,
        columns=4,rows=4,first=function() return history("core"..i) end}
      graph.y=20
      nodes[#nodes+1]=ui.Item {x=function() return (i%cols())*(cell_w()+12) end,
        y=function() return 32+math.floor(i/cols())*100 end,width=cell_w,height=90,
        kit.section_label {text=("%02d"):format(i)},
        kit.section_label {anchors={right=true},text="100%"},graph}
    end
    return ui.Item(nodes),height
  end
  local percent=function() return "100%" end
  local definitions={
    memory={
      {"memory-graph","Memory usage",function() return history("memory") end,nil,100,function() return model.size(model.memory().total) end},
      {"swap-graph","Swap usage",function() return history("swap") end,nil,100,function() return model.size(model.memory().swap.total) end},
    },
    drive={
      {"drive-active","Active time",function() return history("disk:"..model.drive_ref()..":busy") end,nil,100,percent},
      {"drive-throughput","Read / write",function() return history("disk:"..model.drive_ref()..":read") end,
        function() return history("disk:"..model.drive_ref()..":write") end,nil,model.rate},
    },
    net={
      {"net-throughput","Receive / send",function() return history("rx:"..model.net_ref()) end,
        function() return history("tx:"..model.net_ref()) end,nil,model.rate},
    },
    gpu={
      {"gpu-graph","Utilization",function() return history("gpu:"..model.gpu_ref()) end,nil,100,percent},
      {"gpu-video","Video encode / decode",function() return history("gpuenc:"..model.gpu_ref()) end,
        function() return history("gpudec:"..model.gpu_ref()) end,100,percent},
      {"gpu-memory","Memory usage",function() return history("gpumem:"..model.gpu_ref()) end,nil,100,
        function() return model.size(model.the_card().vram_total) end},
    },
    fan={
      {"fan-graph","Speed",function() return history(model.fan_ref()) end,nil,function()
        local f=model.the_fan()
        if f.max and f.max>0 then return f.max end
        local peak=1000
        for _,v in ipairs(history(model.fan_ref())) do peak=math.max(peak,v) end
        return peak*1.15
      end,function(top) return ("%d RPM"):format(math.floor(top)) end},
    },
  }
  local items=definitions[kind]
  local function count()
    if kind=="gpu" then
      if model.the_card().suspended then return 0 end
      return model.the_card().vram_total and 3 or 1
    end
    return #items
  end
  local function cols() return width()>=700 and 2 or 1 end
  local function cell_w() return (width()-12*(cols()-1))/cols() end
  local function plots_h() return math.max(1,math.ceil(count()/cols()))*194 end
  local function height() return plots_h()+(kind=="memory" and 70 or 0) end
  local nodes={width=width,height=height}
  for i,plot in ipairs(items) do
    local function visible() return i<=count() end
    local function series(which)
      return function() return active() and visible() and plot[which]() or {} end
    end
    local top=plot[5]
    local spec={id="performance-"..plot[1],width=cell_w,height=148,color=color,
      first=series(3),second=plot[4] and series(4) or nil,floor=1024,
      top=type(top)=="function" and function() return active() and top() or 100 end or top,
      active=function() return active() and visible() end}
    local node=graphs.captioned(plot[2],function(value) return active() and plot[6](value) or "--" end,spec)
    nodes[#nodes+1]=ui.Item {id="performance-plot-"..plot[1],width=cell_w,height=182,visible=visible,
      x=function() return ((i-1)%cols())*(cell_w()+12) end,
      y=function() return math.floor((i-1)/cols())*194 end,node}
  end
  if kind=="memory" then
    local function composition()
      if not active() then return 0,0 end
      local m=model.memory()
      local used=math.max(0,math.min(1,(m.used or 0)/math.max(1,m.total or 1)))
      return used,math.max(0,math.min(1-used,(m.cached or 0)/math.max(1,m.total or 1)))
    end
    nodes[#nodes+1]=ui.Item {y=plots_h,width=width,height=70,
      kit.heading {id="performance-memory-composition-title",text="Memory composition",level="caption",active=active,width=width},
      ui.Rect {id="performance-memory-composition",y=28,width=width,height=24,color=function() return C.outlineVariant end,
        ui.Rect {height=24,width=function() return width()*composition() end,color=color},
        ui.Rect {height=24,x=function() return width()*composition() end,
          width=function() local _,cached=composition() return width()*cached end,color=function() return C.secondary end}},
    }
  elseif kind=="gpu" then
    nodes[#nodes+1]=ui.Item {id="performance-gpu-suspended",width=width,height=194,
      visible=function() return model.the_card().suspended==true end,
      kit.heading {id="performance-gpu-sleep-title",text="Powered down",level="section",x=16,y=52,
        width=function() return width()-32 end,active=function() return active() and model.the_card().suspended==true end},
      kit.subtitle {x=16,y=92,width=function() return width()-32 end,wrap=true,
        text="Sampling leaves this device asleep."},
    }
  end
  return ui.Item(nodes),height
end
return V
