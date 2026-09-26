-- The bar: a strip along one edge of the frame -- the distro's logo, the
-- windows of the workspace on show, and the network, sound, battery and the
-- time -- for a phone, where there is no rail to hover and no room to spare,
-- and for a desk that wants one.
--
-- It is the frame's own edge grown thick: the opening in the frame draws
-- back from that side by the bar's thickness, windows are kept out of it
-- (the reserve), and everything else in the shell -- the drawers, the rail,
-- the level pills -- lives in the desk that is left (`M.desk()`), so it stays
-- on the opening's edge wherever the bar is.
--
-- `bar.enabled`: "on", "off", or "auto" (on for a narrow screen -- a phone
-- -- and off for a desk). `bar.side`: top, bottom, left or right. The
-- quick settings' Bar tile toggles it; its ">" chooses the side.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local services = require("services")

local C = theme.color
local M = {}

M.THICK = 40          -- across a top or bottom bar
M.WIDE = 48           -- across a left or right one
M.SIDES = { "top", "bottom", "left", "right" }

local function screen()
  morf.screens_revision()
  local s = morf.screens[1]
  return (s and s.width) or 1920, (s and s.height) or 1080
end

--- Whether the bar is up.
function M.on()
  local wanted = config.get("bar.enabled")
  if wanted == "on" or wanted == true then return true end
  if wanted == "off" or wanted == false then return false end
  local w = screen()
  return w < 1000
end

--- Which edge it is on.
function M.side()
  local side = config.get("bar.side")
  for _, s in ipairs(M.SIDES) do if s == side then return side end end
  return "top"
end

function M.vertical() return M.side() == "left" or M.side() == "right" end

--- What the bar takes from each side of the screen: `{ left, top, right,
--- bottom }`, all zero while it is down.
function M.insets()
  local out = { left = 0, top = 0, right = 0, bottom = 0 }
  if M.on() then out[M.side()] = M.vertical() and M.WIDE or M.THICK end
  return out
end

--- The desk: the screen less the bar. `x, y, width, height`.
function M.desk()
  local w, h = screen()
  local i = M.insets()
  return i.left, i.top, w - i.left - i.right, h - i.top - i.bottom
end

--- Sets it up or down.
function M.set_on(on) config.set("bar.enabled", on and "on" or "off") end
function M.set_side(side) config.set("bar.side", side) end

-- ------------------------------------------------------------------ parts --

local DISTRO = {
  arch = "\u{f303}", debian = "\u{f306}", ubuntu = "\u{f31b}", fedora = "\u{f30a}", nixos = "\u{f313}",
  manjaro = "\u{f312}", opensuse = "\u{f314}", ["opensuse-tumbleweed"] = "\u{f314}", gentoo = "\u{f30d}",
  void = "\u{f32e}", endeavouros = "\u{f322}", linuxmint = "\u{f30e}", pop = "\u{f32a}", alpine = "\u{f300}",
  artix = "\u{f31f}", centos = "\u{f304}", elementary = "\u{f309}", kali = "\u{f327}", raspbian = "\u{f315}",
}
local LOGO_FONT = "Iosevka Nerd Font"

local function distro_glyph()
  local ok, text = pcall(morf.fs.read, "/etc/os-release")
  local id = ok and type(text) == "string" and (text:match("\nID=\"?([%w%-]+)") or text:match("^ID=\"?([%w%-]+)")) or ""
  return DISTRO[id] or "\u{f31a}" -- tux
end

local function button(id, size, on_clicked, child, hint)
  local area
  area = ui.MouseArea {
    id = id, width = size, height = size, cursor = "pointer",
    on_clicked = on_clicked,
    ui.Rect {
      anchors = { fill = true }, radius = size / 2,
      color = function()
        return (area and area.hovered) and C.onSurface:alpha(0.08) or C.onSurface:alpha(0)
      end,
      behavior = { color = { duration = theme.duration.small } },
    },
    child,
  }
  return area
end

-- The network, as an icon: Wi-Fi by its signal, the wire, or none.
local function network_icon()
  local n = services.net
  if n and n.state.available then
    local model = n.state.access_points
    for i = 1, model:len() do
      local ap = model:get(i)
      if ap.in_use then
        local s = ap.strength or 0
        if s > 75 then return "network_wifi" end
        if s > 50 then return "network_wifi_3_bar" end
        if s > 25 then return "network_wifi_2_bar" end
        return "network_wifi_1_bar"
      end
    end
    if n.state.wired then return "lan" end
    return n.state.wifi_enabled and "wifi_find" or "wifi_off"
  end
  return "wifi"
end
local function volume_icon()
  local ok, sink = pcall(function() return morf.audio.available() and morf.audio.default_sink() end)
  if not (ok and sink) then return "volume_up" end
  if sink.muted or sink.volume <= 0 then return "volume_off" end
  return sink.volume < 0.5 and "volume_down" or "volume_up"
end
local function battery()
  local u = services.upower
  local d = u and u.state.available and u.state.display or {}
  return d.present and d or nil
end
local function battery_icon()
  local b = battery()
  if not b then return "power" end
  if b.charging then return "battery_charging_full" end
  local pct = b.percentage or 0
  if pct > 90 then return "battery_full" end
  if pct > 60 then return "battery_5_bar" end
  if pct > 30 then return "battery_3_bar" end
  if pct > 10 then return "battery_1_bar" end
  return "battery_alert"
end

-- ------------------------------------------------------------------ build --

