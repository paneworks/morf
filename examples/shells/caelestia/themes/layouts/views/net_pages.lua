-- Material network settings geometry, supplied with shared models.
local ui=require("morf.ui")
local theme=require("theme")
local kit=require("kit")
local C=theme.color
local M={}

--- A row card: an icon badge, a name and what it says, and a switch.
local function row_card(w, spec)
  local area
  area = ui.Item {
    id = spec.id, width = w, height = 76,
    kit.card {
      anchors = { fill = true }, radius = 20,
      color = function() return C.surfaceContainer end,
    },
    kit.surface {
      x = 14, anchors = { vertical_center = true }, width = 44, height = 44, radius = 22,
      color = function() return spec.on() and C.primary or C.surfaceContainerHighest end,
      behavior = { color = { duration = theme.duration.small } },
      kit.icon(spec.icon, 22, function() return spec.on() and C.onPrimary or C.onSurfaceVariant end,
        { anchors = { center_in = true }, fill = true }),
    },
    ui.Column {
      x = 70, anchors = { vertical_center = true }, gap = 2,
      kit.menu_label { text = spec.name, font_weight = 600, width = w - 170, elide = "right" },
      kit.subtitle { text = spec.detail, font_size = theme.size.small, width = w - 170, elide = "right",
        color = function() return C.onSurfaceVariant end },
    },
  }
  if spec.toggle then
    local sw
    sw = kit.action {
      id = spec.id .. "-switch",
      anchors = { right = true, right_margin = 14, vertical_center = true },
      width = 84, height = 36, cursor = "pointer",
      visible = function() return spec.can == nil or spec.can() end,
      on_clicked = function() spec.toggle(not spec.on()) end,
      kit.surface {
        anchors = { fill = true }, radius = 18,
        color = function()
          local base = spec.on() and C.secondaryContainer or C.primary
          return (sw and sw.hovered) and base:mix(C.onSurface, 0.06) or base
        end,
      },
      kit.menu_label {
        anchors = { center_in = true }, font_size = theme.size.small, font_weight = 600,
        text = function() return spec.on() and (spec.off_word or "Disconnect") or (spec.on_word or "Connect") end,
        color = function() return spec.on() and C.onSecondaryContainer or C.onPrimary end,
      },
    }
    local wrapper = { width = w, height = 76, area, sw }
    return ui.Item(wrapper)
  end
  return area
end

local function note(w, text)
  return kit.subtitle {
    width = w, wrap = true, font_size = theme.size.small, text = text,
    color = function() return C.onSurfaceVariant end,
  }
end

local function entry(model,w,row)
  local function live() return model.row(row) or row end
  return row_card(w,{
    id=row.id,icon=function() return live().icon end,name=function() return live().name end,
    on=function() return live().on end,detail=function() return live().detail end,
    can=function() return live().can end,on_word=row.on_word,off_word=row.off_word,
    toggle=row.source~="readout" and function(on) model.toggle(row,on) end or nil,
  })
end
function M.build(model,w,h)
  local viewport
  local kind=model.kind
  local nodes={width=w,gap=(kind=="wired" or kind=="tor") and 12 or 10}
  if kind=="wired" then
    nodes[#nodes+1]=ui.Repeater {as="column",gap=10,width=w,model=model.network,
      delegate=function(row) return entry(model,w,row) end}
    nodes[#nodes+1]=note(w,function() return model.has_network() and model.note or "NetworkManager is not running." end)
  elseif kind=="tor" then
    nodes[#nodes+1]=ui.Repeater {as="column",gap=12,width=w,model=model.tor,
      delegate=function(row) return entry(model,w,row) end}
    nodes[#nodes+1]=note(w,function() return model.available() and model.note or "Tor is not installed." end)
  else
    if kind=="tunnel" then
      nodes[#nodes+1]=kit.heading {id="vpn-tunnel-network-title",scope="settings.tunnel",level="section",
        text="NetworkManager",font_size=theme.size.small,color=function() return C.onSurfaceVariant end,
        viewport=function() return viewport end,visible=function() return model.network:len()>0 end}
      nodes[#nodes+1]=ui.Repeater {as="column",gap=10,width=w,model=model.network,
        delegate=function(row) return entry(model,w,row) end}
      nodes[#nodes+1]=kit.heading {id="vpn-tunnel-apps-title",scope="settings.tunnel",level="section",
        text="Apps",font_size=theme.size.small,color=function() return C.onSurfaceVariant end,
        viewport=function() return viewport end,visible=model.has_network}
    end
    for _,id in ipairs(model.tools) do
      local function row() return model.row("app:"..id) end
      local card=row_card(w,{
        id="vpn-"..id,icon=kind=="mesh" and "hub" or "shield",name=model.names[id],
        on=function() local r=row() return r~=nil and r.on end,
        detail=function() local r=row() return r and r.detail or "Reading…" end,
        can=function() local r=row() return r~=nil and r.can end,
        on_word=kind=="mesh" and "Up" or "Connect",off_word=kind=="mesh" and "Down" or "Disconnect",
        toggle=function(on) model.toggle("app:"..id,on) end,
      })
      card.visible=function() return model.apps:len()==0 or row()~=nil end
      nodes[#nodes+1]=card
    end
    nodes[#nodes+1]=note(w,model.note)
    if kind=="tunnel" then
      nodes[#nodes+1]=kit.heading {id="vpn-tunnel-tor-title",scope="settings.tunnel",level="section",
        text="Tor",font_size=theme.size.small,viewport=function() return viewport end,
        visible=function() return model.tor:len()>0 end}
      nodes[#nodes+1]=ui.Repeater {as="column",gap=10,width=w,model=model.tor,
        delegate=function(row) return entry(model,w,row) end}
      local tor_note=note(w,"Tor runs a SOCKS proxy for apps configured to use it. Starting it does not route all system traffic through Tor.")
      tor_note.visible=function() return model.tor:len()>0 end
      nodes[#nodes+1]=tor_note
    end
  end
  viewport=ui.Flickable {id=(kind=="mesh" or kind=="tunnel") and "vpn-"..kind.."-scroll" or kind.."-scroll",
    width=w,height=h,clip=true,ui.Column(nodes)}
  if kind=="wired" or kind=="tor" then
    local present=kind=="wired" and model.has_network or model.available
    viewport.visible=present
    local missing=note(w,kind=="wired" and "NetworkManager is not running." or "Tor is not installed.")
    missing.visible=function() return not present() end
    return ui.Item {width=w,height=h,viewport,missing}
  end
  return viewport
end
return M
