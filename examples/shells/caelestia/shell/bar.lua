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
-- `edgebar.enabled`: "on", "off", or "auto" (on for a narrow screen -- a phone
-- -- and off for a desk). `edgebar.side`: top, bottom, left or right. The
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
  local wanted = config.get("edgebar.enabled")
  if wanted == "on" or wanted == true then return true end
  if wanted == "off" or wanted == false then return false end
  local w = screen()
  return w < 1000
end

--- Which edge it is on.
function M.side()
  local side = config.get("edgebar.side")
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
function M.set_on(on) config.set("edgebar.enabled", on and "on" or "off") end
function M.set_side(side) config.set("edgebar.side", side) end

-- ------------------------------------------------------------------ parts --

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
-- A phone's mobile network: bars by its signal, or off.
local function mobile_icon()
  local m = services.modem
  -- No modem: the bars crossed out, so the place is always there.
  if not m then return "signal_cellular_nodata" end
  local s = m.state
  if s.locked then return "signal_cellular_connected_no_internet_0_bar" end
  if not s.registered then return "signal_cellular_off" end
  local q = s.signal or 0
  if q > 80 then return "signal_cellular_4_bar" end
  if q > 55 then return "signal_cellular_3_bar" end
  if q > 30 then return "signal_cellular_2_bar" end
  if q > 10 then return "signal_cellular_1_bar" end
  return "signal_cellular_0_bar"
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
  local ITEM = 34

  -- The logo: the author's mark, in the theme's colour. A click opens the
  -- launcher.
  local mark = require("logo")
  local function logo(suffix)
    return button("bar-logo" .. suffix, ITEM, function()
      local ok, launcher = pcall(require, "launcher")
      if ok then launcher.drawer.set(not launcher.drawer.open:get()) end
    end, ui.Item {
      anchors = { center_in = true }, width = 24, height = 24,
      ui.Path {
        x = 24 * mark.offset_x / 100, y = 0, width = 24 * mark.scale_x, height = 24,
        view_box = mark.view_box, d = mark.d,
        fill_color = function() return C.primary end,
      },
    })
  end

  -- --------------------------------------------------------- windows --
  -- A button per window of the workspace on show, after Dash to Panel: the
  -- app's icon (and its title, when `bar.titles` is on and the bar lies
  -- along), a pill behind the focused one, a light behind the one under
  -- the pointer, and a mark on the bar's inner edge -- wide for the focused
  -- window, a dot for the rest. A click focuses it.
  local function titles() return config.get("edgebar.titles") ~= "off" and not M.vertical() end
  local function icon_of(client)
    local hit = apps.icon((client.class or ""):lower()) or apps.icon(client.class or "")
      or apps.icon((client.initial_class or ""):lower())
    if hit and hit.name then
      return ui.Icon { width = 22, height = 22, name = hit.name, source_width = 44, source_height = 44 }
    end
    if hit and hit.path then
      return ui.Image { width = 22, height = 22, source = hit.path, fill_mode = "preserve_aspect_fit" }
    end
    return kit.icon("select_window", 22, function() return C.onSurfaceVariant end)
  end
  local TITLE_W = 140
  local function window_delegate(client)
    local function here() return client.workspace == services.workspace.active() and not client.hidden end
    local function focused() return hyprland.state.active_window.address == client.address end
    local function wide() return titles() and ITEM + 8 + TITLE_W or ITEM + 6 end
    local area
    area = ui.MouseArea {
      id = "bar-window-" .. tostring(client.address),
      cursor = "pointer",
      width = function() return here() and wide() or 0 end,
      height = function() return here() and ITEM or 0 end,
      visible = here,
      on_clicked = function()
        pcall(hyprland.dispatch, "focuswindow", "address:" .. tostring(client.address))
      end,
      ui.Rect {
        anchors = { fill = true }, radius = 10,
        color = function()
          if focused() then return C.primary:alpha(0.16) end
          if area and area.hovered then return C.onSurface:alpha(0.07) end
          return C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      ui.Row {
        x = 7, anchors = { vertical_center = true }, gap = 8, align = "center",
        icon_of(client),
        kit.text {
          visible = titles, width = TITLE_W - 4, elide = "right",
          font_size = theme.size.small, font_weight = 500,
          text = function() return client.title ~= "" and client.title or client.class end,
          color = function() return focused() and C.onSurface or C.onSurfaceVariant end,
        },
      },
      -- The mark on the bar's inner edge.
      ui.Rect {
        height = 3, radius = 2,
        width = function() return focused() and 18 or 5 end,
        x = function() return (wide() - (focused() and 18 or 5)) / 2 end,
        y = function() return M.side() == "bottom" and 0 or ITEM - 3 end,
        color = function() return focused() and C.primary or C.onSurfaceVariant:alpha(0.7) end,
        behavior = { width = { duration = theme.duration.small }, x = { duration = theme.duration.small } },
      },
    }
    return area
  end
  local function windows(as)
    if not hyprland then return ui.Item {} end
    return ui.Repeater { as = as, gap = 6, model = hyprland.state.clients, delegate = window_delegate }
  end

  -- ----------------------------------------------------------- status --
  -- The status icons say, and do nothing on their own: any of them opens
  -- the quick settings, at their top.
  local function open_settings()
    local ok, s = pcall(require, "sidebar")
    if ok then s.drawer.set(true) end
    require("utilities").detail:set("")
  end
  local function status(id, icon_fn)
    return ui.Item {
      id = id, width = ITEM - 6, height = ITEM - 6,
      kit.icon(icon_fn, 20, function() return C.onSurface end, { anchors = { center_in = true }, fill = true }),
    }
  end
  local function tail(vertical)
    local v = vertical and "-v" or ""
    local nodes = { gap = 2, align = "center" }
    -- Tor, while it runs.
    local onion = status("bar-tor" .. v, function() return "travel_explore" end)
    onion.visible = function() local t = services.tor return t ~= nil and t.on() end
    nodes[#nodes + 1] = onion
    -- The ring mode, while it is not sound.
    local ring = status("bar-ringer" .. v, function()
      local r = services.ringer
      return require("lib.ringer").icon(r and r.state.mode or "sound")
    end)
    ring.visible = function() local r = services.ringer return r ~= nil and r.state.mode ~= "sound" end
    nodes[#nodes + 1] = ring
    -- A phone's mobile network, with its generation; crossed out without one.
    nodes[#nodes + 1] = status("bar-mobile" .. v, mobile_icon)
    if services.modem then
      if not vertical then
        nodes[#nodes + 1] = kit.text {
          font_size = theme.size.small - 2, font_weight = 700,
          text = function() local m = services.modem return m and m.state.technology or "" end,
        }
      end
    end
    for _, n in ipairs {
      status("bar-network" .. v, network_icon),
      status("bar-battery" .. v, battery_icon),
    } do nodes[#nodes + 1] = n end
    if not vertical then
      nodes[#nodes + 1] = kit.text {
        font_size = theme.size.small, font_weight = 600,
        visible = function() return battery() ~= nil end,
        text = function() local b = battery() return b and ("%d%%"):format(math.floor(b.percentage + 0.5)) or "" end,
      }
    end
    -- One segment, one click: all of it opens the quick settings.
    local row = (vertical and ui.Column or ui.Row)(nodes)
    local area
    area = ui.MouseArea {
      id = "bar-status" .. v, cursor = "pointer",
      width = function() return vertical and ITEM or (row.layout_width or 160) + 16 end,
      height = function() return vertical and (row.layout_height or 160) + 16 or ITEM end,
      on_clicked = open_settings,
      ui.Rect {
        anchors = { fill = true }, radius = 10,
        color = function()
          local s = package.loaded["sidebar"]
          if s and s.drawer and s.drawer.open and s.drawer.open:get() then return C.primary:alpha(0.16) end
          return (area and area.hovered) and C.onSurface:alpha(0.07) or C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      ui.Item { anchors = { center_in = true },
        width = function() return row.layout_width or 0 end,
        height = function() return row.layout_height or 0 end,
        row },
    }
    return area, row
  end

  -- ------------------------------------------------------------ time --
  -- The date and the time, in the middle; a click opens the dashboard, and
  -- another shuts it.
  local stamp = morf.signal("caelestia.bar.clock", { "", "" })
  local function tick()
    local t = morf.time.now()
    stamp:set({ morf.time.format("%H:%M", t), morf.time.format("%a %-d %b", t) })
  end
  tick()
  morf.timer(1000, tick, true)
  local function toggle_dashboard()
    local d = require("dashboard").drawer
    d.set(not d.open:get())
  end
  local function clock(vertical)
    local area
    area = ui.MouseArea {
      id = "bar-clock" .. (vertical and "-v" or ""), cursor = "pointer",
      width = vertical and ITEM + 4 or 190, height = vertical and 64 or ITEM,
      on_clicked = toggle_dashboard,
      ui.Rect {
        anchors = { fill = true }, radius = 10,
        color = function()
          if require("dashboard").drawer.open:get() then return C.primary:alpha(0.16) end
          return (area and area.hovered) and C.onSurface:alpha(0.07) or C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      vertical and ui.Column {
        anchors = { center_in = true }, gap = 0, align = "center",
        kit.text { text = function() return (stamp:get()[1]:sub(1, 2)) end, font_weight = 700 },
        kit.text { text = function() return (stamp:get()[1]:sub(4, 5)) end, font_weight = 700 },
        kit.text { text = function() return (morf.time.format("%d", morf.time.now())) end,
          font_size = theme.size.small - 3, color = function() return C.onSurfaceVariant end },
      } or ui.Row {
        anchors = { center_in = true }, gap = 10, align = "center",
        kit.text { text = function() return stamp:get()[2] end, font_size = theme.size.small,
          color = function() return C.onSurfaceVariant end },
        kit.text { text = function() return stamp:get()[1] end, font_weight = 700 },
      },
    }
    return area
  end

  -- ---------------------------------------------------------- layout --
  local function strip()
    local w, h = screen()
    local i = M.insets()
    local side = M.side()
    if side == "top" then return 0, 0, w, i.top + theme.BORDER end
    if side == "bottom" then return 0, h - i.bottom - theme.BORDER, w, i.bottom + theme.BORDER end
    if side == "left" then return 0, 0, i.left + theme.LEFT, h end
    return w - i.right - theme.BORDER, 0, i.right + theme.BORDER, h
  end
  -- Well in from the frame's corners.
  local PAD = 34
  local head_h = ui.Row { gap = 10, align = "center", logo(""), windows("row") }
  local tail_h, row_h = tail(false)
  local head_v = ui.Column { gap = 8, align = "center", logo("-v"), windows("column") }
  local tail_v, row_v = tail(true)
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
      ui.Item { anchors = { center_in = true }, width = 190, height = ITEM, clock(false) },
      ui.Item {
        anchors = { right = true, right_margin = PAD, vertical_center = true }, height = ITEM,
        width = function() return (row_h.layout_width or 160) + 16 end,
        tail_h,
      },
    },
    ui.Item {
      visible = function() return M.vertical() end,
      anchors = { fill = true },
      ui.Item { y = PAD, height = 1, anchors = { horizontal_center = true }, width = ITEM, head_v },
      ui.Item { anchors = { center_in = true }, width = ITEM + 4, height = 64, clock(true) },
      ui.Item {
        anchors = { bottom = true, bottom_margin = PAD, horizontal_center = true }, width = ITEM,
        height = function() return (row_v.layout_height or 160) + 16 end,
        tail_v,
      },
    },
  }
end

return M
