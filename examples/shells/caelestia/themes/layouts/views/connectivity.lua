-- The right panel's Network and Bluetooth pages: what the bar's popouts
-- had, given the panel's height. Network: Wi-Fi on or off, the networks
-- (the one in use first, then by strength; a click joins one) and a
-- rescan. Bluetooth: on or off, discovery, the devices (connected, then
-- paired, then by name; a click connects or disconnects) and the settings
-- program. The injected model owns service reads and actions; this builder
-- preserves the original cards, row positions and footer buttons.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local P = require("themes.layouts.page")

local M = {}

local MAX_ROWS = 64

--- Up to `max` (MAX_ROWS) rows, each shown while `count()` reaches it; one
--- saying `none` while there are none.
local function list(w, prefix, count, row, none, max)
  local inner = P.inner(w)
  local col = { width = inner, gap = P.ROW_GAP }
  for i = 1, max or MAX_ROWS do
    col[#col + 1] = ui.Item { id = prefix .. i, width = inner, height = P.ROW_H,
      visible = function() return i <= count() end, row(i, inner) }
  end
  col[#col + 1] = P.row { width = inner, icon = "info", on = function() return false end, title = none,
    visible = function() return count() == 0 end }
  return ui.Column(col)
end

--- The one action under a list, the card's width.
local function action(w, spec)
  spec.width = P.inner(w)
  return P.buttons { width = P.inner(w), spec }
end

--- What a page without its service says, in place of its lists.
local function missing(w, icon, title, text, available)
  local node = P.empty { width = w, icon = icon, title = title, text = text }
  node.visible = function() return not available() end
  return node
end

--- What the last action said ("a password is needed"), under the list.
local function status(id, model, w)
  local node = kit.subtitle { id = id, width = P.inner(w), wrap = true, font_size = P.SUB,
    text = function() return model.message:get() or "" end,
    color = function() return model.failed:get() and theme.color.error or theme.color.onSurfaceVariant end }
  node.visible = function() return (model.message:get() or "") ~= "" end
  return node
end

-- ----------------------------------------------------------------- network --

local function network_page(model, w, h)
  local available = model.available
  local inner = P.inner(w)
  local networks = P.section { id = "network-list", width = w, title = "Networks",
    visible = available,
    list(w, "network-row-", function() return #model.list() end, function(i, rw)
      local function ap() return model.list()[i] or {} end
      return P.row { id = "network-ap-" .. i, width = rw, on = function() return ap().in_use == true end,
        icon = function() return model.signal_icon(ap().strength) end,
        title = function() return ap().ssid or "" end,
        subtitle = function()
          local a = ap()
          if a.in_use then return "Connected" end
          return a.secure and "Secured" or "Open"
        end,
        trailing = kit.icon(function() return ap().secure and "lock" or "" end, 16, kit.ink("lo")),
        on_clicked = function() model.choose(ap()) end }
    end, function() return model.enabled() and "No networks found" or "Turn on Wi-Fi to find networks." end),
    status("network-status", model, w),
    action(w, { id = "network-rescan", icon = "wifi_find", label = "Rescan networks",
      on_clicked = function() model.scan() end }),
  }
  return ui.Item { id = "network-page", width = w, height = h, P.page { id = "network-scroll", width = w, height = h,
    P.section { id = "wifi", caption_id = "wifi-title", width = w, title = "Wi-Fi",
      P.row { id = "network-enabled", width = inner, icon = "wifi", on = model.enabled, title = "Enabled",
        subtitle = function()
          if not available() then return "No network manager" end
          local count = #model.list()
          return ("%d network%s available"):format(count, count == 1 and "" or "s")
        end,
        trailing = P.switch { id = "network-wifi", name = "Wi-Fi", on = model.enabled, on_toggled = model.set_enabled } },
    },
    networks,
    missing(w, "wifi_off", "No network manager", "NetworkManager is not running, so there are no networks to list.",
      available),
  } }
end

-- --------------------------------------------------------------- bluetooth --

