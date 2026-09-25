-- The bar down the left edge, drawn over the frame's wide side: the OS logo,
-- the workspaces, the active window's title turned on its side, the clock
-- stacked hour over minute over am/pm, the status icons and the power
-- button.
--
-- Measured off the reference at 1920x1080: 60 px wide, content centred at
-- x = 30; pills 40 wide; workspace slots 36 apart; status icons 30 apart.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local services = require("services")
local shapes = require("lib.m3shapes")

local M = {}

local C = theme.color

-- ------------------------------------------------------------------ logo --

-- The distribution's logo from the Nerd Font set, by /etc/os-release's ID;
-- Tux when the distribution has none there.
local LOGOS = {
  arch = "\u{f303}", nixos = "\u{f313}", debian = "\u{f306}", ubuntu = "\u{f31b}",
  fedora = "\u{f30a}", opensuse = "\u{f314}", gentoo = "\u{f30d}", void = "\u{f32e}",
  manjaro = "\u{f312}", endeavouros = "\u{f322}", alpine = "\u{f300}", linuxmint = "\u{f30e}",
  cachyos = "\u{f303}", pop = "\u{f32a}",
}

local function os_id()
  local text = morf.fs.read("/etc/os-release") or ""
  return (text:match("\nID=\"?([%w_%-]+)") or text:match("^ID=\"?([%w_%-]+)") or "linux"):lower()
end

local function logo()
  return kit.centred(40, 30, ui.Text {
    text = LOGOS[os_id()] or "\u{f31a}",
    font_family = theme.mono,
    font_size = 18,
    color = function() return C.tertiary end,
  })
end

-- ------------------------------------------------------------ workspaces --

local SLOT = 36
local PILL_PAD = 2

local function workspaces()
  local shown = config.get("bar.workspaces.shown")
  -- The group of `shown` the active workspace is in: 1-5, 6-10, ...
  local function first()
    local active = services.workspace.active()
    return math.floor((active - 1) / shown) * shown + 1
  end
  -- The pill is one distance field: a dot per workspace, and the active
  -- workspace's disc, which rolls from dot to dot on a spring, squashing
  -- and stretching as it goes and melting into each dot it passes (a
  -- smooth union: the dots bulge towards it and pinch off behind).
  local grow = kit.spring(420, 24)
  local layers = {}
  local slots = {}
  for i = 1, shown do
    local id = function() return first() + i - 1 end
    local centre = PILL_PAD + (i - 1) * SLOT + SLOT / 2
    local function size() return services.workspace.occupied(id()) and 10 or 7 end
    layers[#layers + 1] = ui.SdfShape {
      id = "workspace-dot-" .. i,
      shape = "circle",
      operation = i == 1 and "union" or "smooth_union",
      x = function() return 20 - size() / 2 end,
      y = function() return centre - size() / 2 end,
      width = size, height = size,
      fill_color = function()
        return services.workspace.occupied(id()) and C.onSurfaceVariant or C.outlineVariant
      end,
      behavior = { x = grow, y = grow, width = grow, height = grow, fill_color = { duration = theme.duration.small } },
    }
    slots[#slots + 1] = ui.MouseArea {
      id = "workspace-slot-" .. i,
      width = 40, height = SLOT, cursor = "pointer",
      on_clicked = function() services.workspace.go(id()) end,
    }
  end
  local indicator = ui.Item {
    id = "workspace-active",
    x = 4, width = 32, height = 32,
    y = function()
      local active = services.workspace.active()
      return PILL_PAD + (active - first()) * SLOT + (SLOT - 32) / 2
    end,
    behavior = { y = kit.spring(230, 19) },
    stretch = kit.STRETCH,
    -- The flower rolls as the disc does: a quarter turn per workspace.
    ui.Path {
      anchors = { center_in = true }, width = 21, height = 21,
      view_box = { 0, 0, 100, 100 },
      d = shapes.path("flower"),
      fill_color = function() return C.onPrimary end,
      rotation = function() return services.workspace.active() * 90 end,
      behavior = { rotation = kit.spring(160, 16) },
    },
  }
  layers[#layers + 1] = ui.SdfShape {
    id = "workspace-active-shape",
    shape = "box", radius = 16,
    operation = "smooth_union",
    track = indicator,
    fill_color = function() return C.primary end,
  }
  -- The dots and the disc melt into one another only while the disc
  -- travels; at rest each is crisp.
  local rolling = morf.signal("caelestia.workspaces.rolling", false)
  local still
  local last = services.workspace.active()
  morf.effect("caelestia.workspaces.rolling", function()
    local now = services.workspace.active()
    if now == last then return end
    last = now
    rolling:set(true)
    if still then still:cancel() end
    still = morf.timer(420, function() still = nil rolling:set(false) end, false)
  end)
  layers.id = "workspace-field"
  layers.anchors = { fill = true }
  layers.blend = function() return rolling:get() and 11 or 0 end
  layers.behavior = { blend = { duration = 220, easing = theme.ease.standard } }
  return ui.Rect {
    id = "workspaces",
    width = 40,
    height = shown * SLOT + 2 * PILL_PAD,
    radius = 20,
    color = function() return C.surfaceContainer end,
    ui.Sdf(layers),
    indicator,
    ui.Column { y = PILL_PAD, gap = 0, table.unpack(slots) },
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_wheel = function(_, _, _, _, _, step_y)
        if step_y ~= 0 then services.workspace.step(step_y > 0 and 1 or -1) end
      end,
    },
  }
