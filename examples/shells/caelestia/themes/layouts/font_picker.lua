local morf, ui = require("morf"), require("morf.ui")
return function(state)
  local kit, theme = require("kit"), require("theme")
  local C, fonts = theme.color, require("themes.fonts")
  local W, SIZE = 434, 5
  local search = ui.TextInput { id="lule-font-search", x=12,y=9,width=W-48,height=24,
    focus=function() return fonts.opened:get() end,
    font_family=theme.font,font_size=13,placeholder="Search installed fonts…",
    color=function() return C.onSurface end,placeholder_color=function() return C.onSurfaceVariant end,
    caret_color=function() return C.primary end,selection_color=function() return C.primary:alpha(.2) end,
    on_text_changed=function(value) fonts.filter(value) end,on_escape=fonts.close,
    on_accepted=function() local rows=fonts.rows:get() if #rows==1 then fonts.choose(rows[1]) end end }
  local rows={x=12,y=60,gap=2}
  for i=1,SIZE do
    local function family() return fonts.rows:get()[(fonts.page:get()-1)*SIZE+i] end
    local area
    area=kit.action {id="lule-font-option-"..i,width=W-24,height=42,cursor="pointer",
      visible=function() return family()~=nil end,
      on_clicked=function() if not state.busy:get() and not state.appearance.busy:get() then fonts.choose(family()) end end,
      kit.surface {anchors={fill=true},radius=6,
        color=function() return family()==require("themes").font and C.primaryContainer or C.surfaceContainerHigh end},
      kit.text {x=10,y=3,width=W-66,height=18,font_size=12,elide="right",text=function() return family() or "" end},
      kit.text {id="lule-font-preview-"..i,x=10,y=21,width=W-66,height=18,font_size=13,elide="right",
        font_family=function() return family() or theme.font end,font_source="",
        text="The quick brown fox · 0123456789",color=function() return C.primary end},
    }
    rows[#rows+1]=area
  end
  local function button(id,label,width,action)
    return kit.pill {id=id,label=label,width=width,height=30,on_clicked=action}
  end
  local popup=kit.card {id="lule-font-popup",x=510,y=130,width=W,height=330,radius=16,
    ui.MouseArea {anchors={fill=true},on_clicked=function() end},
    kit.surface {x=12,y=10,width=W-24,height=40,radius=8,color=function() return C.surfaceContainerHighest end,search},
    ui.Column(rows),
    kit.subtitle {x=22,y=65,width=W-44,height=40,wrap=true,font_size=12,
      visible=function() return #fonts.rows:get()==0 end,
      text=function() return fonts.status:get()~="" and fonts.status:get() or "No matching fonts" end},
    ui.Row {x=12,y=288,gap=8,
      button("lule-font-default","Theme default",138,function() fonts.choose("") end),
      button("lule-font-prev","Back",64,function() fonts.step(-1,SIZE) end),
      button("lule-font-next","More",64,function() fonts.step(1,SIZE) end),
      kit.subtitle {width=100,y=8,font_size=11,horizontal_alignment="right",
        text=function() return fonts.page:get().." / "..math.max(1,math.ceil(#fonts.rows:get()/SIZE)) end},
    },
  }
  local root=ui.Item {id="lule-font-picker",anchors={fill=true},z=100,
    visible=function() return fonts.opened:get() end,
    ui.MouseArea {anchors={fill=true},on_clicked=fonts.close,
      ui.Rect {anchors={fill=true},color=function() return C.surface:alpha(.5) end}},popup,
  }
  morf.effect("lule.font-picker.close",function()
    if not state.active:get() or state.appearance.busy:get() then fonts.close() end
  end,{owner=root})
  return root
end