local function bluetooth_page(model, w, h)
  local available = model.available
  local inner = P.inner(w)
  local devices = P.section { id = "bluetooth-list", width = w, title = "Devices",
    visible = available,
    list(w, "bluetooth-row-", function() return #model.list() end, function(i, rw)
      local function dev() return model.list()[i] or {} end
      local function on() return dev().connected == true end
      return P.row { id = "bluetooth-device-" .. i, width = rw, on = on,
        icon = function() return on() and "bluetooth_connected" or "bluetooth" end,
        title = function() local d = dev() return d.alias or d.name or d.address or "" end,
        subtitle = function()
          local d = dev()
          if d.connected then return "Connected" end
          return d.paired and "Paired" or "Not paired"
        end,
        on_clicked = function() model.choose(dev()) end }
    end, function() return model.enabled() and "No devices" or "Turn on Bluetooth to find devices." end),
    status("bluetooth-status", model, w),
    action(w, { id = "bluetooth-settings", icon = "settings", label = "Open settings",
      on_clicked = function() model.open_settings() end }),
  }
  return ui.Item { id = "bluetooth-page", width = w, height = h, P.page { id = "bluetooth-scroll", width = w, height = h,
    P.section { id = "bluetooth", caption_id = "bluetooth-title", width = w, title = "Bluetooth",
      P.row { id = "bluetooth-enabled", width = inner, icon = "bluetooth", on = model.enabled, title = "Enabled",
        subtitle = function()
          if not available() then return "No Bluetooth adapter" end
          local count = #model.list()
          return ("%d device%s"):format(count, count == 1 and "" or "s")
        end,
        trailing = P.switch { id = "bluetooth-power", name = "Bluetooth", on = model.enabled,
          on_toggled = model.set_enabled } },
      P.row { id = "bluetooth-discovering", width = inner, icon = "bluetooth_searching", on = model.discovering,
        title = "Discovering", subtitle = "Look for devices nearby",
        trailing = P.switch { id = "bluetooth-discover", name = "Discovering", on = model.discovering,
          on_toggled = model.scan } },
    },
    devices,
    missing(w, "bluetooth_disabled", "No Bluetooth adapter", "There is no Bluetooth adapter to use.", available),
  } }
end

--- A one-line text field the card's width, its text kept in `draft`.
local function field(id, w, placeholder, draft, kind)
  local C = theme.color
  local node, handle = kit.text_field(kind or "entry", { id = id, x = 10, y = 3, width = w - 20, height = 30,
    font_family = theme.font, font_size = 12, placeholder = placeholder, reveal = kind == "password" and false or nil,
    color = function() return C.onSurface end, placeholder_color = function() return C.onSurfaceVariant end,
    caret_color = function() return C.primary end, selection_color = function() return C.primary:alpha(0.25) end,
    on_text_changed = function(value) draft:set(value) end })
  morf.effect("caelestia.mobile.field." .. id, function() handle.text = draft:get() end)
  return kit.surface { width = w, height = 36, radius = kit.round(10),
    color = function() return C.surfaceContainerHighest end, node }
end