function M.build()
  local apps = require("apps")
  local hyprland = services.hyprland
  local function inner() return M.vertical() and M.WIDE or M.THICK end
  local ITEM = 32

  -- The windows of the workspace on show, by their icons; the focused one
  -- lit. A click brings one forward.
  local function icon_of(client)
    local hit = apps.icon((client.class or ""):lower()) or apps.icon(client.class or "")
      or apps.icon(client.initial_class or "")
    if hit and hit.name then return ui.Icon { anchors = { center_in = true }, width = 22, height = 22, name = hit.name, source_width = 44, source_height = 44 } end
    if hit and hit.path then return ui.Image { anchors = { center_in = true }, width = 22, height = 22, source = hit.path, fill_mode = "preserve_aspect_fit" } end
    return kit.icon("select_window", 20, function() return C.onSurfaceVariant end, { anchors = { center_in = true } })
  end
  local function window_delegate(client)
      local function here()
        return client.workspace == services.workspace.active() and not client.hidden
      end
      local function focused()
        return hyprland.state.active_window.address == client.address
      end
      local b = button("bar-window-" .. tostring(client.address), ITEM, function()
        pcall(hyprland.dispatch, "focuswindow", "address:" .. tostring(client.address))
      end, icon_of(client))
      return ui.Item {
        width = function() return here() and ITEM or 0 end,
        height = function() return here() and ITEM or 0 end,
        visible = here,
        b,
        ui.Rect {
          anchors = { horizontal_center = true, bottom = true }, width = 12, height = 3, radius = 2,
          visible = focused, color = function() return C.primary end,
        },
      }
  end
  local function windows(as)
    if not hyprland then return ui.Item {} end
    return ui.Repeater { as = as, gap = 2, model = hyprland.state.clients, delegate = window_delegate }
  end

  local function logo(suffix)
    return button("bar-logo" .. suffix, ITEM, function()
      local ok, launcher = pcall(require, "launcher")
      if ok then launcher.drawer.set(true) end
    end, ui.Text {
      anchors = { center_in = true }, text = distro_glyph(), font_family = LOGO_FONT, font_size = 20,
      color = function() return C.primary end,
    })
  end

  local function open_settings(detail)
    return function()
      local ok, s = pcall(require, "sidebar")
      if ok then s.drawer.set(true) end
      require("utilities").detail:set(detail)
    end
  end
  local function status(id, icon_fn, detail)
    return button(id, ITEM, open_settings(detail), kit.icon(icon_fn, 20, function() return C.onSurface end,
      { anchors = { center_in = true }, fill = true }))
  end

  local now = morf.signal("caelestia.bar.clock", "")
  local function tick() now:set(morf.time.format("%H:%M", morf.time.now())) end
  tick()
  morf.timer(1000, tick, true)
  local function clock(vertical)
    return ui.MouseArea {
      id = "bar-clock" .. (vertical and "-v" or ""), cursor = "pointer",
      width = vertical and ITEM or 56, height = vertical and 44 or ITEM,
      on_clicked = function() require("dashboard").drawer.set(true) end,
      kit.text {
        anchors = { center_in = true }, horizontal_alignment = "center",
        text = function()
          local t = now:get()
          return vertical and (t:gsub(":", "\n")) or t
        end,
        font_size = theme.size.normal, font_weight = 700,
        color = function() return C.onSurface end,
      },
    }
  end

  -- Laid along the bar: logo and windows from its start, the status and
  -- the time at its end.
  local function along(nodes)
    local props = { gap = 4, align = "center" }
    for _, n in ipairs(nodes) do props[#props + 1] = n end
    return props
  end
  local function tail(vertical)
    local v = vertical and "-v" or ""
    local nodes = {
      status("bar-network" .. v, network_icon, "network"),
      status("bar-sound" .. v, volume_icon, "sound"),
      status("bar-battery" .. v, battery_icon, "power"),
    }
    if not vertical then
      nodes[#nodes + 1] = kit.text {
        font_size = theme.size.small, font_weight = 600,
        visible = function() return battery() ~= nil end,
        text = function() local b = battery() return b and ("%d%%"):format(math.floor(b.percentage + 0.5)) or "" end,
      }
    end
    nodes[#nodes + 1] = clock(vertical)
    return (vertical and ui.Column or ui.Row)(along(nodes))
  end
  local head_h = ui.Row(along { logo(""), windows("row") })
  local tail_h = tail(false)
  local head_v = ui.Column(along { logo("-v"), windows("column") })
  local tail_v = tail(true)

  local function strip()
    local w, h = screen()
    local i = M.insets()
    local side = M.side()
    if side == "top" then return 0, 0, w, i.top + theme.BORDER end
    if side == "bottom" then return 0, h - i.bottom - theme.BORDER, w, i.bottom + theme.BORDER end
    if side == "left" then return 0, 0, i.left + theme.LEFT, h end
    return w - i.right - theme.BORDER, 0, i.right + theme.BORDER, h
  end
  local PAD = 18
  return ui.Item {
    id = "bar",
    visible = function() return M.on() end,
    x = function() local x = strip() return x end,
    y = function() local _, y = strip() return y end,
    width = function() local _, _, w = strip() return w end,
    height = function() local _, _, _, h = strip() return h end,
    ui.Item {
      visible = function() return not M.vertical() end,
      anchors = { fill = true },
      ui.Item { x = PAD, width = 1, anchors = { vertical_center = true }, height = ITEM, head_h },
      ui.Item {
        anchors = { right = true, right_margin = PAD, vertical_center = true }, height = ITEM,
        width = function() return tail_h.layout_width or 200 end,
        tail_h,
      },
    },
    ui.Item {
      visible = function() return M.vertical() end,
      anchors = { fill = true },
      ui.Item { y = PAD, height = 1, anchors = { horizontal_center = true }, width = ITEM, head_v },
      ui.Item {
        anchors = { bottom = true, bottom_margin = PAD, horizontal_center = true }, width = ITEM,
        height = function() return tail_v.layout_height or 200 end,
        tail_v,
      },
    },
  }
end

return M