end

-- ----------------------------------------------------------- window title --

local function window_title()
  -- Laid out along the bar, then turned a quarter clockwise: reads top to
  -- bottom, like the reference.
  local LONG = 36 -- characters, the room between the workspaces and the clock
  local label = kit.text {
    id = "window-title",
    text = function()
      local title = services.window.title()
      if utf8.len(title) and utf8.len(title) > LONG then
        title = title:sub(1, utf8.offset(title, LONG) - 1) .. "…"
      end
      return title
    end,
    font_size = 15,
    letter_spacing = 1.8,
    color = function() return C.primary end,
  }
  local turned = ui.Item {
    width = 20,
    height = function() return label.layout_width or 0 end,
    ui.Item {
      anchors = { center_in = true },
      width = function() return label.layout_width or 0 end,
      height = 20,
      rotation = 90,
      label,
    },
  }
  return ui.Column {
    align = "center", gap = 10,
    kit.icon("desktop_windows", 18, function() return C.primary end),
    turned,
  }
end

-- ------------------------------------------------------------------ clock --

local function clock()
  local function part(fmt)
    return kit.text {
      horizontal_alignment = "center",
      width = 40,
      height = 17,
      font_size = 16,
      line_height = "17px",
      color = function() return C.tertiary end,
      text = function()
        morf.minute_clock:get()
        return morf.time.format(fmt)
      end,
    }
  end
  local twelve = config.get("bar.clock.twelve_hour")
  return ui.Column {
    id = "clock", align = "center", gap = 0,
    kit.centred(40, 26, kit.icon("calendar_month", 18, function() return C.tertiary end)),
    part(twelve and "%I" or "%H"),
    part("%M"),
    twelve and kit.text {
      horizontal_alignment = "center", width = 40, height = 17,
      font_size = 15, line_height = "17px",
      color = function() return C.tertiary end,
      text = function() morf.minute_clock:get() return morf.time.format("%p"):lower() end,
    } or nil,
  }
end

-- ---------------------------------------------------------------- status --

local function status()
  -- Each icon opens its popout on hover (popouts.lua).
  local popouts = require("popouts")
  local POPOUT = { ["status-network"] = "network", ["status-bluetooth"] = "bluetooth", ["status-power"] = "power" }
  local function slot(name, id)
    -- The icon swells under the pointer, on a spring with a little bounce.
    local icon = kit.icon(name, 18, function() return C.secondary end)
    local area = popouts.trigger(POPOUT[id], kit.centred(40, 30, icon), { id = id, width = 40, height = 30 })
    local swell = ui.Item {
      anchors = { fill = true },
      scale = function() return area.hovered and 1.15 or 1 end,
      behavior = { scale = kit.spring(460, 14) },
    }
    ui.reparent(swell, area)
    ui.reparent(icon, swell)
    return area
  end
  return ui.Rect {
    id = "status",
    width = 40, height = 106, radius = 20,
    color = function() return C.surfaceContainer end,
    ui.Column {
      y = 8, gap = 0,
      slot(services.network.icon, "status-network"),
      slot(services.bluetooth.icon, "status-bluetooth"),
      slot(services.power.icon, "status-power"),
    },
  }
end

local function power()
  return kit.hover(ui.MouseArea {
    id = "power", width = 40, height = 40, cursor = "pointer",
    on_clicked = function() require("session").drawer.toggle() end,
    kit.icon("power_settings_new", 20, function() return C.error end, { anchors = { center_in = true } }),
  }, function(hovered) return hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end, 20)
end

-- -------------------------------------------------------------------- bar --

function M.build()
  return ui.Item {
    id = "bar",
    width = theme.BAR,
    anchors = { top = true, bottom = true, left = true },
    -- Anywhere on the bar keeps an open popout open.
    require("popouts").area { anchors = { fill = true }, z = -1 },
    ui.Flex {
      anchors = { fill = true },
      direction = "column",
      align = "center",
      padding = 0,
      ui.Item { width = 1, height = 10 },
      logo(),
      ui.Item { width = 1, height = 7 },
      workspaces(),
      ui.Item { width = 1, height = 1, layout = { grow = 1 } },
      window_title(),
      ui.Item { width = 1, height = 1, layout = { grow = 1 } },
      clock(),
      ui.Item { width = 1, height = 15 },
      status(),
      ui.Item { width = 1, height = 3 },
      power(),
      ui.Item { width = 1, height = 10 },
    },
  }
end

return M