local function mobile_page(model,w,h)
  local inner=P.inner(w)
  local function details() return model.details:get() or {} end
  local function modem() return details().modem or {} end
  local function sim() return details().sim or {} end
  local function profile()
    for _,r in ipairs(model.list()) do if (r.active or "")~="" then return r end end
    return model.list()[1]
  end
  local function has_profile() return profile()~=nil end
  local function busy() return model.pending:get() end
  local draft={}
  for _,k in ipairs {"apn","user","password","pin","new_pin","ussd","ussd_reply","sms_to","sms_text"} do
    draft[k]=morf.signal("caelestia.mobile.draft."..k,"")
  end
  morf.effect("caelestia.mobile.draft.profile",function()
    local p=profile() or {}
    draft.apn:set(p.auto_apn and "" or (p.apn or "")) draft.user:set(p.user or "")
  end)
  local function save(change) local p=profile() if p then model.save_profile(p,change) end end

  local IP={{"ipv4","IPv4"},{"ipv4v6","IPv4 + IPv6"},{"ipv6","IPv6"}}
  local ip_buttons={width=inner}
  for _,t in ipairs(IP) do
    ip_buttons[#ip_buttons+1]={id="mobile-ip-"..t[1],label=t[2],enabled=function() return not busy() end,
      selected=function() return (profile() or {}).ip_type==t[1] end,on_clicked=function() save({ip_type=t[1]}) end}
  end
  local mode_buttons={width=inner}
  for i=1,5 do
    local function choice() return (model.modes())[i] end
    mode_buttons[#mode_buttons+1]={id="mobile-mode-"..i,label=function() return (choice() or {}).name or "" end,
      visible=function() return choice()~=nil end,enabled=function() return not busy() end,
      selected=function() local _,now=model.modes() return choice()~=nil and choice().id==now end,
      on_clicked=function() if choice() then model.set_mode(choice()) end end}
  end

  return ui.Item {id="mobile-page",width=w,height=h,
    P.page {id="mobile-scroll",width=w,height=h,active=model.active,
      P.section {id="mobile-radio",width=w,title="Mobile data",visible=model.available,
        P.row {id="mobile-enabled",width=inner,icon="signal_cellular_alt",title="Enabled",on=model.enabled,
          subtitle=model.status,
          trailing=P.switch {id="mobile-data",name="Mobile data",on=model.enabled,
            on_toggled=function(on) if model.can_toggle() then model.set_enabled(on) end end}},
        P.row {id="mobile-carrier",width=inner,icon="cell_tower",title=model.carrier,
          subtitle=model.signal,on=function() return false end},
        P.row {id="mobile-diagnosis",width=inner,icon="report",title="Problem",on=function() return false end,
          visible=function() return model.diagnosis()~=nil end,
          subtitle=function() return (model.diagnosis() or {}).text or "" end,
          trailing=P.button {id="mobile-fix",label=model.fix_label,tone="primary",
            visible=function() return model.fix_label()~="" and not busy() end,on_clicked=model.fix}},
        action(w,{id="mobile-reset",icon="restart_alt",label="Restart mobile radio",
          enabled=function() return not busy() end,on_clicked=model.reset_radio}),
      },
      P.section {id="mobile-connections",width=w,title="Connections",visible=model.available,
        list(w,"mobile-row-",function() return #model.list() end,function(i,rw)
          local function row() return model.list()[i] or {} end
          local function connected() return (row().active or "")~="" end
          return P.row {id="mobile-profile-"..i,width=rw,icon="sim_card",title=function() return row().name or "" end,
            on=connected,subtitle=function()
              local r=row()
              local state=({activated="Connected",activating="Connecting…",deactivating="Disconnecting…"})[r.state] or "Disconnected"
              return state.." · "..((r.apn or "")~="" and ("APN: "..r.apn) or r.auto_apn and "Automatic APN" or "Provider APN")
            end,
            trailing=P.button {id="mobile-connect-"..i,
              label=function() return connected() and "Disconnect" or "Connect" end,
              visible=function() return not model.pending:get() and (connected() or model.can_connect()) end,
              on_clicked=function() model.choose(row()) end}}
        end,"No saved mobile connections"),
        action(w,{id="mobile-setup",icon="add",label="Set up automatically",
          visible=function() return #model.list()==0 and model.can_connect() end,
          on_clicked=model.setup}),
        status("mobile-status",model,w),
        kit.subtitle {width=inner,wrap=true,font_size=P.SUB,text=model.note,color=kit.ink("lo")},
      },
      P.section {id="mobile-profile",width=w,title="Connection settings",
        visible=function() return model.available() and has_profile() end,
        kit.subtitle {width=inner,font_size=P.SUB,color=kit.ink("lo"),text="APN (empty for automatic)"},
        field("mobile-apn",inner,"live.vodafone.com",draft.apn),
        field("mobile-user",inner,"User (optional)",draft.user),
        field("mobile-password",inner,"Password (optional)",draft.password,"password"),
        P.buttons {width=inner,
          {id="mobile-apn-save",label="Save",tone="primary",enabled=function() return not busy() end,
            on_clicked=function()
              local change={apn=draft.apn:get(),user=draft.user:get()}
              if draft.password:get()~="" then change.password=draft.password:get() end
              save(change)
            end},
          {id="mobile-apn-auto",label="Automatic APN",enabled=function() return not busy() end,
            on_clicked=function() draft.apn:set("") save({apn=""}) end}},
        kit.subtitle {width=inner,font_size=P.SUB,color=kit.ink("lo"),text="IP type"},
        P.buttons(ip_buttons),
        P.row {id="mobile-roaming",width=inner,icon="public",title="Roaming",
          on=function() return (profile() or {}).roaming==true end,
          subtitle=function() return (profile() or {}).roaming and "Allowed on other networks" or "Home network only" end,
          trailing=P.switch {id="mobile-roaming-switch",name="Roaming",
            on=function() return (profile() or {}).roaming==true end,
            on_toggled=function(on) save({roaming=on}) end}},
      },
      P.section {id="mobile-network",width=w,title="Network",visible=model.available,
        kit.subtitle {width=inner,font_size=P.SUB,color=kit.ink("lo"),text="Network mode"},
        P.buttons(mode_buttons),
        list(w,"mobile-operator-",function() return #model.networks:get() end,function(i,rw)
          local function n() return model.networks:get()[i] or {} end
          return P.row {id="mobile-operator-row-"..i,width=rw,icon="cell_tower",
            on=function() return n().availability=="current" end,
            title=function() return n().name or "" end,
            subtitle=function() return (n().technology or ""):upper().." · "..(n().availability or "") end,
            on_clicked=function() if n().code then model.register(n().code) end end}
        end,"Scan to list the networks nearby",16),
        P.buttons {width=inner,
          {id="mobile-scan",icon="radar",label=function() return model.scanning:get() and "Scanning…" or "Scan networks" end,
            enabled=function() return not busy() end,on_clicked=model.scan_networks},
          {id="mobile-automatic",label="Automatic",enabled=function() return not busy() end,
            on_clicked=function() model.register(nil) end}},
      },
      P.section {id="mobile-sim",width=w,title="SIM",visible=model.available,
        P.row {id="mobile-sim-info",width=inner,icon="sim_card",on=function() return false end,
          title=function()
            local s=sim()
            if (s.operator or "")~="" then return s.operator end
            return (s.operator_code or "")~="" and ("Home network "..s.operator_code) or "SIM"
          end,
          subtitle=function()
            local s,m,parts=sim(),modem(),{}
            if (m.own_numbers or {})[1] then parts[#parts+1]=m.own_numbers[1] end
            if (s.iccid_tail or "")~="" then parts[#parts+1]="ICCID …"..s.iccid_tail end
            local tries=(m.retries or {})["sim-pin"]
            if tries then parts[#parts+1]=tries.." PIN tries left" end
            return #parts>0 and table.concat(parts," · ") or "No SIM details"
          end},
        field("mobile-pin",inner,"SIM PIN",draft.pin,"password"),
        field("mobile-new-pin",inner,"New PIN (to change it)",draft.new_pin,"password"),
        P.buttons {width=inner,
          {id="mobile-unlock",label="Unlock",tone="primary",visible=function() return modem().locked~=nil end,
            enabled=function() return not busy() end,on_clicked=function() model.unlock(draft.pin:get()) draft.pin:set("") end},
          {id="mobile-change-pin",label="Change PIN",enabled=function() return not busy() end,
            on_clicked=function() model.change_pin(draft.pin:get(),draft.new_pin:get()) draft.pin:set("") draft.new_pin:set("") end},
          {id="mobile-pin-on",label="PIN on",enabled=function() return not busy() end,
            on_clicked=function() model.set_pin_lock(draft.pin:get(),true) draft.pin:set("") end},
          {id="mobile-pin-off",label="PIN off",enabled=function() return not busy() end,
            on_clicked=function() model.set_pin_lock(draft.pin:get(),false) draft.pin:set("") end}},
      },
      P.section {id="mobile-ussd",width=w,title="USSD codes",visible=model.available,
        field("mobile-ussd-code",inner,"*100#",draft.ussd),
        P.buttons {width=inner,
          {id="mobile-ussd-send",label="Send code",enabled=function() return not busy() end,
            on_clicked=function() if draft.ussd:get()~="" then model.ussd(draft.ussd:get()) end end}},
        (function()
          local node=kit.subtitle {id="mobile-ussd-reply",width=inner,wrap=true,font_size=P.SUB,
            text=function() return model.ussd_reply:get().text or "" end}
          node.visible=function() return (model.ussd_reply:get().text or "")~="" end
          return node
        end)(),
        ui.Column {width=inner,gap=P.ROW_GAP,visible=function() return model.ussd_reply:get().open==true end,
          field("mobile-ussd-answer",inner,"Reply",draft.ussd_reply),
          P.buttons {width=inner,
            {id="mobile-ussd-respond",label="Reply",enabled=function() return not busy() end,
              on_clicked=function() model.ussd_respond(draft.ussd_reply:get()) draft.ussd_reply:set("") end},
            {id="mobile-ussd-cancel",label="End",on_clicked=model.ussd_cancel}}},
      },
      P.section {id="mobile-messages",width=w,title="Messages",visible=model.available,
        list(w,"mobile-sms-",function() return #model.messages:get() end,function(i,rw)
          local function s() return model.messages:get()[i] or {} end
          return P.row {id="mobile-sms-row-"..i,width=rw,icon="sms",on=function() return false end,
            title=function() return s().number or "" end,subtitle=function() return s().text or "" end,
            trailing=P.button {id="mobile-sms-delete-"..i,label="Delete",
              on_clicked=function() if s().path then model.delete_message(s().path) end end}}
        end,"No messages loaded",24),
        field("mobile-sms-to",inner,"Phone number",draft.sms_to),
        field("mobile-sms-text",inner,"Message",draft.sms_text),
        P.buttons {width=inner,
          {id="mobile-sms-load",icon="refresh",label="Load messages",on_clicked=model.load_messages},
          {id="mobile-sms-send",label="Send",tone="primary",enabled=function() return not busy() end,
            on_clicked=function()
              if model.send_message(draft.sms_to:get(),draft.sms_text:get()) then draft.sms_text:set("") end
            end}},
      },
      missing(w,"signal_cellular_nodata","No modem","No mobile modem is available. This page updates when one is detected.",model.available),
    },
  }
end

--- The id of the row that shows `row` of `model`'s list.
function M.row_id(model, row)
  for i, r in ipairs(model.list()) do
    if r == row or (r.path ~= nil and r.path == row.path) then
      return (model.wireless and "network" or "bluetooth") .. "-row-" .. i
    end
  end
end

function M.build(model, w, h)
  if model.key=="mobile" then return mobile_page(model,w,h) end
  return model.wireless and network_page(model, w, h) or bluetooth_page(model, w, h)
end
return M
