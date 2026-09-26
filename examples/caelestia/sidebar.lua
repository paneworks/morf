-- The sidebar: a drawer the frame's full height down its right edge, with
-- tabs (tabbed.lua): Settings (utilities.lua: the sliders, the quick
-- settings tiles with their pages -- network, Bluetooth, sound -- power,
-- keep awake, the recorder) and Notifications -- the history grouped by
-- application, the newest group on top, each with its count, opening out
-- to every notification in it with a dismiss and a copy button, a button
-- that clears them all, and "All up to date!" when there are none. While
-- the notifications are on show no popups drop in. More tabs are one more
-- entry in `TABS`.
--
-- Measured off the reference at 1920x1080: 430 wide, from the frame's top
-- edge down to the utilities; the history is a surfaceContainerLow card
-- 408 wide, 16 from the left and 6 from the top, 17 above a hairline in
-- outlineVariant on the drawer's foot; its title 16 in; groups 384 wide,
-- 12 in, 8 apart, 68 tall with one line (22 more for each), rounded 17,
-- a 42 px round icon; the clear button 54 square, 24 in from the card's
-- corner.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local drawer = require("drawer")
local notifs = require("notifs")
local utilities = require("utilities")

local C = theme.color
local M = {}

-- The pages, and the strip on the near side the level pills ride out to.
local WIDTH = theme.SIDE_W + theme.STRIP
local LEFT, TOP = 16, 6
local CARD_W = 408
local ROW_W = 384
local LINE = 22
local GROUPS = 8        -- groups on show at most
local LINES = 4         -- lines of a group shut, cards of one opened
local ITEM_H = 112      -- a notification in an opened group

local function screen_height()
  morf.screens_revision()
  local s = morf.screens[1]
  return (s and s.height) or 1080
end

--- The drawer's height: the frame's opening, top to bottom.
function M.height()
  return screen_height() - 2 * theme.BORDER
end
local tabbed = require("tabbed")
local function page_h() return M.height() - tabbed.TABS_H - 2 * tabbed.PAD end

-- ----------------------------------------------------------------- groups --

local function plain(text)
  return (tostring(text or ""):gsub("<[^>]->", ""):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("&amp;", "&"))
end

