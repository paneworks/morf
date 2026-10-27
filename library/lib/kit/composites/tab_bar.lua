-- AdwTabBar: the tabs of a tab view (lib.kit.archetypes.navigation's
-- `tab_view`) as a strip, each its icon, title and a close button, and a
-- button that adds one (Selection + Drag reorder + Press).
--
--     local pages, view = widgets.tab_view { width = 520, height = 300, tabs = { ... }, on_add = new_tab }
--     local bar = composites.tab_bar { id = "docs", view = view, width = 520 }
--
-- The strip is a view switcher over the tabs (the arrows walk it; Alt+Left
-- and Alt+Right, or a drag along the strip, move the current tab).
-- `closable` (true) gives each tab its close button unless the tab says
-- otherwise; `addable` (true, when the view adds tabs) the add button;
-- `end_widget` a node at the strip's end (a tab overview's button). Ids:
-- `<id>-tabs`, `<id>-tab-<i>`, `<id>-close-<i>`, `<id>-add`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local selection = require("lib.kit.archetypes.selection")
local tokens = require("lib.kit.tokens")

return function(spec)
  spec = spec or {}
  local view = assert(spec.view, "tab_bar wants a tab view's handle (`view`)")
  local id = spec.id
  local W, BAR = spec.width or 520, spec.height or 40
  local TAB_W = spec.tab_width or 132
  local closable_default = spec.closable ~= false
  local addable = spec.addable ~= false
  local BUTTONS = (addable and 36 or 0) + (spec.end_widget and 36 or 0)
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local dim = tokens.color("window_fg", 0.55)
  local strip = selection.make("view_switcher", { id = sid("tabs"), accessible_name = spec.accessible_name or "Tabs",
    width = function() return math.max(1, #view.tabs() * (TAB_W + 2)) end, height = BAR,
    items = function()
      local out = {}
      for i, tab in ipairs(view.tabs()) do out[i] = { label = tab.title, icon = tab.icon, key = tab.key } end
      return out
    end,
    item_width = TAB_W, item_height = BAR, gap = 2, reorderable = true,
    current = function() return view.current() end,
    item_id = function(i) return sid("tab-" .. i) end,
    on_current_changed = function(i) view.select(i) end,
    on_reorder = function(index, step) view.move(index, step) end,
    delegate = function(i, item, s)
      local tab = view.tabs()[i] or {}
      local closable = (tab.closable == nil and closable_default) or tab.closable == true
      local text_x = item.icon and 36 or 12
      local look = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = 3 }, radius = 10,
          color = function()
            if s.current() then return tokens.color("window_fg")():alpha(0.1) end
            return tokens.color("window_fg")():alpha(s.hovered() and 0.05 or 0)
          end },
        widgets.label { x = text_x, anchors = { vertical_center = true }, width = TAB_W - text_x - (closable and 32 or 10),
          elide = "right", text = item.label,
          color = function() return (s.current() and tokens.color("window_fg") or dim)() end } }
      if item.icon then
        ui.reparent(widgets.image { icon = item.icon, pixel_size = 18, x = 12, anchors = { vertical_center = true },
          color = function() return (s.current() and tokens.color("accent") or dim)() end }, look)
      end
      if closable then
        ui.reparent(widgets.button { id = sid("close-" .. i), accessible_name = "Close " .. item.label,
          width = 24, height = 24, icon = "close", variant = "flat circular",
          anchors = { right = true, right_margin = 6, vertical_center = true },
          on_clicked = function() view.close(i) end }, look)
      end
      return look
    end })
  local buttons = ui.Row { x = W - BUTTONS, width = BUTTONS, height = BAR, gap = 0, align = "center" }
  if addable then
    ui.reparent(ui.Item { width = 36, height = BAR,
      widgets.button { id = sid("add"), accessible_name = "New tab", width = 32, height = 32, icon = "add",
        variant = "flat", anchors = { center_in = true }, on_clicked = function() view.add() end } }, buttons)
  end
  if spec.end_widget then
    spec.end_widget.anchors = { center_in = true }
    ui.reparent(ui.Item { width = 36, height = BAR, spec.end_widget }, buttons)
  end
  local root = ui.Item { id = id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = BAR,
    ui.Item { width = W - BUTTONS, height = BAR, clip = true, strip }, buttons }
  return root, { node = root, strip = strip }
end
