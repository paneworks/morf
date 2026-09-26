-- The capture drawer: out of the frame's bottom edge, as the dashboard
-- comes out of its top, with tabs (tabbed.lua) -- one for now, Capture:
-- side by side, the screenshot card -- a region, a window or the whole
-- screen, after a delay if asked, saved and copied -- and the screen
-- recorder (utilities.lua's card) with its recordings. It opens when the pointer reaches the bottom edge, and over
-- IPC (`capture`, `screenshot [region|window|screen]`).
--
-- What each capture runs is a setting (`utilities.commands.screenshot_*`),
-- run through utilities.run, so CAELESTIA_DRY_RUN=1 logs it instead.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local drawer = require("drawer")
local tabbed = require("tabbed")
local utilities = require("utilities")

local C = theme.color
local M = {}

local CARD_W = 408
M.WIDTH = 2 * CARD_W + 12 + 2 * tabbed.PAD
local SHOT_H = 212
local GAP = 12

M.delay = morf.signal("caelestia.capture.delay", 0)

local d -- the drawer, below

--- Takes a screenshot of `what` ("region", "window" or "screen"): the
--- drawer shuts first, so it is not in the picture, then after the delay
--- the command runs.
function M.shoot(what)
  if d then d.set(false) end
  local wait = 250 + (M.delay:get() or 0) * 1000
  morf.timer(wait, function() utilities.run("screenshot_" .. what) end, false)
  return what
end

local KINDS = {
  { id = "region", name = "Region", icon = "screenshot_region" },
  { id = "window", name = "Window", icon = "select_window" },
  { id = "screen", name = "Screen", icon = "fullscreen" },
}
local DELAYS = { { s = 0, name = "Now" }, { s = 3, name = "3 s" }, { s = 5, name = "5 s" } }

--- A big button: a rounded square that squares up under the finger.
local function big(id, icon, label, w, on_clicked)
  local area
  local shape = ui.Rect {
    anchors = { fill = true },
    behavior = { radius = ui.spring { stiffness = 420, damping = 26 }, color = { duration = theme.duration.small } },
  }
  area = ui.MouseArea {
    id = id, width = w, height = 96, cursor = "pointer",
    on_clicked = on_clicked,
    shape,
    ui.Column {
      anchors = { center_in = true }, gap = 6, align = "center",
      kit.icon(icon, 30, function() return C.onSecondaryContainer end),
      kit.text { text = label, font_size = theme.size.normal, color = function() return C.onSecondaryContainer end },
    },
  }
  shape.radius = function() return area.pressed and 14 or 24 end
  shape.color = function()
    local base = C.secondaryContainer
    return area.hovered and base:mix(C.onSecondaryContainer, 0.08) or base
  end
  return area
end

local function chip(t)
  local area
  local function on() return M.delay:get() == t.s end
  local shape = ui.Rect {
    anchors = { fill = true },
    behavior = { radius = kit.spring(260, 16), color = { duration = theme.duration.small } },
  }
  area = ui.MouseArea {
    id = "capture-delay-" .. t.s, width = 64, height = 32, cursor = "pointer",
    on_clicked = function() M.delay:set(t.s) end,
    shape,
    kit.text {
      anchors = { center_in = true }, text = t.name, font_size = theme.size.small,
      color = function() return on() and C.onPrimary or C.onSurface end,
    },
  }
  shape.radius = function() return on() and 8 or 16 end
  shape.color = function()
    local base = on() and C.primary or C.surfaceContainerHighest
    return area.hovered and base:mix(on() and C.onPrimary or C.onSurface, 0.08) or base
  end
  return area
end

local function screenshot_page(w)
  local bw = (w - 2 * GAP - 2 * 16) / 3
  local buttons = {}
  for _, k in ipairs(KINDS) do
    buttons[#buttons + 1] = big("capture-" .. k.id, k.icon, k.name, bw, function() M.shoot(k.id) end)
  end
  local chips = {}
  for _, t in ipairs(DELAYS) do chips[#chips + 1] = chip(t) end
  return kit.card {
    id = "capture-screenshot",
    width = w, height = SHOT_H,
    ui.Row { x = 16, y = 16, gap = GAP, table.unpack(buttons) },
    ui.Item {
      x = 16, y = 16 + 96 + 16, width = w - 32, height = 32,
      kit.text {
        anchors = { vertical_center = true }, text = "Delay", font_size = theme.size.normal,
        color = function() return C.onSurfaceVariant end,
      },
      ui.Row { x = 56, gap = 6, table.unpack(chips) },
    },
    kit.pill {
      id = "capture-folder",
      x = 16, y = SHOT_H - 16 - 32, width = w - 32,
      icon = "folder_open", label = "Open screenshots",
      on_clicked = function() utilities.run("screenshots_folder") end,
    },
  }
end

M.TABS = {
  { key = "capture", name = "Capture", icon = "screenshot_monitor",
    build = function()
      return ui.Row {
        gap = GAP,
        screenshot_page(CARD_W),
        ui.Item { width = CARD_W, height = function() return utilities.recorder.height() end, utilities.recorder.node },
      }
    end },
}

local function page_h() return math.max(SHOT_H, utilities.recorder.height()) end
function M.height() return tabbed.TABS_H + 2 * tabbed.PAD + page_h() end

local panel = tabbed.new { id = "capture", width = M.WIDTH, height = M.height, tabs = M.TABS }
M.panel = panel
M.select = panel.select

d = drawer.new {
  name = "capture",
  edge = "bottom",
  width = M.WIDTH,
  height = M.height,
  content = panel.content,
}
M.drawer = d

morf.effect("caelestia.capture.bud", function()
  local open = d.open:get()
  panel.shown(open)
  utilities.recorder.shown(open)
end)

return M