--- The history by application, the group with the newest notification
--- first, each group's newest first: `{ app, items, urgency }`.
function M.groups()
  local list = notifs.history:get()
  local out, by = {}, {}
  for i = #list, 1, -1 do
    local n = list[i]
    local app = n.app ~= "" and n.app or "Unknown"
    local g = by[app]
    if not g then
      g = { app = app, items = {}, urgency = 0 }
      by[app] = g
      out[#out + 1] = g
    end
    g.items[#g.items + 1] = n
    if (n.urgency or 1) > g.urgency then g.urgency = n.urgency or 1 end
  end
  return out
end

local opened = morf.signal("caelestia.sidebar.opened", {})
local function is_open(app) return opened:get()[app] == true end
local function flip(app)
  local t = {}
  for k, v in pairs(opened:get()) do t[k] = v end
  t[app] = not t[app] or nil
  opened:set(t)
end

--- How long ago, as the reference says it: "now", "5m", "2h", "3d".
local function ago(t)
  morf.minute_clock:get()
  local s = math.max(0, morf.time.now() - (t or 0))
  if s < 60 then return "now" end
  if s < 3600 then return ("%dm"):format(s // 60) end
  if s < 86400 then return ("%dh"):format(s // 3600) end
  return ("%dd"):format(s // 86400)
end

local function group_height(g)
  if not g then return 0 end
  local n = math.min(#g.items, LINES)
  if is_open(g.app) then return 40 + n * ITEM_H + (n - 1) * 8 + 12 end
  return 12 + LINE + n * LINE + 12
end

-- One line of a shut group: the summary, then the body, cut to fit.
local function line(n_of, k, width)
  local summary = kit.text {
    text = function() local n = n_of() return n and plain(n.summary) or "" end,
    font_size = theme.size.larger, font_weight = 500,
  }
  local body = kit.text {
    text = function() local n = n_of() return n and plain(n.body) or "" end,
    font_size = theme.size.larger, elide = "right",
    width = function() return math.max(0, width - (summary.layout_width or 0) - 8) end,
    color = function() return C.onSurfaceVariant end,
  }
  return ui.Row {
    x = 66, y = 12 + LINE + (k - 1) * LINE, height = LINE, gap = 8, align = "center",
    visible = function() return n_of() ~= nil end,
    summary, body,
  }
end

-- A notification of an opened group: its summary and time, its body, and
-- a dismiss and a copy button.
local function item(n_of, k, tag)
  local function button(icon, id, action)
    return kit.hover(ui.MouseArea {
      id = id, width = 139, height = 30, cursor = "pointer",
      on_clicked = function() local n = n_of() if n then action(n) end end,
      kit.icon(icon, 18, function() return C.onSurface end, { anchors = { center_in = true } }),
    }, function(hovered)
      return hovered and C.surfaceContainerHighest:mix(C.onSurface, 0.08) or C.surfaceContainerHighest
    end, 15)
  end
  return ui.Rect {
    x = 66, y = 40 + (k - 1) * (ITEM_H + 8), width = ROW_W - 66 - 12, height = ITEM_H,
    radius = 12, color = function() return C.surfaceContainerHigh end,
    visible = function() return n_of() ~= nil end,
    kit.text {
      x = 10, y = 10, width = 240, elide = "right",
      text = function() local n = n_of() return n and plain(n.summary) or "" end,
      font_size = theme.size.larger,
    },
    kit.text {
      anchors = { right = true, right_margin = 10 }, y = 10,
      text = function() local n = n_of() return n and ago(n.time) or "" end,
      font_size = theme.size.larger, color = function() return C.onSurfaceVariant end,
    },
    kit.text {
      x = 10, y = 34, width = 286, elide = "right",
      text = function() local n = n_of() return n and plain(n.body) or "" end,
      font_size = theme.size.larger, color = function() return C.onSurfaceVariant end,
    },
    ui.Row {
      x = 10, y = 68, gap = 8,
      button("close", "sidebar-dismiss-" .. tag, function(n) notifs.forget(n.id) end),
      button("content_copy", "sidebar-copy-" .. tag, function(n)
        pcall(morf.clipboard.set, plain(n.body ~= "" and n.body or n.summary))
      end),
    },
  }
end

local rows = {}
local function group_row(i)
  local function g() return M.groups()[i] end
  local function critical() local x = g() return x ~= nil and x.urgency == 2 end
  local lines, items = {}, {}
  for k = 1, LINES do
    local function n_of()
      local x = g()
      return x and x.items[k] or nil
    end
    lines[k] = line(n_of, k, ROW_W - 66 - 16)
    items[k] = item(n_of, k, i .. "-" .. k)
  end
  local shut = ui.Item { anchors = { fill = true }, visible = function() local x = g() return x ~= nil and not is_open(x.app) end, table.unpack(lines) }
  local open = ui.Item { anchors = { fill = true }, visible = function() local x = g() return x ~= nil and is_open(x.app) end, table.unpack(items) }
  local row
  row = ui.Rect {
    id = "sidebar-group-" .. i,
    width = ROW_W, radius = 17, clip = true,
    height = function() return group_height(g()) end,
    behavior = { height = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel } },
    visible = function() return g() ~= nil end,
    color = function() return C.surfaceContainer end,
    ui.Rect {
      x = 12, y = 12, width = 42, height = 42, radius = 21,
      color = function() return critical() and C.error or C.secondaryContainer end,
      kit.icon(function() return critical() and "release_alert" or "chat" end, 22,
        function() return critical() and C.onError or C.onSecondaryContainer end,
        { anchors = { center_in = true } }),
    },
    kit.text {
      id = "sidebar-group-app-" .. i,
      x = 66, y = 12, height = LINE,
      text = function() local x = g() return x and x.app or "" end,
      font_size = theme.size.larger,
    },
    kit.text {
      anchors = { right = true, right_margin = 58 }, y = 12, height = LINE,
      text = function() local x = g() return x and x.items[1] and ago(x.items[1].time) or "" end,
      font_size = theme.size.larger, color = function() return C.onSurfaceVariant end,
    },
    kit.hover(ui.MouseArea {
      id = "sidebar-group-expand-" .. i,
      anchors = { right = true, right_margin = 17 }, y = 12, width = 38, height = 24, cursor = "pointer",
      on_clicked = function()
        local x = g()
        if not x then return end
        flip(x.app)
        -- Opening, its notifications come in evenly, one after the other.
        if is_open(x.app) then kit.bud(items, true, { delay = 30, stagger = 40 }) end
      end,
      ui.Row {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.text {
          text = function() local x = g() return x and tostring(#x.items) or "" end,
          font_size = theme.size.small,
          color = function() return critical() and C.onError or C.onSurface end,
        },
        kit.icon(function() local x = g() return (x and is_open(x.app)) and "expand_less" or "expand_more" end, 16,
          function() return critical() and C.onError or C.onSurface end),
      },
    }, function(hovered)
      local base = critical() and C.error or C.surfaceContainerHigh
      return hovered and base:mix(C.onSurface, 0.08) or base
    end, 12),
    shut,
    open,
  }
  return row
end
for i = 1, GROUPS do rows[i] = group_row(i) end

-- ------------------------------------------------------------------ panel --

local function pane_height() return page_h() end

local list = ui.Item {
  x = 12, y = 51, width = ROW_W,
  height = function() return pane_height() - 51 end,
  clip = true,
  ui.Column { gap = 8, table.unpack(rows) },
}

local empty = ui.Column {
  id = "sidebar-empty",
  anchors = { horizontal_center = true },
  y = function() return math.max(40, pane_height() / 2 - 69) end,
  gap = 22, align = "center",
  visible = function() return #notifs.history:get() == 0 end,
  -- The reference draws a picture here; the port a quiet mark of its own.
  ui.Item {
    width = 300, height = 130,
    ui.Rect {
      anchors = { left = true, right = true, bottom = true, bottom_margin = 12 }, height = 2, radius = 1,
      color = function() return C.outlineVariant end,
    },
    kit.icon("notifications_paused", 96, function() return C.outlineVariant end, {
      anchors = { horizontal_center = true, bottom = true, bottom_margin = 16 },
    }),
  },
  kit.text {
    id = "sidebar-empty-label",
    text = "All up to date!", font_size = theme.size.extra, font_weight = 500,
    color = function() return C.outlineVariant end,
  },
}

--- Clears the history: the groups fade and shrink a touch, one after the
--- other, then go.
local clearing
local clear
function M.clear()
  if clearing then return end
  local shown = {}
  for i = 1, GROUPS do if rows[i].visible then shown[#shown + 1] = rows[i] end end
  if #shown == 0 then notifs.clear() return end
  -- The button goes with them, last.
  shown[#shown + 1] = clear
  kit.bud(shown, false, { leave_stagger = 35 })
  clearing = morf.timer(170 + 35 * (#shown - 1), function()
    clearing = nil
    notifs.clear()
    for _, r in ipairs(shown) do r.scale, r.opacity = 1, 1 end
  end, false)
end

clear = kit.hover(ui.MouseArea {
  id = "sidebar-clear",
  anchors = { right = true, bottom = true, right_margin = 24, bottom_margin = 24 },
  width = 54, height = 54, cursor = "pointer",
  visible = function() return #notifs.history:get() > 0 end,
  on_clicked = M.clear,
  -- A soft shadow under it, as the reference's floating button.
  ui.Rect {
    anchors = { fill = true }, z = -2, radius = 14,
    color = function() return C.primary end,
    shadow_color = "#00000070", shadow_blur = 8,
  },
  kit.icon("clear_all", 26, function() return C.onPrimary end, { anchors = { center_in = true } }),
}, function(hovered) return hovered and C.primary:mix(C.onPrimary, 0.08) or C.primary end, 14)

local pane = ui.Rect {
  id = "sidebar-history",
  width = CARD_W, height = pane_height,
  radius = utilities.RADIUS,
  color = function() return C.surfaceContainerLow end,
  kit.text {
    id = "sidebar-title",
    x = 16, y = 16,
    text = function()
      local n = #notifs.history:get()
      if n == 0 then return "Notifications" end
      return ("%d notification%s"):format(n, n == 1 and "" or "s")
    end,
    font_size = theme.size.large,
    color = function() return C.onSurfaceVariant end,
  },
  list,
  empty,
  clear,
}

M.TABS = {
  { key = "settings", name = "Settings", icon = "tune", build = utilities.page },
  { key = "notifications", name = "Notifications", icon = "notifications", build = function() return pane end },
}

local panel = tabbed.new {
  id = "sidebar", width = theme.SIDE_W, height = M.height, tabs = M.TABS,
  on_tab = function(key)
    utilities.shown(key == "settings")
    if key == "notifications" then
      local shown = {}
      for i = 1, GROUPS do shown[#shown + 1] = rows[i] end
      kit.bud(shown, true, { delay = 90, stagger = 30 })
    end
  end,
}
M.panel = panel
M.tab = panel.tab
M.select = panel.select
M.showing = panel.showing

M.drawer = drawer.new {
  name = "sidebar",
  edge = "right",
  width = WIDTH,
  height = M.height,
  content = ui.Item { anchors = { fill = true, left_margin = theme.STRIP }, panel.content },
  props = { anchors = { top = true, right = true } },
}

--- A click on the desk shuts the sidebar: an invisible catcher under the
--- panels, there only while it is open.
function M.catcher()
  return ui.MouseArea {
    id = "sidebar-catcher",
    anchors = { fill = true },
    visible = function() return M.drawer.open:get() end,
    on_clicked = function() M.drawer.set(false) end,
  }
end

-- No popups drop in while the history is on show.
morf.effect("caelestia.sidebar.covered", function()
  notifs.covered:set(M.drawer.open:get() and panel.showing("notifications"))
end)

-- Opening, the chosen page comes in, the settings' cards one after the
-- other, the notifications' groups likewise.
local was = false
local running = {}
morf.effect("caelestia.sidebar.bud", function()
  local open = M.drawer.open:get()
  if open == was then return end
  was = open
  for _, h in ipairs(running) do h:stop() end
  running = {}
  if panel.showing("settings") then
    utilities.shown(open)
  elseif open then
    local shown = {}
    for i = 1, GROUPS do shown[#shown + 1] = rows[i] end
    running = kit.bud(shown, true, { delay = 90, stagger = 30 })
  end
  panel.shown(open)
end)

-- A notification arriving while the sidebar is open buds in at the top;
-- the last one going, "All up to date!" comes in the same way.
local count = #notifs.history:get()
morf.effect("caelestia.sidebar.arrival", function()
  local now = #notifs.history:get()
  if M.drawer.open:get() then
    if now > count then
      running[#running + 1] = kit.bud({ rows[1] }, true, { delay = 0 })[1]
    elseif now == 0 and count > 0 then
      running[#running + 1] = kit.bud({ empty }, true, { delay = 0 })[1]
    end
  end
  count = now
end)

return M
