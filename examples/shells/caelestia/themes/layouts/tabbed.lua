-- A panel with tabs, as the dashboard has them: a row of icons with their
-- labels along the top, the chosen one in the primary colour over an
-- indicator that stretches from tab to tab (kit.elastic) and melts into
-- the hairline under the row while it travels; under it the pages side by
-- side, sliding across as the tab changes, the incoming page growing in
-- evenly as it fades in.
--
-- The side panels are built on it, so a tab is one more entry in a list:
--
--     local panel = tabbed.new {
--       id = "sidebar", width = 430, height = function() return h end,
--       tabs = {
--         { key = "settings", name = "Settings", icon = "tune",
--           build = function(w, h) return node end },   -- h is a function
--         ...
--       },
--     }
--     panel.content  -- the drawer's content
--     panel.tab      -- a signal: the chosen tab's index
--     panel.select("settings")

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")

local C = theme.color
local M = {}

M.TABS_H = 64
M.PAD = 11
local SWITCH = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }

function M.new(spec)
  local W, PAD, TABS_H = spec.width, spec.pad or M.PAD, M.TABS_H
  local tabs = spec.tabs
  local tab = spec.tab or require("themes.session").keep("caelestia." .. spec.id .. ".tab", 1)
  local display = morf.signal("caelestia." .. spec.id .. ".displayed", tab:get())
  local presented = morf.signal("caelestia." .. spec.id .. ".presented", false)
  morf.effect(spec.id .. ".presentation", function()
    for i, t in ipairs(tabs) do
      if spec.publish ~= false then
        require("presentation").set(spec.id .. "." .. t.key, presented:get() and display:get() == i)
      end
    end
  end)
  local page_w = W - 2 * PAD
  local function page_h() return spec.height() - TABS_H - 2 * PAD end
  local panel = { tab = tab, tabs = tabs, displayed = display }

  function panel.index(key)
    for i, t in ipairs(tabs) do if t.key == key then return i end end
    return nil
  end
  function panel.select(key)
    local i = type(key) == "number" and key or panel.index(key)
    if i and tabs[i] then tab:set(i) end
    return i
  end
  function panel.showing(key)
    local t = tabs[tab:get()]
    return t ~= nil and t.key == key
  end

  -- ---------------------------------------------------------------- tabs --

  local function native_tabs()
  local slot = page_w / #tabs
  local labels, buttons = {}, {}
  for i, t in ipairs(tabs) do
    local function on() return tab:get() == i end
    labels[i] = kit.text {
      text = t.name, font_size = theme.size.small,
      color = function() return on() and C.primary or C.onSurface end,
      behavior = { color = { duration = theme.duration.small } },
    }
    local button
    button = kit.action {
      id = spec.id .. "-tab-" .. t.key,
      x = PAD + (i - 1) * slot, y = 4, width = slot, height = TABS_H - 8, cursor = "pointer",
      on_clicked = function() tab:set(i) end,
      kit.surface {
        anchors = { fill = true, top_margin = 2, bottom_margin = 2 }, radius = 10,
        color = function() return button and button.hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0) end,
        behavior = { color = { duration = theme.duration.small } },
      },
      ui.Column {
        anchors = { horizontal_center = true }, y = 6, gap = 3, align = "center",
        t.icon_build and t.icon_build(on, spec.id .. "-tab-" .. t.key .. "-icon")
          or kit.icon(t.icon, 22, function() return on() and C.primary or C.onSurface end, { fill = on }),
        labels[i],
      },
    }
    buttons[i] = button
  end

  -- The indicator under the chosen label, as wide as it; moving to another
  -- tab it stretches out ahead and draws itself in there.
  local function span(i)
    local l = labels[i]
    local w = (l and l.layout_width or 60) + 4
    local x = PAD + (i - 1) * slot + (slot - w) / 2
    return x, x + w
  end
  local indicator = ui.Item { id = spec.id .. "-tab-indicator", y = TABS_H - 4, height = 3, x = 0, width = 0 }
  local moving = morf.signal("caelestia." .. spec.id .. ".indicator.moving", false)
  local still
  local shown, moved = tab:get(), false
  morf.effect("caelestia." .. spec.id .. ".indicator", function()
    local now = tab:get()
    local l1, r1 = span(now)
    if now == shown then
      if not moved then indicator.x, indicator.width = l1, r1 - l1 end
      return
    end
    local l0, r0 = span(shown)
    shown, moved = now, true
    kit.elastic(indicator, "x", l0, r0, l1, r1, { duration = 520 })
    moving:set(true)
    if still then still:cancel() end
    still = morf.timer(560, function() still = nil moving:set(false) end, false)
  end)
  local foot = ui.Sdf {
    id = spec.id .. "-tab-field",
    x = 0, y = TABS_H - 8, width = W, height = 8,
    blend = function() return theme.motion.liquid_cards ~= false and moving:get() and 4 or 0 end,
    behavior = { blend = { duration = 200 } },
    ui.SdfShape {
      shape = "box", operation = "union",
      x = PAD, width = page_w, y = 7, height = 1,
      fill_color = function() return C.outlineVariant end,
    },
    ui.SdfShape {
      shape = "box", operation = "smooth_union",
      top_left_radius = 1.5, top_right_radius = 1.5,
      track = indicator,
      fill_color = function() return C.primary end,
    },
  }

    return ui.Item {anchors={fill=true},ui.Item {anchors={fill=true},table.unpack(buttons)},indicator,foot}
  end

  -- --------------------------------------------------------------- pages --

  local pages = {}
  for i, t in ipairs(tabs) do
    pages[i] = ui.Item {
      id = spec.id .. "-page-" .. t.key,
      width = page_w, height = page_h,
      x=theme.motion.page_wipe and (i-1)*(page_w+2*PAD) or nil,
      visible=function() return not theme.motion.page_wipe or display:get()==i end,
      t.build(page_w, page_h),
    }
  end
  local track = (theme.motion.page_wipe and ui.Item or ui.Row) {
    width=theme.motion.page_wipe and #tabs*(page_w+2*PAD) or nil,height=page_h,
    gap = not theme.motion.page_wipe and 2 * PAD or nil,
    translate_x = function() return -(display:get() - 1) * (page_w + 2 * PAD) end,
    behavior = theme.motion.page_wipe and {} or { translate_x = SWITCH },
    table.unpack(pages),
  }
  local strip = ui.Item {
    id = spec.id .. "-pages",
    x = PAD, y = TABS_H + PAD, width = page_w, height = page_h,
    clip = true,
    track,
  }

  local wipe = theme.motion.page_wipe and theme.motion.page_wipe(strip, function() return page_w end, spec.id)
  local function present(index)
    display:set(index)
    if spec.on_present then spec.on_present(index) end
  end

  -- The incoming page grows in evenly as it fades in.
  local running
  local was = tab:get()
  morf.effect("caelestia." .. spec.id .. ".page", function()
    local now = tab:get()
    if now == was then return end
    was = now
    if running then for _, h in ipairs(running) do h:stop() end end
    local function enter()
      present(now)
      running = kit.bud({ pages[now] }, true, { from = 0.97, delay = 60 })
    end
    if wipe then wipe(enter, not presented:get()) else enter() end
    if spec.on_tab then spec.on_tab(tabs[now].key) end
  end)

  --- Called as the panel opens (`true`) or shuts: the chosen page buds in.
  local shown
  function panel.shown(open)
    -- This is called from an effect. Reading tab below also subscribes that
    -- effect to selection: an unchanged open state must not cancel its wipe.
    if shown == open then return end
    shown = open
    presented:set(open)
    if wipe then wipe(function() present(tab:get()) end, true) else present(tab:get()) end
    if running then for _, h in ipairs(running) do h:stop() end end
    if wipe then
      for _,page in ipairs(pages) do page.opacity,page.translate_x,page.translate_y=1,0,0 end
    else running = kit.bud({ pages[tab:get()] }, open, { from = 0.97 }) end
  end

  panel.content = ui.Item {
    anchors = { fill = true },
    -- Behind everything, so the whole panel takes the pointer.
    ui.MouseArea { anchors = { fill = true }, z = -1 },
    kit.tabs and kit.tabs {id=spec.id,tabs=tabs,tab=tab,width=W,pad=PAD,height=TABS_H}
      or native_tabs(),
    strip,
  }
  panel.pages = pages
  return panel
end

return M
