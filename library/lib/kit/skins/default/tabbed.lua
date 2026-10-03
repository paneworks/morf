-- A panel with tabs for the default kit: the view switcher (`kit.tabs`)
-- along the top, the pages under it, the chosen one shown and fading in
-- as the tab changes (instantly with motion reduced).
--
--     kit.tabbed { id = "prefs", width = 430, height = function() return h end,
--       tabs = { { key = "general", name = "General", icon = "tune",
--                  build = function(w, h) return node end }, ... } }
--     -> { content, tab, select(key), showing(key), shown(open), pages }
local morf = require("morf")
local ui = require("morf.ui")

return function(theme, M, spec)
  local W, PAD, TABS_H = spec.width, spec.pad or 11, spec.tabs_height or 64
  local tabs = spec.tabs
  local tab = spec.tab or morf.signal("kit.default." .. spec.id .. ".tab", 1)
  local page_w = W - 2 * PAD
  local function page_h()
    local h = type(spec.height) == "function" and spec.height() or spec.height or 400
    return h - TABS_H - 2 * PAD
  end
  local panel = { tab = tab, tabs = tabs }
  function panel.index(key)
    for i, t in ipairs(tabs) do if t.key == key then return i end end
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
  local pages = {}
  for i, t in ipairs(tabs) do
    pages[i] = ui.Item { id = spec.id .. "-page-" .. t.key, width = page_w, height = page_h,
      visible = function() return tab:get() == i end, t.build(page_w, page_h) }
  end
  local strip = ui.Item { id = spec.id .. "-pages", x = PAD, y = TABS_H + PAD, width = page_w, height = page_h,
    clip = true, table.unpack(pages) }
  local was, running = tab:get(), nil
  morf.effect("kit.default." .. spec.id .. ".page", function()
    local now = tab:get()
    if now == was then return end
    was = now
    if running then for _, h in ipairs(running) do h:stop() end end
    running = M.bud({ pages[now] }, true, { from = 0.99 })
    if spec.on_tab then spec.on_tab(tabs[now].key) end
  end)
  function panel.shown(open)
    if running then for _, h in ipairs(running) do h:stop() end end
    running = M.bud({ pages[tab:get()] }, open, { from = 0.99 })
  end
  panel.content = ui.Item { anchors = { fill = true },
    ui.MouseArea { anchors = { fill = true }, z = -1 },
    M.tabs { id = spec.id, accessible_name = spec.title or spec.id, tabs = tabs, tab = tab, width = W, pad = PAD,
      height = TABS_H },
    strip,
  }
  panel.pages = pages
  return panel
end
