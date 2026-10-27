-- AdwTabOverview: the tabs of a tab view (lib.kit.archetypes.navigation's
-- `tab_view`) as a grid of cards over it while it is open; a press or
-- Return on a card opens that tab and closes the overview (Collection).
--
--     local overview_node, overview = composites.tab_overview { id = "docs", view = view,
--       width = 520, height = 300 }
--     overview.open() ; overview.close() ; overview.toggle() ; overview.is_open()
--
-- `button(spec)` makes the button that toggles it (AdwTabButton: the count
-- of open tabs), for a tab bar's end or a header bar. Other fields:
-- `columns` (3), `card_height` (96), `empty_text`. Ids: `<id>-grid`,
-- `<id>-card-<i>`, `<id>-overview` (the button).
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local tokens = require("lib.kit.tokens")

return function(spec)
  spec = spec or {}
  local view = assert(spec.view, "tab_overview wants a tab view's handle (`view`)")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 300
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local open = morf.signal("kit.composites.tab_overview." .. tostring(view), false)
  local cards = morf.list_model({})
  local COLS = spec.columns or 3
  local CW = math.floor(W / COLS)
  local CH = spec.card_height or 96
  local function refill()
    local out = {}
    for i, tab in ipairs(view.tabs()) do out[i] = { key = tab.key, slot = i, title = tab.title, icon = tab.icon } end
    cards:replace(out, "key")
  end
  refill()
  local function open_card(i)
    open:set(false)
    view.select(i)
  end
  local grid_node = widgets.grid_view { id = sid("grid"), accessible_name = "Open tabs", width = W,
    height = H, rows = cards, cell_width = CW, cell_height = CH, grid_columns = COLS,
    current = function() return view.current() end,
    on_activated = function(i) open_card(i) end,
    delegate = function(row, s)
      -- (By key: a card moved keeps its delegate, and the delegate's index.)
      local me = morf.state { key = row.key }
      local function index()
        for i, tab in ipairs(view.tabs()) do if tab.key == me.key then return i end end
        return 0
      end
      local function now() return view.tabs()[index()] or row end
      local function is_current() return index() == view.current() end
      local area
      area = ui.MouseArea { id = sid(("card-%d"):format(row.slot)), width = CW, height = CH, cursor = "pointer",
        accessible_role = "button", accessible_name = function() return now().title end,
        on_clicked = function() open_card(index()) end,
        widgets.box { variant = "card", anchors = { fill = true, margins = 6 } },
        ui.Rect { anchors = { fill = true, margins = 6 }, radius = 12,
          border_width = function() return is_current() and 2 or 0 end,
          border_color = tokens.color("accent_bg"),
          color = function()
            local c = tokens.color("accent_bg")()
            return c:alpha(is_current() and 0.14 or (area and area.hovered and 0.07 or 0))
          end },
        widgets.image { icon = function() return now().icon or "tab" end, pixel_size = 22, variant = "accent",
          x = 18, y = 18 },
        widgets.label { x = 18, y = CH - 40, width = CW - 36, elide = "right",
          text = function() return now().title end } }
      return area, function(next_row)
        me.key = next_row.key
        if id then area.id = ("%s-card-%d"):format(id, next_row.slot) end
      end
    end }
  local root = ui.Rect { id = id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H,
    color = tokens.color("window_bg"), visible = function() return open:get() end,
    grid_node,
    widgets.label { anchors = { center_in = true }, variant = "dim", visible = function() return view.count() == 0 end,
      text = spec.empty_text or "No tabs open" } }
  morf.effect("kit.tab_overview.cards." .. tostring(root), refill, { owner = root })
  local handle = { node = root }
  function handle.open() open:set(true) morf.focus.set(grid_node, true) end
  function handle.close() open:set(false) end
  function handle.toggle() if open:get() then handle.close() else handle.open() end end
  function handle.is_open() return open:get() end
  function handle.button(own)
    own = own or {}
    return widgets.tab_button { id = sid("overview"), accessible_name = "Overview", width = own.width or 32,
      height = own.height or 32, count = function() return view.count() end, on_clicked = handle.toggle }
  end
  return root, handle
end
