-- Material notification history; grouping, expansion and actions stay shared.
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
local plain = require("themes.notification_text").plain
local ago = require("themes.notification_text").ago
function V.build(state, width, height)
local CARD_W, ROW_W, LINE, GROUPS, LINES, ITEM_H = 408, 384, 22, 8, 4, 112
local function group_height(g)
  if not g then return 0 end
  local n = math.min(#g.items, LINES)
  if state.is_open(g.app) then return 40 + n * ITEM_H + (n - 1) * 8 + 12 end
  return 12 + LINE + n * LINE + 12
end

-- One line of a shut group: the summary, then the body, cut to fit.
local function line(n_of, k, width, tag, shown)
  local summary = kit.heading { id = "sidebar-summary-" .. tag, scope = "sidebar.notifications", level = "caption", visible = shown,
    text = function() local n = n_of() return n and plain(n.summary) or "" end,
    font_size = theme.size.larger, font_weight = 500,
  }
  local body = kit.subtitle {
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
local function item(n_of, k, tag, shown)
  local function button(icon, id, action)
    return kit.hover(kit.action {
      id = id, width = 139, height = 30, cursor = "pointer",
      on_clicked = function() local n = n_of() if n then action(n) end end,
      kit.icon(icon, 18, function() return C.onSurface end, { anchors = { center_in = true } }),
    }, function(hovered)
      return hovered and C.surfaceContainerHighest:mix(C.onSurface, 0.08) or C.surfaceContainerHighest
    end, 15)
  end
  return kit.surface {
    x = 66, y = 40 + (k - 1) * (ITEM_H + 8), width = ROW_W - 66 - 12, height = ITEM_H,
    radius = 12, color = function() return C.surfaceContainerHigh end,
    visible = function() return n_of() ~= nil end,
    kit.heading { id = "sidebar-item-title-" .. tag, scope = "sidebar.notifications", level = "caption", visible = shown,
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
      button("close", "sidebar-dismiss-" .. tag, function(n) state.forget(n.id) end),
      button("content_copy", "sidebar-copy-" .. tag, function(n)
        state.copy(n)
      end),
    },
  }
end

local rows = {}
local function group_row(i)
  local function g() return state.groups()[i] end
  local function critical() local x = g() return x ~= nil and x.urgency == 2 end
  local lines, items = {}, {}
  for k = 1, LINES do
    local function n_of()
      local x = g()
      return x and x.items[k] or nil
    end
    lines[k] = line(n_of, k, ROW_W - 66 - 16, i .. "-" .. k, function()
      local x = g()
      return n_of() ~= nil and x ~= nil and not state.is_open(x.app)
    end)
    items[k] = item(n_of, k, i .. "-" .. k, function()
      local x = g()
      return n_of() ~= nil and x ~= nil and state.is_open(x.app)
    end)
  end
  local shut = ui.Item { anchors = { fill = true }, visible = function() local x = g() return x ~= nil and not state.is_open(x.app) end, table.unpack(lines) }
  local open = ui.Item { anchors = { fill = true }, visible = function() local x = g() return x ~= nil and state.is_open(x.app) end, table.unpack(items) }
  local row
  row = kit.surface {
    id = "sidebar-group-" .. i,
    width = ROW_W, radius = 17, clip = true,
    height = function() return group_height(g()) end,
    behavior = { height = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel } },
    visible = function() return g() ~= nil end,
    color = function() return C.surfaceContainer end,
    kit.surface {
      x = 12, y = 12, width = 42, height = 42, radius = 21,
      color = function() return critical() and C.error or C.secondaryContainer end,
      kit.icon(function() return critical() and "release_alert" or "chat" end, 22,
        function() return critical() and C.onError or C.onSecondaryContainer end,
        { anchors = { center_in = true } }),
    },
    kit.heading { scope = "sidebar.notifications", level = "section", visible = function() return g() ~= nil end,
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
    kit.hover(kit.action {
      id = "sidebar-group-expand-" .. i,
      anchors = { right = true, right_margin = 17 }, y = 12, width = 38, height = 24, cursor = "pointer",
      on_clicked = function()
        local x = g()
        if not x then return end
        state.toggle(x.app)
        -- Opening, its notifications come in evenly, one after the other.
        if state.is_open(x.app) then kit.bud(items, true, { delay = 30, stagger = 40 }) end
      end,
      ui.Row {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.text {
          text = function() local x = g() return x and tostring(#x.items) or "" end,
          font_size = theme.size.small,
          color = function() return critical() and C.onError or C.onSurface end,
        },
        kit.icon(function() local x = g() return (x and state.is_open(x.app)) and "expand_less" or "expand_more" end, 16,
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



local list = ui.Item {
  x = 12, y = 51, width = ROW_W,
  height = function() return height() - 51 end,
  clip = true,
  ui.Column { gap = 8, table.unpack(rows) },
}

local empty = ui.Column {
  id = "sidebar-empty",
  anchors = { horizontal_center = true },
  y = function() return math.max(40, height() / 2 - 69) end,
  gap = 22, align = "center",
  visible = function() return state.count() == 0 end,
  -- The reference draws a picture here; the port a quiet mark of its own.
  ui.Item {
    width = 300, height = 130,
    kit.surface {
      anchors = { left = true, right = true, bottom = true, bottom_margin = 12 }, height = 2, radius = 1,
      color = function() return C.outlineVariant end,
    },
    kit.icon("notifications_paused", 96, function() return C.outlineVariant end, {
      anchors = { horizontal_center = true, bottom = true, bottom_margin = 16 },
    }),
  },
  kit.heading { scope = "sidebar.notifications", visible = function() return state.count() == 0 end,
    id = "sidebar-empty-label",
    text = "All up to date!", font_size = theme.size.extra, font_weight = 500,
    color = function() return C.outlineVariant end,
  },
}

--- Clears the history: the groups fade and shrink a touch, one after the
--- other, then go.
local clearing = false
local clear
local running, leaving = {}, {}
local function stop(handles)
  for _, handle in ipairs(handles) do handle:stop() end
end
local function animate_clear()
  clearing = true
  stop(running) running = {}
  local shown = {}
  for _, row in ipairs(rows) do if row.visible then shown[#shown + 1] = row end end
  if #shown == 0 then return 0 end
  shown[#shown + 1] = clear
  leaving = kit.bud(shown, false, { leave_stagger = 35 })
  return 170 + 35 * (#shown - 1)
end
local function reset_clear()
  stop(leaving) leaving = {}
  clearing = false
  for _, row in ipairs(rows) do row.scale, row.opacity = 1, 1 end
  clear.scale, clear.opacity = 1, 1
end

clear = kit.hover(kit.action {
  id = "sidebar-clear",
  anchors = { right = true, bottom = true, right_margin = 24, bottom_margin = 24 },
  width = 54, height = 54, cursor = "pointer",
  visible = function() return state.count() > 0 end,
  on_clicked = state.clear,
  -- A soft shadow under it, as the reference's floating button.
  kit.surface {
    anchors = { fill = true }, z = -2, radius = 14,
    color = function() return C.primary end,
    shadow_color = "#00000070", shadow_blur = 8,
  },
  kit.icon("clear_all", 26, function() return C.onPrimary end, { anchors = { center_in = true } }),
}, function(hovered) return hovered and C.primary:mix(C.onPrimary, 0.08) or C.primary end, 14)

local pane = kit.surface {
  id = "sidebar-history",
  width = CARD_W, height = height,
  radius = 15,
  color = function() return C.surfaceContainerLow end,
  kit.heading { scope = "sidebar.notifications",
    id = "sidebar-title",
    x = 16, y = 16,
    text = function()
      local n = state.count()
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

local was, count = false, state.count()
morf.effect("material.history.presentation", function()
  local active, now = state.active(), state.count()
  if not clearing then
    if active ~= was then
      stop(running) running = {}
      if active then running = kit.bud(rows, true, { delay = 90, stagger = 30 }) end
    elseif active and now ~= count then
      stop(running)
      running = kit.bud({ now == 0 and empty or rows[1] }, true, { delay = 0 })
    end
  end
  was, count = active, now
end, { owner = pane })
return { node = pane, animate_clear = animate_clear, reset_clear = reset_clear }
end
return V
