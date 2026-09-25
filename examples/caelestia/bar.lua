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
  local slots = {}
  for i = 1, shown do
    local id = function() return first() + i - 1 end
    slots[#slots + 1] = ui.Item {
      id = "workspace-slot-" .. i,
      width = 40, height = SLOT,
      ui.Rect {
        anchors = { center_in = true },
        width = function() return services.workspace.occupied(id()) and 10 or 8 end,
        height = function() return services.workspace.occupied(id()) and 10 or 8 end,
        radius = 5,
        color = function()
          return services.workspace.occupied(id()) and C.onSurfaceVariant or C.outlineVariant
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function() services.workspace.go(id()) end,
      },
    }
  end
  -- The active one: a disc that slides from slot to slot, with a cookie
  -- cut into it.
  local indicator = ui.Item {
    id = "workspace-active",
    x = 4, width = 32, height = 32,
    y = function()
      local active = services.workspace.active()
      return PILL_PAD + (active - first()) * SLOT + (SLOT - 32) / 2
    end,
    behavior = { y = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel } },
    ui.Rect { anchors = { fill = true }, radius = 16, color = function() return C.primary end },
    ui.Path {
      anchors = { center_in = true }, width = 21, height = 21,
      view_box = { 0, 0, 100, 100 },
      d = shapes.path("cookie9"),
      fill_color = function() return C.onPrimary end,
    },
  }
  return ui.Rect {
    id = "workspaces",
    width = 40,
    height = shown * SLOT + 2 * PILL_PAD,
    radius = 20,
    color = function() return C.surfaceContainer end,
    ui.Column { y = PILL_PAD, gap = 0, table.unpack(slots) },
    indicator,
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
    font_size = theme.size.normal,
    letter_spacing = 1.2,
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
    align = "center", gap = 6,
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
      font_size = theme.size.larger,
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
      font_size = theme.size.normal, line_height = "17px",
      color = function() return C.tertiary end,
      text = function() morf.minute_clock:get() return morf.time.format("%p"):lower() end,
    } or nil,
  }
end

-- ---------------------------------------------------------------- status --

local function status()
  local function slot(name, id)
    return kit.centred(40, 30, kit.icon(name, 18, function() return C.secondary end), { id = id })
  end
  return ui.Rect {
    id = "status",
    width = 40, height = 96, radius = 20,
    color = function() return C.surfaceContainer end,
    ui.Column {
      y = 3, gap = 0,
      slot(services.network.icon, "status-network"),
      slot(services.bluetooth.icon, "status-bluetooth"),
      slot(services.power.icon, "status-power"),
    },
  }
end

local function power()
  return kit.hover(ui.MouseArea {
    id = "power", width = 40, height = 40, cursor = "pointer",
    on_clicked = function()
      -- TODO(phase 2): the session drawer on the right edge.
      morf.log("info", "caelestia: power pressed (the session menu is not ported yet)")
    end,
    kit.icon("power_settings_new", 20, function() return C.error end, { anchors = { center_in = true } }),
  }, function(hovered) return hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end, 20)
end

-- -------------------------------------------------------------------- bar --

function M.build()
  return ui.Item {
    id = "bar",
    width = theme.BAR,
    anchors = { top = true, bottom = true, left = true },
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
      ui.Item { width = 1, height = 20 },
      status(),
      ui.Item { width = 1, height = 9 },
      power(),
      ui.Item { width = 1, height = 10 },
    },
  }
end

return M
