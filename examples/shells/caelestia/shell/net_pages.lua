-- The right panel's Wired and VPN pages, behind their tiles' ">":
--
--   Wired   every wired port NetworkManager has -- the dock's, the laptop's
--           own -- its state, address and speed, connected or not by a click
--   VPN     NetworkManager's VPN and WireGuard profiles, and the mesh VPNs
--           beside it (NetBird, Tailscale, ZeroTier: lib.vpns), each up or
--           down by a click where it can be switched from here

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local services = require("services")

local C = theme.color
local M = {}

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

--- A row card: an icon badge, a name and what it says, and a switch.
local function row_card(w, spec)
  local area
  area = ui.Item {
    id = spec.id, width = w, height = 76,
    kit.card {
      anchors = { fill = true }, radius = 20,
      color = function() return C.surfaceContainer end,
    },
    ui.Rect {
      x = 14, anchors = { vertical_center = true }, width = 44, height = 44, radius = 22,
      color = function() return spec.on() and C.primary or C.surfaceContainerHighest end,
      behavior = { color = { duration = theme.duration.small } },
      kit.icon(spec.icon, 22, function() return spec.on() and C.onPrimary or C.onSurfaceVariant end,
        { anchors = { center_in = true }, fill = true }),
    },
    ui.Column {
      x = 70, anchors = { vertical_center = true }, gap = 2,
      kit.text { text = spec.name, font_weight = 600, width = w - 170, elide = "right" },
      kit.text { text = spec.detail, font_size = theme.size.small, width = w - 170, elide = "right",
        color = function() return C.onSurfaceVariant end },
    },
  }
  if spec.toggle then
    local sw
    sw = ui.MouseArea {
      id = spec.id .. "-switch",
      anchors = { right = true, right_margin = 14, vertical_center = true },
      width = 84, height = 36, cursor = "pointer",
      visible = function() return spec.can == nil or spec.can() end,
      on_clicked = function() spec.toggle(not spec.on()) end,
      ui.Rect {
        anchors = { fill = true }, radius = 18,
        color = function()
          local base = spec.on() and C.secondaryContainer or C.primary
          return (sw and sw.hovered) and base:mix(C.onSurface, 0.06) or base
        end,
      },
      kit.text {
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
  return kit.text {
    width = w, wrap = true, font_size = theme.size.small, text = text,
    color = function() return C.onSurfaceVariant end,
  }
end

-- ------------------------------------------------------------------ wired --

function M.wired_page(w, h)
  local net = services.net
  if not (net and net.state) then
    return ui.Item { width = w, height = h, note(w, "NetworkManager is not running.") }
  end
  local STATE_WORDS = {
    activated = "Connected", disconnected = "Disconnected", unavailable = "No cable",
    prepare = "Connecting", config = "Connecting", ip_config = "Getting an address",
    need_auth = "Needs a password", deactivating = "Disconnecting", failed = "Failed",
  }
  local list = ui.Repeater {
    as = "column", gap = 10, width = w,
    model = net.state.devices,
    delegate = function(device)
      if device.type ~= "ethernet" then return ui.Item { width = 0, height = 0, visible = false } end
      local function live()
        for _, row in ipairs(net.snapshot().devices) do
          if row.path == device.path then return row end
        end
        return device
      end
      local function connected() return live().state == "activated" end
      return row_card(w, {
        id = "wired-" .. device.interface,
        icon = function() return live().carrier == false and "settings_ethernet" or "lan" end,
        name = device.interface,
        on = connected,
        detail = function()
          local d = live()
          local parts = { STATE_WORDS[d.state] or d.state }
          if d.connection ~= "" and connected() then parts[#parts + 1] = d.connection end
          if d.ip4 and d.ip4 ~= "" then parts[#parts + 1] = d.ip4 end
          if (d.speed or 0) > 0 then parts[#parts + 1] = ("%d Mb/s"):format(d.speed) end
          if d.hw_address ~= "" then parts[#parts + 1] = d.hw_address end
          return table.concat(parts, " · ")
        end,
        can = function() return live().state ~= "unavailable" end,
        toggle = function(on)
          if dry_run() then morf.log("info", "caelestia: wired " .. device.interface .. " " .. tostring(on) .. " (dry run)") return end
          if on then pcall(net.connect_device, device.interface) else pcall(net.disconnect, device.interface) end
        end,
      })
    end,
  }
  return ui.Flickable {
    width = w, height = h, clip = true,
    ui.Column { gap = 12, width = w, list,
      note(w, "Every wired port NetworkManager looks after: a laptop's own, a dock's, a Thunderbolt link.") },
  }
end

-- -------------------------------------------------------------------- vpn --

--- Whether a connection is a mesh VPN's own tunnel.
function M.is_mesh_link(name) return require("lib.vpns").is_mesh_link(name) end

local NAMES = { netbird = "NetBird", tailscale = "Tailscale", zerotier = "ZeroTier",
  mullvad = "Mullvad", protonvpn = "Proton VPN" }
local KIND_TOOLS = { mesh = { "netbird", "tailscale", "zerotier" }, tunnel = { "mullvad", "protonvpn" } }

--- The page for one kind of VPN: "mesh" (NetBird, Tailscale, ZeroTier) or
--- "tunnel" (Mullvad, Proton VPN, and NetworkManager's VPN and WireGuard
--- profiles -- where Proton's app keeps its connections).
function M.vpn_page(kind, w, h, detail)
  local vpns = require("lib.vpns")
  local shown = function() return detail:get() == kind end
  local rows = vpns.rows[kind]
  local holding = false
  -- Started and stopped a moment after the page comes and goes, outside
  -- the effect: what an effect makes is the effect's.
  morf.effect("caelestia.vpn.watch." .. kind, function()
    local on = shown()
    morf.timer(1, function()
      if on and not holding then
        holding = true
        vpns.watch(kind, 8000)
      elseif not on and holding then
        holding = false
        vpns.release(kind)
      end
    end, false)
  end)

  local nodes = { gap = 10, width = w }
  local net = services.net
  if kind == "tunnel" and net and net.state then
    nodes[#nodes + 1] = kit.text { text = "NetworkManager", font_size = theme.size.small,
      color = function() return C.onSurfaceVariant end,
      visible = function()
        local model = net.state.vpn_connections
        for i = 1, model:len() do
          if not M.is_mesh_link(model:get(i).id) then return true end
        end
        return false
      end }
    nodes[#nodes + 1] = ui.Repeater {
      as = "column", gap = 10, width = w,
      model = net.state.vpn_connections,
      delegate = function(v)
        -- A mesh VPN's own tunnel (netbird0, tailscale0, zt*) is listed by
        -- NetworkManager too: it is the Mesh page's, and switching it off
        -- here would pull it from under the mesh's own daemon.
        if M.is_mesh_link(v.id) then return ui.Item { width = 0, height = 0, visible = false } end
        return row_card(w, {
          id = "vpn-nm-" .. tostring(v.uuid),
          icon = v.type == "wireguard" and "vpn_lock" or "vpn_key",
          name = v.id,
          on = function() return v.active == true end,
          detail = function() return (v.type == "wireguard" and "WireGuard" or "VPN") .. " · " .. (v.active and "Connected" or "Off") end,
          toggle = function(on)
            if dry_run() then morf.log("info", "caelestia: vpn " .. v.id .. " " .. tostring(on) .. " (dry run)") return end
            if on then pcall(net.activate_vpn, v.uuid) else pcall(net.deactivate_vpn, v.uuid) end
          end,
        })
      end,
    }
    nodes[#nodes + 1] = kit.text { text = "Apps", font_size = theme.size.small,
      color = function() return C.onSurfaceVariant end }
  end
  for _, id in ipairs(KIND_TOOLS[kind]) do
    local function row()
      for _, r in ipairs(rows:get()) do if r.id == id then return r end end
      return nil
    end
    local card = row_card(w, {
      id = "vpn-" .. id,
      icon = kind == "mesh" and "hub" or "shield",
      name = NAMES[id],
      on = function() local r = row() return r ~= nil and r.up end,
      detail = function()
        local r = row()
        if not r then return "Reading…" end
        return r.detail .. (r.address ~= "" and (" · " .. r.address) or "")
      end,
      can = function() local r = row() return r ~= nil and r.can_toggle end,
      on_word = kind == "mesh" and "Up" or "Connect", off_word = kind == "mesh" and "Down" or "Disconnect",
      toggle = function(on)
        if dry_run() then morf.log("info", "caelestia: " .. id .. " " .. tostring(on) .. " (dry run)") return end
        vpns.set(id, on)
      end,
    })
    -- Only the ones installed, once they have been looked for.
    card.visible = function() return #rows:get() == 0 or row() ~= nil end
    nodes[#nodes + 1] = card
  end
  nodes[#nodes + 1] = note(w, kind == "mesh"
    and "ZeroTier's own command needs root to list its networks: it is shown by its link, and switched with zerotier-cli."
    or "Proton VPN's app keeps its connections in NetworkManager, above; its command-line client, when installed, is here.")
  return ui.Flickable { width = w, height = h, clip = true, ui.Column(nodes) }
end

return M
